use super::{char_at, line_at, node_text, run_query};
use crate::imports::RawImport;
use crate::types::{SymbolEntry, SymbolKind};
use tree_sitter::Tree;
use tree_sitter_c_sharp::LANGUAGE;

const SYMBOLS_QUERY: &str = r#"
(class_declaration name: (identifier) @name) @item
(interface_declaration name: (identifier) @name) @item
(struct_declaration name: (identifier) @name) @item
(enum_declaration name: (identifier) @name) @item
(method_declaration name: (identifier) @name) @item
(property_declaration name: (identifier) @name) @item
"#;

const IMPORTS_QUERY: &str = r#"
(using_directive) @import
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
            "class_declaration" | "struct_declaration" => SymbolKind::Struct,
            "interface_declaration" => SymbolKind::Trait,
            "enum_declaration" => SymbolKind::Enum,
            "method_declaration" => SymbolKind::Function,
            "property_declaration" => SymbolKind::Variable,
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
        if let Some(rest) = text.strip_prefix("using ") {
            let ns = rest.trim_end_matches(';').trim();
            let ns = ns.strip_prefix("static ").unwrap_or(ns).trim();
            if !ns.is_empty() && !ns.starts_with("System") {
                imports.push(RawImport {
                    text: ns.to_string(),
                    line: line_at(node),
                    character: char_at(node),
                });
            }
        }
    });

    (symbols, imports)
}
