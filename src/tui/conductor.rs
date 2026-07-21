//! Conductor / Gallery session meta, spawn queue, and prompts for multi-agent.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const ENV_SESSION: &str = "CODEBEACON_MA_SESSION";
pub const ENV_ROLE: &str = "CODEBEACON_MA_ROLE";
pub const ENV_BLOCK_KEY: &str = "CODEBEACON_MA_BLOCK_KEY";

const ACTIVE_FILE: &str = "ACTIVE";
const META_FILE: &str = "meta.json";
const QUEUE_FILE: &str = "queue.json";
const AGENTS_FILE: &str = "agents.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionMode {
    Gallery,
    Conductor,
}

impl SessionMode {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "gallery" => Ok(Self::Gallery),
            "conductor" => Ok(Self::Conductor),
            other => bail!("unknown mode `{other}` (expected gallery | conductor)"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gallery => "gallery",
            Self::Conductor => "conductor",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentRole {
    Conductor,
    Ensemble,
}

impl AgentRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Conductor => "conductor",
            Self::Ensemble => "ensemble",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "conductor" => Some(Self::Conductor),
            "ensemble" => Some(Self::Ensemble),
            _ => None,
        }
    }

    pub fn sidebar_prefix(self) -> &'static str {
        match self {
            Self::Conductor => "♪",
            Self::Ensemble => "·",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub session_id: String,
    pub mode: SessionMode,
    pub conductor_key: String,
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnRequest {
    pub id: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub enqueued_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct QueueFile {
    #[serde(default)]
    pending: Vec<SpawnRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRecord {
    pub block_key: String,
    pub role: AgentRole,
    pub status: String,
    #[serde(default)]
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AgentsFile {
    #[serde(default)]
    agents: Vec<AgentRecord>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn flock_exclusive(file: &fs::File) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
        if rc != 0 {
            return Err(format!(
                "flock LOCK_EX failed: {}",
                io::Error::last_os_error()
            ));
        }
    }
    let _ = file;
    Ok(())
}

fn flock_unlock(file: &fs::File) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
        if rc != 0 {
            return Err(format!(
                "flock LOCK_UN failed: {}",
                io::Error::last_os_error()
            ));
        }
    }
    let _ = file;
    Ok(())
}

fn with_locked_json<T, R>(
    path: &Path,
    default: T,
    f: impl FnOnce(&mut T) -> R,
) -> Result<R, String>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let lock_path = path.with_extension("json.lock");
    let lock_file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|e| format!("open lock: {e}"))?;
    flock_exclusive(&lock_file)?;
    let mut value = if path.exists() {
        let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&raw).unwrap_or(default)
    } else {
        default
    };
    let out = f(&mut value);
    let pretty = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    fs::write(path, pretty).map_err(|e| e.to_string())?;
    flock_unlock(&lock_file)?;
    Ok(out)
}

/// Root of all multi-agent sessions: `.codeindex/multi-agent/`.
pub fn multi_agent_root(workspace: &Path) -> PathBuf {
    crate::config::codeindex_dir(workspace).join("multi-agent")
}

pub fn session_dir(workspace: &Path, session_id: &str) -> PathBuf {
    multi_agent_root(workspace).join(session_id)
}

pub fn new_session_id() -> String {
    format!("ma-{}", now_unix())
}

