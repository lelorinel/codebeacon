use crate::compact::dict::DictSession;
use crate::compact::schema::{
    CompactChangeImpact, CompactFileEntry, CompactFocusNeighbor, CompactFocusResponse,
    CompactLoopSignals, CompactLoopTick, CompactPackageDetail, CompactPackageSummary,
    CompactQueryMatch, CompactRepoIndex, CompactSymbolEntry, CompactSymbolRef,
    CompactTaskContext, CompactTaskDrill,
};
use crate::intelligence::{
    ChangeImpactResponse, FocusResponse, TaskContextResponse,
};
use crate::loop_coord::tick::LoopTickBundle;
use crate::query::{MatchKind, QueryMatch};
use crate::types::{PackageDetail, RepoIndex, SymbolKind};
use serde_json::{json, Value};

pub fn kind_short(kind: &SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "fn",
        SymbolKind::Struct => "st",
        SymbolKind::Enum => "en",
        SymbolKind::Trait => "tr",
        SymbolKind::Module => "md",
        SymbolKind::Variable => "vr",
        SymbolKind::Other => "ot",
    }
}

pub fn path_ref_for(session: &mut DictSession, path: &str) -> String {
    session.path_id(path)
}

pub fn encode_repo_index(index: &RepoIndex, _session: &mut DictSession) -> CompactRepoIndex {
    CompactRepoIndex {
        repo: index.repo.clone(),
        generated_at: index.generated_at.clone(),
        pk: index
            .packages
            .iter()
            .map(|p| CompactPackageSummary {
                n: p.name.clone(),
                p: p.purpose.clone(),
                f: p.files,
                s: p.score,
            })
            .collect(),
        hs: index.hot_symbols.clone(),
    }
}

pub fn encode_package(pkg: &PackageDetail, session: &mut DictSession) -> CompactPackageDetail {
    CompactPackageDetail {
        n: pkg.name.clone(),
        f: pkg
            .files
            .iter()
            .map(|file| {
                let path_str = file.path.to_string_lossy();
                let pid = session.path_id(&path_str);
                CompactFileEntry {
                    p: pid,
                    sy: file
                        .symbols
                        .iter()
                        .map(|sym| CompactSymbolEntry {
                            n: sym.name.clone(),
                            g: sym.signature.clone(),
                            k: Some(kind_short(&sym.kind).to_string()),
                            l: sym.line,
                            c: sym.character,
                        })
                        .collect(),
                    d: file.depends_on.clone(),
                    b: file.depended_by.clone(),
                }
            })
            .collect(),
    }
}

pub fn encode_index_response(
    index: &RepoIndex,
    session: &mut DictSession,
    base_dict: Option<&crate::compact::dict::PersistentDict>,
) -> Value {
    let compact = encode_repo_index(index, session);
    let mut out = json!({
        "rev": session.rev,
        "dict": session.to_persistent(),
        "index": compact,
    });
    if let Some(base) = base_dict {
        if let Some(delta) = session.delta_since(base) {
            out["dict_delta"] = json!(delta);
        }
    }
    out
}

pub fn encode_package_response(
    pkg: &PackageDetail,
    session: &mut DictSession,
    base_dict: Option<&crate::compact::dict::PersistentDict>,
) -> Value {
    let compact = encode_package(pkg, session);
    let mut out = json!({
        "rev": session.rev,
        "dict": session.to_persistent(),
        "package": compact,
    });
    if let Some(base) = base_dict {
        if let Some(delta) = session.delta_since(base) {
            out["dict_delta"] = json!(delta);
        }
    }
    out
}

pub fn encode_query_matches(matches: &[QueryMatch], session: &mut DictSession) -> Vec<CompactQueryMatch> {
    matches
        .iter()
        .map(|m| {
            let (name, detail) = match m.kind {
                MatchKind::File => {
                    let id = session.path_id(&m.name);
                    (id, m.detail.clone())
                }
                _ => (m.name.clone(), m.detail.clone()),
            };
            CompactQueryMatch {
                k: match_kind_char(&m.kind),
                n: name,
                d: detail,
                s: m.score,
                h: m.hint.clone(),
            }
        })
        .collect()
}

