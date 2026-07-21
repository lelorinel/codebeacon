use crate::compact::{
    compact_mode,
    encode_index_response, encode_package_response, encode_query_matches, expand_path,
    read_dict, record_usage, resolve_file_arg_with_root, session_for_repo, DictSession,
};
use crate::config::codeindex_dir;
use crate::config_file::{CompactConfig, IntelligenceConfig, LoopConfig};
use crate::graph::{persistence as graph_persistence, DependencyGraph};
use crate::indexer::writer::{read_index, read_package};
use crate::lsp::pool::LspPool;
use crate::mcp::protocol::text_content;
use crate::query::RepoQueryCtx;
use crate::report;
use crate::security::{decide, verify_fragment, GateAction, SecurityPolicy};
use anyhow::{Context, Result};

use crate::mcp::conductor_handlers;
use crate::mcp::docs_handlers;
use crate::mcp::intelligence_handlers;
use crate::mcp::lock_handlers;
use crate::mcp::loop_handlers;
use crate::locks::SharedLockStore;
use serde_json::Value;
use std::path::{Component, PathBuf};
use std::sync::Mutex;

/// Per-repo context: index root, LSP pool, and a short display name.
pub struct RepoCtx {
    /// Short name used in multi-repo output prefixes (directory basename).
    pub name: String,
    /// Absolute path to the repo root (where `.codeindex/` lives).
    pub root: PathBuf,
    pub lsp_pool: Mutex<LspPool>,
    pub security: SecurityPolicy,
    pub compact: CompactConfig,
    pub intelligence: IntelligenceConfig,
    pub loop_config: LoopConfig,
    pub dict_session: Mutex<DictSession>,
    /// Absolute docs directory when sidecar docs are enabled.
    pub docs_root: Option<PathBuf>,
}

impl RepoCtx {
    pub(crate) fn codeindex(&self) -> PathBuf {
        codeindex_dir(&self.root)
    }

    fn load_graph(&self) -> DependencyGraph {
        let path = self.codeindex().join("graph.bin");
        graph_persistence::load(&path).unwrap_or_default()
    }

    pub(crate) fn dict_session_mut(&self) -> std::sync::MutexGuard<'_, DictSession> {
        self.dict_session.lock().unwrap()
    }

    pub(crate) fn ensure_dict_session(&self) {
        let mut session = self.dict_session_mut();
        if session.paths.is_empty() {
            *session = session_for_repo(&self.codeindex());
        }
    }

    pub(crate) fn resolve_file(&self, file: &str) -> PathBuf {
        self.ensure_dict_session();
        let session = self.dict_session.lock().unwrap();
        resolve_file_arg_with_root(&session, &self.root, file)
    }

    pub(crate) fn maybe_record_usage(&self, tool: &str, key: &str, compact: bool) {
        if compact {
            let _ = record_usage(&self.codeindex(), tool, key);
        }
    }

    /// Map a repo-relative path to a dict ref (`p1`) when compact mode is on.
    fn path_ref(&self, path: &str) -> String {
        self.ensure_dict_session();
        let mut session = self.dict_session_mut();
        session.path_id(path)
    }
}

/// Tool context handed to every MCP tool handler.
///
/// Single-repo: `repos` has one element; output format is identical to the
/// original single-root behaviour (no repo prefix, no wrapping object).
///
/// Multi-repo: `repos` has N>1 elements; file paths in output are prefixed with
/// `"repo_name/"` and `get_context` returns a `{ repos: [...] }` envelope.
pub struct ToolContext {
    pub repos: Vec<RepoCtx>,
    /// When false, file-system tools (read_file, write_file, edit_file,
    /// list_directory) are disabled. Enable with `codebeacon serve --fs-tools`.
    pub fs_tools: bool,
    /// Shared path-lock store (None when locks disabled).
    pub lock_store: Option<SharedLockStore>,
}

impl ToolContext {
    /// Returns the repos that match an optional `repo` name filter.
    /// With no filter all repos are returned.
    pub(crate) fn repos_for<'a>(&'a self, filter: Option<&str>) -> Vec<&'a RepoCtx> {
        match filter {
            Some(name) => self.repos.iter().filter(|r| r.name == name).collect(),
            None => self.repos.iter().collect(),
        }
    }

    /// True when serving more than one repo (multi-repo workspace mode).
    pub(crate) fn multi(&self) -> bool {
        self.repos.len() > 1
    }
}

pub fn dispatch(ctx: &ToolContext, name: &str, args: &Value) -> Result<Value> {
    match name {
        "get_context"      => handle_get_context(ctx, args),
        "drill_package"    => handle_drill_package(ctx, args),
        "find_references"  => handle_find_references(ctx, args),
        "find_definition"  => handle_find_definition(ctx, args),
        "get_dependents"   => handle_get_dependents(ctx, args),
        "init_workspace"   => handle_init_workspace(ctx, args),
        "verify_security"  => handle_verify_security(ctx, args),
        "query_context"    => handle_query_context(ctx, args),
        "shortest_path"    => handle_shortest_path(ctx, args),
        "hotspots"         => handle_hotspots(ctx, args),
        "get_report"       => handle_get_report(ctx, args),
        "get_index_summary"=> handle_get_index_summary(ctx, args),
        "get_hotspots"     => handle_get_hotspots_resource(ctx, args),
        "focus_context"    => intelligence_handlers::handle_focus_context(ctx, args),
        "task_context"     => intelligence_handlers::handle_task_context(ctx, args),
        "change_impact"    => intelligence_handlers::handle_change_impact(ctx, args),
        "index_status"     => intelligence_handlers::handle_index_status(ctx, args),
        "package_conventions" => intelligence_handlers::handle_package_conventions(ctx, args),
        "similar_symbols"  => intelligence_handlers::handle_similar_symbols(ctx, args),
        "api_surface"      => intelligence_handlers::handle_api_surface(ctx, args),
        "why_file"         => intelligence_handlers::handle_why_file(ctx, args),
        "fragile_files"    => intelligence_handlers::handle_fragile_files(ctx, args),
        "loop_begin"       => loop_handlers::handle_loop_begin(ctx, args),
        "loop_tick"        => loop_handlers::handle_loop_tick(ctx, args),
        "loop_record"      => loop_handlers::handle_loop_record(ctx, args),
        "loop_end"         => loop_handlers::handle_loop_end(ctx, args),
        "claim_path"       => lock_handlers::handle_claim_path(ctx, args),
        "release_path"     => lock_handlers::handle_release_path(ctx, args),
        "await_path"       => lock_handlers::handle_await_path(ctx, args),
        "list_locks"       => lock_handlers::handle_list_locks(ctx, args),
        "list_done"        => lock_handlers::handle_list_done(ctx, args),
        "session_done"     => lock_handlers::handle_session_done(ctx, args),
        "list_sessions"    => lock_handlers::handle_list_sessions(ctx, args),
        "spawn_agent"      => conductor_handlers::handle_spawn_agent(ctx, args),
        "list_agents"      => conductor_handlers::handle_list_agents(ctx, args),
        "agent_status"     => conductor_handlers::handle_agent_status(ctx, args),
        "query_docs"       => docs_handlers::handle_query_docs(ctx, args),
        "resolve_doc"      => docs_handlers::handle_resolve_doc(ctx, args),
        "docs_status"      => docs_handlers::handle_docs_status(ctx, args),
        "update_docs"      => docs_handlers::handle_update_docs(ctx, args),
        "read_file" | "write_file" | "edit_file" | "list_directory" => {
            if !ctx.fs_tools {
                anyhow::bail!(
                    "File-system tools are disabled. \
                     Restart codebeacon with `--fs-tools` to enable them."
                );
            }
            match name {
                "read_file"      => handle_read_file(ctx, args),
                "write_file"     => handle_write_file(ctx, args),
                "edit_file"      => handle_edit_file(ctx, args),
                "list_directory" => handle_list_directory(ctx, args),
                _ => unreachable!(),
            }
        }
        other => anyhow::bail!("unknown tool: {other}"),
    }
}

