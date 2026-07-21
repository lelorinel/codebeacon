//! MCP handlers for conductor multi-agent spawn / list / status.

use crate::mcp::protocol::text_content;
use crate::mcp::tools::ToolContext;
use crate::tui::conductor::{
    self, AgentRole, SessionMode, ENV_BLOCK_KEY, ENV_ROLE, ENV_SESSION,
};
use anyhow::{bail, Result};
use serde_json::{json, Value};

fn primary_root(ctx: &ToolContext) -> Result<&std::path::Path> {
    ctx.repos
        .first()
        .map(|r| r.root.as_path())
        .ok_or_else(|| anyhow::anyhow!("no repo root"))
}

fn session_dir(ctx: &ToolContext) -> Result<std::path::PathBuf> {
    let root = primary_root(ctx)?;
    let Some(session_id) = conductor::resolve_session_id(root) else {
        bail!(
            "no active multi-agent session (set {ENV_SESSION} or run `codebeacon multi-agent --mode conductor`)"
        );
    };
    Ok(conductor::session_dir(root, &session_id))
}

fn load_conductor_meta(ctx: &ToolContext) -> Result<(std::path::PathBuf, conductor::SessionMeta)> {
    let dir = session_dir(ctx)?;
    let meta = conductor::read_meta(&dir)?;
    if meta.mode != SessionMode::Conductor {
        bail!("spawn tools are only available in conductor mode");
    }
    Ok((dir, meta))
}

/// Caller identity from env. When unset (shared Cursor MCP), allow — ensemble
/// prompts instruct not to call spawn_agent; Claude per-pane MCP sets ROLE.
fn caller_is_conductor(meta: &conductor::SessionMeta, _args: &Value) -> bool {
    if let Ok(role) = std::env::var(ENV_ROLE) {
        if AgentRole::parse(&role) == Some(AgentRole::Ensemble) {
            return false;
        }
        if AgentRole::parse(&role) == Some(AgentRole::Conductor) {
            return true;
        }
    }
    if let Ok(key) = std::env::var(ENV_BLOCK_KEY) {
        let key = key.trim();
        if !key.is_empty() {
            return key == meta.conductor_key;
        }
    }
    true
}

pub fn handle_spawn_agent(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let (dir, meta) = load_conductor_meta(ctx)?;
    if !caller_is_conductor(&meta, args) {
        bail!("only the conductor may spawn agents");
    }
    let prompt = args
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if prompt.is_empty() {
        bail!("prompt is required");
    }
    let block_key = args
        .get("block_key")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.as_str() != meta.conductor_key);
    let model = args
        .get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let req = conductor::make_spawn_request(prompt.to_string(), block_key.clone(), model);
    let id = req.id.clone();
    conductor::enqueue_spawn(&dir, req).map_err(|e| anyhow::anyhow!(e))?;

    Ok(text_content(format!(
        "enqueued spawn id={id}{}",
        block_key
            .map(|k| format!(" block_key={k}"))
            .unwrap_or_default()
    )))
}

pub fn handle_list_agents(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let _ = args;
    let (dir, _meta) = load_conductor_meta(ctx)?;
    let mut agents = conductor::list_agent_records(&dir).map_err(|e| anyhow::anyhow!(e))?;

    // Enrich status from lock store when available.
    if let Some(store) = &ctx.lock_store {
        if let Ok(sessions) = store.list_sessions() {
            for a in &mut agents {
                if let Some(s) = sessions.iter().find(|s| s.block_key == a.block_key) {
                    a.status = format!("{:?}", s.status).to_lowercase();
                    if !s.summary.is_empty() {
                        a.summary = s.summary.clone();
                    }
                }
            }
        }
    }

    Ok(json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&agents)?
        }]
    }))
}

pub fn handle_agent_status(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let (dir, _meta) = load_conductor_meta(ctx)?;
    let key = args
        .get("block_key")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if key.is_empty() {
        bail!("block_key is required");
    }
    let mut record = conductor::get_agent_record(&dir, key)
        .map_err(|e| anyhow::anyhow!(e))?
        .ok_or_else(|| anyhow::anyhow!("unknown agent `{key}`"))?;

    if let Some(store) = &ctx.lock_store {
        if let Ok(sessions) = store.list_sessions() {
            if let Some(s) = sessions.iter().find(|s| s.block_key == key) {
                record.status = format!("{:?}", s.status).to_lowercase();
                if !s.summary.is_empty() {
                    record.summary = s.summary.clone();
                }
            }
        }
    }

    Ok(json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&record)?
        }]
    }))
}

/// True when conductor MCP tools should appear in tools/list.
pub fn conductor_tools_enabled(ctx: &ToolContext) -> bool {
    let Ok(root) = primary_root(ctx) else {
        return false;
    };
    let Some(session_id) = conductor::resolve_session_id(root) else {
        return false;
    };
    let dir = conductor::session_dir(root, &session_id);
    match conductor::read_meta(&dir) {
        Ok(meta) => meta.mode == SessionMode::Conductor,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::conductor::{
        set_active_session, write_meta, AgentRecord, SessionMeta, ENV_ROLE,
    };
    use tempfile::TempDir;

    fn write_conductor_session(root: &std::path::Path) -> std::path::PathBuf {
        let session_id = "ma-test";
        let dir = conductor::session_dir(root, session_id);
        std::fs::create_dir_all(&dir).unwrap();
        write_meta(
            &dir,
            &SessionMeta {
                session_id: session_id.into(),
                mode: SessionMode::Conductor,
                conductor_key: "conductor".into(),
                provider: "cursor".into(),
                model: String::new(),
            },
        )
        .unwrap();
        set_active_session(root, session_id).unwrap();
        dir
    }

    #[test]
    fn ensemble_role_rejected() {
        let tmp = TempDir::new().unwrap();
        let dir = write_conductor_session(tmp.path());
        std::env::set_var(ENV_SESSION, "ma-test");
        std::env::set_var(ENV_ROLE, "ensemble");
        let meta = conductor::read_meta(&dir).unwrap();
        assert!(!caller_is_conductor(&meta, &json!({})));
        std::env::set_var(ENV_ROLE, "conductor");
        assert!(caller_is_conductor(&meta, &json!({})));
        std::env::remove_var(ENV_ROLE);
        std::env::remove_var(ENV_SESSION);
        // No role env → allowed (shared MCP)
        assert!(caller_is_conductor(&meta, &json!({})));
    }

    #[test]
    fn enqueue_via_queue_file() {
        let tmp = TempDir::new().unwrap();
        let dir = write_conductor_session(tmp.path());
        conductor::upsert_agent(
            &dir,
            AgentRecord {
                block_key: "conductor".into(),
                role: AgentRole::Conductor,
                status: "running".into(),
                summary: String::new(),
            },
        )
        .unwrap();
        let req = conductor::make_spawn_request("task".into(), None, None);
        conductor::enqueue_spawn(&dir, req).unwrap();
        assert_eq!(conductor::drain_spawn_queue(&dir).unwrap().len(), 1);
    }
}
