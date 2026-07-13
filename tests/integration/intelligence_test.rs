use codebeacon::config_file::IntelligenceConfig;
use codebeacon::indexer::Indexer;
use codebeacon::intelligence::{change_impact, focus_context, index_status};
use codebeacon::query::RepoQueryCtx;
use std::path::Path;

fn fixture_root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/simple_rust"))
}

fn ensure_index(root: &Path) {
    let codeindex = root.join(".codeindex");
    if !codeindex.join("index.json").exists() {
        let mut indexer = Indexer::new(root);
        indexer.full_index().expect("index fixture");
    }
}

#[test]
fn focus_context_on_auth_file() {
    let root = fixture_root();
    ensure_index(root);
    let qctx = RepoQueryCtx::load(root).unwrap();
    let cfg = IntelligenceConfig::default();
    let out = focus_context(&qctx, "src/auth.rs", 2, &cfg).unwrap();
    assert_eq!(out.anchor, "src/auth.rs");
    assert!(!out.neighbors.is_empty());
    assert!(out.neighbors.iter().any(|n| n.path.contains("auth.rs")));
}

#[test]
fn change_impact_for_login_symbol() {
    let root = fixture_root();
    ensure_index(root);
    let qctx = RepoQueryCtx::load(root).unwrap();
    let cfg = IntelligenceConfig::default();
    let out = change_impact(&qctx, "login", None, true, &cfg).unwrap();
    assert_eq!(out.symbol, "login");
    assert!(out.definition.is_some());
}

#[test]
fn index_status_reports_indexed_at() {
    let root = fixture_root();
    ensure_index(root);
    let cfg = IntelligenceConfig::default();
    let out = index_status(root, &cfg).unwrap();
    assert_ne!(out.indexed_at, "never");
}

#[test]
fn why_file_skips_git_when_not_repo() {
    use codebeacon::intelligence::why_file;
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let mut indexer = Indexer::new(tmp.path());
    indexer.full_index().unwrap();
    let qctx = RepoQueryCtx::load(tmp.path()).unwrap();
    let out = why_file(&qctx, "src/lib.rs", vec![], None);
    assert!(out.recent_commits.is_empty());
}
