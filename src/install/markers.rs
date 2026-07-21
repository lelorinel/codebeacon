//! Idempotent merge of Codebeacon sections into user files.

pub const MD_START: &str = "<!-- codebeacon-start -->";
pub const MD_END: &str = "<!-- codebeacon-end -->";

/// Strip `//` line comments so JSONC (e.g. prior install output) parses with serde_json.
fn strip_jsonc_comments(input: &str) -> String {
    input
        .lines()
        .filter(|line| !line.trim().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Replace or append a marked section in markdown/text.
pub fn merge_marked_section(existing: &str, section: &str) -> String {
    if let Some(start) = existing.find(MD_START) {
        if let Some(end) = existing.find(MD_END) {
            let before = &existing[..start];
            let after = &existing[end + MD_END.len()..];
            return format!("{before}{MD_START}\n{section}\n{MD_END}{after}");
        }
    }
    if existing.trim().is_empty() {
        format!("{MD_START}\n{section}\n{MD_END}\n")
    } else {
        format!("{existing}\n\n{MD_START}\n{section}\n{MD_END}\n")
    }
}

/// Remove marked section from markdown/text.
pub fn remove_marked_section(existing: &str) -> String {
    if let Some(start) = existing.find(MD_START) {
        if let Some(end) = existing.find(MD_END) {
            let before = &existing[..start];
            let after = &existing[end + MD_END.len()..];
            return format!("{before}{after}").trim_end().to_string() + "\n";
        }
    }
    existing.to_string()
}

/// Merge codebeacon MCP entry into JSON object (mcpServers or mcp).
pub fn merge_mcp_json(existing: &str, block: &str) -> Result<String, serde_json::Error> {
    let block_val: serde_json::Value = serde_json::from_str(block)?;
    let codebeacon_entry = block_val
        .get("codebeacon")
        .cloned()
        .unwrap_or(block_val);

    if existing.trim().is_empty() {
        let inner = serde_json::to_string_pretty(&codebeacon_entry)?;
        return Ok(format!(
            "{{\n  \"mcpServers\": {{\n    \"codebeacon\": {inner}\n  }}\n}}"
        ));
    }

    let mut root: serde_json::Value = serde_json::from_str(&strip_jsonc_comments(existing))?;
    let obj = root.as_object_mut().ok_or_else(|| {
        serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "expected JSON object",
        ))
    })?;

    let servers_key = if obj.contains_key("mcpServers") {
        "mcpServers"
    } else if obj.contains_key("mcp") {
        "mcp"
    } else {
        obj.insert("mcpServers".into(), serde_json::json!({}));
        "mcpServers"
    };

    let servers = obj
        .get_mut(servers_key)
        .and_then(|v| v.as_object_mut())
        .ok_or_else(|| {
            serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "mcpServers must be object",
            ))
        })?;
    servers.insert("codebeacon".into(), codebeacon_entry);
    Ok(serde_json::to_string_pretty(&root)?)
}

/// Remove codebeacon from MCP JSON.
pub fn remove_mcp_json(existing: &str) -> Result<String, serde_json::Error> {
    if existing.trim().is_empty() {
        return Ok(String::new());
    }
    let mut root: serde_json::Value = serde_json::from_str(&strip_jsonc_comments(existing))?;
    if let Some(obj) = root.as_object_mut() {
        for key in ["mcpServers", "mcp"] {
            if let Some(servers) = obj.get_mut(key).and_then(|v| v.as_object_mut()) {
                servers.remove("codebeacon");
            }
        }
    }
    Ok(serde_json::to_string_pretty(&root)?)
}

/// Markers for Codex `config.toml` MCP block (OpenAI: `[mcp_servers.<name>]`).
pub const CODEX_MCP_START: &str = "# codebeacon-mcp-start";
pub const CODEX_MCP_END: &str = "# codebeacon-mcp-end";

