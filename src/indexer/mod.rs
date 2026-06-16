pub mod package;
pub mod writer;

use crate::config::{codeindex_dir, detect_language, Language};
use crate::config_file::CodeIndexConfig;
use crate::graph::DependencyGraph;
use crate::graph::bfs::score_files;
use crate::graph::persistence;
use crate::indexer::package::{group_into_packages, hot_symbols};
use crate::indexer::writer::{write_index, write_package};
use crate::lsp::pool::LspPool;
use crate::lsp::parser::parse_document_symbols;
use crate::types::{FileEntry, PackageSummary, RepoIndex};
use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub static DEFAULT_IGNORE_DIRS: &[&str] = &[
    "node_modules", "vendor", "dist", "build", "out", ".next", ".nuxt",
    "target", "__pycache__", ".venv", "venv", "env", ".tox",
    ".git", ".codeindex", ".idea", ".vscode",
    "Library", "Temp", "Obj", "obj", "Logs", "Build", "Builds",
    "MemoryCaptures", "UserSettings", "bin", ".vs",
];

fn build_ignore(repo_root: &Path) -> ignore::gitignore::Gitignore {
    let mut builder = ignore::gitignore::GitignoreBuilder::new(repo_root);
    let gitignore = repo_root.join(".gitignore");
    if gitignore.exists() {
        let _ = builder.add(gitignore);
    }
    builder.build().unwrap_or_else(|_| ignore::gitignore::Gitignore::empty())
}

pub struct Indexer {
    pub repo_root: PathBuf,
    pub graph: DependencyGraph,
    pub config: CodeIndexConfig,
}

impl Indexer {
    pub fn new(repo_root: &Path) -> Self {
        let codeindex = codeindex_dir(repo_root);
        let graph_path = codeindex.join("graph.bin");
        let graph = persistence::load(&graph_path).unwrap_or_default();
        let config = crate::config_file::load(repo_root).unwrap_or_default();
        Self { repo_root: repo_root.to_path_buf(), graph, config }
    }

    pub fn extract_symbols(&mut self, path: &Path, pool: &mut LspPool) -> Vec<crate::types::SymbolEntry> {
        if let Some(lang) = detect_language(path) {
            if let Some(client) = pool.get_or_start(&lang) {
                let raw = client.document_symbols(path, lang.language_id())
                    .unwrap_or(serde_json::Value::Null);
                let mut symbols = parse_document_symbols(&raw);
                // csharp-ls loads Roslyn in the background; first response is often empty
                if symbols.is_empty() && lang == Language::CSharp {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    if let Some(client2) = pool.get_or_start(&lang) {
                        let raw2 = client2.document_symbols(path, lang.language_id())
                            .unwrap_or(serde_json::Value::Null);
                        symbols = parse_document_symbols(&raw2);
                    }
                }
                return symbols;
            }
        }
        vec![]
    }

    pub fn index_file(&mut self, path: &Path, pool: &mut LspPool) -> Result<()> {
        let symbols = self.extract_symbols(path, pool);
        self.index_file_no_lsp(path, symbols)
    }

    pub fn index_file_no_lsp(&mut self, path: &Path, symbols: Vec<crate::types::SymbolEntry>) -> Result<()> {
        let rel = path.strip_prefix(&self.repo_root).unwrap_or(path);
        let file_entry = FileEntry {
            path: rel.to_path_buf(),
            symbols,
            depends_on: vec![],
            depended_by: vec![],
        };

        let all_files = self.collect_all_file_entries_except(rel);
        let mut all = all_files;
        all.push(file_entry);

        self.rebuild_index(all)?;
        self.save_graph()?;
        Ok(())
    }

    pub fn load_all_entries(&self) -> Vec<FileEntry> {
        let codeindex = codeindex_dir(&self.repo_root);
        let packages_dir = codeindex.join("packages");
        if !packages_dir.exists() { return vec![]; }
        let mut entries = vec![];
        for pkg_file in std::fs::read_dir(&packages_dir).into_iter().flatten().flatten() {
            if let Ok(text) = std::fs::read_to_string(pkg_file.path()) {
                if let Ok(pkg) = serde_json::from_str::<crate::types::PackageDetail>(&text) {
                    entries.extend(pkg.files);
                }
            }
        }
        entries
    }