// ---------------------------------------------------------------------------
// get_context
// ---------------------------------------------------------------------------

pub fn handle_get_context(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);

    if repos.is_empty() {
        let available: Vec<&str> = ctx.repos.iter().map(|r| r.name.as_str()).collect();
        return Ok(text_content(format!(
            "No repo named '{}' found in this workspace.\nAvailable repos: {}.\n\
             Call get_context without the `repo` argument to see all repos.",
            repo_filter.unwrap_or("(none)"),
            available.join(", ")
        )));
    }

    if repos.len() == 1 {
        let repo = repos[0];
        let compact = compact_mode(args, &repo.compact);
        match read_index(&repo.codeindex())? {
            Some(index) => {
                let text = if compact {
                    repo.ensure_dict_session();
                    let base = read_dict(&repo.codeindex())?;
                    let mut session = repo.dict_session_mut();
                    let out = encode_index_response(&index, &mut session, base.as_ref());
                    serde_json::to_string_pretty(&out)?
                } else {
                    serde_json::to_string_pretty(&index)?
                };
                let hint = if repo.docs_root.is_some() {
                    "\n\n# docs enabled — use query_docs / resolve_doc / docs_status when you need documentation context"
                } else {
                    ""
                };
                return Ok(text_content(format!("{text}{hint}")));
            }
            None => return Ok(text_content(format!(
                "No index found for repo '{}'.\n\
                 Call `init_workspace` to build the index (may take a moment for large repos).",
                repo.name
            ))),
        }
    }

    // Multi-repo: { repos: [ { repo, index | error }, … ] }
    let mut all = vec![];
    for repo in repos {
        let compact = compact_mode(args, &repo.compact);
        match read_index(&repo.codeindex())? {
            Some(index) => {
                let payload = if compact {
                    repo.ensure_dict_session();
                    let base = read_dict(&repo.codeindex())?;
                    let mut session = repo.dict_session_mut();
                    encode_index_response(&index, &mut session, base.as_ref())
                } else {
                    serde_json::json!(index)
                };
                all.push(serde_json::json!({
                    "repo": repo.name,
                    "index": payload,
                }));
            }
            None => all.push(serde_json::json!({
                "repo": repo.name,
                "status": "not indexed — call `init_workspace` to build the index",
            })),
        }
    }
    Ok(text_content(serde_json::to_string_pretty(
        &serde_json::json!({ "repos": all }),
    )?))
}

// ---------------------------------------------------------------------------
// drill_package
// ---------------------------------------------------------------------------

pub fn handle_drill_package(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let raw_name = args["name"].as_str().context("missing 'name'")?;
    let repo_filter = args["repo"].as_str();

    // Accept "repo/package" notation as an alternative to the `repo` argument
    let (repo_hint, pkg_name): (Option<&str>, &str) =
        if let Some(slash) = raw_name.find('/') {
            (Some(&raw_name[..slash]), &raw_name[slash + 1..])
        } else {
            (repo_filter, raw_name)
        };

    let repos = ctx.repos_for(repo_hint);
    let add_prefix = ctx.multi() && repo_hint.is_none();

    for repo in repos {
        if let Some(pkg) = read_package(pkg_name, &repo.codeindex())? {
            let compact = compact_mode(args, &repo.compact);
            repo.maybe_record_usage("drill_package", pkg_name, compact);
            let text = if compact {
                repo.ensure_dict_session();
                let base = read_dict(&repo.codeindex())?;
                let mut session = repo.dict_session_mut();
                let out = encode_package_response(&pkg, &mut session, base.as_ref());
                serde_json::to_string_pretty(&out)?
            } else {
                serde_json::to_string_pretty(&pkg)?
            };
            return Ok(text_content(if add_prefix {
                format!("[repo: {}]\n{}", repo.name, text)
            } else {
                text
            }));
        }
    }

    anyhow::bail!("package '{raw_name}' not found")
}

// ---------------------------------------------------------------------------
// find_definition
// ---------------------------------------------------------------------------

