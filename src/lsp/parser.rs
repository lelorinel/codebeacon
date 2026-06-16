use crate::types::{ReferenceLocation, SymbolEntry, SymbolKind};
use serde_json::Value;
use std::path::PathBuf;

pub fn parse_document_symbols(value: &Value) -> Vec<SymbolEntry> {
    let arr = match value.as_array() {
        Some(a) => a,
        None => return vec![],
    };
    arr.iter()
        .filter_map(|s| {
            let name = s["name"].as_str()?.to_string();
            let kind_num = s["kind"].as_u64().unwrap_or(0);
            let kind = lsp_kind_to_symbol_kind(kind_num);
            let line = s["range"]["start"]["line"].as_u64().unwrap_or(0) as u32;
            let signature = s
                .get("detail")
                .and_then(|d| d.as_str())
                .unwrap_or(&name)
                .to_string();
            Some(SymbolEntry {
                name,
                signature,
                kind,
                line,
            })
        })
        .collect()
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
    PathBuf::from(uri.trim_start_matches("file://"))
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
        assert_eq!(
            symbols[0].signature,
            "fn login(email: &str, password: &str) -> Result<Token>"
        );
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
