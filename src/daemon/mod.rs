pub mod watcher;

use crate::config::detect_language;
use crate::daemon::watcher::start_watcher;
use crate::indexer::Indexer;
use crate::lsp::pool::LspPool;
use crate::types::FileEntry;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Background LSP enrichment: reads the existing heuristic index, uses LSP
/// `definition` calls to discover additional dependency edges, and syncs them
/// back to the FileEntry JSON files.
///
/// Silently skips any language whose LSP binary is unavailable.
pub fn lsp_enrich(repo_root: &Path, lsp_overrides: HashMap<String, String>) -> Result<()> {
    let mut indexer = Indexer::new(repo_root);
    let entries = indexer.load_all_entries();

    if entries.is_empty() {
        return Ok(());
    }

    let known: HashSet<PathBuf> = entries.iter().map(|e| e.path.clone()).collect();
    let root_uri = format!("file://{}", repo_root.display());
    let mut pool = LspPool::new(&root_uri).with_overrides(lsp_overrides);
    let mut enriched = false;

    for entry in &entries {
        let abs = repo_root.join(&entry.path);
        let lang = match detect_language(&abs) {
            Some(l) => l,
            None => continue,
        };

        // Skip silently if the LSP binary is not installed
        if !crate::lsp::pool::is_binary_available(lang.lsp_binary()) {
            continue;
        }

        let raw = indexer.extract_file(&abs).imports;
        if raw.is_empty() {
            continue;
        }

        for imp in &raw {
            let lsp_line = imp.line.saturating_sub(1); // convert 1-based → 0-based
            let client = match pool.get_or_start(&lang) {
                Some(c) => c,
                None => continue,
            };

            match client.definition(&abs, lsp_line, imp.character) {
                Ok(result) => {
                    if let Some((def_path, _)) = crate::lsp::parser::parse_definition(&result) {
                        if let Ok(rel) = def_path.strip_prefix(repo_root) {
                            let rel_buf = rel.to_path_buf();
                            if known.contains(&rel_buf)
                                && !indexer.graph.has_dependency(&entry.path, &rel_buf)
                            {
                                indexer.graph.add_dependency(&entry.path, &rel_buf);
                                enriched = true;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!(
                        "LSP definition lookup failed for {}: {e}",
                        abs.display()
                    );
                }
            }
        }
    }

    if enriched {
        tracing::info!("LSP enrichment added new dependency edges");
        indexer.save_graph()?;
        indexer.sync_entries_from_graph()?;
    } else {
        tracing::info!("LSP enrichment: no new edges found");
    }

    Ok(())
}

pub async fn start(repo_root: PathBuf, docs_root: Option<PathBuf>) -> Result<()> {
    tracing::info!("Starting codebeacon daemon for {}", repo_root.display());

    let mut indexer = Indexer::with_docs(&repo_root, docs_root.as_deref());
    // Prefer explicit docs_root from serve even if config also set
    if docs_root.is_some() {
        indexer.docs_root = docs_root.clone();
    }

    // Re-index files changed while the daemon was offline
    if let Err(e) = indexer.catchup_index() {
        tracing::warn!("catch-up index failed: {e}");
    }

    if let Err(e) = indexer.reindex_docs_if_enabled(true) {
        tracing::warn!("docs reindex on start failed: {e}");
    }

    // Faz 2: LSP background enrichment (runs once after heuristic index is ready)
    let timeout_secs = indexer.config.lsp_enrich_timeout_secs;
    if timeout_secs > 0 {
        let root_clone = repo_root.clone();
        let lsp_overrides = indexer.config.lsp.overrides.clone();
        tokio::spawn(async move {
            let _ = tokio::time::timeout(
                tokio::time::Duration::from_secs(timeout_secs),
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = lsp_enrich(&root_clone, lsp_overrides) {
                        tracing::warn!("LSP enrichment failed: {e}");
                    }
                }),
            )
            .await;
            tracing::info!("LSP enrichment phase done (timeout={}s)", timeout_secs);
        });
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

        let is_doc = matches!(
            changed_file.extension().and_then(|e| e.to_str()),
            Some("md") | Some("mdx")
        ) && indexer
            .docs_root
            .as_ref()
            .map(|d| changed_file.starts_with(d))
            .unwrap_or(false);

        if is_doc {
            match crate::docs::reindex_docs(
                &repo_root,
                indexer.docs_root.as_ref().unwrap(),
                true,
            ) {
                Ok(mut idx) => {
                    let rel_s = rel.to_string_lossy().replace('\\', "/");
                    crate::docs::clear_stale_for_docs_file(&mut idx, &rel_s);
                    if let Err(e) =
                        crate::docs::write_docs_index(&idx, &crate::config::codeindex_dir(&repo_root))
                    {
                        tracing::warn!("docs write error: {e}");
                    }
                }
                Err(e) => tracing::warn!("docs reindex error: {e}"),
            }
            continue;
        }

        if detect_language(&changed_file).is_none() && changed_file.exists() {
            // Non-code, non-docs under watch — ignore for code index
            continue;
        }

        if changed_file.exists() {
            let extracted = indexer.extract_file(&changed_file);
            let entry = FileEntry {
                path: rel.clone(),
                symbols: extracted.symbols,
                depends_on: vec![],
                depended_by: vec![],
            };
            entry_map.insert(rel.clone(), entry);
        } else {
            // File was deleted
            entry_map.remove(&rel);
        }

        if let Err(e) = indexer.rebuild_index_from_map(&entry_map) {
            tracing::warn!("Index error: {e}");
        } else if let Err(e) = indexer.save_graph() {
            tracing::warn!("Graph save error: {e}");
        }

        if indexer.docs_root.is_some() {
            if let Err(e) = crate::docs::mark_stale_for_code_path(&repo_root, &rel) {
                tracing::warn!("docs stale mark error: {e}");
            }
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

    #[test]
    fn lsp_enrich_returns_ok_without_binary() {
        // lsp_enrich must complete without panic/error even when no LSP binary
        // is available. In that case it simply skips all files silently.
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();
        fs::create_dir_all(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src/lib.rs"), "pub mod auth;\n").unwrap();
        fs::write(tmp.path().join("src/auth.rs"), "pub fn login() {}").unwrap();

        // Build the initial heuristic index
        let mut indexer = Indexer::new(tmp.path());
        indexer.full_index().unwrap();

        // lsp_enrich should complete without error (LSP binary may or may not exist)
        let result = lsp_enrich(tmp.path(), HashMap::new());
        assert!(result.is_ok(), "lsp_enrich must not fail: {:?}", result);
    }

    #[tokio::test]
    async fn daemon_indexes_on_file_change() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();
        fs::create_dir_all(tmp.path().join("src")).unwrap();

        let root = tmp.path().to_path_buf();
        let handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(
                Duration::from_secs(1),
                start(root.clone(), None)
            ).await;
        });

        sleep(Duration::from_millis(200)).await;
        fs::write(tmp.path().join("src/main.rs"), "fn main() {}").unwrap();
        sleep(Duration::from_millis(500)).await;
        handle.abort();

        assert!(tmp.path().join(".codeindex/index.json").exists());
    }
}