    pub fn rebuild_index_from_map(&self, map: &HashMap<PathBuf, FileEntry>) -> Result<()> {
        let entries: Vec<FileEntry> = map.values().cloned().collect();
        self.rebuild_index(entries)
    }

    fn collect_all_file_entries_except(&self, exclude: &Path) -> Vec<FileEntry> {
        let codeindex = codeindex_dir(&self.repo_root);
        let packages_dir = codeindex.join("packages");
        if !packages_dir.exists() { return vec![]; }

        let mut entries = vec![];
        for pkg_file in std::fs::read_dir(packages_dir).into_iter().flatten().flatten() {
            if let Ok(text) = std::fs::read_to_string(pkg_file.path()) {
                if let Ok(pkg) = serde_json::from_str::<crate::types::PackageDetail>(&text) {
                    for fe in pkg.files {
                        if fe.path != exclude { entries.push(fe); }
                    }
                }
            }
        }
        entries
    }

    fn rebuild_index(&self, files: Vec<FileEntry>) -> Result<()> {
        let codeindex = codeindex_dir(&self.repo_root);
        let packages = group_into_packages(files);

        let scores = score_files(&self.graph, &[]);
        let mut summaries: Vec<PackageSummary> = packages.iter().map(|p| {
            let avg_score: f32 = if p.files.is_empty() { 0.1 } else {
                p.files.iter().map(|f| {
                    let abs = self.repo_root.join(&f.path);
                    scores.get(&abs).copied().unwrap_or(0.1)
                }).sum::<f32>() / p.files.len() as f32
            };
            PackageSummary {
                name: p.name.clone(),
                purpose: String::new(),
                files: p.files.len(),
                score: avg_score,
            }
        }).collect();

        summaries.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        summaries.retain(|s| s.score >= 0.05);

        let hot = hot_symbols(&packages, 10);
        let repo_name = self.repo_root.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "repo".into());

        let index = RepoIndex {
            repo: repo_name,
            generated_at: Utc::now().to_rfc3339(),
            packages: summaries,
            hot_symbols: hot,
        };

