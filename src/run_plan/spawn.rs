//! Spawn Cursor / Claude / Codex agents for run-plan waves.

use crate::locks::SharedLockStore;
use crate::run_plan::PlanDoc;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunPlanProvider {
    Cursor,
    Claude,
    Codex,
}

impl RunPlanProvider {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "cursor" | "agent" | "cli:cursor" => Ok(Self::Cursor),
            "claude" | "cli:claude" => Ok(Self::Claude),
            "codex" | "cli:codex" => Ok(Self::Codex),
            other => bail!(
                "unknown provider `{other}` (expected cursor | claude | codex)"
            ),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cursor => "cursor",
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

/// Cursor Agent CLI (`agent` / `CURSOR_AGENT`).
pub fn resolve_agent_bin() -> Result<String> {
    resolve_bin(
        &["CURSOR_AGENT"],
        &["agent", "cursor-agent"],
        "Cursor agent CLI not found; set CURSOR_AGENT or install `agent`",
    )
}

/// Claude Code CLI (`claude` / `CLAUDE_BIN`).
pub fn resolve_claude_bin() -> Result<String> {
    resolve_bin(
        &["CLAUDE_BIN", "CLAUDE_CODE"],
        &["claude"],
        "Claude Code CLI not found; set CLAUDE_BIN or install `claude`",
    )
}

/// Codex CLI (`codex` / `CODEX_BIN`).
pub fn resolve_codex_bin() -> Result<String> {
    resolve_bin(
        &["CODEX_BIN"],
        &["codex"],
        "Codex CLI not found; set CODEX_BIN or install `codex` (npm i -g @openai/codex)",
    )
}

fn resolve_bin(env_keys: &[&str], names: &[&str], err: &str) -> Result<String> {
    for key in env_keys {
        if let Ok(p) = std::env::var(key) {
            if !p.is_empty() {
                return Ok(p);
            }
        }
    }
    for name in names {
        if let Ok(p) = which::which(name) {
            return Ok(p.display().to_string());
        }
    }
    bail!("{err}")
}

/// Write a Claude-compatible MCP config that only exposes codebeacon (locks + index).
pub fn write_claude_mcp_config(path: &Path, workspace: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let exe = std::env::current_exe()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "codebeacon".into());
    let root = workspace.display().to_string();
    let json = serde_json::json!({
        "mcpServers": {
            "codebeacon": {
                "command": exe,
                "args": ["serve", "--root", root]
            }
        }
    });
    std::fs::write(path, serde_json::to_string_pretty(&json)?)
        .with_context(|| format!("write mcp config {}", path.display()))?;
    Ok(())
}

fn mission_prompt(brief_path: &Path, block_key: &str, signal_path: &Path) -> String {
    format!(
        "Read ONLY {} (mission brief). Then WRITE CODE — do not narrate. \
         Implement ONLY plan block `{}` in this workspace. \
         Prefer Edit/StrReplace. \
         MCP (optional): server name is exactly `codebeacon`. \
         If missing / \"not found\", skip MCP — do not explore MCP catalogs. \
         When MCP works: claim_path → edit → release_path; finish with session_done. \
         When finished REQUIRED: (1) MCP session_done block_key=`{}` ok=true summary≤1 line if MCP works \
         AND (2) Bash `touch {}` then print a line that is exactly: CBDONE {}. \
         If stuck: one short question max — otherwise silence until done.",
        brief_path.display(),
        block_key,
        block_key,
        signal_path.display(),
        block_key
    )
}

pub struct SpawnWaveOpts<'a> {
    pub chunk: &'a [(PlanDoc, PathBuf, PathBuf)],
    pub workspace: &'a Path,
    pub model: &'a str,
    pub provider: RunPlanProvider,
    pub dry_run: bool,
    pub store: &'a SharedLockStore,
    /// Claude `--mcp-config` path (optional).
    pub mcp_config: Option<&'a Path>,
}

pub fn run_wave(opts: SpawnWaveOpts<'_>) -> Result<()> {
    if opts.dry_run {
        for (plan, brief, signal) in opts.chunk {
            println!(
                "[dry-run] would spawn {} for block={} brief={} signal={}",
                opts.provider.as_str(),
                plan.block_key,
                brief.display(),
                signal.display()
            );
            let _ = opts.store.session_done(&plan.block_key, "dry-run", true);
        }
        return Ok(());
    }

    let mut children: Vec<(String, Child, PathBuf)> = Vec::new();
    for (plan, brief, signal) in opts.chunk {
        let _ = std::fs::remove_file(signal);
        let child = spawn_one(
            opts.provider,
            opts.workspace,
            opts.model,
            brief,
            &plan.block_key,
            signal,
            opts.mcp_config,
        )?;
        children.push((plan.block_key.clone(), child, signal.clone()));
    }

    let deadline = Instant::now() + Duration::from_secs(60 * 60); // 1h cap per wave
    while !children.is_empty() {
        if Instant::now() >= deadline {
            for (key, mut child, _) in children.drain(..) {
                let _ = child.kill();
                let _ = opts.store.mark_session_timed_out(&key);
                eprintln!("[codebeacon] timed out: {key}");
            }
            break;
        }

        children.retain_mut(|(key, child, signal)| {
            if opts.store.session_is_terminal(key).unwrap_or(false) {
                let _ = child.kill();
                return false;
            }
            if signal.exists() {
                if !opts.store.session_is_terminal(key).unwrap_or(false) {
                    let _ = opts.store.session_done(key, "signal file", true);
                }
                let _ = child.kill();
                return false;
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    if !opts.store.session_is_terminal(key).unwrap_or(false) {
                        let ok = status.success();
                        let _ = opts.store.session_done(
                            key,
                            if ok {
                                "process exit 0"
                            } else {
                                "process failed"
                            },
                            ok,
                        );
                    }
                    false
                }
                Ok(None) => true,
                Err(_) => {
                    let _ = opts.store.session_done(key, "spawn error", false);
                    false
                }
            }
        });

        if !children.is_empty() {
            thread::sleep(Duration::from_millis(400));
        }
    }
    Ok(())
}

