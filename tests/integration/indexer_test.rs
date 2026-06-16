use codebeacon::indexer::Indexer;
use codebeacon::lsp::pool::LspPool;
use std::path::Path;

fn fixture_root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/simple_rust"))
}

#[test]
fn full_index_creates_codeindex_dir() {
    let root = fixture_root();
    let root_uri = format!("file://{}", root.display());
    let mut pool = LspPool::new(&root_uri);
    let mut indexer = Indexer::new(root);

    let _ = indexer.full_index(&mut pool);

    let codeindex = root.join(".codeindex");
    assert!(codeindex.exists(), ".codeindex dir should be created");
    assert!(codeindex.join("index.json").exists(), "index.json should exist");
}

#[test]
fn index_json_contains_packages() {
    let root = fixture_root();
    let codeindex = root.join(".codeindex");
    if !codeindex.join("index.json").exists() {
        let root_uri = format!("file://{}", root.display());
        let mut pool = LspPool::new(&root_uri);
        let mut indexer = Indexer::new(root);
        let _ = indexer.full_index(&mut pool);
    }

    let text = std::fs::read_to_string(codeindex.join("index.json")).unwrap();
    let idx: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(idx["packages"].as_array().is_some());
    assert!(!idx["packages"].as_array().unwrap().is_empty());
}
