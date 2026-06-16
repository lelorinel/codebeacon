pub mod package;
pub mod writer;

use crate::config::{codeindex_dir, detect_language};
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
}

impl Indexer {
    pub fn new(repo_root: &Path) -> Self {
        let codeindex = codeindex_dir(repo_root);
        let graph_path = codeindex.join("graph.bin");
        let graph = persistence::load(&graph_path).unwrap_or_default();
        Self { repo_root: repo_root.to_path_buf(), graph }
    }

    pub fn index_file(&mut self, path: &Path, pool: &mut LspPool) -> Result<()> {
        let symbols = if let Some(lang) = detect_language(path) {
            if let Some(client) = pool.get_or_start(&lang) {
                let raw = client.document_symbols(path).unwrap_or(serde_json::Value::Null);
                parse_document_symbols(&raw)
            } else { vec![] }
        } else { vec![] };

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

        let stale: Vec<PathBuf> = self.collect_source_files()?
            .into_iter()
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
        for path in stale {
            if let Err(e) = self.index_file(&path, pool) {
                tracing::warn!("catch-up error for {}: {e}", path.display());
            }
        }
        Ok(())
    }

    fn save_graph(&self) -> Result<()> {
        let path = codeindex_dir(&self.repo_root).join("graph.bin");
        persistence::save(&self.graph, &path)
    }

    pub fn full_index(&mut self, pool: &mut LspPool) -> Result<()> {
        let files = self.collect_source_files()?;
        tracing::info!("Indexing {} files", files.len());
        for file in files {
            if let Err(e) = self.index_file(&file, pool) {
                tracing::warn!("Failed to index {}: {e}", file.display());
            }
        }
        Ok(())
    }

    fn collect_source_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = vec![];
        let codeindex = codeindex_dir(&self.repo_root);
        let ignore = build_ignore(&self.repo_root);

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
            if detect_language(&path).is_some() {
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
