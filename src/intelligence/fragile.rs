use crate::graph::path::hotspots as graph_hotspots;
use crate::intelligence::git::git_churn;
use crate::query::RepoQueryCtx;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct FragileFile {
    pub path: String,
    pub dependents: usize,
    pub churn_30d: u32,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct FragileFilesResponse {
    pub files: Vec<FragileFile>,
}

pub fn fragile_files(
    ctx: &RepoQueryCtx,
    limit: usize,
    git_enabled: bool,
) -> FragileFilesResponse {
    let hs = graph_hotspots(&ctx.graph, limit.max(20));
    let paths: Vec<String> = hs
        .iter()
        .map(|(p, _)| p.to_string_lossy().into_owned())
        .collect();

    let churn_map = if git_enabled {
        git_churn(&ctx.root, &paths, 50)
            .unwrap_or_default()
            .into_iter()
            .collect::<std::collections::HashMap<_, _>>()
    } else {
        std::collections::HashMap::new()
    };

    let mut files: Vec<FragileFile> = hs
        .into_iter()
        .map(|(path, dependents)| {
            let path_str = path.to_string_lossy().into_owned();
            let churn_30d = churn_map.get(&path_str).copied().unwrap_or(0);
            let score = dependents as f32 * (1.0 + churn_30d as f32 * 0.1);
            FragileFile {
                path: path_str,
                dependents,
                churn_30d,
                score,
            }
        })
        .collect();

    files.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    files.truncate(limit);

    FragileFilesResponse { files }
}