pub fn handle_find_definition(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let symbol = args["symbol"].as_str().context("missing 'symbol'")?;
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);
    let add_prefix = ctx.multi() && repo_filter.is_none();

    // If the caller supplies file + position, try LSP first
    if let (Some(file), Some(line), Some(character)) = (
        args["file"].as_str(),
        args["line"].as_u64(),
        args["character"].as_u64(),
    ) {
        for repo in &repos {
            let compact = compact_mode(args, &repo.compact);
            let abs_path = repo.resolve_file(file);
            if !abs_path.exists() { continue; }
            if let Some(lang) = crate::config::detect_language(&abs_path) {
                let mut pool = repo.lsp_pool.lock().unwrap();
                if let Some(client) = pool.get_or_start(&lang) {
                    match client.definition(&abs_path, line as u32, character as u32) {
                        Ok(result) => {
                            if let Some((def_path, def_line)) =
                                crate::lsp::parser::parse_definition(&result)
                            {
                                let rel = def_path.strip_prefix(&repo.root).unwrap_or(&def_path);
                                let prefix = if add_prefix {
                                    format!("{}/", repo.name)
                                } else {
                                    String::new()
                                };
                                let path_part = if compact {
                                    repo.path_ref(&rel.to_string_lossy())
                                } else {
                                    rel.display().to_string()
                                };
                                return Ok(text_content(format!(
                                    "{}{}:{} (via LSP)",
                                    prefix,
                                    path_part,
                                    def_line
                                )));
                            }
                        }
                        Err(e) => tracing::warn!("LSP definition failed: {e}"),
                    }
                }
            }
            break; // file resolved to this repo; no need to search further
        }
    }

    // Index-based fallback: search all (or filtered) repos
    let mut found: Vec<String> = vec![];
    for repo in &repos {
        let compact = compact_mode(args, &repo.compact);
        if compact {
            repo.maybe_record_usage("find_definition", symbol, true);
        }
        let packages_dir = repo.codeindex().join("packages");
        let mut pkg_files: Vec<_> = std::fs::read_dir(&packages_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.path().to_str().map(str::to_string))
            .collect();
        pkg_files.sort(); // deterministic

        for pkg_path in pkg_files {
            if let Ok(text) = std::fs::read_to_string(&pkg_path) {
                if let Ok(pkg) = serde_json::from_str::<crate::types::PackageDetail>(&text) {
                    for file in pkg.files {
                        for sym in &file.symbols {
                            if sym.name == symbol {
                                let prefix = if add_prefix {
                                    format!("{}/", repo.name)
                                } else {
                                    String::new()
                                };
                                let path_str = file.path.to_string_lossy();
                                let path_part = if compact {
                                    repo.path_ref(&path_str)
                                } else {
                                    path_str.into_owned()
                                };
                                found.push(format!(
                                    "{}{}:{} — {}",
                                    prefix,
                                    path_part,
                                    sym.line,
                                    sym.signature
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    if found.is_empty() {
        Ok(text_content(format!("Definition of '{symbol}' not found")))
    } else {
        Ok(text_content(found.join("\n")))
    }
}

// ---------------------------------------------------------------------------
// find_references
// ---------------------------------------------------------------------------

pub fn handle_find_references(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let symbol = args["symbol"].as_str().context("missing 'symbol'")?;
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);
    let add_prefix = ctx.multi() && repo_filter.is_none();

    // If the caller supplies file + position, try LSP first
    if let (Some(file), Some(line), Some(character)) = (
        args["file"].as_str(),
        args["line"].as_u64(),
        args["character"].as_u64(),
    ) {
        for repo in &repos {
            let compact = compact_mode(args, &repo.compact);
            let abs_path = repo.resolve_file(file);
            if !abs_path.exists() { continue; }
            if let Some(lang) = crate::config::detect_language(&abs_path) {
                let mut pool = repo.lsp_pool.lock().unwrap();
                if let Some(client) = pool.get_or_start(&lang) {
                    match client.references(&abs_path, line as u32, character as u32) {
                        Ok(result) => {
                            let refs = crate::lsp::parser::parse_references(&result);
                            if !refs.is_empty() {
                                let lines: Vec<String> = refs
                                    .iter()
                                    .map(|r| {
                                        let rel = r
                                            .file
                                            .strip_prefix(&repo.root)
                                            .unwrap_or(&r.file);
                                        let prefix = if add_prefix {
                                            format!("{}/", repo.name)
                                        } else {
                                            String::new()
                                        };
                                        let path_part = if compact {
                                            repo.path_ref(&rel.to_string_lossy())
                                        } else {
                                            rel.display().to_string()
                                        };
                                        format!("{}{}:{}", prefix, path_part, r.line)
                                    })
                                    .collect();
                                return Ok(text_content(format!(
                                    "References to '{}' (via LSP):\n{}",
                                    symbol,
                                    lines.join("\n")
                                )));
                            }
                        }
                        Err(e) => tracing::warn!("LSP references failed: {e}"),
                    }
                }
            }
            break;
        }
    }

    // Try to auto-locate the symbol's definition in the index, then use LSP
    'outer: for repo in &repos {
        let compact = compact_mode(args, &repo.compact);
        let packages_dir = repo.codeindex().join("packages");
        let mut pkg_files: Vec<_> = std::fs::read_dir(&packages_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.path().to_str().map(str::to_string))
            .collect();
        pkg_files.sort();

        for pkg_path in &pkg_files {
            if let Ok(text) = std::fs::read_to_string(pkg_path) {
                if let Ok(pkg) = serde_json::from_str::<crate::types::PackageDetail>(&text) {
                    for file in &pkg.files {
                        for sym in &file.symbols {
                            if sym.name == symbol {
                                let abs_path = repo.root.join(&file.path);
                                if let Some(lang) = crate::config::detect_language(&abs_path) {
                                    let mut pool = repo.lsp_pool.lock().unwrap();
                                    if let Some(client) = pool.get_or_start(&lang) {
                                        match client.references(
                                            &abs_path,
                                            sym.line,
                                            sym.character,
                                        ) {
                                            Ok(result) => {
                                                let refs =
                                                    crate::lsp::parser::parse_references(&result);
                                                if !refs.is_empty() {
                                                    let lines: Vec<String> = refs
                                                        .iter()
                                                        .map(|r| {
                                                            let rel = r
                                                                .file
                                                                .strip_prefix(&repo.root)
                                                                .unwrap_or(&r.file);
                                                            let prefix = if add_prefix {
                                                                format!("{}/", repo.name)
                                                            } else {
                                                                String::new()
                                                            };
                                                            let path_part = if compact {
                                                                repo.path_ref(&rel.to_string_lossy())
                                                            } else {
                                                                rel.display().to_string()
                                                            };
                                                            format!(
                                                                "{}{}:{}",
                                                                prefix,
                                                                path_part,
                                                                r.line
                                                            )
                                                        })
                                                        .collect();
                                                    return Ok(text_content(format!(
                                                        "References to '{}' (via LSP from index):\n{}",
                                                        symbol,
                                                        lines.join("\n")
                                                    )));
                                                }
                                            }
                                            Err(e) => {
                                                tracing::warn!(
                                                    "LSP references (auto) failed: {e}"
                                                )
                                            }
                                        }
                                    }
                                }
                                break 'outer;
                            }
                        }
                    }
                }
            }
        }
    }

    // Final fallback: index substring search
    let mut found: Vec<String> = vec![];
    for repo in &repos {
        let compact = compact_mode(args, &repo.compact);
        if compact {
            repo.maybe_record_usage("find_references", symbol, true);
        }
        let packages_dir = repo.codeindex().join("packages");
        let mut pkg_files: Vec<_> = std::fs::read_dir(&packages_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.path().to_str().map(str::to_string))
            .collect();
        pkg_files.sort();

        for pkg_path in &pkg_files {
            if let Ok(text) = std::fs::read_to_string(pkg_path) {
                if let Ok(pkg) = serde_json::from_str::<crate::types::PackageDetail>(&text) {
                    for file in pkg.files {
                        for sym in &file.symbols {
                            if sym.name.contains(symbol) {
                                let prefix = if add_prefix {
                                    format!("{}/", repo.name)
                                } else {
                                    String::new()
                                };
                                let path_str = file.path.to_string_lossy();
                                let path_part = if compact {
                                    repo.path_ref(&path_str)
                                } else {
                                    path_str.into_owned()
                                };
                                found.push(format!(
                                    "{}{}:{} — {} [index fallback]",
                                    prefix,
                                    path_part,
                                    sym.line,
                                    sym.signature
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    if found.is_empty() {
        Ok(text_content(format!("No references found for '{symbol}'")))
    } else {
        Ok(text_content(found.join("\n")))
    }
}

// ---------------------------------------------------------------------------
// get_dependents
// ---------------------------------------------------------------------------

pub fn handle_get_dependents(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let file = args["file"].as_str().context("missing 'file'")?;
    let repo_filter = args["repo"].as_str();
    let add_prefix = ctx.multi() && repo_filter.is_none();

    // Accept "repo/path" notation — but only if the first segment matches a known repo
    let (repo_hint, file_path): (Option<&str>, &str) = if let Some(slash) = file.find('/') {
        let potential_repo = &file[..slash];
        if ctx.repos.iter().any(|r| r.name == potential_repo) {
            (Some(potential_repo), &file[slash + 1..])
        } else {
            (repo_filter, file)
        }
    } else {
        (repo_filter, file)
    };

    let repos = ctx.repos_for(repo_hint);

    let resolved_path = repos
        .first()
        .map(|r| {
            let abs = r.resolve_file(file_path);
            abs.strip_prefix(&r.root)
                .unwrap_or(&abs)
                .to_string_lossy()
                .into_owned()
        })
        .unwrap_or_else(|| file_path.to_string());

    let mut all_dependents: Vec<String> = vec![];

    for repo in &repos {
        let compact = compact_mode(args, &repo.compact);
        let graph = repo.load_graph();
        let abs_path = repo.root.join(&resolved_path);
        let rel_path = PathBuf::from(&resolved_path);
        let dependents = {
            let by_abs = graph.reverse_neighbors(&abs_path);
            if !by_abs.is_empty() {
                by_abs
            } else {
                graph.reverse_neighbors(&rel_path)
            }
        };
        for p in dependents {
            let rel = p.strip_prefix(&repo.root).unwrap_or(&p);
            let prefix = if add_prefix {
                format!("{}/", repo.name)
            } else {
                String::new()
            };
            let path_part = if compact {
                repo.path_ref(&rel.to_string_lossy())
            } else {
                rel.display().to_string()
            };
            all_dependents.push(format!("{prefix}{path_part}"));
        }
    }

    if all_dependents.is_empty() {
        Ok(text_content(format!("No files depend on '{file}'")))
    } else {
        Ok(text_content(all_dependents.join("\n")))
    }
}

// ---------------------------------------------------------------------------
// init_workspace
// ---------------------------------------------------------------------------

/// Build (or rebuild) the `.codeindex/` for one or all repos in the workspace.
///
/// Intended to be called by the LLM when `get_context` reports that no index
/// exists yet. Pass `repo` to limit indexing to a single repo.
pub fn handle_init_workspace(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);

    let mut lines: Vec<String> = vec![];
    for repo in repos {
        tracing::info!("init_workspace: indexing {}", repo.root.display());
        match crate::indexer::Indexer::with_docs(&repo.root, repo.docs_root.as_deref()).full_index() {
            Ok(()) => {
                let extra = if repo.docs_root.is_some() {
                    " (docs sidecar indexed)"
                } else {
                    ""
                };
                lines.push(format!("'{}' indexed successfully{}.", repo.name, extra));
            }
            Err(e) => lines.push(format!("'{}' failed: {e}", repo.name)),
        }
    }
    Ok(text_content(lines.join("\n")))
}

// ---------------------------------------------------------------------------
// Graph query tools (query_context, shortest_path, hotspots)
// ---------------------------------------------------------------------------

fn repo_query_ctx(repo: &RepoCtx) -> Result<RepoQueryCtx> {
    RepoQueryCtx::load(&repo.root)
}

pub fn handle_query_context(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let question = args["question"].as_str().context("missing 'question'")?;
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);
    let add_prefix = ctx.multi() && repo_filter.is_none();

    for repo in repos {
        match repo_query_ctx(repo) {
            Ok(qctx) => {
                let compact = compact_mode(args, &repo.compact);
                repo.maybe_record_usage("query_context", question, compact);
                let text = if compact {
                    let matches = qctx.query(question, 10);
                    repo.ensure_dict_session();
                    let mut session = repo.dict_session_mut();
                    let compact_matches = encode_query_matches(&matches, &mut session);
                    serde_json::to_string_pretty(&serde_json::json!({
                        "question": question,
                        "matches": compact_matches,
                    }))?
                } else {
                    qctx.format_query(question, 10)
                };
                return Ok(text_content(if add_prefix {
                    format!("[repo: {}]\n{text}", repo.name)
                } else {
                    text
                }));
            }
            Err(e) => {
                if repo_filter.is_some() {
                    return Ok(text_content(format!("Index error for '{}': {e}", repo.name)));
                }
            }
        }
    }
    Ok(text_content(
        "No indexed repo found. Call `init_workspace` first.",
    ))
}

pub fn handle_shortest_path(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let from = args["from"].as_str().context("missing 'from'")?;
    let to = args["to"].as_str().context("missing 'to'")?;
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);

    for repo in repos {
        if let Ok(qctx) = repo_query_ctx(repo) {
            let compact = compact_mode(args, &repo.compact);
            let from_resolved = {
                repo.ensure_dict_session();
                let session = repo.dict_session.lock().unwrap();
                expand_path(&session, from)
            };
            let to_resolved = {
                let session = repo.dict_session.lock().unwrap();
                expand_path(&session, to)
            };
            let mut path = qctx.path_between(&from_resolved, &to_resolved)?;
            if compact {
                path = path
                    .split(" --imports--> ")
                    .map(|s| repo.path_ref(s))
                    .collect::<Vec<_>>()
                    .join(" --imports--> ");
            }
            let prefix = if ctx.multi() && repo_filter.is_none() {
                format!("[repo: {}] ", repo.name)
            } else {
                String::new()
            };
            return Ok(text_content(format!("{prefix}{path}")));
        }
    }
    anyhow::bail!("no indexed repo for shortest_path")
}

pub fn handle_hotspots(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let limit = args["limit"].as_u64().unwrap_or(10) as usize;
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);

    for repo in repos {
        if let Ok(qctx) = repo_query_ctx(repo) {
            let compact = compact_mode(args, &repo.compact);
            let text = if compact {
                qctx.hotspots_compact_text(limit, |path| repo.path_ref(path))
            } else {
                qctx.hotspots_text(limit)
            };
            let prefix = if ctx.multi() && repo_filter.is_none() {
                format!("[repo: {}]\n", repo.name)
            } else {
                String::new()
            };
            return Ok(text_content(format!("{prefix}{text}")));
        }
    }
    anyhow::bail!("no indexed repo for hotspots")
}

// ---------------------------------------------------------------------------
// Pseudo-resource tools (codebeacon://report, //index, //hotspots)
// ---------------------------------------------------------------------------

pub fn handle_get_report(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);
    for repo in repos {
        match report::generate_or_read(&repo.root) {
            Ok(md) => {
                let prefix = if ctx.multi() && repo_filter.is_none() {
                    format!("[repo: {}]\n", repo.name)
                } else {
                    String::new()
                };
                return Ok(text_content(format!("{prefix}{md}")));
            }
            Err(e) if repo_filter.is_some() => {
                return Ok(text_content(format!("Report error: {e}")));
            }
            Err(_) => continue,
        }
    }
    anyhow::bail!("no repo for get_report")
}

pub fn handle_get_index_summary(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);

    for repo in repos {
        if let Some(index) = read_index(&repo.codeindex())? {
            let compact = compact_mode(args, &repo.compact);
            let text = if compact {
                repo.ensure_dict_session();
                let base = read_dict(&repo.codeindex())?;
                let mut session = repo.dict_session_mut();
                let out = encode_index_response(&index, &mut session, base.as_ref());
                serde_json::to_string_pretty(&out)?
            } else {
                serde_json::to_string_pretty(&index)?
            };
            let prefix = if ctx.multi() && repo_filter.is_none() {
                format!("[repo: {}]\n", repo.name)
            } else {
                String::new()
            };
            return Ok(text_content(format!("{prefix}{text}")));
        }
    }
    Ok(text_content("No index found. Call init_workspace."))
}

pub fn handle_get_hotspots_resource(ctx: &ToolContext, args: &Value) -> Result<Value> {
    handle_hotspots(ctx, args)
}

// ---------------------------------------------------------------------------
// File-system tools (read_file / write_file / edit_file / list_directory)
// ---------------------------------------------------------------------------
//
// All operations are sandboxed to the configured repo roots.
// Paths are normalised (resolving `..`) without requiring the file to exist,
// then verified to be within a repo root before any I/O is performed.

/// Normalise a path by resolving `.` and `..` without touching the filesystem.
fn normalize_path(path: &std::path::Path) -> PathBuf {
    let mut out: Vec<Component> = vec![];
    for c in path.components() {
        match c {
            Component::ParentDir => { out.pop(); }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out.iter().collect()
}

/// Resolve a user-supplied path relative to `repo_root`, ensuring the result
/// stays inside `repo_root`. Returns the absolute, normalised path on success.
fn safe_path(repo_root: &std::path::Path, user_path: &str) -> Result<PathBuf> {
    let abs = if std::path::Path::new(user_path).is_absolute() {
        PathBuf::from(user_path)
    } else {
        repo_root.join(user_path)
    };
    let resolved = normalize_path(&abs);
    if !resolved.starts_with(repo_root) {
        anyhow::bail!(
            "Path '{}' escapes the repo root — path traversal denied.",
            user_path
        );
    }
    Ok(resolved)
}

/// Pick a single repo for a write/edit operation.
///
/// Rules:
/// - If `repo` arg given → use that repo.
/// - If only one repo → use it.
/// - Otherwise error: user must specify which repo to write to.
fn single_repo_for_write<'a>(ctx: &'a ToolContext, repo_filter: Option<&str>) -> Result<&'a RepoCtx> {
    let repos = ctx.repos_for(repo_filter);
    match repos.len() {
        0 => anyhow::bail!("No repo found matching filter '{}'", repo_filter.unwrap_or("(none)")),
        1 => Ok(repos[0]),
        _ => anyhow::bail!(
            "Multiple repos in workspace. Use the `repo` argument to specify which one to write to."
        ),
    }
}

/// Run security verification on a code fragment; block or warn per repo policy.
fn apply_security_gate(repo: &RepoCtx, path: &std::path::Path, content: &str) -> Result<Option<String>> {
    let report = verify_fragment(path, content, &repo.security);
    match decide(&report, &repo.security) {
        GateAction::Allow => Ok(None),
        GateAction::Warn { message } => Ok(Some(message)),
        GateAction::Block { message } => anyhow::bail!(message),
    }
}

// ---------------------------------------------------------------------------
// read_file

pub fn handle_read_file(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let path_str = args["path"].as_str().context("missing 'path'")?;
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);

    // Try repos in order; return the first match that resolves safely and exists.
    for repo in &repos {
        let abs = match safe_path(&repo.root, path_str) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if !abs.exists() { continue; }

        let content = std::fs::read_to_string(&abs)
            .with_context(|| format!("Failed to read '{}'", abs.display()))?;

        let prefix = if ctx.multi() && repo_filter.is_none() {
            format!("# {}/{}\n\n", repo.name, path_str)
        } else {
            format!("# {}\n\n", path_str)
        };
        return Ok(text_content(format!("{prefix}{content}")));
    }

    anyhow::bail!("File '{}' not found in any repo.", path_str)
}

// ---------------------------------------------------------------------------
// write_file

pub fn handle_write_file(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let path_str = args["path"].as_str().context("missing 'path'")?;
    let content  = args["content"].as_str().context("missing 'content'")?;
    let repo_filter = args["repo"].as_str();

    let repo = single_repo_for_write(ctx, repo_filter)?;
    let abs  = safe_path(&repo.root, path_str)?;

    let warning = apply_security_gate(&repo, &abs, content)?;

    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directories for '{}'", abs.display()))?;
    }
    std::fs::write(&abs, content)
        .with_context(|| format!("Failed to write '{}'", abs.display()))?;

    let mut msg = format!("Written: {}/{}", repo.name, path_str);
    if let Some(w) = warning {
        msg.push_str(&format!("\n\n{w}"));
    }
    Ok(text_content(msg))
}

// ---------------------------------------------------------------------------
// edit_file

pub fn handle_edit_file(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let path_str   = args["path"].as_str().context("missing 'path'")?;
    let old_string = args["old_string"].as_str().context("missing 'old_string'")?;
    let new_string = args["new_string"].as_str().context("missing 'new_string'")?;
    let repo_filter = args["repo"].as_str();

    let repo = single_repo_for_write(ctx, repo_filter)?;
    let abs  = safe_path(&repo.root, path_str)?;

    let warning = apply_security_gate(&repo, &abs, new_string)?;

    let original = std::fs::read_to_string(&abs)
        .with_context(|| format!("Failed to read '{}' for editing", abs.display()))?;

    let count = original.matches(old_string).count();
    if count == 0 {
        anyhow::bail!(
            "old_string not found in '{}'. Make sure it matches the file contents exactly.",
            path_str
        );
    }

    // Replace first occurrence
    let updated = original.replacen(old_string, new_string, 1);
    std::fs::write(&abs, &updated)
        .with_context(|| format!("Failed to write edited '{}'", abs.display()))?;

    let note = if count > 1 {
        format!(" ({} occurrences found; replaced the first one)", count)
    } else {
        String::new()
    };
    let mut msg = format!("Edited: {}/{}{}", repo.name, path_str, note);
    if let Some(w) = warning {
        msg.push_str(&format!("\n\n{w}"));
    }
    Ok(text_content(msg))
}

// ---------------------------------------------------------------------------
// verify_security
// ---------------------------------------------------------------------------

pub fn handle_verify_security(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let content = args["content"].as_str().context("missing 'content'")?;
    let path_str = args["path"].as_str().unwrap_or("fragment");
    let repo_filter = args["repo"].as_str();

    let repo = single_repo_for_write(ctx, repo_filter)?;
    let abs = if path_str == "fragment" {
        repo.root.join("fragment")
    } else {
        safe_path(&repo.root, path_str)?
    };

    if !repo.security.enabled {
        return Ok(text_content(
            "Security verification is disabled. \
             Enable with `codebeacon serve --security` or `[security] enabled = true` in .codeindex.toml.",
        ));
    }

    let report = verify_fragment(&abs, content, &repo.security);
    let action = decide(&report, &repo.security);

    let text = match &action {
        GateAction::Allow if report.findings.is_empty() => format!(
            "No security issues found in `{}` ({} ms).",
            report.path, report.elapsed_ms
        ),
        GateAction::Allow => format!(
            "All {} site(s) in `{}` are proven safe ({} Z3 call(s), {} ms).",
            report.sites_checked, report.path, report.z3_invocations, report.elapsed_ms
        ),
        GateAction::Warn { message } | GateAction::Block { message } => message.clone(),
    };

    Ok(text_content(text))
}

// ---------------------------------------------------------------------------
// list_directory

pub fn handle_list_directory(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let path_str = args["path"].as_str().unwrap_or(".");
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);
    let multi = ctx.multi() && repo_filter.is_none();

    let mut output = vec![];

    for repo in &repos {
        let abs = safe_path(&repo.root, path_str)?;
        if !abs.exists() { continue; }

        let mut entries: Vec<String> = std::fs::read_dir(&abs)
            .with_context(|| format!("Cannot list '{}'", abs.display()))?
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                // Skip hidden dirs and codeindex
                if name.starts_with('.') { return None; }
                let suffix = if e.path().is_dir() { "/" } else { "" };
                Some(format!("{name}{suffix}"))
            })
            .collect();
        entries.sort();

        if multi {
            output.push(format!("[{}]\n{}", repo.name, entries.join("\n")));
        } else {
            output.push(entries.join("\n"));
        }
    }

    if output.is_empty() {
        Ok(text_content(format!("'{}' not found or empty.", path_str)))
    } else {
        Ok(text_content(output.join("\n\n")))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compact::{build_dict_from_packages, write_dict};
    use crate::types::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Build a single-repo ToolContext pointing at `tmp`.
    fn single_ctx(tmp: &TempDir) -> ToolContext {
        ToolContext {
            repos: vec![test_repo_ctx("test", tmp.path())],
            fs_tools: true,
            lock_store: None,
        }
    }

    fn test_repo_ctx(name: &str, root: &std::path::Path) -> RepoCtx {
        RepoCtx {
            name: name.into(),
            root: root.to_path_buf(),
            lsp_pool: Mutex::new(LspPool::new("file:///tmp")),
            security: SecurityPolicy::default(),
            compact: CompactConfig::default(),
            intelligence: IntelligenceConfig::default(),
            loop_config: LoopConfig::default(),
            dict_session: Mutex::new(DictSession::default()),
            docs_root: None,
        }
    }

    fn setup_codeindex(tmp: &TempDir) {
        use crate::indexer::writer::{write_index, write_package};
        let idx = RepoIndex {
            repo: "test".into(),
            generated_at: "2026-06-16T00:00:00Z".into(),
            packages: vec![PackageSummary {
                name: "auth".into(),
                purpose: "auth".into(),
                files: 1,
                score: 0.9,
            }],
            hot_symbols: vec!["login".into()],
        };
        let pkg = PackageDetail {
            name: "auth".into(),
            files: vec![FileEntry {
                path: PathBuf::from("src/auth.rs"),
                symbols: vec![SymbolEntry {
                    name: "login".into(),
                    signature: "fn login()".into(),
                    kind: SymbolKind::Function,
                    line: 1,
                    character: 0,
                }],
                depends_on: vec![],
                depended_by: vec![],
            }],
        };
        let ci = tmp.path().join(".codeindex");
        write_index(&idx, &ci).unwrap();
        write_package(&pkg, &ci).unwrap();
    }

    #[test]
    fn get_context_returns_index_json() {
        let tmp = TempDir::new().unwrap();
        setup_codeindex(&tmp);
        let ctx = single_ctx(&tmp);
        let result =
            handle_get_context(&ctx, &serde_json::json!({"files": []})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("auth"));
    }

    #[test]
    fn get_context_suggests_init_when_no_index() {
        let tmp = TempDir::new().unwrap();
        // No .codeindex created
        let ctx = single_ctx(&tmp);
        let result = handle_get_context(&ctx, &serde_json::json!({})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("init_workspace"), "expected init_workspace hint in: {text}");
    }

    // --- File-system tool tests ---

    #[test]
    fn read_file_returns_content() {
        use std::fs;
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("hello.rs"), "fn main() {}").unwrap();
        let ctx = single_ctx(&tmp);
        let result = handle_read_file(&ctx, &serde_json::json!({"path": "hello.rs"})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("fn main()"), "expected file content in: {text}");
    }

    #[test]
    fn read_file_errors_for_missing_file() {
        let tmp = TempDir::new().unwrap();
        let ctx = single_ctx(&tmp);
        let result = handle_read_file(&ctx, &serde_json::json!({"path": "nonexistent.rs"}));
        assert!(result.is_err() || {
            let t = result.unwrap();
            t["content"][0]["text"].as_str().unwrap().contains("not found")
        });
    }

    #[test]
    fn write_file_creates_file() {
        let tmp = TempDir::new().unwrap();
        let ctx = single_ctx(&tmp);
        handle_write_file(&ctx, &serde_json::json!({
            "path": "src/new.rs",
            "content": "pub fn greet() {}"
        })).unwrap();
        let written = std::fs::read_to_string(tmp.path().join("src/new.rs")).unwrap();
        assert_eq!(written, "pub fn greet() {}");
    }

    #[test]
    fn write_file_denied_outside_repo() {
        let tmp = TempDir::new().unwrap();
        let ctx = single_ctx(&tmp);
        let result = handle_write_file(&ctx, &serde_json::json!({
            "path": "../../etc/passwd",
            "content": "evil"
        }));
        assert!(result.is_err(), "expected path traversal to be denied");
    }

    #[test]
    fn edit_file_replaces_string() {
        use std::fs;
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("lib.rs"), "fn old_name() {}").unwrap();
        let ctx = single_ctx(&tmp);
        handle_edit_file(&ctx, &serde_json::json!({
            "path": "lib.rs",
            "old_string": "old_name",
            "new_string": "new_name"
        })).unwrap();
        let content = fs::read_to_string(tmp.path().join("lib.rs")).unwrap();
        assert_eq!(content, "fn new_name() {}");
    }

    #[test]
    fn edit_file_errors_when_old_string_not_found() {
        use std::fs;
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("lib.rs"), "fn hello() {}").unwrap();
        let ctx = single_ctx(&tmp);
        let result = handle_edit_file(&ctx, &serde_json::json!({
            "path": "lib.rs",
            "old_string": "nonexistent",
            "new_string": "replacement"
        }));
        assert!(result.is_err(), "expected error when old_string not found");
    }

    #[test]
    fn write_file_security_warns_on_cwe190_pattern() {
        use std::fs;
        let tmp = TempDir::new().unwrap();
        let mut ctx = single_ctx(&tmp);
        ctx.repos[0].security.enabled = true;
        let result = handle_write_file(&ctx, &serde_json::json!({
            "path": "alloc.c",
            "content": "void* p = malloc(n * sizeof(int));"
        }));

        #[cfg(feature = "security-z3")]
        {
            let err = result.expect_err("expected block on proven CWE-190");
            let text = err.to_string();
            assert!(text.contains("CWE-190"), "expected CWE-190 in: {text}");
            assert!(
                text.contains("BLOCKED") || text.contains("PROVEN"),
                "expected block/proven in: {text}"
            );
            assert!(fs::read_to_string(tmp.path().join("alloc.c")).is_err());
        }

        #[cfg(not(feature = "security-z3"))]
        {
            let result = result.unwrap();
            let text = result["content"][0]["text"].as_str().unwrap();
            assert!(text.contains("WARNING"), "expected warning in: {text}");
            assert!(text.contains("CWE-190"), "expected CWE-190 in: {text}");
            assert!(fs::read_to_string(tmp.path().join("alloc.c")).is_ok());
        }
    }

    #[test]
    fn verify_security_reports_pattern_finding() {
        let mut ctx = single_ctx(&TempDir::new().unwrap());
        ctx.repos[0].security.enabled = true;
        let result = handle_verify_security(&ctx, &serde_json::json!({
            "content": "int* p = malloc(n * sizeof(int));"
        })).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("CWE-190"), "expected CWE-190 in: {text}");
    }

    #[test]
    fn list_directory_returns_entries() {
        use std::fs;
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("README.md"), "").unwrap();
        let ctx = single_ctx(&tmp);
        let result = handle_list_directory(&ctx, &serde_json::json!({})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("src/"), "expected src/ dir in: {text}");
        assert!(text.contains("README.md"), "expected README.md in: {text}");
    }

    #[test]
    fn init_workspace_builds_index() {
        use std::fs;
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src/lib.rs"), "pub fn hello() {}").unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();

        let ctx = single_ctx(&tmp);
        let result = handle_init_workspace(&ctx, &serde_json::json!({})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("indexed successfully"), "expected success in: {text}");
        assert!(tmp.path().join(".codeindex/index.json").exists(), ".codeindex/index.json should exist");
    }

    #[test]
    fn drill_package_returns_package_detail() {
        let tmp = TempDir::new().unwrap();
        setup_codeindex(&tmp);
        let ctx = single_ctx(&tmp);
        let result =
            handle_drill_package(&ctx, &serde_json::json!({"name": "auth"})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("login"));
    }

    fn setup_codeindex_multi(tmp: &TempDir) {
        use crate::indexer::writer::{write_index, write_package};
        let idx = RepoIndex {
            repo: "test".into(),
            generated_at: "2026-06-16T00:00:00Z".into(),
            packages: vec![
                PackageSummary {
                    name: "auth".into(),
                    purpose: "auth".into(),
                    files: 1,
                    score: 0.9,
                },
                PackageSummary {
                    name: "api".into(),
                    purpose: "api".into(),
                    files: 1,
                    score: 0.8,
                },
            ],
            hot_symbols: vec!["validate".into()],
        };
        let pkg_auth = PackageDetail {
            name: "auth".into(),
            files: vec![FileEntry {
                path: PathBuf::from("src/auth.rs"),
                symbols: vec![SymbolEntry {
                    name: "validate".into(),
                    signature: "fn validate() -> bool".into(),
                    kind: SymbolKind::Function,
                    line: 3,
                    character: 0,
                }],
                depends_on: vec![],
                depended_by: vec![],
            }],
        };
        let pkg_api = PackageDetail {
            name: "api".into(),
            files: vec![FileEntry {
                path: PathBuf::from("src/api.rs"),
                symbols: vec![SymbolEntry {
                    name: "validate".into(),
                    signature: "fn validate() -> Result<()>".into(),
                    kind: SymbolKind::Function,
                    line: 10,
                    character: 0,
                }],
                depends_on: vec![],
                depended_by: vec![],
            }],
        };
        let ci = tmp.path().join(".codeindex");
        write_index(&idx, &ci).unwrap();
        write_package(&pkg_auth, &ci).unwrap();
        write_package(&pkg_api, &ci).unwrap();
    }

    #[test]
    fn find_definition_returns_all_matches_sorted() {
        let tmp = TempDir::new().unwrap();
        setup_codeindex_multi(&tmp);
        let ctx = single_ctx(&tmp);
        let result =
            handle_find_definition(
                &ctx,
                &serde_json::json!({"symbol": "validate", "compact": false}),
            )
                .unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("src/auth.rs"), "expected auth.rs in: {text}");
        assert!(text.contains("src/api.rs"), "expected api.rs in: {text}");
        // Sorted by package file path: api.rs before auth.rs alphabetically
        let auth_pos = text.find("auth.rs").unwrap();
        let api_pos = text.find("api.rs").unwrap();
        assert!(
            api_pos < auth_pos,
            "expected api.rs before auth.rs (sorted): {text}"
        );
    }

    #[test]
    fn find_references_index_fallback() {
        let tmp = TempDir::new().unwrap();
        setup_codeindex(&tmp);
        let ctx = single_ctx(&tmp);
        let result =
            handle_find_references(&ctx, &serde_json::json!({"symbol": "login"}))
                .unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("[index fallback]"),
            "expected '[index fallback]' label in: {text}"
        );
        assert!(text.contains("login"), "expected 'login' in: {text}");
    }

    #[test]
    fn get_dependents_returns_list() {
        let tmp = TempDir::new().unwrap();
        let mut g = crate::graph::DependencyGraph::new();
        g.add_dependency(
            &PathBuf::from("src/api.rs"),
            &PathBuf::from("src/auth.rs"),
        );
        crate::graph::persistence::save(
            &g,
            &tmp.path().join(".codeindex/graph.bin"),
        )
        .unwrap();
        let ctx = single_ctx(&tmp);
        let result =
            handle_get_dependents(
                &ctx,
                &serde_json::json!({"file": "src/auth.rs", "compact": false}),
            )
                .unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("api.rs"));
    }

    // --- Multi-repo tests ---

    #[test]
    fn get_context_multi_repo_returns_envelope() {
        let tmp_a = TempDir::new().unwrap();
        let tmp_b = TempDir::new().unwrap();
        setup_codeindex(&tmp_a);
        setup_codeindex(&tmp_b);

        let ctx = ToolContext {
            repos: vec![
                test_repo_ctx("repoA", tmp_a.path()),
                test_repo_ctx("repoB", tmp_b.path()),
            ],
            fs_tools: false,
            lock_store: None,
        };

        let result = handle_get_context(&ctx, &serde_json::json!({})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("repoA"), "expected repoA in: {text}");
        assert!(text.contains("repoB"), "expected repoB in: {text}");
        assert!(text.contains("\"repos\""), "expected repos envelope in: {text}");
    }

    #[test]
    fn get_context_multi_repo_filtered_by_repo() {
        let tmp_a = TempDir::new().unwrap();
        let tmp_b = TempDir::new().unwrap();
        setup_codeindex(&tmp_a);
        // tmp_b intentionally has no .codeindex

        let ctx = ToolContext {
            repos: vec![
                test_repo_ctx("repoA", tmp_a.path()),
                test_repo_ctx("repoB", tmp_b.path()),
            ],
            fs_tools: false,
            lock_store: None,
        };

        // With `repo: "repoA"` filter, returns single-repo format (no envelope)
        let result =
            handle_get_context(&ctx, &serde_json::json!({"repo": "repoA"})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("auth"), "expected auth package in: {text}");
        assert!(!text.contains("\"repos\""), "single-repo format should have no envelope");
    }

    #[test]
    fn find_definition_multi_repo_prefixes_output() {
        let tmp_a = TempDir::new().unwrap();
        let tmp_b = TempDir::new().unwrap();

        // Both repos define a `handle` symbol
        use crate::indexer::writer::{write_index, write_package};
        for (tmp, repo_name) in &[(&tmp_a, "repoA"), (&tmp_b, "repoB")] {
            let idx = RepoIndex {
                repo: (*repo_name).into(),
                generated_at: "2026-06-16T00:00:00Z".into(),
                packages: vec![PackageSummary {
                    name: "core".into(),
                    purpose: String::new(),
                    files: 1,
                    score: 0.9,
                }],
                hot_symbols: vec!["handle".into()],
            };
            let pkg = PackageDetail {
                name: "core".into(),
                files: vec![FileEntry {
                    path: PathBuf::from("src/core.rs"),
                    symbols: vec![SymbolEntry {
                        name: "handle".into(),
                        signature: "fn handle()".into(),
                        kind: SymbolKind::Function,
                        line: 5,
                        character: 0,
                    }],
                    depends_on: vec![],
                    depended_by: vec![],
                }],
            };
            write_index(&idx, &tmp.path().join(".codeindex")).unwrap();
            write_package(&pkg, &tmp.path().join(".codeindex")).unwrap();
        }

        let ctx = ToolContext {
            repos: vec![
                test_repo_ctx("repoA", tmp_a.path()),
                test_repo_ctx("repoB", tmp_b.path()),
            ],
            fs_tools: false,
            lock_store: None,
        };

        let result =
            handle_find_definition(&ctx, &serde_json::json!({"symbol": "handle"}))
                .unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("repoA/"), "expected repoA/ prefix in: {text}");
        assert!(text.contains("repoB/"), "expected repoB/ prefix in: {text}");
    }

    #[test]
    fn shortest_path_mcp_dispatch() {
        let root = PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/simple_rust"
        ));
        if !root.join(".codeindex/index.json").exists() {
            let mut indexer = crate::indexer::Indexer::new(&root);
            indexer.full_index().unwrap();
        }
        let ctx = ToolContext {
            repos: vec![test_repo_ctx("simple_rust", &root)],
            fs_tools: false,
            lock_store: None,
        };
        let result = handle_shortest_path(
            &ctx,
            &serde_json::json!({
                "from": "src/auth.rs",
                "to": "src/db.rs",
                "compact": false,
            }),
        )
        .unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("auth.rs"));
        assert!(text.contains("db.rs"));
    }

    #[test]
    fn hotspots_mcp_dispatch() {
        let root = PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/simple_rust"
        ));
        if !root.join(".codeindex/index.json").exists() {
            let mut indexer = crate::indexer::Indexer::new(&root);
            indexer.full_index().unwrap();
        }
        let ctx = ToolContext {
            repos: vec![test_repo_ctx("simple_rust", &root)],
            fs_tools: false,
            lock_store: None,
        };
        let result = handle_hotspots(&ctx, &serde_json::json!({"limit": 5, "compact": false})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("db.rs"));
        assert!(text.contains("dependents"));
    }

    #[test]
    fn get_context_compact_uses_short_keys() {
        let tmp = TempDir::new().unwrap();
        setup_codeindex(&tmp);
        let ctx = single_ctx(&tmp);
        let result = handle_get_context(&ctx, &serde_json::json!({})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"pk\""), "expected compact pk key in: {text}");
        assert!(text.contains("\"dict\""), "expected dict block in: {text}");
    }

    #[test]
    fn get_context_legacy_when_compact_false() {
        let tmp = TempDir::new().unwrap();
        setup_codeindex(&tmp);
        let ctx = single_ctx(&tmp);
        let result =
            handle_get_context(&ctx, &serde_json::json!({"compact": false})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"packages\""), "expected legacy packages key in: {text}");
        assert!(!text.contains("\"pk\""), "compact keys should be absent: {text}");
    }

    #[test]
    fn drill_package_compact_uses_path_refs() {
        let tmp = TempDir::new().unwrap();
        setup_codeindex(&tmp);
        let ctx = single_ctx(&tmp);
        let result =
            handle_drill_package(&ctx, &serde_json::json!({"name": "auth"})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"p\""), "expected path ref key in: {text}");
        assert!(text.contains("\"package\""), "expected package wrapper in: {text}");
    }

    #[test]
    fn find_definition_compact_uses_path_refs() {
        let tmp = TempDir::new().unwrap();
        setup_codeindex_multi(&tmp);
        let ctx = single_ctx(&tmp);
        let result =
            handle_find_definition(&ctx, &serde_json::json!({"symbol": "validate"}))
                .unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("p1:") || text.contains("p2:"), "expected dict path ref in: {text}");
        assert!(!text.contains("src/auth.rs"), "full path should be compressed: {text}");
    }

    #[test]
    fn get_dependents_resolves_dict_file_arg() {
        let tmp = TempDir::new().unwrap();
        let mut g = crate::graph::DependencyGraph::new();
        g.add_dependency(
            &PathBuf::from("src/api.rs"),
            &PathBuf::from("src/auth.rs"),
        );
        let ci = tmp.path().join(".codeindex");
        std::fs::create_dir_all(&ci).unwrap();
        crate::graph::persistence::save(&g, &ci.join("graph.bin")).unwrap();
        let packages = vec![
            PackageDetail {
                name: "auth".into(),
                files: vec![FileEntry {
                    path: PathBuf::from("src/auth.rs"),
                    symbols: vec![],
                    depends_on: vec![],
                    depended_by: vec![],
                }],
            },
            PackageDetail {
                name: "api".into(),
                files: vec![FileEntry {
                    path: PathBuf::from("src/api.rs"),
                    symbols: vec![],
                    depends_on: vec![],
                    depended_by: vec![],
                }],
            },
        ];
        let dict = build_dict_from_packages(&packages, 0);
        write_dict(&dict, &ci).unwrap();
        let ctx = single_ctx(&tmp);
        let auth_id = dict
            .paths
            .iter()
            .find(|(_, p)| p.as_str() == "src/auth.rs")
            .map(|(id, _)| id.clone())
            .unwrap();
        let result =
            handle_get_dependents(&ctx, &serde_json::json!({"file": auth_id})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains('p'),
            "expected compact path ref for dependent in: {text}"
        );
    }

    #[test]
    fn dict_rev_increments_on_build() {
        let packages = vec![PackageDetail {
            name: "auth".into(),
            files: vec![FileEntry {
                path: PathBuf::from("src/auth.rs"),
                symbols: vec![],
                depends_on: vec![],
                depended_by: vec![],
            }],
        }];
        let d1 = build_dict_from_packages(&packages, 0);
        assert_eq!(d1.rev, 1);
        let d2 = build_dict_from_packages(&packages, d1.rev);
        assert_eq!(d2.rev, 2);
    }

    #[test]
    fn docs_mcp_tools_smoke() {
        use std::fs;
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("docs")).unwrap();
        fs::create_dir_all(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src/auth.rs"), "pub fn login() {}\n").unwrap();
        fs::write(
            tmp.path().join("docs/design.md"),
            "## Auth Flow\n\n<!-- codebeacon: src/auth.rs -->\nJWT.\n",
        )
        .unwrap();
        crate::docs::reindex_docs(tmp.path(), &tmp.path().join("docs"), false).unwrap();
        crate::docs::mark_stale_for_code_path(tmp.path(), PathBuf::from("src/auth.rs").as_path())
            .unwrap();

        let mut ctx = single_ctx(&tmp);
        ctx.repos[0].docs_root = Some(tmp.path().join("docs"));

        let q = dispatch(
            &ctx,
            "query_docs",
            &serde_json::json!({"question": "auth jwt"}),
        )
        .unwrap();
        let qt = q["content"][0]["text"].as_str().unwrap();
        assert!(qt.contains("Auth"), "query_docs: {qt}");

        let r = dispatch(
            &ctx,
            "resolve_doc",
            &serde_json::json!({"reference": "docs/design.md::## Auth Flow"}),
        )
        .unwrap();
        let rt = r["content"][0]["text"].as_str().unwrap();
        assert!(rt.contains("JWT"), "resolve_doc: {rt}");

        let s = dispatch(&ctx, "docs_status", &serde_json::json!({})).unwrap();
        let st = s["content"][0]["text"].as_str().unwrap();
        assert!(st.contains("stale"), "docs_status: {st}");

        let u = dispatch(&ctx, "update_docs", &serde_json::json!({})).unwrap();
        let ut = u["content"][0]["text"].as_str().unwrap();
        assert!(ut.contains("Docs update brief"), "update_docs: {ut}");
    }

    #[test]
    fn docs_tools_error_when_disabled() {
        let tmp = TempDir::new().unwrap();
        let ctx = single_ctx(&tmp);
        let err = dispatch(
            &ctx,
            "query_docs",
            &serde_json::json!({"question": "x"}),
        );
        assert!(err.is_err());
    }
}
