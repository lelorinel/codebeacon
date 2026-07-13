use crate::config_file::IntelligenceConfig;
use crate::indexer::package::package_name_for;
use crate::query::{MatchKind, QueryMatch, RepoQueryCtx};
use crate::types::PackageDetail;
use serde::Serialize;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct TaskPackageSummary {
    pub name: String,
    pub file_count: usize,
    pub top_symbols: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskContextResponse {
    pub question: String,
    pub matches: Vec<QueryMatch>,
    pub package_drill: Option<TaskPackageSummary>,
}

pub fn task_context(
    ctx: &RepoQueryCtx,
    question: &str,
    proximity_file: Option<&str>,
    limit: usize,
    _cfg: &IntelligenceConfig,
) -> TaskContextResponse {
    let active = proximity_file.map(|p| {
        ctx.graph
            .resolve_node(p)
            .map(|pb| vec![pb])
            .unwrap_or_else(|| vec![PathBuf::from(p)])
    });
    let matches = ctx.query_with_active(question, limit, active.as_deref());

    let package_drill = matches
        .iter()
        .find(|m| m.kind == MatchKind::Package)
        .and_then(|m| drill_summary(ctx, &m.name));

    TaskContextResponse {
        question: question.to_string(),
        matches,
        package_drill,
    }
}

fn drill_summary(ctx: &RepoQueryCtx, package_name: &str) -> Option<TaskPackageSummary> {
    let pkg: &PackageDetail = ctx.packages.get(package_name)?;
    let mut symbols: Vec<String> = pkg
        .files
        .iter()
        .flat_map(|f| f.symbols.iter().map(|s| s.name.clone()))
        .collect();
    symbols.sort();
    symbols.dedup();
    symbols.truncate(8);
    Some(TaskPackageSummary {
        name: package_name.to_string(),
        file_count: pkg.files.len(),
        top_symbols: symbols,
    })
}

pub fn package_name_for_path(path: &str) -> String {
    package_name_for(Path::new(path))
}