        write_index(&index, &codeindex)?;
        for pkg in &packages { write_package(pkg, &codeindex)?; }
        Ok(())
    }

    /// Re-index files modified after graph.bin was last written (startup catch-up).
    pub fn catchup_index(&mut self, pool: &mut LspPool) -> Result<()> {
        let graph_path = codeindex_dir(&self.repo_root).join("graph.bin");
        let cutoff = std::fs::metadata(&graph_path)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

        use rayon::prelude::*;
        let all_files = self.collect_source_files()?;
        let stale: Vec<PathBuf> = all_files
            .into_par_iter()
            .filter(|p| {
                std::fs::metadata(p)
                    .and_then(|m| m.modified())
                    .map(|mtime| mtime > cutoff)
                    .unwrap_or(false)
            })
            .collect();

        if stale.is_empty() {
            tracing::info!("catch-up: index is fresh, nothing to re-index");
            return Ok(());
        }

        tracing::info!("catch-up: re-indexing {} changed file(s)", stale.len());

        // Build a set of stale relative paths for fast lookup
        let stale_set: std::collections::HashSet<PathBuf> = stale.iter()
            .map(|p| p.strip_prefix(&self.repo_root).unwrap_or(p).to_path_buf())
            .collect();

        // Load existing index entries, excluding stale ones
        let mut all_entries: Vec<FileEntry> = self.load_all_entries()
            .into_iter()
            .filter(|e| !stale_set.contains(&e.path))
            .collect();

        // Re-index all stale files in a single batch
        for path in &stale {
            let symbols = self.extract_symbols(path, pool);
            let rel = path.strip_prefix(&self.repo_root).unwrap_or(path);
            all_entries.push(FileEntry {
                path: rel.to_path_buf(),
                symbols,
                depends_on: vec![],
                depended_by: vec![],
            });
        }

        self.rebuild_index(all_entries)?;
        self.save_graph()?;
        Ok(())
    }

    pub fn save_graph(&self) -> Result<()> {
        let path = codeindex_dir(&self.repo_root).join("graph.bin");
        persistence::save(&self.graph, &path)
    }

    fn pool_index_language_group(
        &self,
        lang: &crate::config::Language,
        files: Vec<PathBuf>,
        concurrency: usize,
        overrides: &std::collections::HashMap<String, String>,
    ) -> Vec<(PathBuf, Vec<crate::types::SymbolEntry>)> {
        use std::sync::{Arc, Mutex};
        use std::sync::mpsc;
        use crate::lsp::client::LspClient;
        use crate::lsp::parser::parse_document_symbols;

        let workers = concurrency.max(1);
        let binary = overrides.get(lang.language_id())
            .map(String::as_str)
            .unwrap_or(lang.lsp_binary())
            .to_string();
        let args: Vec<String> = lang.lsp_args().iter().map(|s| s.to_string()).collect();
        let root_uri = crate::lsp::client::path_to_uri(&self.repo_root);
        let lang_id = lang.language_id().to_string();

        // Check if LSP binary is available; if not, return empty symbols for all files
        if !crate::lsp::pool::is_binary_available(&binary) {
            return files.into_iter().map(|p| (p, vec![])).collect();
        }

        let (tx_work, rx_work) = mpsc::sync_channel::<PathBuf>(workers * 8);
        let (tx_result, rx_result) = mpsc::sync_channel::<(PathBuf, Vec<crate::types::SymbolEntry>)>(workers * 8);
        let rx_work = Arc::new(Mutex::new(rx_work));

        // Spawn W worker threads
        let handles: Vec<_> = (0..workers).map(|_| {
            let rx = Arc::clone(&rx_work);
            let tx = tx_result.clone();
            let binary = binary.clone();
            let args = args.clone();
            let root_uri = root_uri.clone();
            let lang_id = lang_id.clone();
            std::thread::spawn(move || {
                let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
                let mut client = match LspClient::start(&binary, &arg_refs, &root_uri) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("Worker failed to start {binary}: {e}");
                        return;
                    }
                };
                loop {
                    let path = match rx.lock().unwrap().recv() {
                        Ok(p) => p,
                        Err(_) => break, // no more work
                    };
                    let raw = client.document_symbols(&path, &lang_id)
                        .unwrap_or(serde_json::Value::Null);
                    let symbols = parse_document_symbols(&raw);
                    if tx.send((path, symbols)).is_err() {
                        break;
                    }
                }
            })
        }).collect();
        drop(tx_result); // drop extra sender so rx_result knows when workers are done

        // Feed work to the pool
        for file in files {
            if tx_work.send(file).is_err() {
                break; // workers died
            }
        }
        drop(tx_work); // signal workers no more work

        // Collect results
        let mut results = Vec::new();
        while let Ok(r) = rx_result.recv() {
            results.push(r);
        }

        // Wait for all worker threads
        for handle in handles {
            let _ = handle.join();
        }

        results
    }

    pub fn full_index(&mut self, _pool: &mut LspPool) -> Result<()> {
        let files = self.collect_source_files()?;
        tracing::info!("Indexing {} files with LSP concurrency={}", files.len(), self.config.lsp_concurrency);

        // Group files by language key (None = no LSP)
        let mut by_lang: std::collections::HashMap<Option<String>, Vec<PathBuf>> = std::collections::HashMap::new();
        for file in files {
            let lang_key = detect_language(&file).map(|l| l.language_id().to_owned());
            by_lang.entry(lang_key).or_default().push(file);
        }
        for files in by_lang.values_mut() { files.sort(); }

        let total = by_lang.values().map(|v| v.len()).sum::<usize>();
        let mut all_entries: Vec<FileEntry> = Vec::with_capacity(total);

        for (lang_key, lang_files) in by_lang {
            let results: Vec<(PathBuf, Vec<crate::types::SymbolEntry>)> = if let Some(ref key) = lang_key {
                if let Some(lang) = crate::config::language_from_id(key) {
                    let concurrency = self.config.lsp_concurrency;
                    let overrides = self.config.lsp.overrides.clone();
                    self.pool_index_language_group(&lang, lang_files, concurrency, &overrides)
                } else {
                    lang_files.into_iter().map(|p| (p, vec![])).collect()
                }
            } else {
                lang_files.into_iter().map(|p| (p, vec![])).collect()
            };

            for (file, symbols) in results {
                let rel = file.strip_prefix(&self.repo_root).unwrap_or(&file).to_path_buf();
                all_entries.push(FileEntry {
                    path: rel,
                    symbols,
                    depends_on: vec![],
                    depended_by: vec![],
                });
            }

            tracing::debug!("Indexed {}/{} files so far", all_entries.len(), total);
        }

        self.rebuild_index(all_entries)?;
        self.save_graph()?;
        Ok(())
    }

    fn collect_source_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = vec![];
        let codeindex = codeindex_dir(&self.repo_root);
        let ignore = build_ignore(&self.repo_root);

        // Build a secondary gitignore for user-configured glob patterns.
        let glob_ignore = {
            let mut builder = ignore::gitignore::GitignoreBuilder::new(&self.repo_root);
            for glob in &self.config.ignore_globs {
                let _ = builder.add_line(None, glob);
            }
            builder.build().unwrap_or_else(|_| ignore::gitignore::Gitignore::empty())
        };

        // Lowercase languages filter (empty = all languages allowed).
        let lang_filter: Vec<String> = self.config.languages
            .iter()
            .map(|l| l.to_lowercase())
            .collect();

        // Helper to convert Language enum to lowercase name for filter matching.
        let lang_name = |lang: &Language| -> &'static str {
            match lang {
                Language::Rust => "rust",
                Language::Go => "go",
                Language::Python => "python",
                Language::TypeScript => "typescript",
                Language::CSharp => "csharp",
            }
        };

        for entry in walkdir::WalkDir::new(&self.repo_root)
            .into_iter()
            .filter_entry(|e| {
                // prune entire ignored directories so we never descend into them
                if e.file_type().is_dir() {
                    let name = e.file_name().to_string_lossy();
                    return !DEFAULT_IGNORE_DIRS.contains(&name.as_ref());
                }
                true
            })
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path().to_path_buf();
            if path.starts_with(&codeindex) { continue; }
            if ignore.matched(&path, false).is_ignore() { continue; }

            // Check extra_ignore_dirs: skip files under any matching ancestor directory.
            if !self.config.extra_ignore_dirs.is_empty() {
                let in_extra_dir = path.parent().map(|parent| {
                    parent.components().any(|c| {
                        let s = c.as_os_str().to_string_lossy();
                        self.config.extra_ignore_dirs.iter().any(|d| d == s.as_ref())
                    })
                }).unwrap_or(false);
                if in_extra_dir { continue; }
            }

            // Check user glob patterns.
            if glob_ignore.matched(&path, false).is_ignore() { continue; }

            if let Some(lang) = detect_language(&path) {
                // Apply language filter if specified.
                if !lang_filter.is_empty() {
                    if !lang_filter.iter().any(|l| l == lang_name(&lang)) { continue; }
                }
                files.push(path);
            }
        }
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn index_file_writes_codeindex() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();
        fs::create_dir_all(repo_root.join("src")).unwrap();
        let file = repo_root.join("src/main.rs");
        fs::write(&file, "fn main() {}").unwrap();
        fs::create_dir(repo_root.join(".git")).unwrap();

        let mut indexer = Indexer::new(repo_root);
        indexer.index_file_no_lsp(&file, vec![]).unwrap();

        assert!(repo_root.join(".codeindex/index.json").exists());
    }
}
