pub mod protocol;
pub mod tools;

use crate::mcp::protocol::{tool_list, McpRequest, McpResponse};
use crate::mcp::tools::{dispatch, ToolContext};
use anyhow::Result;
use serde_json::json;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

pub fn handle_request_inner(req: McpRequest, ctx: Option<&ToolContext>) -> McpResponse {
    let id = req.id.clone().unwrap_or(json!(null));

    match req.method.as_str() {
        "initialize" => McpResponse::result(id, json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "codebeacon", "version": "0.1.0" }
        })),
        "initialized" => McpResponse::notification(json!({})),
        "tools/list" => McpResponse::result(id, tool_list()),
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

pub fn run_stdio_server(repo_root: PathBuf) -> Result<()> {
    let ctx = ToolContext { repo_root };
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }

        let req: McpRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                let err = McpResponse::error(json!(null), -32700, &format!("Parse error: {e}"));
                let body = serde_json::to_string(&err)?;
                writeln!(out, "{body}")?;
                out.flush()?;
                continue;
            }
        };

        let resp = handle_request_inner(req, Some(&ctx));
        if resp.id.is_some() || resp.error.is_some() {
            let body = serde_json::to_string(&resp)?;
            writeln!(out, "{body}")?;
            out.flush()?;
        }
    }
    Ok(())
}

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
        assert!(tools.as_array().unwrap().len() == 5);
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
}
