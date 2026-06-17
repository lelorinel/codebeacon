pub mod protocol;
pub mod tools;

use crate::config;
use crate::lsp::client::path_to_uri;
use crate::lsp::pool::LspPool;
use crate::mcp::protocol::{tool_list, McpRequest, McpResponse};
use crate::mcp::tools::{dispatch, RepoCtx, ToolContext};
use anyhow::Result;
use serde_json::{json, Value};
use std::io::{self, BufRead as _, Write};
use std::path::PathBuf;
use std::sync::Mutex;

pub fn handle_request_inner(req: McpRequest, ctx: Option<&ToolContext>) -> McpResponse {
    let id = req.id.clone().unwrap_or(json!(null));

    match req.method.as_str() {
        "initialize" => McpResponse::result(id, json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "codebeacon", "version": "0.1.0" }
        })),
        "initialized" => McpResponse::notification(json!({})),
        "tools/list" => McpResponse::result(id, tool_list(ctx.map_or(false, |c| c.fs_tools))),
        "tools/call" => {
            let params = req.params.unwrap_or(json!({}));
            let name = params["name"].as_str().unwrap_or("");
            let args = &params["arguments"];
            match ctx {
                None => McpResponse::error(id, -32603, "no tool context"),
                Some(ctx) => match dispatch(ctx, name, args) {
                    Ok(result) => McpResponse::result(id, result),
                    Err(e) => McpResponse::error(id, -32603, &e.to_string()),
                }
            }
        }
        _ => McpResponse::error(id, -32601, "Method not found"),
    }
}

/// Start the MCP stdio server.
///
/// `override_root` is the `--root` CLI argument, if provided.
/// When omitted, roots are discovered in this order:
///   1. MCP `roots/list` request to the client (if client declares roots capability)
///   2. `CLAUDE_PROJECT_DIR` env var (Claude Code)
///   3. `CURSOR_WORKSPACE` env var (Cursor)
///   4. `cwd` — works for VS Code, Zed, Cline (they set cwd = workspace folder)
pub fn run_stdio_server(override_root: Option<PathBuf>, fs_tools: bool) -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut sin = stdin.lock();
    let mut out = stdout.lock();
    let mut buf = String::new();
    let mut client_has_roots = false;

    // ── Phase 1: MCP handshake ────────────────────────────────────────────
    // Handle `initialize` and `initialized` inline so we can:
    //   a) capture the client's roots capability
    //   b) send a `roots/list` request right after `initialized`
    //
    // Some clients (e.g. LM Studio) skip the `initialized` notification and
    // jump straight to tool calls. We detect that in the `_` arm and buffer
    // the request so it can be replayed in Phase 2 with a real ToolContext.
    let mut buffered_req: Option<McpRequest> = None;
    let roots_list_result: Option<PathBuf> = 'handshake: loop {
        buf.clear();
        if sin.read_line(&mut buf)? == 0 {
            return Ok(()); // client disconnected during handshake
        }
        let t = buf.trim();
        if t.is_empty() { continue; }

        let req: McpRequest = match serde_json::from_str(t) {
            Ok(r) => r,
            Err(e) => {
                write_msg(&mut out, &McpResponse::error(json!(null), -32700, &format!("Parse error: {e}")))?;
                continue;
            }
        };

        let id = req.id.clone().unwrap_or(json!(null));
        match req.method.as_str() {
            "initialize" => {
                if let Some(ref p) = req.params {
                    // Client declares roots support via capabilities.roots object
                    client_has_roots = p["capabilities"]["roots"].is_object();
                }
                write_msg(&mut out, &McpResponse::result(id, json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "codebeacon", "version": "0.1.0" }
                })))?;
            }
            "initialized" => {
                if override_root.is_none() && client_has_roots {
                    // Ask client for workspace roots (MCP protocol standard)
                    let req_str = serde_json::to_string(&json!({
                        "jsonrpc": "2.0", "id": 9001, "method": "roots/list", "params": {}
                    }))?;
                    writeln!(out, "{req_str}")?;
                    out.flush()?;

                    // Read roots/list response — the next non-empty line from client.
                    // Clients that don't support roots will respond with a -32601 error;
                    // we catch that and fall through to env/cwd discovery.
                    let root = loop {
                        buf.clear();
                        if sin.read_line(&mut buf)? == 0 { break None; }
                        let t = buf.trim();
                        if t.is_empty() { continue; }
                        break serde_json::from_str::<Value>(t).ok()
                            .and_then(|v| {
                                // Success: parse first root URI
                                v["result"]["roots"]
                                    .as_array()
                                    .and_then(|arr| arr.first())
                                    .and_then(|r| r["uri"].as_str())
                                    .and_then(uri_to_path)
                            });
                    };
                    break 'handshake root;
                } else {
                    break 'handshake None; // will use override_root or env/cwd
                }
            }
            _ => {
                // Client skipped `initialized` and sent something else (e.g. a
                // tools/call).  Treat the handshake as complete, then replay
                // this request in Phase 2 with a real ToolContext so it gets a
                // proper response instead of the "no tool context" error.
                buffered_req = Some(req);
                break 'handshake None;
            }
        }
    };

    // ── Resolve workspace start ────────────────────────────────────────────
    // Priority: --root > roots/list response > CLAUDE_PROJECT_DIR / CURSOR_WORKSPACE > cwd
    let start = override_root
        .or(roots_list_result)
        .unwrap_or_else(config::workspace_start_from_env);

    let repos = {
        let discovered = config::discover_repos(&start);
        if discovered.is_empty() { vec![start] } else { discovered }
    };

    tracing::info!("codebeacon workspace: {} repo(s)", repos.len());
    for r in &repos { tracing::info!("  repo: {}", r.display()); }

    // ── Spawn daemons ──────────────────────────────────────────────────────
    // Use the tokio runtime handle — safe because we're inside #[tokio::main].
    let tokio_handle = tokio::runtime::Handle::current();
    for repo in repos.clone() {
        tokio_handle.spawn(async move {
            if let Err(e) = crate::daemon::start(repo).await {
                tracing::error!("Daemon error: {e}");
            }
        });
    }

    // ── Build tool context ─────────────────────────────────────────────────
    let ctx_repos: Vec<RepoCtx> = repos
        .into_iter()
        .map(|root| {
            let name = root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "repo".into());
            let root_uri = path_to_uri(&root);
            let cfg = crate::config_file::load(&root).unwrap_or_default();
            let lsp_pool =
                Mutex::new(LspPool::new(&root_uri).with_overrides(cfg.lsp.overrides.clone()));
            RepoCtx { name, root, lsp_pool }
        })
        .collect();
    let ctx = ToolContext { repos: ctx_repos, fs_tools };

    // ── Phase 2: tool loop ─────────────────────────────────────────────────
    // Replay any request that arrived before `initialized` (e.g. LM Studio
    // skipping the notification and going straight to a tools/call).
    if let Some(req) = buffered_req {
        let resp = handle_request_inner(req, Some(&ctx));
        if resp.id.is_some() || resp.error.is_some() {
            write_msg(&mut out, &resp)?;
        }
    }

    loop {
        buf.clear();
        if sin.read_line(&mut buf)? == 0 { break; }
        let t = buf.trim();
        if t.is_empty() { continue; }

        let req: McpRequest = match serde_json::from_str(t) {
            Ok(r) => r,
            Err(e) => {
                write_msg(&mut out, &McpResponse::error(json!(null), -32700, &format!("Parse error: {e}")))?;
                continue;
            }
        };
        let resp = handle_request_inner(req, Some(&ctx));
        if resp.id.is_some() || resp.error.is_some() {
            write_msg(&mut out, &resp)?;
        }
    }
    Ok(())
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn write_msg(out: &mut impl Write, resp: &McpResponse) -> Result<()> {
    writeln!(out, "{}", serde_json::to_string(resp)?)?;
    out.flush()?;
    Ok(())
}

