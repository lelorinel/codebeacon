pub mod watcher;

use crate::daemon::watcher::start_watcher;
use crate::indexer::Indexer;
use crate::lsp::pool::LspPool;
use crate::types::FileEntry;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

pub async fn start(repo_root: PathBuf) -> Result<()> {
    tracing::info!("Starting codebeacon daemon for {}", repo_root.display());

    let root_uri = format!("file://{}", repo_root.display());
    let config = crate::config_file::load(&repo_root).unwrap_or_default();
    let mut pool = LspPool::new(&root_uri).with_overrides(config.lsp.overrides.clone());
    let mut indexer = Indexer::new(&repo_root);

    // Re-index files changed while the daemon was offline
    if let Err(e) = indexer.catchup_index(&mut pool) {
        tracing::warn!("catch-up index failed: {e}");
    }

    // Load all entries into memory for O(1) incremental updates
    let mut entry_map: HashMap<PathBuf, FileEntry> = indexer
        .load_all_entries()
        .into_iter()
        .map(|fe| (fe.path.clone(), fe))
        .collect();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<PathBuf>(100);
    let _watcher = start_watcher(repo_root.clone(), tx)?;

    while let Some(changed_file) = rx.recv().await {
        tracing::info!("File changed: {}", changed_file.display());
        let rel = changed_file
            .strip_prefix(&repo_root)
            .unwrap_or(&changed_file)
            .to_path_buf();

        if changed_file.exists() {
            let symbols = indexer.extract_symbols(&changed_file, &mut pool);
            let entry = FileEntry {
                path: rel.clone(),
                symbols,
                depends_on: vec![],
                depended_by: vec![],
            };
            entry_map.insert(rel, entry);
        } else {
            // File was deleted
            entry_map.remove(&rel);
        }

        if let Err(e) = indexer.rebuild_index_from_map(&entry_map) {
            tracing::warn!("Index error: {e}");
        } else if let Err(e) = indexer.save_graph() {
            tracing::warn!("Graph save error: {e}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn daemon_indexes_on_file_change() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();
        fs::create_dir_all(tmp.path().join("src")).unwrap();

        let root = tmp.path().to_path_buf();
        let handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(
                Duration::from_secs(1),
                start(root.clone())
            ).await;
        });

        sleep(Duration::from_millis(200)).await;
        fs::write(tmp.path().join("src/main.rs"), "fn main() {}").unwrap();
        sleep(Duration::from_millis(500)).await;
        handle.abort();

        assert!(tmp.path().join(".codeindex/index.json").exists());
    }
}