fn spawn_one(
    provider: RunPlanProvider,
    workspace: &Path,
    model: &str,
    brief: &Path,
    block_key: &str,
    signal: &Path,
    mcp_config: Option<&Path>,
) -> Result<Child> {
    let prompt = mission_prompt(brief, block_key, signal);
    match provider {
        RunPlanProvider::Cursor => spawn_cursor(workspace, model, &prompt),
        RunPlanProvider::Claude => spawn_claude(workspace, model, &prompt, mcp_config),
        RunPlanProvider::Codex => spawn_codex(workspace, model, &prompt),
    }
}

fn spawn_cursor(workspace: &Path, model: &str, prompt: &str) -> Result<Child> {
    let bin = resolve_agent_bin()?;
    let mut cmd = Command::new(&bin);
    cmd.arg("--workspace")
        .arg(workspace)
        .arg("--force")
        .arg("--approve-mcps");
    if !model.is_empty() {
        cmd.arg("--model").arg(model);
    }
    cmd.arg(prompt);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .current_dir(workspace)
        .spawn()
        .with_context(|| format!("spawn {bin}"))
}

fn spawn_claude(
    workspace: &Path,
    model: &str,
    prompt: &str,
    mcp_config: Option<&Path>,
) -> Result<Child> {
    let bin = resolve_claude_bin()?;
    let mut cmd = Command::new(&bin);
    // Non-interactive print mode + permission bypass for automation.
    cmd.arg("--print")
        .arg("--permission-mode")
        .arg("bypassPermissions");
    if let Some(mcp) = mcp_config {
        let abs = std::fs::canonicalize(mcp).unwrap_or_else(|_| mcp.to_path_buf());
        cmd.arg("--mcp-config").arg(&abs);
        cmd.arg("--strict-mcp-config");
    }
    if !model.is_empty() {
        cmd.arg("--model").arg(model);
    }
    cmd.arg(prompt);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .current_dir(workspace)
        .spawn()
        .with_context(|| format!("spawn {bin}"))
}

fn spawn_codex(workspace: &Path, model: &str, prompt: &str) -> Result<Child> {
    let bin = resolve_codex_bin()?;
    let mut cmd = Command::new(&bin);
    // Headless exec with workspace writes, no approval prompts.
    cmd.arg("exec")
        .arg("--full-auto")
        .arg("--sandbox")
        .arg("workspace-write")
        .arg("--cd")
        .arg(workspace)
        .arg("--skip-git-repo-check");
    if !model.is_empty() {
        cmd.arg("--model").arg(model);
    }
    cmd.arg(prompt);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .current_dir(workspace)
        .spawn()
        .with_context(|| format!("spawn {bin}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_parse() {
        assert_eq!(
            RunPlanProvider::parse("cursor").unwrap(),
            RunPlanProvider::Cursor
        );
        assert_eq!(
            RunPlanProvider::parse("claude").unwrap(),
            RunPlanProvider::Claude
        );
        assert_eq!(
            RunPlanProvider::parse("codex").unwrap(),
            RunPlanProvider::Codex
        );
        assert!(RunPlanProvider::parse("nope").is_err());
    }

    #[test]
    fn dry_run_wave_marks_done() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("locks.json");
        let store = SharedLockStore::open(path, 600, vec![]);
        store.register_session("auth", None).unwrap();
        let plan = PlanDoc {
            path: PathBuf::from("auth.md"),
            block_key: "auth".into(),
            body: "# auth".into(),
        };
        let brief = tmp.path().join("auth-brief.md");
        let signal = tmp.path().join("DONE.auth");
        run_wave(SpawnWaveOpts {
            chunk: &[(plan, brief, signal)],
            workspace: tmp.path(),
            model: "",
            provider: RunPlanProvider::Codex,
            dry_run: true,
            store: &store,
            mcp_config: None,
        })
        .unwrap();
        assert!(store.session_succeeded("auth").unwrap());
    }

    #[test]
    fn write_claude_mcp_config_shape() {
        let tmp = tempfile::TempDir::new().unwrap();
        let cfg = tmp.path().join("mcp.json");
        write_claude_mcp_config(&cfg, tmp.path()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
        assert!(v["mcpServers"]["codebeacon"]["command"].is_string());
        assert_eq!(
            v["mcpServers"]["codebeacon"]["args"][0].as_str(),
            Some("serve")
        );
    }
}
