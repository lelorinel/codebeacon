//! MCP handlers for sidecar documentation tools.

use crate::docs::{
    build_update_brief, load_docs_index, parse_reference, query_docs, resolve, DocsIndex,
};
use crate::mcp::protocol::text_content;
use crate::mcp::tools::{RepoCtx, ToolContext};
use anyhow::{bail, Context, Result};
use serde_json::Value;

fn require_docs(repo: &RepoCtx) -> Result<()> {
    if repo.docs_root.is_none() {
        bail!(
            "docs not enabled; pass --docs or set [docs] path in .codeindex.toml, \
             then restart serve / run init"
        );
    }
    Ok(())
}

fn load_index(repo: &RepoCtx) -> Result<DocsIndex> {
    require_docs(repo)?;
    load_docs_index(&repo.codeindex())?
        .context("no docs index — call init_workspace or `codebeacon init --docs …`")
}

pub fn handle_query_docs(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let question = args["question"].as_str().context("missing 'question'")?;
    let limit = args["limit"].as_u64().unwrap_or(10) as usize;
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);
    let mut lines = Vec::new();
    for repo in repos {
        let idx = load_index(repo)?;
        let hits = query_docs(&idx, question, limit);
        if hits.is_empty() {
            lines.push(format!("[{}] (no matches)", repo.name));
            continue;
        }
        for h in hits {
            let prefix = if ctx.multi() {
                format!("{}: ", repo.name)
            } else {
                String::new()
            };
            lines.push(format!(
                "{}{:.2}  {}{}\n    {}",
                prefix,
                h.score,
                h.id,
                if h.stale { " [stale]" } else { "" },
                h.snippet
            ));
        }
    }
    Ok(text_content(lines.join("\n")))
}

pub fn handle_resolve_doc(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let reference = args["reference"]
        .as_str()
        .context("missing 'reference' (e.g. docs/a.md::## Auth)")?;
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);
    let repo = repos
        .first()
        .copied()
        .context("no repo in workspace")?;
    require_docs(repo)?;
    let r = parse_reference(reference);
    match resolve(&repo.root, &r) {
        Ok(slice) => Ok(text_content(format!(
            "--- {} (lines {}-{}) ---\n{}",
            slice.label, slice.start_line, slice.end_line, slice.content
        ))),
        Err(e) => bail!("{e}"),
    }
}

pub fn handle_docs_status(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);
    let mut out = Vec::new();
    for repo in repos {
        let idx = load_index(repo)?;
        let stale: Vec<_> = idx.sections.iter().filter(|s| s.stale).map(|s| &s.id).collect();
        let broken: Vec<String> = idx
            .sections
            .iter()
            .flat_map(|s| {
                s.links
                    .iter()
                    .filter(|l| l.broken)
                    .map(move |l| format!("{} -> {}", s.id, l.target))
            })
            .collect();
        out.push(serde_json::json!({
            "repo": repo.name,
            "docs_root": idx.docs_root,
            "files": idx.files.len(),
            "sections": idx.sections.len(),
            "stale": stale,
            "broken_links": broken,
        }));
    }
    Ok(text_content(serde_json::to_string_pretty(&out)?))
}

pub fn handle_update_docs(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let section = args["section"].as_str();
    let repo_filter = args["repo"].as_str();
    let repos = ctx.repos_for(repo_filter);
    let repo = repos
        .first()
        .copied()
        .context("no repo in workspace")?;
    let idx = load_index(repo)?;
    let brief = build_update_brief(&repo.root, &idx, section);
    Ok(text_content(brief))
}
