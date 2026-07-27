//! Natural-language → code navigation (BM25 + docs + graph paths; no embeddings).

use crate::config::codeindex_dir;
use crate::docs::index::{load_docs_index, query_docs};
use crate::graph::path::shortest_path;
use crate::query::{MatchKind, RepoQueryCtx};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct NavigateResponse {
    pub question: String,
    pub anchors: Vec<NavigateAnchor>,
    pub suggested_read_order: Vec<String>,
    pub related_docs: Vec<String>,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NavigateAnchor {
    pub kind: String,
    pub name: String,
    pub detail: String,
    pub score: f32,
    pub hint: String,
}

pub fn navigate_to_feature(
    ctx: &RepoQueryCtx,
    question: &str,
    limit: usize,
) -> NavigateResponse {
    let matches = ctx.query(question, limit.max(5));
    let anchors: Vec<NavigateAnchor> = matches
        .iter()
        .map(|m| NavigateAnchor {
            kind: format!("{:?}", m.kind),
            name: m.name.clone(),
            detail: m.detail.clone(),
            score: m.score,
            hint: m.hint.clone(),
        })
        .collect();

    let mut files: Vec<String> = Vec::new();
    for m in &matches {
        match m.kind {
            MatchKind::File => files.push(m.name.clone()),
            MatchKind::Symbol | MatchKind::HotSymbol => {
                if let Some(f) = ctx.resolve_to_file(&m.name) {
                    let s = f.to_string_lossy().into_owned();
                    if !files.contains(&s) {
                        files.push(s);
                    }
                }
            }
            MatchKind::Package => {
                if let Some(pkg) = ctx.packages.get(&m.name) {
                    for f in pkg.files.iter().take(3) {
                        let s = f.path.to_string_lossy().into_owned();
                        if !files.contains(&s) {
                            files.push(s);
                        }
                    }
                }
            }
        }
    }

    let mut paths = Vec::new();
    if files.len() >= 2 {
        if let (Some(a), Some(b)) = (files.first(), files.get(1)) {
            if let (Some(fa), Some(fb)) = (
                ctx.resolve_to_file(a),
                ctx.resolve_to_file(b),
            ) {
                if let Some(p) = shortest_path(&ctx.graph, &fa, &fb) {
                    paths.push(
                        p.iter()
                            .map(|x| x.display().to_string())
                            .collect::<Vec<_>>()
                            .join(" → "),
                    );
                }
            }
        }
    }

    let related_docs = load_docs_index(&codeindex_dir(&ctx.root))
        .ok()
        .flatten()
        .map(|idx| {
            query_docs(&idx, question, 5)
                .into_iter()
                .map(|s| s.id.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let suggested_read_order = files.into_iter().take(limit).collect();

    NavigateResponse {
        question: question.to_string(),
        anchors,
        suggested_read_order,
        related_docs,
        paths,
    }
}
