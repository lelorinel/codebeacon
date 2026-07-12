mod csharp;
mod go;
mod python;
mod rust;
mod typescript;

use crate::config::Language;
use crate::imports::RawImport;
use crate::types::SymbolEntry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tree_sitter::StreamingIterator;

static POOL: OnceLock<Mutex<ParserPool>> = OnceLock::new();

fn pool() -> &'static Mutex<ParserPool> {
    POOL.get_or_init(|| Mutex::new(ParserPool::new()))
}

/// Top-level tree-sitter extraction with timeout and incremental parse.
pub fn extract(
    path: &Path,
    code: &str,
    lang: &Language,
    timeout_ms: u64,
) -> Result<(Vec<SymbolEntry>, Vec<RawImport>), String> {
    let path_key = path.to_path_buf();
    let code = code.to_string();
    let lang = lang.clone();

    with_timeout(timeout_ms, move || {
        let mut pool = pool().lock().map_err(|e| e.to_string())?;
        pool.parse_and_extract(&path_key, &code, &lang)
    })
}

fn with_timeout<T, F>(timeout_ms: u64, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_millis(timeout_ms.max(1)))
        .map_err(|_| "parse timeout".to_string())?
}

struct ParserEntry {
    parser: tree_sitter::Parser,
    tree: Option<tree_sitter::Tree>,
    lang: Language,
}

struct ParserPool {
    entries: HashMap<PathBuf, ParserEntry>,
}

impl ParserPool {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    fn parse_and_extract(
        &mut self,
        path: &Path,
        code: &str,
        lang: &Language,
    ) -> Result<(Vec<SymbolEntry>, Vec<RawImport>), String> {
        let entry = self.entry_for(path, lang)?;
        let old_tree = entry.tree.take();
        let tree = entry
            .parser
            .parse(code, old_tree.as_ref())
            .ok_or_else(|| "parse failed".to_string())?;
        entry.tree = Some(tree.clone());

        if tree.root_node().has_error() && error_node_count(&tree.root_node()) > 3 {
            return Err("parse has errors".to_string());
        }

        let (symbols, imports) = match lang {
            Language::Rust => rust::extract(&tree, code),
            Language::Go => go::extract(&tree, code),
            Language::Python => python::extract(&tree, code),
            Language::TypeScript => typescript::extract(path, &tree, code),
            Language::CSharp => csharp::extract(&tree, code),
        };
        Ok((symbols, imports))
    }

    fn entry_for(&mut self, path: &Path, lang: &Language) -> Result<&mut ParserEntry, String> {
        let needs_new = match self.entries.get(path) {
            Some(e) => e.lang != *lang,
            None => true,
        };
        if needs_new {
            let mut parser = tree_sitter::Parser::new();
            let ts_lang = language_for(lang, path)?;
            parser
                .set_language(&ts_lang)
                .map_err(|e| format!("set_language: {e}"))?;
            self.entries.insert(
                path.to_path_buf(),
                ParserEntry {
                    parser,
                    tree: None,
                    lang: lang.clone(),
                },
            );
        }
        self.entries
            .get_mut(path)
            .ok_or_else(|| "parser entry missing".to_string())
    }
}

fn language_for(lang: &Language, path: &Path) -> Result<tree_sitter::Language, String> {
    match lang {
        Language::Rust => Ok(tree_sitter_rust::LANGUAGE.into()),
        Language::Go => Ok(tree_sitter_go::LANGUAGE.into()),
        Language::Python => Ok(tree_sitter_python::LANGUAGE.into()),
        Language::TypeScript => typescript::grammar_for(path),
        Language::CSharp => Ok(tree_sitter_c_sharp::LANGUAGE.into()),
    }
}

fn error_node_count(node: &tree_sitter::Node) -> usize {
    let mut count = 0;
    if node.is_error() || node.is_missing() {
        count += 1;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count += error_node_count(&child);
    }
    count
}

pub(crate) fn node_text<'a>(node: tree_sitter::Node, source: &'a str) -> Option<&'a str> {
    node.utf8_text(source.as_bytes()).ok()
}

pub(crate) fn line_at(node: tree_sitter::Node) -> u32 {
    node.start_position().row as u32 + 1
}

pub(crate) fn char_at(node: tree_sitter::Node) -> u32 {
    node.start_position().column as u32
}

pub(crate) fn run_query(
    tree: &tree_sitter::Tree,
    source: &str,
    query_src: &str,
    lang: tree_sitter::Language,
    mut on_capture: impl FnMut(&str, tree_sitter::Node),
) -> Result<(), String> {
    let query = tree_sitter::Query::new(&lang, query_src)
        .map_err(|e| format!("query error: {e}"))?;
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    while let Some(m) = matches.next() {
        for cap in m.captures {
            let name = query.capture_names()[cap.index as usize];
            on_capture(name, cap.node);
        }
    }
    Ok(())
}

#[cfg(all(test, feature = "tree-sitter"))]
mod tests {
    use super::*;
    use crate::config::Language;
    use std::path::Path;

    #[test]
    fn rust_fixture_parses() {
        let code = include_str!("../../../tests/fixtures/extract/rust_sample.rs");
        let result = extract(Path::new("rust_sample.rs"), code, &Language::Rust, 5000);
        assert!(result.is_ok(), "rust extract failed: {:?}", result.err());
        let (syms, _) = result.unwrap();
        assert!(!syms.is_empty(), "expected symbols from rust fixture");
    }

    #[test]
    fn csharp_fixture_parses() {
        let code = include_str!("../../../tests/fixtures/extract/csharp_sample.cs");
        let result = extract(Path::new("csharp_sample.cs"), code, &Language::CSharp, 5000);
        assert!(result.is_ok(), "csharp extract failed: {:?}", result.err());
        let (syms, _) = result.unwrap();
        assert!(!syms.is_empty(), "expected symbols from csharp fixture");
    }
}
