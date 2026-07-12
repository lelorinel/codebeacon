//! Performance budgets for extraction (run with `cargo test --ignored --features tree-sitter`).

use codebeacon::config::detect_language;
use codebeacon::config_file::ExtractorConfig;
use codebeacon::extract::{extract_file, extract_from_source};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/extract")
        .join(name)
}

#[test]
#[ignore]
fn extract_perf_cold_parse_rust_1k() {
    let mut code = String::new();
    for i in 0..200 {
        code.push_str(&format!("pub fn f{i}() {{}}\n"));
    }
    let path = fixture("rust_sample.rs");
    let lang = detect_language(&path).unwrap();
    let config = ExtractorConfig::default();

    let start = Instant::now();
    extract_from_source(&path, &code, &lang, &config);
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 30,
        "cold parse budget exceeded: {}ms",
        elapsed.as_millis()
    );
}

#[test]
#[ignore]
fn extract_perf_incremental_rust_1k() {
    let mut code = String::new();
    for i in 0..200 {
        code.push_str(&format!("pub fn f{i}() {{}}\n"));
    }
    let path = fixture("rust_sample.rs");
    let lang = detect_language(&path).unwrap();
    let config = ExtractorConfig::default();

    extract_from_source(&path, &code, &lang, &config);
    let start = Instant::now();
    let mut edited = code.clone();
    edited.push_str("pub fn extra() {}\n");
    extract_from_source(&path, &edited, &lang, &config);
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 15,
        "incremental parse budget exceeded: {}ms",
        elapsed.as_millis()
    );
}

#[test]
#[ignore]
fn extract_perf_regex_vs_tree_sitter_fixture_repo() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/extract");
    let regex_cfg = ExtractorConfig {
        mode: "regex".into(),
        ..ExtractorConfig::default()
    };
    let ts_cfg = ExtractorConfig::default();

    let regex_start = Instant::now();
    for entry in fs::read_dir(&dir).unwrap().flatten() {
        let p = entry.path();
        if p.extension().is_some() {
            extract_file(&p, &regex_cfg);
        }
    }
    let regex_elapsed = regex_start.elapsed();

    let ts_start = Instant::now();
    for entry in fs::read_dir(&dir).unwrap().flatten() {
        let p = entry.path();
        if p.extension().is_some() {
            extract_file(&p, &ts_cfg);
        }
    }
    let ts_elapsed = ts_start.elapsed();

    assert!(
        ts_elapsed <= regex_elapsed * 2,
        "tree-sitter {:?} > 2× regex {:?}",
        ts_elapsed,
        regex_elapsed
    );
}