fn match_kind_char(kind: &MatchKind) -> char {
    match kind {
        MatchKind::Package => 'P',
        MatchKind::File => 'F',
        MatchKind::Symbol => 'S',
        MatchKind::HotSymbol => 'H',
    }
}

pub fn encode_focus_response(
    focus: &FocusResponse,
    session: &mut DictSession,
) -> CompactFocusResponse {
    let anc = session.path_id(&focus.anchor);
    let nbr = focus
        .neighbors
        .iter()
        .map(|n| {
            let pid = session.path_id(&n.path);
            CompactFocusNeighbor {
                p: pid,
                sc: n.score,
                sy: n
                    .symbols
                    .iter()
                    .map(|sym| CompactSymbolEntry {
                        n: sym.name.clone(),
                        g: sym.signature.clone(),
                        k: Some(kind_short(&sym.kind).to_string()),
                        l: sym.line,
                        c: sym.character,
                    })
                    .collect(),
                d: n.depends_on.clone(),
                b: n.depended_by.clone(),
            }
        })
        .collect();
    CompactFocusResponse {
        anc,
        pkg: focus.package.clone(),
        nbr,
        hints: focus.hints.clone(),
    }
}

pub fn encode_change_impact(
    impact: &ChangeImpactResponse,
    session: &mut DictSession,
) -> CompactChangeImpact {
    let def = impact.definition.as_ref().map(|d| CompactSymbolRef {
        p: session.path_id(&d.file),
        l: d.line,
        g: d.signature.clone(),
    });
    let rf = impact
        .references
        .iter()
        .map(|r| CompactSymbolRef {
            p: session.path_id(&r.file),
            l: r.line,
            g: r.signature.clone(),
        })
        .collect();
    CompactChangeImpact {
        sym: impact.symbol.clone(),
        def,
        rf,
        rc: impact.ref_count,
        df: impact.dependent_files.clone(),
        risk: impact.risk.clone(),
    }
}

pub fn encode_task_context(
    task: &TaskContextResponse,
    session: &mut DictSession,
) -> CompactTaskContext {
    let m = encode_query_matches(&task.matches, session);
    let drill = task.package_drill.as_ref().map(|d| CompactTaskDrill {
        n: d.name.clone(),
        f: d.file_count,
        sy: d.top_symbols.clone(),
    });
    CompactTaskContext {
        q: task.question.clone(),
        m,
        drill,
    }
}

pub fn encode_loop_tick(
    bundle: &LoopTickBundle,
    session: &mut DictSession,
) -> CompactLoopTick {
    let fc = bundle
        .focus
        .as_ref()
        .map(|f| encode_focus_response(f, session));
    let tk = bundle
        .task
        .as_ref()
        .map(|t| encode_task_context(t, session));
    let st = serde_json::to_value(&bundle.status).unwrap_or(serde_json::json!({}));
    CompactLoopTick {
        sid: bundle.session_id.clone(),
        it: bundle.iteration,
        g: bundle.goal.clone(),
        st,
        fc,
        tk,
        sig: CompactLoopSignals {
            sc: bundle.signals.stale_count,
            rr: bundle.signals.reindex_recommended,
            ri: bundle.signals.reindexed,
            sp: bundle.signals.should_pause,
            ss: bundle.signals.should_stop,
            h: bundle.signals.hints.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PackageSummary;

    #[test]
    fn encode_index_uses_short_keys() {
        let index = RepoIndex {
            repo: "test".into(),
            generated_at: "now".into(),
            packages: vec![PackageSummary {
                name: "auth".into(),
                purpose: String::new(),
                files: 2,
                score: 0.9,
            }],
            hot_symbols: vec!["login".into()],
        };
        let mut session = DictSession::default();
        let compact = encode_repo_index(&index, &mut session);
        assert_eq!(compact.pk[0].n, "auth");
        assert_eq!(compact.hs, vec!["login"]);
    }
}
