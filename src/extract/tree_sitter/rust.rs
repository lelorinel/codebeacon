use super::{char_at, line_at, node_text, run_query};
use crate::imports::RawImport;
use crate::types::{SymbolEntry, SymbolKind};
use tree_sitter::Tree;
use tree_sitter_rust::LANGUAGE;

const SYMBOLS_QUERY: &str = r#"
(function_item name: (identifier) @name) @item
(struct_item name: (type_identifier) @name) @item
(enum_item name: (type_identifier) @name) @item
(trait_item name: (type_identifier) @name) @item
(mod_item name: (identifier) @name) @item
(type_item name: (type_identifier) @name) @item
(const_item name: (identifier) @name) @item
(static_item name: (identifier) @name) @item
(macro_definition name: (identifier) @name) @item
"#;

const IMPORTS_QUERY: &str = r#"
(use_declaration) @import
(mod_item) @mod_item
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
            "function_item" => SymbolKind::Function,
            "struct_item" => SymbolKind::Struct,
            "enum_item" => SymbolKind::Enum,
            "trait_item" => SymbolKind::Trait,
            "mod_item" => SymbolKind::Module,
            "type_item" => SymbolKind::Other,
            "const_item" | "static_item" => SymbolKind::Variable,
            "macro_definition" => SymbolKind::Other,
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
        match name {
            "import" => {
                let text = node_text(node, source).unwrap_or("");
                let trimmed = text.trim();
                if let Some(rest) = trimmed.strip_prefix("use ") {
                    let path = rest.trim_end_matches(';').split("::").take_while(|s| {
                        !s.contains('{') && !s.contains('*')
                    }).collect::<Vec<_>>().join("::");
                    if path.starts_with("crate::") || path.starts_with("super::") {
                        imports.push(RawImport {
                            text: path,
                            line: line_at(node),
                            character: char_at(node),
                        });
                    }
                }
            }
            "mod_item" => {
                let text = node_text(node, source).unwrap_or("");
                if text.contains(';') && !text.contains('{') {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        if let Some(mod_name) = node_text(name_node, source) {
                            imports.push(RawImport {
                                text: mod_name.to_string(),
                                line: line_at(node),
                                character: char_at(name_node),
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    });

    (symbols, imports)
}
