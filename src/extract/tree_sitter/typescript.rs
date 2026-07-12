use super::{char_at, line_at, node_text, run_query};
use crate::imports::RawImport;
use crate::types::{SymbolEntry, SymbolKind};
use std::path::Path;
use tree_sitter::Tree;

pub fn grammar_for(path: &Path) -> Result<tree_sitter::Language, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let lang = match ext {
        "tsx" | "jsx" => tree_sitter_typescript::LANGUAGE_TSX,
        _ => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
    };
    Ok(lang.into())
}

const SYMBOLS_QUERY: &str = r#"
(function_declaration name: (identifier) @name) @item
(class_declaration name: (type_identifier) @name) @item
(interface_declaration name: (type_identifier) @name) @item
(type_alias_declaration name: (type_identifier) @name) @item
(lexical_declaration (variable_declarator name: (identifier) @name)) @item
"#;

const IMPORTS_QUERY: &str = r#"
(import_statement source: (string (string_fragment) @path)) @import
(export_statement source: (string (string_fragment) @path)) @import
"#;

pub fn extract(path: &Path, tree: &Tree, source: &str) -> (Vec<SymbolEntry>, Vec<RawImport>) {
    let lang = grammar_for(path).unwrap_or(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into());
    let mut symbols = Vec::new();
    let mut imports = Vec::new();

    let _ = run_query(tree, source, SYMBOLS_QUERY, lang.clone(), |name, node| {
        if name != "name" {
            return;
        }
        let Some(sym_name) = node_text(node, source) else {
            return;
        };
        let mut parent = node.parent().unwrap_or(node);
        while parent.kind() == "variable_declarator" || parent.kind() == "lexical_declaration" {
            if let Some(p) = parent.parent() {
                parent = p;
            } else {
                break;
            }
        }
        let kind = match parent.kind() {
            "function_declaration" => SymbolKind::Function,
            "class_declaration" => SymbolKind::Struct,
            "interface_declaration" => SymbolKind::Trait,
            "type_alias_declaration" => SymbolKind::Other,
            "lexical_declaration" => SymbolKind::Variable,
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
        if text.starts_with('.') || text.starts_with('/') {
            imports.push(RawImport {
                text: text.to_string(),
                line: line_at(node),
                character: char_at(node),
            });
        }
    });

    (symbols, imports)
}
