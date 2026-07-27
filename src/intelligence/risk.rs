//! Configurable logistic risk scoring over file/symbol features.

use crate::config_file::RiskConfig;
use crate::graph::path::hotspots as graph_hotspots;
use crate::intelligence::callgraph::call_fan_in;
use crate::intelligence::git::{git_bugfix_ratio, git_churn};
use crate::intelligence::testgaps::test_gaps;
use crate::query::RepoQueryCtx;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct RiskFeatures {
    pub dependents: f32,
    pub call_fan_in: f32,
    pub churn_30d: f32,
    pub bugfix_ratio: f32,
    pub complexity: f32,
    pub test_gap: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RiskPrediction {
    pub target: String,
    pub kind: String,
    pub score: f32,
    pub tier: String,
    pub features: RiskFeatures,
}

#[derive(Debug, Clone, Serialize)]
pub struct PredictRiskResponse {
    pub predictions: Vec<RiskPrediction>,
}

pub fn predict_risk(
    ctx: &RepoQueryCtx,
    cfg: &RiskConfig,
    file: Option<&str>,
    symbol: Option<&str>,
    limit: usize,
) -> PredictRiskResponse {
    if !cfg.enabled {
        return PredictRiskResponse {
            predictions: vec![],
        };
    }

    let mut predictions = Vec::new();

    if let Some(sym) = symbol {
        predictions.push(score_symbol(ctx, cfg, sym, file));
    } else if let Some(f) = file {
        predictions.push(score_file(ctx, cfg, f));
    } else {
        let hs = graph_hotspots(&ctx.graph, limit.max(10));
        for (path, _) in hs.into_iter().take(limit) {
            let s = path.to_string_lossy().into_owned();
            predictions.push(score_file(ctx, cfg, &s));
        }
    }

    predictions.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    predictions.truncate(limit);

    PredictRiskResponse { predictions }
}

fn score_file(ctx: &RepoQueryCtx, cfg: &RiskConfig, file: &str) -> RiskPrediction {
    let path = PathBuf::from(file);
    let dependents = ctx.graph.reverse_neighbors(&path).len() as f32;
    let churn = git_churn(&ctx.root, &[file.to_string()], 1)
        .ok()
        .and_then(|v| v.first().map(|(_, n)| *n as f32))
        .unwrap_or(0.0);
    let bugfix = git_bugfix_ratio(&ctx.root, file).unwrap_or(0.0);
    let complexity = file_complexity(ctx, file);
    let gap = file_has_test_gap(ctx, file);

    let features = RiskFeatures {
        dependents,
        call_fan_in: 0.0,
        churn_30d: churn,
        bugfix_ratio: bugfix,
        complexity,
        test_gap: if gap { 1.0 } else { 0.0 },
    };
    let score = logistic(cfg, &features);
    RiskPrediction {
        target: file.to_string(),
        kind: "file".into(),
        score,
        tier: tier(score),
        features,
    }
}

fn score_symbol(
    ctx: &RepoQueryCtx,
    cfg: &RiskConfig,
    symbol: &str,
    file_hint: Option<&str>,
) -> RiskPrediction {
    let fan = call_fan_in(ctx, symbol) as f32;
    let file = file_hint
        .map(|s| s.to_string())
        .or_else(|| {
            ctx.resolve_to_file(symbol)
                .map(|p| p.to_string_lossy().into_owned())
        })
        .unwrap_or_default();
    let dependents = if file.is_empty() {
        0.0
    } else {
        ctx.graph
            .reverse_neighbors(&PathBuf::from(&file))
            .len() as f32
    };
    let churn = if file.is_empty() {
        0.0
    } else {
        git_churn(&ctx.root, &[file.clone()], 1)
            .ok()
            .and_then(|v| v.first().map(|(_, n)| *n as f32))
            .unwrap_or(0.0)
    };
    let bugfix = if file.is_empty() {
        0.0
    } else {
        git_bugfix_ratio(&ctx.root, &file).unwrap_or(0.0)
    };
    let gaps = test_gaps(ctx, None, file_hint, 500);
    let gap = gaps.gaps.iter().any(|g| g.symbol == symbol);

    let features = RiskFeatures {
        dependents,
        call_fan_in: fan,
        churn_30d: churn,
        bugfix_ratio: bugfix,
        complexity: 1.0,
        test_gap: if gap { 1.0 } else { 0.0 },
    };
    let score = logistic(cfg, &features);
    RiskPrediction {
        target: symbol.to_string(),
        kind: "symbol".into(),
        score,
        tier: tier(score),
        features,
    }
}

fn logistic(cfg: &RiskConfig, f: &RiskFeatures) -> f32 {
    let z = cfg.bias
        + cfg.w_dependents * f.dependents
        + cfg.w_fan_in * f.call_fan_in
        + cfg.w_churn * f.churn_30d
        + cfg.w_bugfix * f.bugfix_ratio
        + cfg.w_complexity * f.complexity
        + cfg.w_test_gap * f.test_gap;
    1.0 / (1.0 + (-z).exp())
}

fn tier(score: f32) -> String {
    if score >= 0.7 {
        "high".into()
    } else if score >= 0.4 {
        "medium".into()
    } else {
        "low".into()
    }
}

fn file_complexity(ctx: &RepoQueryCtx, file: &str) -> f32 {
    for pkg in ctx.packages.values() {
        for f in &pkg.files {
            let p = f.path.to_string_lossy();
            if p == file || p.ends_with(file) {
                return (f.symbols.len() as f32).ln_1p();
            }
        }
    }
    0.0
}

fn file_has_test_gap(ctx: &RepoQueryCtx, file: &str) -> bool {
    let gaps = test_gaps(ctx, None, Some(file), 1);
    !gaps.gaps.is_empty()
}

/// Rewrite fragile_files using risk scores when risk config enabled.
pub fn fragile_files_scored(
    ctx: &RepoQueryCtx,
    cfg: &RiskConfig,
    limit: usize,
    git_enabled: bool,
) -> crate::intelligence::FragileFilesResponse {
    if !cfg.enabled {
        return crate::intelligence::fragile::fragile_files(ctx, limit, git_enabled);
    }
    let pred = predict_risk(ctx, cfg, None, None, limit.max(20));
    let paths: Vec<String> = pred.predictions.iter().map(|p| p.target.clone()).collect();
    let churn_map: HashMap<String, u32> = if git_enabled {
        git_churn(&ctx.root, &paths, 50)
            .unwrap_or_default()
            .into_iter()
            .collect()
    } else {
        HashMap::new()
    };

    let mut files: Vec<crate::intelligence::FragileFile> = pred
        .predictions
        .into_iter()
        .map(|p| {
            let path = PathBuf::from(&p.target);
            let dependents = ctx.graph.reverse_neighbors(&path).len();
            crate::intelligence::FragileFile {
                path: p.target.clone(),
                dependents,
                churn_30d: churn_map.get(&p.target).copied().unwrap_or(0),
                score: p.score,
            }
        })
        .collect();
    files.truncate(limit);
    crate::intelligence::FragileFilesResponse { files }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logistic_increases_with_features() {
        let cfg = RiskConfig::default();
        let low = RiskFeatures {
            dependents: 0.0,
            call_fan_in: 0.0,
            churn_30d: 0.0,
            bugfix_ratio: 0.0,
            complexity: 0.0,
            test_gap: 0.0,
        };
        let high = RiskFeatures {
            dependents: 20.0,
            call_fan_in: 15.0,
            churn_30d: 10.0,
            bugfix_ratio: 1.0,
            complexity: 5.0,
            test_gap: 1.0,
        };
        assert!(logistic(&cfg, &high) > logistic(&cfg, &low));
    }
}
