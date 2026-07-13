//! MCP handlers for edit-intelligence tools.

use crate::compact::{
    compact_mode, encode_change_impact, encode_focus_response, encode_task_context,
};
use crate::intelligence::{
    api_surface, change_impact, focus_context, fragile_files, index_status,
    package_conventions, read_conventions, resolve_rel_path, similar_symbols, task_context,
    why_file,
};
use crate::intelligence::git::{git_blame_first_line, git_log_file};
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

pub fn handle_focus_context(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let file = args["file"].as_str().context("missing 'file'")?;
    let radius = args["radius"].as_u64().unwrap_or(0) as u32;
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);

    for repo in repos {
        let qctx = repo_query(repo)?;
        let rel = {
            let abs = repo.resolve_file(file);
            resolve_rel_path(&repo.root, &abs)
        };
        let r = if radius == 0 {
            repo.intelligence.focus_default_radius
        } else {
            radius
        };
        let out = focus_context(&qctx, &rel, r, &repo.intelligence)?;
        let text = compact_or_verbose(repo, args, &out, encode_focus_response)?;
        let prefix = if ctx.multi() && repo_filter.is_none() {
            format!("[repo: {}]\n", repo.name)
        } else {
            String::new()
        };
        return Ok(text_content(format!("{prefix}{text}")));
    }
    anyhow::bail!("no indexed repo for focus_context")
}

pub fn handle_task_context(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let question = args["question"].as_str().context("missing 'question'")?;
    let file = args["file"].as_str();
    let limit = args["limit"].as_u64().unwrap_or(10) as usize;
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);

    for repo in repos {
        let qctx = repo_query(repo)?;
        let compact = compact_mode(args, &repo.compact);
        repo.maybe_record_usage("task_context", question, compact);
        let proximity = file.map(|f| {
            let abs = repo.resolve_file(f);
            resolve_rel_path(&repo.root, &abs)
        });
        let out = task_context(
            &qctx,
            question,
            proximity.as_deref(),
            limit,
            &repo.intelligence,
        );
        let text = compact_or_verbose(repo, args, &out, encode_task_context)?;
        let prefix = if ctx.multi() && repo_filter.is_none() {
            format!("[repo: {}]\n", repo.name)
        } else {
            String::new()
        };
        return Ok(text_content(format!("{prefix}{text}")));
    }
    anyhow::bail!("no indexed repo for task_context")
}

pub fn handle_change_impact(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let symbol = args["symbol"].as_str().context("missing 'symbol'")?;
    let file = args["file"].as_str();
    let exact = args["exact"].as_bool().unwrap_or(true);
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);

    for repo in repos {
        let qctx = repo_query(repo)?;
        let file_rel = file.map(|f| {
            let abs = repo.resolve_file(f);
            resolve_rel_path(&repo.root, &abs)
        });
        let out = change_impact(
            &qctx,
            symbol,
            file_rel.as_deref(),
            exact,
            &repo.intelligence,
        )?;
        let text = compact_or_verbose(repo, args, &out, encode_change_impact)?;
        return Ok(text_content(text));
    }
    anyhow::bail!("no indexed repo for change_impact")
}

pub fn handle_index_status(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);

    for repo in repos {
        let out = index_status(&repo.root, &repo.intelligence)?;
        return json_response(&out);
    }
    anyhow::bail!("no repo for index_status")
}

pub fn handle_package_conventions(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let package = args["package"].as_str().context("missing 'package'")?;
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);

    for repo in repos {
        let qctx = repo_query(repo)?;
        let store = read_conventions(&repo.codeindex())?;
        let pkg = qctx
            .packages
            .get(package)
            .with_context(|| format!("package '{package}' not found"))?;
        let out = package_conventions(pkg, &store);
        return json_response(&out);
    }
    anyhow::bail!("no indexed repo for package_conventions")
}

pub fn handle_similar_symbols(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let symbol = args["symbol"].as_str().context("missing 'symbol'")?;
    let file = args["file"].as_str();
    let limit = args["limit"].as_u64().unwrap_or(5) as usize;
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);

    for repo in repos {
        let qctx = repo_query(repo)?;
        let file_rel = file.map(|f| {
            let abs = repo.resolve_file(f);
            resolve_rel_path(&repo.root, &abs)
        });
        let out = similar_symbols(&qctx, symbol, file_rel.as_deref(), limit);
        return json_response(&out);
    }
    anyhow::bail!("no indexed repo for similar_symbols")
}

pub fn handle_api_surface(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let package = args["package"].as_str().context("missing 'package'")?;
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);

    for repo in repos {
        let qctx = repo_query(repo)?;
        let pkg = qctx
            .packages
            .get(package)
            .with_context(|| format!("package '{package}' not found"))?;
        let out = api_surface(pkg);
        return json_response(&out);
    }
    anyhow::bail!("no indexed repo for api_surface")
}

pub fn handle_why_file(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let file = args["file"].as_str().context("missing 'file'")?;
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);

    for repo in repos {
        let qctx = repo_query(repo)?;
        let rel = {
            let abs = repo.resolve_file(file);
            resolve_rel_path(&repo.root, &abs)
        };
        let (recent_commits, blame_first_line) = if repo.intelligence.git_context_enabled {
            (
                git_log_file(&repo.root, &rel, 3),
                git_blame_first_line(&repo.root, &rel),
            )
        } else {
            (vec![], None)
        };
        let out = why_file(&qctx, &rel, recent_commits, blame_first_line);
        return json_response(&out);
    }
    anyhow::bail!("no indexed repo for why_file")
}

pub fn handle_fragile_files(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let limit = args["limit"].as_u64().unwrap_or(10) as usize;
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);

    for repo in repos {
        let qctx = repo_query(repo)?;
        let out = fragile_files(
            &qctx,
            limit,
            repo.intelligence.git_context_enabled,
        );
        return json_response(&out);
    }
    anyhow::bail!("no indexed repo for fragile_files")
}
