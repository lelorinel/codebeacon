use crate::config_file::IntelligenceConfig;
use crate::graph::bfs::{min_score_for_radius, score_files_bidirectional};
use crate::indexer::package::package_name_for;
use crate::query::RepoQueryCtx;
use crate::types::{FileEntry, SymbolEntry};
use anyhow::{Context, Result};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct FocusNeighbor {
    pub path: String,
    pub score: f32,
    pub symbols: Vec<SymbolEntry>,
    pub depends_on: Vec<String>,
    pub depended_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FocusResponse {
    pub anchor: String,
    pub package: String,
    pub neighbors: Vec<FocusNeighbor>,
    pub hints: Vec<String>,
}

pub fn focus_context(
    ctx: &RepoQueryCtx,
    rel_path: &str,
    radius: u32,
    _cfg: &IntelligenceConfig,
) -> Result<FocusResponse> {
    let graph_path = ctx.graph.resolve_node(rel_path).with_context(|| {
        format!("could not resolve '{rel_path}' in dependency graph")
    })?;

    let scores = score_files_bidirectional(&ctx.graph, &[graph_path.clone()]);
    let min_score = min_score_for_radius(radius);

    let anchor_entry = find_file_entry(ctx, rel_path)?;
    let package = package_name_for(Path::new(rel_path));
    let _ = &anchor_entry.depends_on;

    let mut neighbors: Vec<FocusNeighbor> = Vec::new();
    for (pkg_name, pkg) in &ctx.packages {
        for file in &pkg.files {
            let path_str = file.path.to_string_lossy().into_owned();
            let score = scores
                .get(&file.path)
                .copied()
                .unwrap_or(0.1);
            if score < min_score && path_str != rel_path {
                continue;
            }
            neighbors.push(FocusNeighbor {
                path: path_str,
                score,
                symbols: file.symbols.clone(),
                depends_on: file.depends_on.clone(),
                depended_by: file.depended_by.clone(),
            });
            let _ = pkg_name;
        }
    }

    neighbors.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });

    let hints = vec![
        format!("drill_package name={package}"),
        format!("get_dependents file={rel_path}"),
    ];

    Ok(FocusResponse {
        anchor: rel_path.to_string(),
        package,
        neighbors,
        hints,
    })
}

fn find_file_entry<'a>(ctx: &'a RepoQueryCtx, rel_path: &str) -> Result<&'a FileEntry> {
    for pkg in ctx.packages.values() {
        for file in &pkg.files {
            if file.path.to_string_lossy() == rel_path {
                return Ok(file);
            }
        }
    }
    anyhow::bail!("file '{rel_path}' not found in index")
}

pub fn resolve_rel_path(root: &Path, abs_or_rel: &Path) -> String {
    abs_or_rel
        .strip_prefix(root)
        .unwrap_or(abs_or_rel)
        .to_string_lossy()
        .replace('\\', "/")
}
