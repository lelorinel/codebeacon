//! Per-plan mission brief for run-plan agents.

use crate::run_plan::PlanDoc;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub fn write_brief(
    brief_path: &Path,
    plan: &PlanDoc,
    prompt: &str,
    block_key: &str,
    signal_path: &Path,
) -> Result<()> {
    if let Some(parent) = brief_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = signal_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = format!(
        r#"# Codebeacon run-plan brief

## block_key

`{block_key}`

## Shared prompt

{prompt}

## Plan (`{plan_name}`)

{body}

## File locks (MCP codebeacon) — optional

Server name is exactly `codebeacon`. If lock tools are missing / MCP errors "not found", skip locks — do not explore MCP catalogs.

Before editing a shared path: call `claim_path` with path + block_key=`{block_key}` + intent.
If held: call `await_path` then retry claim.
After finishing that path: call `release_path` with a short summary.
When the whole plan is complete: call `session_done` with block_key=`{block_key}`, ok=true, and a short summary.
If blocked: `session_done` with ok=false.

Also required when finished: Bash `touch {signal}` then print a line that is exactly: `CBDONE {block_key}`.
"#,
        block_key = block_key,
        prompt = if prompt.is_empty() {
            "(none — implement the plan as written)"
        } else {
            prompt
        },
        plan_name = plan.path.display(),
        body = plan.body.trim(),
        signal = signal_path.display(),
    );
    fs::write(brief_path, text).with_context(|| format!("write brief {}", brief_path.display()))?;
    Ok(())
}
