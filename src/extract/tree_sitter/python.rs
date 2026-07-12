use super::{char_at, line_at, node_text, run_query};
use crate::imports::RawImport;
use crate::types::{SymbolEntry, SymbolKind};
use tree_sitter::Tree;
use tree_sitter_python::LANGUAGE;

const SYMBOLS_QUERY: &str = r#"
(function_definition name: (identifier) @name) @item
(class_definition name: (identifier) @name) @item
"#;

const IMPORTS_QUERY: &str = r#"
(import_statement) @import
(import_from_statement) @import
"#;

pub fn extract(tree: &Tree, source: &str) -> (Vec<SymbolEntry>, Vec<RawImport>) {
    let lang: tree_sitter::Language = LANGUAGE.into();
    let mut symbols = Vec::new();
    let mut imports = Vec::new();

    let _ = run_query(tree, source, SYMBOLS_QUERY, lang.clone(), |name, node| {
        if name != "name" {
            return;
        }
        let Some(sym_name) = node_text(node, source) else {
            return;
        };
        let parent = node.parent().unwrap_or(node);
        let kind = match parent.kind() {
            "function_definition" => SymbolKind::Function,
            "class_definition" => SymbolKind::Struct,
            _ => SymbolKind::Other,
        };
        let sig = node_text(parent, source).unwrap_or(sym_name);
        let first_line = sig.lines().next().unwrap_or(sym_name);
        symbols.push(SymbolEntry {
            name: sym_name.to_string(),
            signature: first_line.to_string(),
            kind,
            line: line_at(node),
            character: char_at(node),
        });
    });

    let _ = run_query(tree, source, IMPORTS_QUERY, lang, |name, node| {
        if name != "import" {
            return;
        }
        let text = node_text(node, source).unwrap_or("").trim();
        if text.starts_with("from ") {
            if let Some(rest) = text.strip_prefix("from ") {
                let module = rest.split(" import").next().unwrap_or(rest).trim();
                if !module.is_empty() {
                    imports.push(RawImport {
                        text: module.to_string(),
                        line: line_at(node),
                        character: char_at(node),
                    });
                }
            }
        } else if text.starts_with("import ") {
            let module = text.strip_prefix("import ").unwrap_or(text).split(',').next().unwrap_or("").trim();
            if !module.is_empty() {
                imports.push(RawImport {
                    text: module.to_string(),
                    line: line_at(node),
                    character: char_at(node),
                });
            }
        }
    });

    (symbols, imports)
}
