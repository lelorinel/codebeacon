//! Extraction corpus: regex and tree-sitter must agree on symbol names per fixture.

use codebeacon::config::detect_language;
use codebeacon::config_file::ExtractorConfig;
use codebeacon::extract::{extract_file, extract_from_source, ExtractEngine};
use std::fs;
use std::path::{Path, PathBuf};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/extract")
}

fn load_expected(stem: &str) -> Vec<String> {
    let path = fixtures_dir().join(format!("{stem}.json"));
    let text = fs::read_to_string(&path).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn symbol_names(path: &Path, config: &ExtractorConfig) -> Vec<String> {
    extract_file(path, config)
        .symbols
        .into_iter()
        .map(|s| s.name)
        .collect()
}

fn corpus_cases() -> Vec<(&'static str, &'static str)> {
    vec![
        ("rust_sample.rs", "rust_sample"),
        ("go_sample.go", "go_sample"),
        ("typescript_multiline.ts", "typescript_multiline"),
        ("csharp_sample.cs", "csharp_sample"),
    ]
}

fn corpus_cases_tree_sitter() -> Vec<(&'static str, &'static str)> {
    let mut cases = corpus_cases();
    cases.push(("python_nested.py", "python_nested"));
    cases
}

#[test]
fn corpus_regex_matches_expected_symbols() {
    let config = ExtractorConfig {
        mode: "regex".into(),
        ..ExtractorConfig::default()
    };
    for (file, stem) in corpus_cases() {
        let path = fixtures_dir().join(file);
        let expected = load_expected(stem);
        let names = symbol_names(&path, &config);
        assert_eq!(names, expected, "regex corpus mismatch for {file}");
    }
}

#[cfg(feature = "tree-sitter")]
#[test]
fn corpus_tree_sitter_matches_expected_symbols() {
    let config = ExtractorConfig {
        parse_timeout_ms: 2000,
        ..ExtractorConfig::default()
    };
    for (file, stem) in corpus_cases_tree_sitter() {
        let path = fixtures_dir().join(file);
        let expected = load_expected(stem);
        let result = extract_file(&path, &config);
        assert_eq!(
            result.engine,
            ExtractEngine::TreeSitter,
            "expected tree-sitter for {file}"
        );
        let names: Vec<String> = result.symbols.into_iter().map(|s| s.name).collect();
        assert_eq!(names, expected, "tree-sitter corpus mismatch for {file}");
    }
}

#[test]
fn nested_python_method_found_with_tree_sitter_or_skipped_without_feature() {
    let path = fixtures_dir().join("python_nested.py");
    let code = fs::read_to_string(&path).unwrap();
    let lang = detect_language(&path).unwrap();

    let regex_config = ExtractorConfig {
        mode: "regex".into(),
        ..ExtractorConfig::default()
    };
    let regex_names = extract_from_source(&path, &code, &lang, &regex_config)
        .symbols
        .into_iter()
        .map(|s| s.name)
        .collect::<Vec<_>>();
    assert!(
        !regex_names.contains(&"inner_method".to_string()),
        "regex should not find indented method"
    );

    #[cfg(feature = "tree-sitter")]
    {
        let ts_config = ExtractorConfig {
            parse_timeout_ms: 2000,
            ..ExtractorConfig::default()
        };
        let ts_names = extract_from_source(&path, &code, &lang, &ts_config)
            .symbols
            .into_iter()
            .map(|s| s.name)
            .collect::<Vec<_>>();
        assert!(ts_names.contains(&"inner_method".to_string()));
    }
}

#[test]
fn extractor_mode_regex_forces_regex_even_with_tree_sitter_feature() {
    let path = fixtures_dir().join("python_nested.py");
    let config = ExtractorConfig {
        mode: "regex".into(),
        ..ExtractorConfig::default()
    };
    let result = extract_file(&path, &config);
    assert_eq!(result.engine, ExtractEngine::Regex);
}

#[test]
fn csharp_full_index_depends_on_resolved_using() {
    use codebeacon::indexer::Indexer;
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    fs::create_dir(root.join(".git")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("MyApp")).unwrap();
    fs::write(
        root.join("src/Program.cs"),
        "using MyApp.Auth;\n\nclass Program { static void Main() {} }\n",
    )
    .unwrap();
    fs::write(root.join("MyApp/Auth.cs"), "namespace MyApp.Auth { class Auth {} }\n").unwrap();

    let mut indexer = Indexer::new(root);
    indexer.full_index().unwrap();

    let entries = indexer.load_all_entries();
    let program = entries
        .iter()
        .find(|e| e.path == PathBuf::from("src/Program.cs"))
        .unwrap();
    assert!(
        program.depends_on.contains(&"MyApp/Auth.cs".to_string()),
        "C# using should resolve to depends_on, got {:?}",
        program.depends_on
    );
}

#[test]
fn typescript_multiline_import_extracted() {
    let path = fixtures_dir().join("typescript_multiline.ts");
    let config = ExtractorConfig::default();
    let imports = extract_file(&path, &config).imports;
    assert!(imports.iter().any(|i| i.text == "./utils"));
}
