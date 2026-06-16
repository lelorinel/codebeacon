use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

#[derive(Debug, Serialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
}

impl McpResponse {
    pub fn result(id: Value, result: Value) -> Self {
        Self { jsonrpc: "2.0".into(), id: Some(id), result: Some(result), error: None }
    }

    pub fn error(id: Value, code: i32, message: &str) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: Some(id),
            result: None,
            error: Some(McpError { code, message: message.into() }),
        }
    }

    pub fn notification(method_result: Value) -> Self {
        Self { jsonrpc: "2.0".into(), id: None, result: Some(method_result), error: None }
    }
}

pub fn text_content(text: impl Into<String>) -> Value {
    serde_json::json!({
        "content": [{ "type": "text", "text": text.into() }]
    })
}

pub fn tool_list() -> Value {
    serde_json::json!({
        "tools": [
            {
                "name": "get_context",
                "description": "Returns relevance-sorted code index for given open files",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "files": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["files"]
                }
            },
            {
                "name": "drill_package",
                "description": "Returns detailed file and symbol listing for a package",
                "inputSchema": {
                    "type": "object",
                    "properties": { "name": { "type": "string" } },
                    "required": ["name"]
                }
            },
            {
                "name": "find_references",
                "description": "Find all usages of a symbol across the codebase",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "symbol": { "type": "string" },
                        "file": { "type": "string", "description": "Absolute or repo-relative file path (enables LSP lookup)" },
                        "line": { "type": "integer", "description": "0-based line of the symbol (required with file)" },
                        "character": { "type": "integer", "description": "0-based character offset (required with file)" }
                    },
                    "required": ["symbol"]
                }
            },
            {
                "name": "find_definition",
                "description": "Find where a symbol is defined",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "symbol": { "type": "string" },
                        "file": { "type": "string", "description": "Absolute or repo-relative file path (enables LSP lookup)" },
                        "line": { "type": "integer", "description": "0-based line of the symbol (required with file)" },
                        "character": { "type": "integer", "description": "0-based character offset (required with file)" }
                    },
                    "required": ["symbol"]
                }
            },
            {
                "name": "get_dependents",
                "description": "List files that depend on the given file",
                "inputSchema": {
                    "type": "object",
                    "properties": { "file": { "type": "string" } },
                    "required": ["file"]
                }
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_tool_call_request() {
        let raw = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "get_context",
                "arguments": { "files": ["src/main.rs"] }
            }
        });
        let req: McpRequest = serde_json::from_value(raw).unwrap();
        assert_eq!(req.method, "tools/call");
        assert_eq!(req.id, Some(json!(1)));
    }

    #[test]
    fn serializes_tool_result() {
        let resp = McpResponse::result(json!(1), json!({"content": [{"type": "text", "text": "hello"}]}));
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains("hello"));
        assert!(s.contains("jsonrpc"));
    }

    #[test]
    fn serializes_error() {
        let err = McpResponse::error(json!(1), -32600, "Invalid Request");
        let s = serde_json::to_string(&err).unwrap();
        assert!(s.contains("Invalid Request"));
    }
}
