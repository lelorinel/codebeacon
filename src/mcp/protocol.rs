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

fn compact_property() -> Value {
    serde_json::json!({
        "type": "boolean",
        "description": "Return token-compressed response with dictionary refs. Default from [compact] enabled in .codeindex.toml (true). Set false for local LLMs."
    })
}

pub fn tool_list(fs_tools: bool, security: bool, intelligence: bool, loop_enabled: bool) -> Value {
    let mut tools = vec![
        serde_json::json!({
            "name": "get_context",
            "description": "Returns relevance-sorted code index (L0 summary). Prefer over grep/Read. Graphify equivalent: browse graph concepts.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "files": { "type": "array", "items": { "type": "string" } },
                    "repo": { "type": "string", "description": "Repo name to query (only needed in a multi-repo workspace)" },
                    "compact": compact_property()
                },
                "required": []
            }
        }),
        serde_json::json!({
            "name": "drill_package",
            "description": "Returns detailed file and symbol listing for a package. Use 'repo/package' notation or the `repo` argument in multi-repo workspaces.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Package name, or 'repo/package' in a multi-repo workspace" },
                    "repo": { "type": "string", "description": "Repo name to search (only needed in a multi-repo workspace)" },
                    "compact": compact_property()
                },
                "required": ["name"]
            }
        }),
        serde_json::json!({
            "name": "find_references",
            "description": "Find all usages of a symbol across the codebase (all repos by default; use `repo` to scope to one).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "symbol": { "type": "string" },
                    "file": { "type": "string", "description": "Absolute or repo-relative file path (enables LSP lookup)" },
                    "line": { "type": "integer", "description": "0-based line of the symbol (required with file)" },
                    "character": { "type": "integer", "description": "0-based character offset (required with file)" },
                    "repo": { "type": "string", "description": "Repo name to search (only needed in a multi-repo workspace)" },
                    "compact": compact_property()
                },
                "required": ["symbol"]
            }
        }),
        serde_json::json!({
            "name": "find_definition",
            "description": "Find where a symbol is defined (all repos by default; use `repo` to scope to one).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "symbol": { "type": "string" },
                    "file": { "type": "string", "description": "Absolute or repo-relative file path (enables LSP lookup)" },
                    "line": { "type": "integer", "description": "0-based line of the symbol (required with file)" },
                    "character": { "type": "integer", "description": "0-based character offset (required with file)" },
                    "repo": { "type": "string", "description": "Repo name to search (only needed in a multi-repo workspace)" },
                    "compact": compact_property()
                },
                "required": ["symbol"]
            }
        }),
        serde_json::json!({
            "name": "get_dependents",
            "description": "List files that depend on the given file (impact analysis). Graphify equivalent: reverse neighbors.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "File path, or 'repo/path' in a multi-repo workspace" },
                    "repo": { "type": "string", "description": "Repo name to search (only needed in a multi-repo workspace)" }
                },
                "required": ["file"]
            }
        }),
        serde_json::json!({
            "name": "init_workspace",
            "description": "Build (or rebuild) the code index for the workspace. Call this when get_context reports that no index exists yet.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "repo": { "type": "string", "description": "Repo name to index (only needed in a multi-repo workspace; omit to index all repos)" }
                },
                "required": []
            }
        }),
        serde_json::json!({
            "name": "query_context",
            "description": "Search packages, symbols, and files by keywords (Graphify equivalent: query_graph).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "question": { "type": "string", "description": "Search query / keywords" },
                    "repo": { "type": "string", "description": "Repo name (multi-repo workspace)" },
                    "compact": compact_property()
                },
                "required": ["question"]
            }
        }),
        serde_json::json!({
            "name": "shortest_path",
            "description": "Shortest dependency path between two files/symbols (Graphify equivalent: graphify path).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "from": { "type": "string" },
                    "to": { "type": "string" },
                    "repo": { "type": "string" },
                    "compact": compact_property()
                },
                "required": ["from", "to"]
            }
        }),
        serde_json::json!({
            "name": "hotspots",
            "description": "Top files by reverse-dependency count (Graphify equivalent: god_nodes).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "description": "Max results (default 10)" },
                    "repo": { "type": "string" }
                },
                "required": []
            }
        }),
        serde_json::json!({
            "name": "get_report",
            "description": "Returns CODEBEACON_REPORT.md (MCP resource equivalent: codebeacon://report).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "repo": { "type": "string" }
                },
                "required": []
            }
        }),
        serde_json::json!({
            "name": "get_index_summary",
            "description": "Returns index.json L0 summary (MCP resource equivalent: codebeacon://index).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "repo": { "type": "string" },
                    "compact": compact_property()
                },
                "required": []
            }
        }),
        serde_json::json!({
            "name": "get_hotspots",
            "description": "Alias for hotspots (MCP resource equivalent: codebeacon://hotspots).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": { "type": "integer" },
                    "repo": { "type": "string" }
                },
                "required": []
            }
        }),
    ];

    if fs_tools {
        tools.extend([
            serde_json::json!({
                "name": "read_file",
                "description": "Read the contents of a file in the workspace.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path relative to the repo root (or absolute)" },
                        "repo": { "type": "string", "description": "Repo name (only needed in a multi-repo workspace)" }
                    },
                    "required": ["path"]
                }
            }),
            serde_json::json!({
                "name": "write_file",
                "description": "Create or overwrite a file in the workspace. Creates parent directories if needed.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path":    { "type": "string", "description": "Path relative to the repo root" },
                        "content": { "type": "string", "description": "Full file content to write" },
                        "repo":    { "type": "string", "description": "Repo name (required if multiple repos in workspace)" }
                    },
                    "required": ["path", "content"]
                }
            }),
            serde_json::json!({
                "name": "edit_file",
                "description": "Replace the first occurrence of old_string with new_string in a file. Fails if old_string is not found — make sure it matches the file exactly.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path":       { "type": "string", "description": "Path relative to the repo root" },
                        "old_string": { "type": "string", "description": "Exact string to find in the file" },
                        "new_string": { "type": "string", "description": "Replacement string" },
                        "repo":       { "type": "string", "description": "Repo name (required if multiple repos in workspace)" }
                    },
                    "required": ["path", "old_string", "new_string"]
                }
            }),
            serde_json::json!({
                "name": "list_directory",
                "description": "List files and subdirectories at a path in the workspace.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Directory path relative to the repo root (defaults to repo root)" },
                        "repo": { "type": "string", "description": "Repo name (only needed in a multi-repo workspace)" }
                    },
                    "required": []
                }
            }),
        ]);
    }

    if security {
        tools.push(serde_json::json!({
            "name": "verify_security",
            "description": "Run security verification on a code fragment without writing to disk. Checks CWE-190/131/191/369/680 (Z3) and optional pattern CWEs per policy. Returns SAT witness, UNSAT proof, or pattern-only warnings.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "content": { "type": "string", "description": "Code fragment to verify (e.g. the new_string from an edit)" },
                    "path":    { "type": "string", "description": "File path for context (defaults to 'fragment')" },
                    "repo":    { "type": "string", "description": "Repo name (required if multiple repos in workspace)" }
                },
                "required": ["content"]
            }
        }));
    }

    if intelligence {
        tools.extend([
            serde_json::json!({
                "name": "focus_context",
                "description": "Subgraph around a file for edit-time context (anchor package, neighbors, symbols).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "file": { "type": "string", "description": "File path or dict ref (p1)" },
                        "radius": { "type": "integer", "description": "BFS hop cap (default from [intelligence] focus_default_radius)" },
                        "repo": { "type": "string" },
                        "compact": compact_property()
                    },
                    "required": ["file"]
                }
            }),
            serde_json::json!({
                "name": "task_context",
                "description": "Keyword search plus package drill summary for a task (e.g. 'login bug').",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "question": { "type": "string" },
                        "file": { "type": "string", "description": "Optional file for proximity boost" },
                        "limit": { "type": "integer", "description": "Max matches (default 10)" },
                        "repo": { "type": "string" },
                        "compact": compact_property()
                    },
                    "required": ["question"]
                }
            }),
            serde_json::json!({
                "name": "change_impact",
                "description": "Blast radius before changing a symbol (definitions, references, dependent files, risk tier).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "symbol": { "type": "string" },
                        "file": { "type": "string", "description": "Scope to package containing this file" },
                        "exact": { "type": "boolean", "description": "Exact symbol name match (default true)" },
                        "repo": { "type": "string" },
                        "compact": compact_property()
                    },
                    "required": ["symbol"]
                }
            }),
            serde_json::json!({
                "name": "index_status",
                "description": "Index freshness vs working tree (stale files, git dirty count).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "repo": { "type": "string" }
                    },
                    "required": []
                }
            }),
            serde_json::json!({
                "name": "package_conventions",
                "description": "Convention fingerprint for a package (error style, logging, async, test patterns).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "package": { "type": "string" },
                        "repo": { "type": "string" },
                        "compact": compact_property()
                    },
                    "required": ["package"]
                }
            }),
            serde_json::json!({
                "name": "similar_symbols",
                "description": "Lightweight symbol similarity by kind and signature token overlap.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "symbol": { "type": "string" },
                        "file": { "type": "string" },
                        "limit": { "type": "integer", "description": "Max results (default 5)" },
                        "repo": { "type": "string" }
                    },
                    "required": ["symbol"]
                }
            }),
            serde_json::json!({
                "name": "api_surface",
                "description": "Public exports for a package (language-specific heuristics).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "package": { "type": "string" },
                        "repo": { "type": "string" },
                        "compact": compact_property()
                    },
                    "required": ["package"]
                }
            }),
            serde_json::json!({
                "name": "why_file",
                "description": "Git history and dependency context for a file.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "file": { "type": "string" },
                        "repo": { "type": "string" }
                    },
                    "required": ["file"]
                }
            }),
            serde_json::json!({
                "name": "fragile_files",
                "description": "High-churn files with many dependents (hotspots × git churn).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "description": "Max results (default 10)" },
                        "repo": { "type": "string" }
                    },
                    "required": []
                }
            }),
        ]);
    }

    if loop_enabled {
        tools.extend([
            serde_json::json!({
                "name": "loop_begin",
                "description": "Start a loop context session for iterative agent work.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "goal": { "type": "string", "description": "Task goal / prompt context" },
                        "file": { "type": "string", "description": "Primary active file" },
                        "files": { "type": "array", "items": { "type": "string" } },
                        "tick": { "type": "boolean", "description": "Run first loop_tick immediately (default true)" },
                        "repo": { "type": "string" },
                        "compact": compact_property()
                    },
                    "required": ["goal"]
                }
            }),
            serde_json::json!({
                "name": "loop_tick",
                "description": "Next loop iteration: index status, focus context, reindex policy, signals.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "string" },
                        "file": { "type": "string" },
                        "repo": { "type": "string" },
                        "compact": compact_property()
                    },
                    "required": ["session_id"]
                }
            }),
            serde_json::json!({
                "name": "loop_record",
                "description": "Record files touched after an edit; optional change_impact for symbol.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "string" },
                        "files": { "type": "array", "items": { "type": "string" } },
                        "symbol": { "type": "string" },
                        "repo": { "type": "string" }
                    },
                    "required": ["session_id", "files"]
                }
            }),
            serde_json::json!({
                "name": "loop_end",
                "description": "Close loop session and return summary.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "string" },
                        "repo": { "type": "string" }
                    },
                    "required": ["session_id"]
                }
            }),
        ]);
    }

    serde_json::json!({ "tools": tools })
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