fn escape_toml_basic(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Build the marked `[mcp_servers.codebeacon]` block for Codex config.toml.
pub fn codex_mcp_toml_block(exe: &str, args: &[String]) -> String {
    let args_toml: Vec<String> = args
        .iter()
        .map(|a| format!("\"{}\"", escape_toml_basic(a)))
        .collect();
    format!(
        "{CODEX_MCP_START}\n[mcp_servers.codebeacon]\ncommand = \"{}\"\nargs = [{}]\n{CODEX_MCP_END}\n",
        escape_toml_basic(exe),
        args_toml.join(", ")
    )
}

/// Insert or replace the marked Codex MCP block in config.toml text.
pub fn merge_codex_mcp_toml(existing: &str, exe: &str, args: &[String]) -> String {
    let block = codex_mcp_toml_block(exe, args);
    if let Some(start) = existing.find(CODEX_MCP_START) {
        if let Some(end) = existing.find(CODEX_MCP_END) {
            let before = &existing[..start];
            let after = &existing[end + CODEX_MCP_END.len()..];
            let after = after.strip_prefix('\n').unwrap_or(after);
            let mut out = String::new();
            out.push_str(before);
            out.push_str(&block);
            out.push_str(after);
            return out;
        }
    }
    // Also replace a bare [mcp_servers.codebeacon] table if present without markers.
    if let Some(replaced) = replace_bare_codex_mcp_table(existing, &block) {
        return replaced;
    }
    if existing.trim().is_empty() {
        block
    } else {
        let mut out = existing.trim_end().to_string();
        out.push_str("\n\n");
        out.push_str(&block);
        out
    }
}

fn replace_bare_codex_mcp_table(existing: &str, block: &str) -> Option<String> {
    let header = "[mcp_servers.codebeacon]";
    let start = existing.find(header)?;
    let rest = &existing[start + header.len()..];
    let rel_end = rest
        .find("\n[")
        .map(|i| i + 1) // keep the following [
        .unwrap_or(rest.len());
    let end = start + header.len() + rel_end;
    let before = &existing[..start];
    let after = &existing[end..];
    let after = after.strip_prefix('\n').unwrap_or(after);
    let mut out = String::new();
    out.push_str(before);
    out.push_str(block);
    out.push_str(after);
    Some(out)
}

/// Remove marked (or bare) Codex MCP codebeacon table from config.toml.
pub fn remove_codex_mcp_toml(existing: &str) -> String {
    if let Some(start) = existing.find(CODEX_MCP_START) {
        if let Some(end) = existing.find(CODEX_MCP_END) {
            let before = &existing[..start];
            let after = &existing[end + CODEX_MCP_END.len()..];
            let after = after.strip_prefix('\n').unwrap_or(after);
            return format!("{before}{after}")
                .trim_end()
                .to_string()
                + if before.is_empty() && after.is_empty() {
                    ""
                } else {
                    "\n"
                };
        }
    }
    if let Some(cleaned) = replace_bare_codex_mcp_table(existing, "") {
        return cleaned.trim_end().to_string()
            + if cleaned.trim().is_empty() { "" } else { "\n" };
    }
    existing.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_marked_section_idempotent() {
        let section = "Use get_context first.";
        let once = merge_marked_section("", section);
        assert!(once.contains(MD_START));
        let twice = merge_marked_section(&once, "Updated.");
        assert!(twice.contains("Updated."));
        assert_eq!(twice.matches(MD_START).count(), 1);
    }

    #[test]
    fn remove_marked_section_works() {
        let merged = merge_marked_section("# Title\n", "content");
        let removed = remove_marked_section(&merged);
        assert!(!removed.contains(MD_START));
        assert!(removed.contains("# Title"));
    }

    #[test]
    fn merge_mcp_json_adds_codebeacon() {
        let existing = r#"{"mcpServers": {}}"#;
        let block = r#"{"codebeacon": {"command": "codebeacon", "args": ["serve"]}}"#;
        let out = merge_mcp_json(existing, block).unwrap();
        assert!(out.contains("codebeacon"));
    }

    #[test]
    fn merge_mcp_json_handles_jsonc_from_prior_install() {
        let existing = r#"{
  // codebeacon-start
  "mcpServers": {
    "codebeacon": {
      "command": "codebeacon",
      "args": ["serve"]
    }
  }
  // codebeacon-end
}"#;
        let block = r#"{"codebeacon": {"command": "/new/codebeacon", "args": ["serve"]}}"#;
        let out = merge_mcp_json(existing, block).unwrap();
        assert!(out.contains("/new/codebeacon"));
        assert!(!out.contains("// codebeacon"));
    }

    #[test]
    fn merge_codex_mcp_toml_writes_absolute_command() {
        let args = vec!["serve".into()];
        let out = merge_codex_mcp_toml("", "/home/me/.cargo/bin/codebeacon", &args);
        assert!(out.contains("[mcp_servers.codebeacon]"));
        assert!(out.contains("command = \"/home/me/.cargo/bin/codebeacon\""));
        assert!(out.contains("args = [\"serve\"]"));
        assert!(out.contains(CODEX_MCP_START));
    }

    #[test]
    fn merge_codex_mcp_toml_idempotent_and_preserves_other() {
        let existing = "model = \"o4-mini\"\n\n[mcp_servers.other]\ncommand = \"npx\"\n";
        let args = vec!["serve".into()];
        let once = merge_codex_mcp_toml(existing, "/bin/codebeacon", &args);
        let twice = merge_codex_mcp_toml(&once, "/bin/codebeacon2", &args);
        assert_eq!(twice.matches(CODEX_MCP_START).count(), 1);
        assert!(twice.contains("/bin/codebeacon2"));
        assert!(twice.contains("model = \"o4-mini\""));
        assert!(twice.contains("[mcp_servers.other]"));
    }

    #[test]
    fn remove_codex_mcp_toml_strips_block() {
        let args = vec!["serve".into()];
        let merged = merge_codex_mcp_toml("foo = 1\n", "/x/codebeacon", &args);
        let removed = remove_codex_mcp_toml(&merged);
        assert!(!removed.contains("codebeacon"));
        assert!(removed.contains("foo = 1"));
    }
}
