use anyhow::{bail, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopSession {
    pub id: String,
    pub goal: String,
    pub started_at: String,
    pub iteration: u32,
    pub active_files: Vec<String>,
    pub touched_files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_tick_at: Option<String>,
    pub closed: bool,
}

pub fn new_session_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:012x}", nanos % (1u128 << 48))
}

pub fn begin_session(goal: &str, active_files: Vec<String>) -> LoopSession {
    LoopSession {
        id: new_session_id(),
        goal: goal.to_string(),
        started_at: Utc::now().to_rfc3339(),
        iteration: 0,
        active_files,
        touched_files: Vec::new(),
        last_tick_at: None,
        closed: false,
    }
}

impl LoopSession {
    pub fn ensure_open(&self) -> Result<()> {
        if self.closed {
            bail!("loop session '{}' is closed", self.id);
        }
        Ok(())
    }

    pub fn bump_iteration(&mut self) {
        self.iteration += 1;
        self.last_tick_at = Some(Utc::now().to_rfc3339());
    }

    pub fn record_files(&mut self, files: &[String]) {
        for f in files {
            if !self.touched_files.contains(f) {
                self.touched_files.push(f.clone());
            }
            if !self.active_files.contains(f) {
                self.active_files.push(f.clone());
            }
        }
    }

    pub fn close(&mut self) {
        self.closed = true;
        self.last_tick_at = Some(Utc::now().to_rfc3339());
    }

    pub fn primary_file(&self) -> Option<&str> {
        self.active_files.first().map(String::as_str)
    }
}
