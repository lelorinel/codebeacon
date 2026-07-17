//! File-backed shared lock store with flock (process-safe).

use crate::locks::store::{
    ClaimResult, DoneInfo, LockInfo, LockStore, SessionDoneResult, SessionInfo,
};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const LOCKS_FILE: &str = "apply-locks.json";
pub const LOCKS_SUBDIR: &str = "locks";

/// Stable locks path under `.codeindex/locks/apply-locks.json`.
pub fn stable_locks_path(project_root: &Path) -> PathBuf {
    crate::config::codeindex_dir(project_root)
        .join(LOCKS_SUBDIR)
        .join(LOCKS_FILE)
}

/// Reset the stable locks store (used by `run-plan` at start).
pub fn reset_stable_locks(project_root: &Path) -> Result<PathBuf, String> {
    let path = stable_locks_path(project_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let _ = fs::remove_file(&path);
    let _ = fs::remove_file(path.with_extension("json.lock"));
    Ok(path)
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

#[derive(Clone)]
pub struct SharedLockStore {
    path: PathBuf,
    ttl_secs: u64,
    allow_prefixes: Vec<PathBuf>,
    inner: Arc<Mutex<()>>,
}

impl SharedLockStore {
    pub fn open(path: PathBuf, ttl_secs: u64, allow_prefixes: Vec<PathBuf>) -> Self {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if !path.exists() {
            let empty = LockStore::new(ttl_secs, allow_prefixes.clone());
            let _ = empty.save_to(&path);
        }
        Self {
            path,
            ttl_secs,
            allow_prefixes,
            inner: Arc::new(Mutex::new(())),
        }
    }

    /// Open the stable project store under `.codeindex/locks/`.
    pub fn open_for_project(
        project_root: &Path,
        ttl_secs: u64,
        allow_prefixes: Vec<PathBuf>,
    ) -> Self {
        let prefixes = if allow_prefixes.is_empty() {
            // Empty allowlist = accept any relative path under the workspace.
            vec![]
        } else {
            allow_prefixes
        };
        Self::open(stable_locks_path(project_root), ttl_secs, prefixes)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn with_store<R>(&self, f: impl FnOnce(&mut LockStore) -> R) -> Result<R, String> {
        let _guard = self
            .inner
            .lock()
            .map_err(|_| "lock mutex poisoned".to_string())?;
        let lock_path = self.path.with_extension("json.lock");
        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let lock_file = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|e| format!("open lock file: {e}"))?;
        flock_exclusive(&lock_file)?;
        let mut store =
            LockStore::load_from(&self.path, self.ttl_secs, self.allow_prefixes.clone())?;
        let out = f(&mut store);
        store.save_to(&self.path)?;
        flock_unlock(&lock_file)?;
        drop(lock_file);
        Ok(out)
    }

    pub fn claim(&self, path: &str, intent: &str, block_key: &str) -> Result<ClaimResult, String> {
        self.with_store(|s| s.claim(path, intent, block_key))
    }

    pub fn release(&self, path: &str, summary: &str, block_key: &str) -> Result<(), String> {
        self.with_store(|s| s.release(path, summary, block_key))?
    }

    pub fn try_await(&self, path: &str, waiter: &str) -> Result<Option<DoneInfo>, String> {
        self.with_store(|s| s.try_await(path, waiter))?
    }

    pub fn list_locks(&self) -> Result<Vec<LockInfo>, String> {
        self.with_store(|s| s.list_locks())
    }

    pub fn list_done(&self) -> Result<Vec<DoneInfo>, String> {
        self.with_store(|s| s.list_done())
    }

    pub fn register_session(
        &self,
        block_key: &str,
        pane_id: Option<String>,
    ) -> Result<(), String> {
        self.with_store(|s| s.register_session(block_key, pane_id))
    }

    pub fn session_done(
        &self,
        block_key: &str,
        summary: &str,
        ok: bool,
    ) -> Result<SessionDoneResult, String> {
        self.with_store(|s| s.session_done(block_key, summary, ok))
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionInfo>, String> {
        self.with_store(|s| s.list_sessions())
    }

    pub fn session_is_terminal(&self, block_key: &str) -> Result<bool, String> {
        self.with_store(|s| s.session_is_terminal(block_key))
    }

    pub fn session_succeeded(&self, block_key: &str) -> Result<bool, String> {
        self.with_store(|s| s.session_succeeded(block_key))
    }

    pub fn mark_session_timed_out(&self, block_key: &str) -> Result<(), String> {
        self.with_store(|s| s.mark_session_timed_out(block_key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locks::store::ClaimResult;
    use tempfile::TempDir;

    #[test]
    fn shared_claim_across_handles() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("locks.json");
        let a = SharedLockStore::open(path.clone(), 600, vec![PathBuf::from("src")]);
        let b = SharedLockStore::open(path, 600, vec![PathBuf::from("src")]);
        assert_eq!(a.claim("src/a.rs", "w", "A").unwrap(), ClaimResult::Ok);
        assert!(matches!(
            b.claim("src/a.rs", "w", "B").unwrap(),
            ClaimResult::Held { .. }
        ));
        a.release("src/a.rs", "done", "A").unwrap();
        assert_eq!(b.claim("src/a.rs", "w", "B").unwrap(), ClaimResult::Ok);
    }
}
