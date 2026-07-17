//! Multi-agent path locks (file-backed JSON + flock).

mod shared;
mod store;

pub use shared::{
    reset_stable_locks, stable_locks_path, SharedLockStore, LOCKS_FILE, LOCKS_SUBDIR,
};
pub use store::{
    AwaitResult, ClaimResult, DoneInfo, LockInfo, LockStore, SessionDoneResult, SessionInfo,
    SessionStatus,
};
