//! MCP handlers for loop context coordinator tools.

use crate::compact::{compact_mode, encode_loop_tick};
use crate::loop_coord::{
    loop_begin_with_tick, loop_end, loop_record, loop_tick, read_session, resolve_active_files,
    write_session, LoopSession,
};
use crate::mcp::protocol::text_content;
use crate::mcp::tools::{RepoCtx, ToolContext};
use crate::query::RepoQueryCtx;
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;

fn repo_query(repo: &RepoCtx) -> Result<RepoQueryCtx> {
    RepoQueryCtx::load(&repo.root)
}

fn json_response<T: Serialize>(v: &T) -> Result<Value> {
    Ok(text_content(serde_json::to_string_pretty(v)?))
}

fn compact_or_verbose<T: Serialize, U: Serialize>(
    repo: &RepoCtx,
    args: &Value,
    verbose: &T,
    compact_fn: impl FnOnce(&T, &mut crate::compact::DictSession) -> U,
) -> Result<String> {
    if compact_mode(args, &repo.compact) {
        repo.ensure_dict_session();
        let mut session = repo.dict_session_mut();
        let body = compact_fn(verbose, &mut session);
        Ok(serde_json::to_string_pretty(&body)?)
    } else {
        Ok(serde_json::to_string_pretty(verbose)?)
    }
}

fn load_session(repo: &RepoCtx, session_id: &str) -> Result<LoopSession> {
    read_session(&repo.codeindex(), session_id)
}

pub fn handle_loop_begin(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let goal = args["goal"].as_str().context("missing 'goal'")?;
    let file = args["file"].as_str();
    let files: Option<Vec<String>> = args["files"].as_array().map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    });
    let include_tick = args["tick"].as_bool().unwrap_or(true);
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);

    for repo in repos {
        let qctx = repo_query(repo)?;
        let active = {
            let mut paths = resolve_active_files(&repo.root, file, files);
            if let Some(f) = file {
                let abs = repo.resolve_file(f);
                let rel = crate::intelligence::resolve_rel_path(&repo.root, &abs);
                if !paths.contains(&rel) {
                    paths.insert(0, rel);
                }
            }
            paths
        };
        let (_session, resp) = loop_begin_with_tick(
            &repo.root,
            &repo.codeindex(),
            goal,
            active,
            &repo.loop_config,
            &repo.intelligence,
            &qctx,
            include_tick,
        )?;
        return json_response(&resp);
    }
    anyhow::bail!("no indexed repo for loop_begin")
}

pub fn handle_loop_tick(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let session_id = args["session_id"].as_str().context("missing 'session_id'")?;
    let file = args["file"].as_str();
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);

    for repo in repos {
        let qctx = repo_query(repo)?;
        let mut session = load_session(repo, session_id)?;
        let file_rel = file.map(|f| {
            let abs = repo.resolve_file(f);
            crate::intelligence::resolve_rel_path(&repo.root, &abs)
        });
        let out = loop_tick(
            &repo.root,
            &repo.codeindex(),
            &mut session,
            &repo.loop_config,
            &repo.intelligence,
            &qctx,
            file_rel.as_deref(),
        )?;
        if repo.loop_config.persist_sessions {
            write_session(&repo.codeindex(), &session)?;
        }
        let text = compact_or_verbose(repo, args, &out, encode_loop_tick)?;
        return Ok(text_content(text));
    }
    anyhow::bail!("no indexed repo for loop_tick")
}

pub fn handle_loop_record(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let session_id = args["session_id"].as_str().context("missing 'session_id'")?;
    let symbol = args["symbol"].as_str();
    let files: Vec<String> = args["files"]
        .as_array()
        .context("missing 'files' array")?
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);

    for repo in repos {
        let qctx = repo_query(repo)?;
        let mut session = load_session(repo, session_id)?;
        let out = loop_record(
            &repo.root,
            &repo.codeindex(),
            &mut session,
            &repo.loop_config,
            &repo.intelligence,
            &qctx,
            &files,
            symbol,
        )?;
        return json_response(&out);
    }
    anyhow::bail!("no indexed repo for loop_record")
}

pub fn handle_loop_end(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let session_id = args["session_id"].as_str().context("missing 'session_id'")?;
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);

    for repo in repos {
        let mut session = load_session(repo, session_id)?;
        let out = loop_end(
            &repo.root,
            &repo.codeindex(),
            &mut session,
            &repo.loop_config,
            &repo.intelligence,
        )?;
        return json_response(&out);
    }
    anyhow::bail!("no indexed repo for loop_end")
}
