use anyhow::Result;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::Sender;

const DEBOUNCE_MS: u64 = 100;

pub fn start_watcher(root: PathBuf, tx: Sender<PathBuf>) -> Result<RecommendedWatcher> {
    let (notify_tx, notify_rx) = mpsc::channel::<notify::Result<Event>>();

    let mut watcher = RecommendedWatcher::new(notify_tx, Config::default())?;
    watcher.watch(&root, RecursiveMode::Recursive)?;

    thread::spawn(move || {
        let mut pending: HashMap<PathBuf, Instant> = HashMap::new();

        loop {
            while let Ok(Ok(event)) = notify_rx.try_recv() {
                if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                    for path in event.paths {
                        pending.insert(path, Instant::now());
                    }
                }
            }

            let now = Instant::now();
            let ready: Vec<PathBuf> = pending.iter()
                .filter(|(_, t)| now.duration_since(**t) >= Duration::from_millis(DEBOUNCE_MS))
                .map(|(p, _)| p.clone())
                .collect();

            for path in ready {
                pending.remove(&path);
                let _ = tx.blocking_send(path);
            }

            thread::sleep(Duration::from_millis(20));
        }
    });

    Ok(watcher)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn detects_file_change() {
        let tmp = TempDir::new().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let root = tmp.path().to_path_buf();
        let _watcher = start_watcher(root.clone(), tx).unwrap();

        let file = root.join("test.rs");
        fs::write(&file, "fn foo() {}").unwrap();

        sleep(Duration::from_millis(300)).await;

        let event = rx.try_recv();
        assert!(event.is_ok(), "expected file change event");
        assert_eq!(event.unwrap(), file);
    }
}