/// Convert a `file://` URI to a `PathBuf`, returning `None` if the path
/// doesn't exist on disk.
fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let without_scheme = uri.strip_prefix("file://")?;
    // Unix:    file:///home/user/project → /home/user/project
    // Windows: file:///C:/project        → trim leading '/' → C:/project
    #[cfg(windows)]
    let path_str = without_scheme.trim_start_matches('/');
    #[cfg(not(windows))]
    let path_str = without_scheme;
    let p = PathBuf::from(path_str);
    p.exists().then_some(p)
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn routes_tools_list() {
        let req = crate::mcp::protocol::McpRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: "tools/list".into(),
            params: None,
        };
        let resp = handle_request_inner(req, None);
        let tools = &resp.result.unwrap()["tools"];
        assert!(tools.is_array());
        // ctx is None here so fs_tools=false → 6 core tools (no fs tools)
        assert!(tools.as_array().unwrap().len() == 6);
    }

    #[test]
    fn routes_unknown_method_to_error() {
        let req = crate::mcp::protocol::McpRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(2)),
            method: "unknown/method".into(),
            params: None,
        };
        let resp = handle_request_inner(req, None);
        assert!(resp.error.is_some());
    }

    #[test]
    fn uri_to_path_parses_unix_file_uri() {
        // Only run on non-Windows where the path will actually exist-check correctly
        #[cfg(not(windows))]
        {
            // /tmp always exists on Unix
            let result = uri_to_path("file:///tmp");
            assert!(result.is_some(), "expected /tmp to parse and exist");
            assert_eq!(result.unwrap(), PathBuf::from("/tmp"));
        }
    }

    #[test]
    fn uri_to_path_returns_none_for_nonexistent() {
        let result = uri_to_path("file:///this/path/does/not/exist/ever");
        assert!(result.is_none());
    }
}
