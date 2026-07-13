use crate::config_file::IntelligenceConfig;
use crate::graph::path::hotspots as graph_hotspots;
use crate::graph::DependencyGraph;
use crate::query::RepoQueryCtx;
use crate::types::PackageDetail;
use anyhow::Result;
use serde::Serialize;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct SymbolRef {
    pub file: String,
    pub line: u32,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangeImpactResponse {
    pub symbol: String,
    pub definition: Option<SymbolRef>,
    pub references: Vec<SymbolRef>,
    pub ref_count: usize,
    pub dependent_files: Vec<String>,
    pub risk: String,
}

pub fn change_impact(
    ctx: &RepoQueryCtx,
    symbol: &str,
    file_filter: Option<&str>,
    exact: bool,
    cfg: &IntelligenceConfig,
) -> Result<ChangeImpactResponse> {
    let packages = load_all_packages(ctx)?;
    let mut definitions = Vec::new();
    let mut references = Vec::new();

    for pkg in &packages {
        for file in &pkg.files {
            if let Some(f) = file_filter {
                let path_str = file.path.to_string_lossy();
                if path_str != f && !path_str.ends_with(f) {
                    continue;
                }
            }
            for sym in &file.symbols {
                let matches = if exact {
                    sym.name == symbol
                } else {
                    sym.name == symbol || sym.name.contains(symbol)
                };
                if !matches {
                    continue;
                }
                let entry = SymbolRef {
                    file: file.path.to_string_lossy().into_owned(),
                    line: sym.line,
                    signature: sym.signature.clone(),
                };
                if sym.name == symbol {
                    if definitions.is_empty() || exact {
                        definitions.push(entry);
                    }
                } else {
                    references.push(entry);
                }
            }
        }
    }

    // Index fallback: name match for references when exact symbol search
    if exact {
        for pkg in &packages {
            for file in &pkg.files {
                if let Some(f) = file_filter {
                    let path_str = file.path.to_string_lossy();
                    if path_str != f && !path_str.ends_with(f) {
                        continue;
                    }
                }
                for sym in &file.symbols {
                    if sym.name == symbol {
                        let path_str = file.path.to_string_lossy().into_owned();
                        let is_def = definitions.iter().any(|d| d.file == path_str && d.line == sym.line);
                        if !is_def {
                            references.push(SymbolRef {
                                file: path_str,
                                line: sym.line,
                                signature: sym.signature.clone(),
                            });
                        }
                    }
                }
            }
        }
    }

    let definition = definitions.into_iter().next();
    let ref_count = references.len();

    let dependent_files = definition
        .as_ref()
        .map(|d| dependent_files_for(&ctx.graph, &PathBuf::from(&d.file)))
        .unwrap_or_default();

    let risk = assess_risk(
        ref_count,
        &dependent_files,
        &ctx.graph,
        cfg.change_impact_high_ref_threshold,
    );

    Ok(ChangeImpactResponse {
        symbol: symbol.to_string(),
        definition,
        references,
        ref_count,
        dependent_files,
        risk,
    })
}

fn load_all_packages(ctx: &RepoQueryCtx) -> Result<Vec<PackageDetail>> {
    Ok(ctx.packages.values().cloned().collect())
}

fn dependent_files_for(graph: &DependencyGraph, file: &PathBuf) -> Vec<String> {
    let mut out = HashSet::new();
    let direct = graph.reverse_neighbors(file);
    for d in direct {
        out.insert(d.to_string_lossy().into_owned());
        for d2 in graph.reverse_neighbors(&d) {
            out.insert(d2.to_string_lossy().into_owned());
        }
    }
    let mut v: Vec<_> = out.into_iter().collect();
    v.sort();
    v
}

fn assess_risk(
    ref_count: usize,
    dependent_files: &[String],
    graph: &DependencyGraph,
    threshold: u32,
) -> String {
    if ref_count as u32 > threshold {
        return "high".into();
    }
    let hs: HashSet<String> = graph_hotspots(graph, 10)
        .into_iter()
        .map(|(p, _)| p.to_string_lossy().into_owned())
        .collect();
    if dependent_files.iter().any(|f| hs.contains(f)) {
        return "high".into();
    }
    if ref_count > 3 || !dependent_files.is_empty() {
        "medium".into()
    } else {
        "low".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::DependencyGraph;
    use std::path::PathBuf;

    #[test]
    fn risk_high_when_refs_exceed_threshold() {
        let g = DependencyGraph::new();
        assert_eq!(assess_risk(11, &[], &g, 10), "high");
    }

    #[test]
    fn risk_low_when_no_refs_or_dependents() {
        let g = DependencyGraph::new();
        assert_eq!(assess_risk(0, &[], &g, 10), "low");
    }

    #[test]
    fn risk_medium_with_many_refs() {
        let g = DependencyGraph::new();
        assert_eq!(assess_risk(4, &[], &g, 10), "medium");
    }

    #[test]
    fn risk_high_when_dependent_is_hotspot() {
        let mut g = DependencyGraph::new();
        g.add_dependency(&PathBuf::from("a.rs"), &PathBuf::from("b.rs"));
        let deps = vec!["b.rs".into()];
        assert_eq!(assess_risk(1, &deps, &g, 10), "high");
    }
}
