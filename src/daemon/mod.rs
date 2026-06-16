pub mod watcher;

use crate::daemon::watcher::start_watcher;
use crate::indexer::Indexer;
use crate::lsp::pool::LspPool;
use anyhow::Result;
use std::path::PathBuf;

pub async fn start(repo_root: PathBuf) -> Result<()> {
    tracing::info!("Starting LCP daemon for {}", repo_root.display());

    let root_uri = format!("file://{}", repo_root.display());
    let mut pool = LspPool::new(&root_uri);
    let mut indexer = Indexer::new(&repo_root);

    let (tx, mut rx) = tokio::sync::mpsc::channel::<PathBuf>(100);
    let _watcher = start_watcher(repo_root.clone(), tx)?;

    while let Some(changed_file) = rx.recv().await {
        tracing::info!("File changed: {}", changed_file.display());
        if let Err(e) = indexer.index_file(&changed_file, &mut pool) {
            tracing::warn!("Index error: {e}");
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
