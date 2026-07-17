//! In-memory + JSON-persisted path lock store (ported from veld-apply).

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockInfo {
    pub path: String,
    pub block_key: String,
    pub intent: String,
    pub expires_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoneInfo {
    pub path: String,
    pub block_key: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimResult {
    Ok,
    Held { by: String, intent: String },
    Rejected(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AwaitResult {
    Ready(DoneInfo),
    Timeout,
    Deadlock(Vec<String>),
    Rejected(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Running,
    Done,
    Failed,
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub block_key: String,
    pub status: SessionStatus,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub pane_id: Option<String>,
    pub started_unix: u64,
    #[serde(default)]
    pub ended_unix: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionDoneResult {
    Ok { status: SessionStatus },
    Rejected(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LockEntry {
    block_key: String,
    intent: String,
    expires_unix: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistFile {
    locks: HashMap<String, LockEntry>,
    done: HashMap<String, DoneInfo>,
    waiting: HashMap<String, HashSet<String>>,
    #[serde(default)]
    sessions: HashMap<String, SessionInfo>,
}

#[derive(Debug)]
pub struct LockStore {
    ttl: Duration,
    allow_prefixes: Vec<PathBuf>,
    locks: HashMap<String, LockEntry>,
    done: HashMap<String, DoneInfo>,
    waiting: HashMap<String, HashSet<String>>,
    sessions: HashMap<String, SessionInfo>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl LockStore {
    pub fn new(ttl_secs: u64, allow_prefixes: Vec<PathBuf>) -> Self {
        Self {
            ttl: Duration::from_secs(ttl_secs.max(1)),
            allow_prefixes,
            locks: HashMap::new(),
            done: HashMap::new(),
            waiting: HashMap::new(),
            sessions: HashMap::new(),
        }
    }

    pub fn load_from(
        path: &Path,
        ttl_secs: u64,
        allow_prefixes: Vec<PathBuf>,
    ) -> Result<Self, String> {
        let mut store = Self::new(ttl_secs, allow_prefixes);
        if !path.exists() {
            return Ok(store);
        }
        let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
        if raw.trim().is_empty() {
            return Ok(store);
        }
        let file: PersistFile = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        store.locks = file.locks;
        store.done = file.done;
        store.waiting = file.waiting;
        store.sessions = file.sessions;
        store.expire_stale(now_unix());
        Ok(store)
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let file = PersistFile {
            locks: self.locks.clone(),
            done: self.done.clone(),
            waiting: self.waiting.clone(),
            sessions: self.sessions.clone(),
        };
        let tmp = path.with_extension("json.tmp");
        fs::write(
            &tmp,
            serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::rename(&tmp, path).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn claim(&mut self, path: &str, intent: &str, block_key: &str) -> ClaimResult {
        let now = now_unix();
        self.expire_stale(now);
        let key = match self.normalize_allowed(path) {
            Ok(k) => k,
            Err(e) => return ClaimResult::Rejected(e),
        };
        let expires = now + self.ttl.as_secs().max(1);
        if let Some(entry) = self.locks.get_mut(&key) {
            if entry.block_key == block_key {
                entry.intent = intent.to_string();
                entry.expires_unix = expires;
                self.clear_wait(block_key, &key);
                return ClaimResult::Ok;
            }
            return ClaimResult::Held {
                by: entry.block_key.clone(),
                intent: entry.intent.clone(),
            };
        }
        self.locks.insert(
            key.clone(),
            LockEntry {
                block_key: block_key.to_string(),
                intent: intent.to_string(),
                expires_unix: expires,
            },
        );
        self.done.remove(&key);
        self.clear_wait(block_key, &key);
        ClaimResult::Ok
    }

    pub fn release(&mut self, path: &str, summary: &str, block_key: &str) -> Result<(), String> {
        let now = now_unix();
        self.expire_stale(now);
        let key = self.normalize_allowed(path)?;
        match self.locks.get(&key) {
            Some(entry) if entry.block_key == block_key => {
                self.locks.remove(&key);
                self.done.insert(
                    key.clone(),
                    DoneInfo {
                        path: key,
                        block_key: block_key.to_string(),
                        summary: summary.to_string(),
                    },
                );
                Ok(())
            }
            Some(entry) => Err(format!(
                "lock held by {} (you are {})",
                entry.block_key, block_key
            )),
            None => Err(format!("no active lock on {key}")),
        }
    }

    pub fn try_await(&mut self, path: &str, waiter: &str) -> Result<Option<DoneInfo>, String> {
        let now = now_unix();
        self.expire_stale(now);
        let key = self.normalize_allowed(path)?;
        if let Some(done) = self.done.get(&key).cloned() {
            self.clear_wait(waiter, &key);
            return Ok(Some(done));
        }
        if !self.locks.contains_key(&key) {
            self.clear_wait(waiter, &key);
            return Ok(Some(DoneInfo {
                path: key,
                block_key: String::new(),
                summary: String::new(),
            }));
        }
        self.waiting
            .entry(waiter.to_string())
            .or_default()
            .insert(key);
        if let Some(cycle) = self.detect_deadlock() {
            return Err(format!("deadlock: {}", cycle.join(" → ")));
        }
        Ok(None)
    }

    pub fn await_ready(
        &mut self,
        path: &str,
        waiter: &str,
        timeout: Duration,
        poll: Duration,
    ) -> AwaitResult {
        let deadline = Instant::now() + timeout;
        loop {
            match self.try_await(path, waiter) {
                Ok(Some(done)) => return AwaitResult::Ready(done),
                Ok(None) => {}
                Err(msg) if msg.starts_with("deadlock:") => {
                    let cycle = msg
                        .trim_start_matches("deadlock: ")
                        .split(" → ")
                        .map(str::to_string)
                        .collect();
                    return AwaitResult::Deadlock(cycle);
                }
                Err(e) => return AwaitResult::Rejected(e),
            }
            if Instant::now() >= deadline {
                return AwaitResult::Timeout;
            }
            std::thread::sleep(poll);
        }
    }

    pub fn list_locks(&self) -> Vec<LockInfo> {
        let mut out: Vec<_> = self
            .locks
            .iter()
            .map(|(path, e)| LockInfo {
                path: path.clone(),
                block_key: e.block_key.clone(),
                intent: e.intent.clone(),
                expires_unix: e.expires_unix,
            })
            .collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    pub fn list_done(&self) -> Vec<DoneInfo> {
        let mut out: Vec<_> = self.done.values().cloned().collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    /// Orchestrator-only: mark a block session as running.
    pub fn register_session(&mut self, block_key: &str, pane_id: Option<String>) {
        let now = now_unix();
        self.sessions.insert(
            block_key.to_string(),
            SessionInfo {
                block_key: block_key.to_string(),
                status: SessionStatus::Running,
                summary: String::new(),
                pane_id,
                started_unix: now,
                ended_unix: None,
            },
        );
    }

    /// Agent calls when the whole block is finished. Drops remaining claims for this block.
    pub fn session_done(
        &mut self,
        block_key: &str,
        summary: &str,
        ok: bool,
    ) -> SessionDoneResult {
        if !self.sessions.contains_key(block_key) {
            self.register_session(block_key, None);
        }
        let Some(sess) = self.sessions.get_mut(block_key) else {
            return SessionDoneResult::Rejected(format!(
                "unknown session `{block_key}` — register_session first"
            ));
        };
        match sess.status {
            SessionStatus::Done | SessionStatus::Failed => {
                let want = if ok {
                    SessionStatus::Done
                } else {
                    SessionStatus::Failed
                };
                if sess.status == want {
                    return SessionDoneResult::Ok { status: sess.status };
                }
                return SessionDoneResult::Rejected(format!(
                    "session `{block_key}` already {:?}",
                    sess.status
                ));
            }
            SessionStatus::TimedOut => {
                return SessionDoneResult::Rejected(format!(
                    "session `{block_key}` already timed_out"
                ));
            }
            SessionStatus::Running => {}
        }
        let status = if ok {
            SessionStatus::Done
        } else {
            SessionStatus::Failed
        };
        sess.status = status;
        sess.summary = summary.to_string();
        sess.ended_unix = Some(now_unix());
        self.drop_claims_for(block_key);
        self.waiting.remove(block_key);
        SessionDoneResult::Ok { status }
    }

    pub fn mark_session_timed_out(&mut self, block_key: &str) {
        if let Some(sess) = self.sessions.get_mut(block_key) {
            if sess.status == SessionStatus::Running {
                sess.status = SessionStatus::TimedOut;
                sess.ended_unix = Some(now_unix());
                self.drop_claims_for(block_key);
                self.waiting.remove(block_key);
            }
        }
    }

    pub fn list_sessions(&self) -> Vec<SessionInfo> {
        let mut out: Vec<_> = self.sessions.values().cloned().collect();
        out.sort_by(|a, b| a.block_key.cmp(&b.block_key));
        out
    }

    pub fn session(&self, block_key: &str) -> Option<&SessionInfo> {
        self.sessions.get(block_key)
    }

    pub fn session_is_terminal(&self, block_key: &str) -> bool {
        matches!(
            self.sessions.get(block_key).map(|s| s.status),
            Some(SessionStatus::Done | SessionStatus::Failed | SessionStatus::TimedOut)
        )
    }

    pub fn session_succeeded(&self, block_key: &str) -> bool {
        matches!(
            self.sessions.get(block_key).map(|s| s.status),
            Some(SessionStatus::Done)
        )
    }

    fn drop_claims_for(&mut self, block_key: &str) {
        self.locks.retain(|_, e| e.block_key != block_key);
    }

    pub fn expire_stale(&mut self, now_unix: u64) {
        self.locks.retain(|_, e| e.expires_unix > now_unix);
    }

    pub fn detect_deadlock(&self) -> Option<Vec<String>> {
        let mut adj: HashMap<String, HashSet<String>> = HashMap::new();
        for (waiter, paths) in &self.waiting {
            for path in paths {
                if let Some(lock) = self.locks.get(path) {
                    adj.entry(waiter.clone())
                        .or_default()
                        .insert(lock.block_key.clone());
                }
            }
        }
        let nodes: HashSet<String> = adj
            .keys()
            .cloned()
            .chain(adj.values().flat_map(|s| s.iter().cloned()))
            .collect();
        let mut visiting = HashSet::new();
        let mut visited = HashSet::new();
        let mut stack = Vec::new();
        for n in nodes {
            if let Some(cycle) = dfs_cycle(&n, &adj, &mut visiting, &mut visited, &mut stack) {
                return Some(cycle);
            }
        }
        None
    }

    fn clear_wait(&mut self, waiter: &str, path: &str) {
        if let Some(set) = self.waiting.get_mut(waiter) {
            set.remove(path);
            if set.is_empty() {
                self.waiting.remove(waiter);
            }
        }
    }

    fn normalize_allowed(&self, path: &str) -> Result<String, String> {
        let raw = Path::new(path);
        if raw
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        {
            return Err("path must not contain ..".into());
        }
        let normalized = raw
            .components()
            .filter(|c| !matches!(c, Component::CurDir))
            .collect::<PathBuf>();
        if normalized.as_os_str().is_empty() {
            return Err("empty path".into());
        }
        if !self.allow_prefixes.is_empty() {
            let under = self
                .allow_prefixes
                .iter()
                .any(|p| path_under(&normalized, p));
            if under {
                return Ok(normalized.to_string_lossy().replace('\\', "/"));
            }
            for prefix in &self.allow_prefixes {
                let candidate = prefix.join(&normalized);
                if path_under(&candidate, prefix) {
                    return Ok(candidate.to_string_lossy().replace('\\', "/"));
                }
            }
            return Err(format!(
                "path `{path}` not under allowlist ({})",
                self.allow_prefixes
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        Ok(normalized.to_string_lossy().replace('\\', "/"))
    }
}

fn path_under(path: &Path, prefix: &Path) -> bool {
    let mut pi = path.components();
    for pc in prefix.components() {
        match pi.next() {
            Some(c) if c == pc => {}
            _ => return false,
        }
    }
    true
}

fn dfs_cycle(
    node: &str,
    adj: &HashMap<String, HashSet<String>>,
    visiting: &mut HashSet<String>,
    visited: &mut HashSet<String>,
    stack: &mut Vec<String>,
) -> Option<Vec<String>> {
    if visited.contains(node) {
        return None;
    }
    if visiting.contains(node) {
        let idx = stack.iter().position(|s| s == node).unwrap_or(0);
        let mut cycle = stack[idx..].to_vec();
        cycle.push(node.to_string());
        return Some(cycle);
    }
    visiting.insert(node.to_string());
    stack.push(node.to_string());
    if let Some(nexts) = adj.get(node) {
        for n in nexts {
            if let Some(c) = dfs_cycle(n, adj, visiting, visited, stack) {
                return Some(c);
            }
        }
    }
    stack.pop();
    visiting.remove(node);
    visited.insert(node.to_string());
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> LockStore {
        LockStore::new(600, vec![PathBuf::from("output")])
    }

    #[test]
    fn claim_release_and_done() {
        let mut s = store();
        assert_eq!(
            s.claim("output/a.ts", "write", "block:A"),
            ClaimResult::Ok
        );
        assert!(s.release("output/a.ts", "created a", "block:A").is_ok());
        let done = s.list_done();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].summary, "created a");
    }

    #[test]
    fn claim_held_by_other() {
        let mut s = store();
        assert_eq!(s.claim("output/a.ts", "w", "A"), ClaimResult::Ok);
        assert_eq!(
            s.claim("output/a.ts", "w", "B"),
            ClaimResult::Held {
                by: "A".into(),
                intent: "w".into(),
            }
        );
    }

    #[test]
    fn same_block_claim_is_heartbeat() {
        let mut s = store();
        assert_eq!(s.claim("output/a.ts", "w1", "A"), ClaimResult::Ok);
        assert_eq!(s.claim("output/a.ts", "w2", "A"), ClaimResult::Ok);
        assert_eq!(s.list_locks()[0].intent, "w2");
    }

    #[test]
    fn reject_parent_dir_and_outside_allowlist() {
        let mut s = store();
        assert!(matches!(
            s.claim("../etc/passwd", "w", "A"),
            ClaimResult::Rejected(_)
        ));
    }

    #[test]
    fn relative_to_output_dir_is_rewritten() {
        let mut s = store();
        assert_eq!(s.claim("a.ts", "w", "A"), ClaimResult::Ok);
        assert_eq!(s.list_locks()[0].path, "output/a.ts");
    }

    #[test]
    fn ttl_expiry_frees_lock() {
        let mut s = LockStore::new(1, vec![PathBuf::from("output")]);
        assert_eq!(s.claim("output/a.ts", "w", "A"), ClaimResult::Ok);
        s.locks.get_mut("output/a.ts").unwrap().expires_unix = now_unix().saturating_sub(1);
        assert_eq!(s.claim("output/a.ts", "w", "B"), ClaimResult::Ok);
    }

    #[test]
    fn try_await_ready_after_release() {
        let mut s = store();
        assert_eq!(s.claim("output/a.ts", "w", "A"), ClaimResult::Ok);
        assert_eq!(s.try_await("output/a.ts", "B").unwrap(), None);
        s.release("output/a.ts", "done", "A").unwrap();
        let ready = s.try_await("output/a.ts", "B").unwrap().unwrap();
        assert_eq!(ready.summary, "done");
    }

    #[test]
    fn deadlock_detected() {
        let mut s = store();
        assert_eq!(s.claim("output/a.ts", "w", "A"), ClaimResult::Ok);
        assert_eq!(s.claim("output/b.ts", "w", "B"), ClaimResult::Ok);
        assert_eq!(s.try_await("output/b.ts", "A").unwrap(), None);
        let err = s.try_await("output/a.ts", "B").unwrap_err();
        assert!(err.starts_with("deadlock:"), "{err}");
    }

    #[test]
    fn await_timeout() {
        let mut s = store();
        assert_eq!(s.claim("output/a.ts", "w", "A"), ClaimResult::Ok);
        let r = s.await_ready(
            "output/a.ts",
            "B",
            Duration::from_millis(30),
            Duration::from_millis(5),
        );
        assert_eq!(r, AwaitResult::Timeout);
    }

    #[test]
    fn persist_roundtrip() {
        let dir = std::env::temp_dir().join(format!("cb-locks-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("locks.json");
        let mut s = store();
        s.claim("output/a.ts", "w", "A");
        s.register_session("A", None);
        s.save_to(&path).unwrap();
        let loaded = LockStore::load_from(&path, 600, vec![PathBuf::from("output")]).unwrap();
        assert_eq!(loaded.list_locks().len(), 1);
        assert_eq!(loaded.list_sessions().len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn register_and_session_done_drops_claims() {
        let mut s = store();
        s.register_session("A", Some("pane-0".into()));
        assert_eq!(s.claim("output/a.ts", "w", "A"), ClaimResult::Ok);
        assert_eq!(
            s.session_done("A", "built a", true),
            SessionDoneResult::Ok {
                status: SessionStatus::Done
            }
        );
        assert!(s.list_locks().is_empty());
        assert!(s.session_succeeded("A"));
        assert_eq!(
            s.session_done("A", "again", true),
            SessionDoneResult::Ok {
                status: SessionStatus::Done
            }
        );
    }

    #[test]
    fn session_done_unknown_upserts() {
        let mut s = store();
        assert!(matches!(
            s.session_done("X", "late", true),
            SessionDoneResult::Ok {
                status: SessionStatus::Done
            }
        ));
        assert!(s.session_is_terminal("X"));
    }

    #[test]
    fn session_done_failed() {
        let mut s = store();
        s.register_session("A", None);
        assert_eq!(
            s.session_done("A", "blocked", false),
            SessionDoneResult::Ok {
                status: SessionStatus::Failed
            }
        );
        assert!(!s.session_succeeded("A"));
        assert!(s.session_is_terminal("A"));
    }

    #[test]
    fn empty_allowlist_accepts_any_relative() {
        let mut s = LockStore::new(600, vec![]);
        assert_eq!(s.claim("src/foo.rs", "w", "A"), ClaimResult::Ok);
        assert_eq!(s.list_locks()[0].path, "src/foo.rs");
    }
}
