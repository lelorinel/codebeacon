use super::{char_at, line_at, node_text, run_query};
use crate::imports::RawImport;
use crate::types::{SymbolEntry, SymbolKind};
use tree_sitter::Tree;
use tree_sitter_go::LANGUAGE;

const SYMBOLS_QUERY: &str = r#"
(function_declaration name: (identifier) @name) @item
(method_declaration name: (field_identifier) @name) @item
(type_declaration (type_spec name: (type_identifier) @name)) @item
"#;

const IMPORTS_QUERY: &str = r#"
(import_spec path: (interpreted_string_literal) @path) @import
(import_spec path: (raw_string_literal) @path) @import
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
            "function_declaration" | "method_declaration" => SymbolKind::Function,
            "type_spec" => {
                let type_decl = parent.parent().map(|p| p.kind()).unwrap_or("");
                if type_decl == "type_declaration" {
                    let text = node_text(parent, source).unwrap_or("");
                    if text.contains("struct") {
                        SymbolKind::Struct
                    } else if text.contains("interface") {
                        SymbolKind::Trait
                    } else {
                        SymbolKind::Other
                    }
                } else {
                    SymbolKind::Other
                }
            }
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
        if name != "path" {
            return;
        }
        let text = node_text(node, source).unwrap_or("");
        let path = text.trim_matches('"').trim_matches('`');
        if !path.is_empty() {
            imports.push(RawImport {
                text: path.to_string(),
                line: line_at(node),
                character: char_at(node),
            });
        }
    });

    (symbols, imports)
}
