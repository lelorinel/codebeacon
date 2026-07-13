use codebeacon::config::codeindex_dir;
use codebeacon::config_file::{IntelligenceConfig, LoopConfig, ReindexPolicy};
use codebeacon::indexer::Indexer;
use codebeacon::loop_coord::{
    loop_begin_with_tick, loop_end, loop_tick, read_session, LoopSession,
};
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
fn loop_begin_creates_session() {
    let root = fixture_root();
    ensure_index(root);
    let qctx = RepoQueryCtx::load(root).unwrap();
    let loop_cfg = LoopConfig {
        reindex: ReindexPolicy::Never,
        ..LoopConfig::default()
    };
    let intel = IntelligenceConfig::default();
    let codeindex = codeindex_dir(root);
    let (session, resp) = loop_begin_with_tick(
        root,
        &codeindex,
        "fix login",
        vec!["src/auth.rs".into()],
        &loop_cfg,
        &intel,
        &qctx,
        false,
    )
    .unwrap();
    assert_eq!(resp.session_id, session.id);
    assert!(read_session(&codeindex, &session.id).is_ok());
}

#[test]
fn loop_tick_increments_iteration() {
    let root = fixture_root();
    ensure_index(root);
    let qctx = RepoQueryCtx::load(root).unwrap();
    let loop_cfg = LoopConfig {
        reindex: ReindexPolicy::Never,
        max_iterations: 50,
        ..LoopConfig::default()
    };
    let intel = IntelligenceConfig::default();
    let codeindex = codeindex_dir(root);
    let (mut session, _) = loop_begin_with_tick(
        root,
        &codeindex,
        "test",
        vec!["src/auth.rs".into()],
        &loop_cfg,
        &intel,
        &qctx,
        false,
    )
    .unwrap();
    let tick = loop_tick(
        root,
        &codeindex,
        &mut session,
        &loop_cfg,
        &intel,
        &qctx,
        None,
    )
    .unwrap();
    assert_eq!(tick.iteration, 1);
    assert!(!tick.signals.should_stop);
}

#[test]
fn loop_tick_should_stop_at_max_iterations() {
    let root = fixture_root();
    ensure_index(root);
    let qctx = RepoQueryCtx::load(root).unwrap();
    let loop_cfg = LoopConfig {
        reindex: ReindexPolicy::Never,
        max_iterations: 1,
        ..LoopConfig::default()
    };
    let intel = IntelligenceConfig::default();
    let codeindex = codeindex_dir(root);
    let (mut session, _) = loop_begin_with_tick(
        root,
        &codeindex,
        "test",
        vec![],
        &loop_cfg,
        &intel,
        &qctx,
        false,
    )
    .unwrap();
    let tick = loop_tick(
        root,
        &codeindex,
        &mut session,
        &loop_cfg,
        &intel,
        &qctx,
        None,
    )
    .unwrap();
    assert!(tick.signals.should_stop);
}

#[test]
fn loop_end_closes_session() {
    let root = fixture_root();
    ensure_index(root);
    let qctx = RepoQueryCtx::load(root).unwrap();
    let loop_cfg = LoopConfig::default();
    let intel = IntelligenceConfig::default();
    let codeindex = codeindex_dir(root);
    let (mut session, _) = loop_begin_with_tick(
        root,
        &codeindex,
        "test",
        vec![],
        &loop_cfg,
        &intel,
        &qctx,
        false,
    )
    .unwrap();
    let end = loop_end(root, &codeindex, &mut session, &loop_cfg, &intel).unwrap();
    assert_eq!(end.session_id, session.id);
    assert!(session.closed);
}
