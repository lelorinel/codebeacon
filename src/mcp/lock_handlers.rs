//! MCP handlers for multi-agent path locks.

use crate::locks::{ClaimResult, SessionDoneResult, SessionStatus, SharedLockStore};
use crate::mcp::protocol::text_content;
use crate::mcp::tools::ToolContext;
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::time::Duration;

fn store(ctx: &ToolContext) -> Result<&SharedLockStore> {
    ctx.lock_store
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("path locks are disabled (codebeacon serve --no-locks or [locks] enabled=false)"))
}

pub fn handle_claim_path(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let store = store(ctx)?;
    let path = args["path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("path required"))?;
    let intent = args["intent"].as_str().unwrap_or("write");
    let block_key = args["block_key"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("block_key required"))?;
    match store
        .claim(path, intent, block_key)
        .map_err(|e| anyhow::anyhow!(e))?
    {
        ClaimResult::Ok => Ok(text_content(json!({ "ok": true }).to_string())),
        ClaimResult::Held { by, intent } => Ok(text_content(
            json!({ "ok": false, "held_by": by, "intent": intent }).to_string(),
        )),
        ClaimResult::Rejected(msg) => bail!(msg),
    }
}

pub fn handle_release_path(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let store = store(ctx)?;
    let path = args["path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("path required"))?;
    let summary = args["summary"].as_str().unwrap_or("");
    let block_key = args["block_key"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("block_key required"))?;
    store
        .release(path, summary, block_key)
        .map_err(|e| anyhow::anyhow!(e))?;
    Ok(text_content(json!({ "ok": true }).to_string()))
}

pub fn handle_await_path(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let store = store(ctx)?;
    let path = args["path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("path required"))?;
    let waiter = args["block_key"]
        .as_str()
        .or_else(|| args["waiter"].as_str())
        .ok_or_else(|| anyhow::anyhow!("block_key required"))?;
    let timeout_ms = args["timeout_ms"].as_u64().unwrap_or(30_000);
    let poll_ms = args["poll_ms"].as_u64().unwrap_or(200);
    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        match store
            .try_await(path, waiter)
            .map_err(|e| anyhow::anyhow!(e))?
        {
            Some(done) => {
                return Ok(text_content(
                    json!({
                        "ready": true,
                        "path": done.path,
                        "block_key": done.block_key,
                        "summary": done.summary,
                    })
                    .to_string(),
                ));
            }
            None => {
                if std::time::Instant::now() >= deadline {
                    return Ok(text_content(
                        json!({ "ready": false, "timeout": true }).to_string(),
                    ));
                }
                std::thread::sleep(Duration::from_millis(poll_ms));
            }
        }
    }
}

pub fn handle_list_locks(ctx: &ToolContext, _args: &Value) -> Result<Value> {
    let store = store(ctx)?;
    let locks = store.list_locks().map_err(|e| anyhow::anyhow!(e))?;
    let rows: Vec<_> = locks
        .iter()
        .map(|l| {
            json!({
                "path": l.path,
                "block_key": l.block_key,
                "intent": l.intent,
            })
        })
        .collect();
    Ok(text_content(
        serde_json::to_string_pretty(&rows).unwrap_or_default(),
    ))
}

pub fn handle_list_done(ctx: &ToolContext, _args: &Value) -> Result<Value> {
    let store = store(ctx)?;
    let done = store.list_done().map_err(|e| anyhow::anyhow!(e))?;
    Ok(text_content(
        serde_json::to_string_pretty(&done).unwrap_or_default(),
    ))
}

pub fn handle_session_done(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let store = store(ctx)?;
    let block_key = args["block_key"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("block_key required"))?;
    let summary = args["summary"].as_str().unwrap_or("");
    let ok = args["ok"].as_bool().unwrap_or(true);
    match store
        .session_done(block_key, summary, ok)
        .map_err(|e| anyhow::anyhow!(e))?
    {
        SessionDoneResult::Ok { status } => Ok(text_content(
            json!({
                "ok": true,
                "status": match status {
                    SessionStatus::Done => "done",
                    SessionStatus::Failed => "failed",
                    SessionStatus::Running => "running",
                    SessionStatus::TimedOut => "timed_out",
                },
            })
            .to_string(),
        )),
        SessionDoneResult::Rejected(msg) => bail!(msg),
    }
}

pub fn handle_list_sessions(ctx: &ToolContext, _args: &Value) -> Result<Value> {
    let store = store(ctx)?;
    let sessions = store.list_sessions().map_err(|e| anyhow::anyhow!(e))?;
    Ok(text_content(
        serde_json::to_string_pretty(&sessions).unwrap_or_default(),
    ))
}
