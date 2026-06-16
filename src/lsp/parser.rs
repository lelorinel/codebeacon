use crate::types::{ReferenceLocation, SymbolEntry, SymbolKind};
use serde_json::Value;
use std::path::PathBuf;

pub fn parse_document_symbols(value: &Value) -> Vec<SymbolEntry> {
    let arr = match value.as_array() {
        Some(a) => a,
        None => return vec![],
    };
    let mut out = Vec::new();
    for s in arr {
        collect_symbol(s, &mut out);
    }
    out
}

fn collect_symbol(s: &Value, out: &mut Vec<SymbolEntry>) {
    // Determine position: DocumentSymbol uses "range", SymbolInformation uses "location.range"
    let range = s.get("range")
        .or_else(|| s.pointer("/location/range"));
    let line = range
        .and_then(|r| r.pointer("/start/line"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let character = range
        .and_then(|r| r.pointer("/start/character"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    let name = match s["name"].as_str() {
        Some(n) => n.to_string(),
        None => return,
    };
    let kind_num = s["kind"].as_u64().unwrap_or(0);
    let kind = lsp_kind_to_symbol_kind(kind_num);
    let signature = s.get("detail")
        .and_then(|d| d.as_str())
        .unwrap_or(&name)
        .to_string();

    out.push(SymbolEntry { name, signature, kind, line, character });

    // Recurse into children (DocumentSymbol only)
    if let Some(children) = s["children"].as_array() {
        for child in children {
            collect_symbol(child, out);
        }
    }
}

pub fn parse_references(value: &Value) -> Vec<ReferenceLocation> {
    let arr = match value.as_array() {
        Some(a) => a,
        None => return vec![],
    };
    arr.iter()
        .filter_map(|r| {
            let uri = r["uri"].as_str()?;
            let file = uri_to_path(uri);
            let line = r["range"]["start"]["line"].as_u64().unwrap_or(0) as u32;
            Some(ReferenceLocation {
                file,
                line,
                context: String::new(),
            })
        })
        .collect()
}

pub fn parse_definition(value: &Value) -> Option<(PathBuf, u32)> {
    let loc = if value.is_array() { value.get(0)? } else { value };
    let uri = loc["uri"].as_str()?;
    let line = loc["range"]["start"]["line"].as_u64().unwrap_or(0) as u32;
    Some((uri_to_path(uri), line))
}

fn uri_to_path(uri: &str) -> PathBuf {
    let path = uri.trim_start_matches("file://");
    let decoded = path.replace("%20", " ").replace("%23", "#");
    PathBuf::from(decoded)
}

fn lsp_kind_to_symbol_kind(kind: u64) -> SymbolKind {
    match kind {
        12 => SymbolKind::Function,
        5 => SymbolKind::Struct,
        10 => SymbolKind::Enum,
        11 => SymbolKind::Trait,
        2 | 3 => SymbolKind::Module,
        13 | 14 => SymbolKind::Variable,
        _ => SymbolKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_function_symbol() {
        let raw = json!([{
            "name": "login",
            "kind": 12,
            "range": {
                "start": { "line": 5, "character": 0 },
                "end": { "line": 10, "character": 1 }
            },
            "detail": "fn login(email: &str, password: &str) -> Result<Token>"
        }]);

        let symbols = parse_document_symbols(&raw);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "login");
        assert_eq!(symbols[0].kind, crate::types::SymbolKind::Function);
        assert_eq!(symbols[0].line, 5);
        assert_eq!(symbols[0].character, 0);
        assert_eq!(
            symbols[0].signature,
            "fn login(email: &str, password: &str) -> Result<Token>"
        );
    }

    #[test]
    fn parses_hierarchical_document_symbols() {
        // Simulates a C# class (kind 5 = Struct/Class) with a nested method (kind 12 = Function)
        let raw = json!([{
            "name": "PlayerController",
            "kind": 5,
            "range": {
                "start": { "line": 2, "character": 0 },
                "end": { "line": 30, "character": 1 }
            },
            "detail": "class PlayerController",
            "children": [{
                "name": "Update",
                "kind": 12,
                "range": {
                    "start": { "line": 10, "character": 4 },
                    "end": { "line": 20, "character": 5 }
                },
                "detail": "void Update()"
            }]
        }]);

        let symbols = parse_document_symbols(&raw);
        assert_eq!(symbols.len(), 2, "class and its child method should both be present");

        assert_eq!(symbols[0].name, "PlayerController");
        assert_eq!(symbols[0].kind, crate::types::SymbolKind::Struct);
        assert_eq!(symbols[0].line, 2);
        assert_eq!(symbols[0].character, 0);

        assert_eq!(symbols[1].name, "Update");
        assert_eq!(symbols[1].kind, crate::types::SymbolKind::Function);
        assert_eq!(symbols[1].line, 10);
        assert_eq!(symbols[1].character, 4);
    }

    #[test]
    fn parses_reference_location() {
        let raw = json!([{
            "uri": "file:///repo/src/api.rs",
            "range": { "start": { "line": 42, "character": 8 }, "end": { "line": 42, "character": 13 } }
        }]);

        let refs = parse_references(&raw);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].line, 42);
        assert!(refs[0].file.to_string_lossy().contains("api.rs"));
    }

    #[test]
    fn unknown_symbol_kind_maps_to_other() {
        let raw = json!([{
            "name": "MY_CONST",
            "kind": 14,
            "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 10 } }
        }]);
        let symbols = parse_document_symbols(&raw);
        assert_eq!(symbols[0].kind, crate::types::SymbolKind::Variable);
    }
}
