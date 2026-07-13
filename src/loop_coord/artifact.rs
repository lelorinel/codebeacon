use crate::loop_coord::session::LoopSession;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationArtifact {
    pub iteration: u32,
    pub recorded_at: String,
    pub stale_count: usize,
    pub reindexed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recorded_files: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
}

pub fn loop_root(codeindex_dir: &Path) -> PathBuf {
    codeindex_dir.join("loop")
}

pub fn session_dir(codeindex_dir: &Path, session_id: &str) -> PathBuf {
    loop_root(codeindex_dir).join(session_id)
}

pub fn write_session(codeindex_dir: &Path, session: &LoopSession) -> Result<()> {
    let dir = session_dir(codeindex_dir, &session.id);
    fs::create_dir_all(&dir)?;
    let path = dir.join("session.json");
    fs::write(path, serde_json::to_string_pretty(session)?)?;
    Ok(())
}

pub fn read_session(codeindex_dir: &Path, session_id: &str) -> Result<LoopSession> {
    let path = session_dir(codeindex_dir, session_id).join("session.json");
    let text = fs::read_to_string(&path)
        .with_context(|| format!("loop session '{session_id}' not found"))?;
    Ok(serde_json::from_str(&text)?)
}

pub fn write_iteration(
    codeindex_dir: &Path,
    session_id: &str,
    artifact: &IterationArtifact,
) -> Result<()> {
    let dir = session_dir(codeindex_dir, session_id);
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("iteration-{:03}.json", artifact.iteration));
    fs::write(path, serde_json::to_string_pretty(artifact)?)?;
    Ok(())
}

pub fn delete_session(codeindex_dir: &Path, session_id: &str) -> Result<()> {
    let dir = session_dir(codeindex_dir, session_id);
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    Ok(())
}
