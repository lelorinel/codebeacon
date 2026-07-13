use crate::compact::dict::DictSession;
use crate::compact::schema::{
    CompactFileEntry, CompactPackageDetail, CompactPackageSummary, CompactQueryMatch,
    CompactRepoIndex, CompactSymbolEntry,
};
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