pub fn write_meta(dir: &Path, meta: &SessionMeta) -> Result<()> {
    fs::create_dir_all(dir)?;
    let path = dir.join(META_FILE);
    fs::write(&path, serde_json::to_string_pretty(meta)?)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

pub fn read_meta(dir: &Path) -> Result<SessionMeta> {
    let path = dir.join(META_FILE);
    let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn set_active_session(workspace: &Path, session_id: &str) -> Result<()> {
    let root = multi_agent_root(workspace);
    fs::create_dir_all(&root)?;
    fs::write(root.join(ACTIVE_FILE), session_id)?;
    Ok(())
}

pub fn clear_active_session(workspace: &Path) {
    let path = multi_agent_root(workspace).join(ACTIVE_FILE);
    let _ = fs::remove_file(path);
}

pub fn resolve_session_id(workspace: &Path) -> Option<String> {
    if let Ok(s) = std::env::var(ENV_SESSION) {
        let t = s.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    let path = multi_agent_root(workspace).join(ACTIVE_FILE);
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn enqueue_spawn(dir: &Path, req: SpawnRequest) -> Result<(), String> {
    let path = dir.join(QUEUE_FILE);
    with_locked_json(&path, QueueFile::default(), |q| {
        q.pending.push(req);
    })?;
    Ok(())
}

pub fn drain_spawn_queue(dir: &Path) -> Result<Vec<SpawnRequest>, String> {
    let path = dir.join(QUEUE_FILE);
    with_locked_json(&path, QueueFile::default(), |q| {
        let out = std::mem::take(&mut q.pending);
        out
    })
}

pub fn upsert_agent(dir: &Path, record: AgentRecord) -> Result<(), String> {
    let path = dir.join(AGENTS_FILE);
    with_locked_json(&path, AgentsFile::default(), |f| {
        if let Some(existing) = f.agents.iter_mut().find(|a| a.block_key == record.block_key) {
            *existing = record;
        } else {
            f.agents.push(record);
        }
    })?;
    Ok(())
}

pub fn list_agent_records(dir: &Path) -> Result<Vec<AgentRecord>, String> {
    let path = dir.join(AGENTS_FILE);
    if !path.exists() {
        return Ok(vec![]);
    }
    with_locked_json(&path, AgentsFile::default(), |f| f.agents.clone())
}

pub fn get_agent_record(dir: &Path, block_key: &str) -> Result<Option<AgentRecord>, String> {
    Ok(list_agent_records(dir)?
        .into_iter()
        .find(|a| a.block_key == block_key))
}

pub fn make_spawn_request(
    prompt: String,
    block_key: Option<String>,
    model: Option<String>,
) -> SpawnRequest {
    SpawnRequest {
        id: format!("spawn-{}", now_unix()),
        prompt,
        block_key,
        model,
        enqueued_unix: now_unix(),
    }
}

/// Initial prompt for the conductor agent.
pub fn conductor_brief(block_key: &str, signal_path: &Path) -> String {
    format!(
        "You are the **conductor** of a codebeacon multi-agent session (block_key=`{block_key}`). \
         Orchestrate; spawn **ensemble** agents — do not ask the user to press `n`.\n\n\
         MCP server name is exactly `codebeacon`. When MCP works:\n\
         - `spawn_agent` — args: prompt (required), optional block_key, optional model\n\
         - `list_agents` / `agent_status` — track ensemble members\n\
         - `claim_path` → edit → `release_path` for your own edits (your block_key=`{block_key}`)\n\
         - Each ensemble member has its own block_key and must claim paths before editing\n\
         - Finish with `session_done` block_key=`{block_key}` ok=true\n\n\
         When finished also: Bash `touch {}` then print exactly: CBDONE {block_key}. \
         Prefer Edit/StrReplace. Write code — do not narrate.",
        signal_path.display()
    )
}

/// Prompt wrapper for an ensemble member spawned by the conductor.
pub fn ensemble_brief(block_key: &str, signal_path: &Path, task: &str) -> String {
    format!(
        "You are an **ensemble** agent in a codebeacon conductor session (block_key=`{block_key}`). \
         Implement ONLY your assigned task. Do not spawn other agents.\n\n\
         Task:\n{task}\n\n\
         MCP server `codebeacon`: claim_path → edit → release_path with block_key=`{block_key}`; \
         finish with session_done. Also: `touch {}` then CBDONE {block_key}. \
         Prefer Edit/StrReplace. Write code — do not narrate.",
        signal_path.display()
    )
}

/// Env vars for an agent PTY / MCP serve process.
pub fn ma_env(
    session_id: &str,
    role: AgentRole,
    block_key: &str,
) -> Vec<(String, String)> {
    vec![
        (ENV_SESSION.to_string(), session_id.to_string()),
        (ENV_ROLE.to_string(), role.as_str().to_string()),
        (ENV_BLOCK_KEY.to_string(), block_key.to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn mode_parse() {
        assert_eq!(SessionMode::parse("gallery").unwrap(), SessionMode::Gallery);
        assert_eq!(
            SessionMode::parse("conductor").unwrap(),
            SessionMode::Conductor
        );
        assert!(SessionMode::parse("mux").is_err());
    }

    #[test]
    fn queue_enqueue_drain() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        enqueue_spawn(
            dir,
            make_spawn_request("do thing".into(), Some("e-1".into()), None),
        )
        .unwrap();
        enqueue_spawn(dir, make_spawn_request("other".into(), None, None)).unwrap();
        let drained = drain_spawn_queue(dir).unwrap();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].prompt, "do thing");
        assert!(drain_spawn_queue(dir).unwrap().is_empty());
    }

    #[test]
    fn agent_upsert_and_list() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        upsert_agent(
            dir,
            AgentRecord {
                block_key: "conductor".into(),
                role: AgentRole::Conductor,
                status: "running".into(),
                summary: String::new(),
            },
        )
        .unwrap();
        upsert_agent(
            dir,
            AgentRecord {
                block_key: "conductor".into(),
                role: AgentRole::Conductor,
                status: "done".into(),
                summary: "ok".into(),
            },
        )
        .unwrap();
        let list = list_agent_records(dir).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].status, "done");
        assert_eq!(list[0].summary, "ok");
    }

    #[test]
    fn ensemble_cannot_match_conductor_role_str() {
        assert_eq!(AgentRole::parse("ensemble"), Some(AgentRole::Ensemble));
        assert_ne!(
            AgentRole::Conductor.as_str(),
            AgentRole::Ensemble.as_str()
        );
    }
}
