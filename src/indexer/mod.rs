pub mod package;
pub mod writer;

use crate::config::{codeindex_dir, detect_language, Language};
use crate::config_file::CodeIndexConfig;
use crate::extract::{extract_file, ExtractResult};
use crate::graph::DependencyGraph;
use crate::graph::bfs::score_files;
use crate::graph::persistence;
use crate::imports::{resolve_imports, RawImport};
use crate::indexer::package::{group_into_packages, hot_symbols};
use crate::indexer::writer::{write_index, write_package};
use crate::types::{FileEntry, PackageDetail, PackageSummary, RepoIndex};
use anyhow::Result;
use chrono::Utc;
use std::collections::{HashMap, HashSet};
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

    pub fn extract_file(&self, path: &Path) -> ExtractResult {
        extract_file(path, &self.config.extractor)
    }

    pub fn extract_symbols(&self, path: &Path) -> Vec<crate::types::SymbolEntry> {
        self.extract_file(path).symbols
    }

    pub fn index_file(&mut self, path: &Path) -> Result<()> {
        let symbols = self.extract_symbols(path);
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

    pub fn rebuild_index_from_map(&mut self, map: &HashMap<PathBuf, FileEntry>) -> Result<()> {
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

    fn build_package_summaries(
        repo_root: &Path,
        graph: &DependencyGraph,
        packages: &[PackageDetail],
    ) -> Result<Vec<PackageSummary>> {
        let cfg = crate::config_file::load(repo_root).unwrap_or_default();
        let conv_store = if cfg.intelligence.conventions_enabled {
            let store = crate::intelligence::build_conventions_store(packages, repo_root);
            let codeindex = codeindex_dir(repo_root);
            crate::intelligence::write_conventions(&store, &codeindex)?;
            store
        } else {
            crate::intelligence::ConventionsStore::default()
        };

        let scores = score_files(graph, &[]);
        let mut summaries: Vec<PackageSummary> = packages
            .iter()
            .map(|p| {
                let avg_score: f32 = if p.files.is_empty() {
                    0.1
                } else {
                    p.files
                        .iter()
                        .map(|f| {
                            let abs = repo_root.join(&f.path);
                            scores
                                .get(&abs)
                                .or_else(|| scores.get(&f.path))
                                .copied()
                                .unwrap_or(0.1)
                        })
                        .sum::<f32>()
                        / p.files.len() as f32
                };
                PackageSummary {
                    name: p.name.clone(),
                    purpose: crate::intelligence::purpose_for_package(
                        p,
                        conv_store.packages.get(&p.name),
                    ),
                    files: p.files.len(),
                    score: avg_score,
                }
            })
            .collect();

        summaries.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        summaries.retain(|s| s.score >= 0.05);
        Ok(summaries)
    }

    fn rebuild_index(&mut self, mut files: Vec<FileEntry>) -> Result<()> {
        self.resolve_dependencies(&mut files);
        let codeindex = codeindex_dir(&self.repo_root);
        let packages = group_into_packages(files);

        let summaries = Self::build_package_summaries(&self.repo_root, &self.graph, &packages)?;

        let repo_name = self.repo_root.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "repo".into());

        Self::write_index_artifacts(&codeindex, &packages, summaries, repo_name)?;
        Ok(())
    }

    fn write_index_artifacts(
        codeindex: &Path,
        packages: &[PackageDetail],
        summaries: Vec<PackageSummary>,
        repo_name: String,
    ) -> Result<()> {
        let usage = crate::compact::read_usage(codeindex).unwrap_or_default();
        let mut hot = hot_symbols(packages, 20);
        hot = crate::compact::boost_hot_symbols(hot, &usage, 10);

        let prev_rev = crate::compact::read_dict(codeindex)?
            .map(|d| d.rev)
            .unwrap_or(0);
        let dict = crate::compact::build_dict_from_packages(packages, prev_rev);
        crate::compact::write_dict(&dict, codeindex)?;

        let index = RepoIndex {
            repo: repo_name,
            generated_at: Utc::now().to_rfc3339(),
            packages: summaries,
            hot_symbols: hot,
        };
        write_index(&index, codeindex)?;
        for pkg in packages {
            write_package(pkg, codeindex)?;
        }
        Ok(())
    }

    /// Re-index files modified after graph.bin was last written (startup catch-up).
    pub fn catchup_index(&mut self) -> Result<()> {
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
            let extracted = self.extract_file(path);
            let rel = path.strip_prefix(&self.repo_root).unwrap_or(path);
            all_entries.push(FileEntry {
                path: rel.to_path_buf(),
                symbols: extracted.symbols,
                depends_on: vec![],
                depended_by: vec![],
            });
        }

        self.rebuild_index(all_entries)?;
        self.save_graph()?;
        Ok(())
    }

    /// Populate `depends_on` / `depended_by` on each `FileEntry` and rebuild
    /// `self.graph` from scratch using heuristic import resolution.
    fn resolve_dependencies(&mut self, entries: &mut Vec<FileEntry>) {
        let known: HashSet<PathBuf> = entries.iter().map(|e| e.path.clone()).collect();

        for entry in entries.iter_mut() {
            let abs = self.repo_root.join(&entry.path);
            let lang = match detect_language(&abs) {
                Some(l) => l,
                None => continue,
            };
            let raw = self.extract_imports_for_file(&abs);
            let resolved = resolve_imports(&self.repo_root, &entry.path, &raw, &lang, &known);
            entry.depends_on = resolved
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
        }

        // Pass 2: build reverse map for depended_by.
        // Map from target path string → list of dependents.
        let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
        for entry in entries.iter() {
            for dep in &entry.depends_on {
                reverse
                    .entry(dep.clone())
                    .or_default()
                    .push(entry.path.to_string_lossy().into_owned());
            }
        }
        for entry in entries.iter_mut() {
            let key = entry.path.to_string_lossy().into_owned();
            entry.depended_by = reverse.get(&key).cloned().unwrap_or_default();
        }

        // Pass 3: rebuild DependencyGraph from the resolved edges.
        let mut graph = DependencyGraph::new();
        for entry in entries.iter() {
            for dep in &entry.depends_on {
                graph.add_dependency(&entry.path, &PathBuf::from(dep));
            }
        }
        self.graph = graph;
    }

    /// Rebuild `FileEntry.depends_on` / `depended_by` fields from `self.graph`
    /// (which may contain LSP-enriched edges) and rewrite package JSON files.
    /// Does NOT re-run import extraction or replace the graph.
    pub fn sync_entries_from_graph(&self) -> Result<()> {
        let mut entries = self.load_all_entries();

        // Rebuild depends_on from graph forward edges
        for entry in entries.iter_mut() {
            let mut deps: Vec<String> = self.graph
                .neighbors(&entry.path)
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            deps.sort();
            deps.dedup();
            entry.depends_on = deps;
        }

        // Rebuild depended_by from reverse
        let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
        for entry in &entries {
            for dep in &entry.depends_on {
                reverse
                    .entry(dep.clone())
                    .or_default()
                    .push(entry.path.to_string_lossy().into_owned());
            }
        }
        for entry in entries.iter_mut() {
            let key = entry.path.to_string_lossy().into_owned();
            let mut rev = reverse.get(&key).cloned().unwrap_or_default();
            rev.sort();
            rev.dedup();
            entry.depended_by = rev;
        }

        let codeindex = codeindex_dir(&self.repo_root);
        let packages = group_into_packages(entries);

        let summaries =
            Self::build_package_summaries(&self.repo_root, &self.graph, &packages)?;
        let repo_name = self.repo_root.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "repo".into());
        Self::write_index_artifacts(&codeindex, &packages, summaries, repo_name)
    }

    pub fn save_graph(&self) -> Result<()> {
        let path = codeindex_dir(&self.repo_root).join("graph.bin");
        persistence::save(&self.graph, &path)
    }

    pub fn full_index(&mut self) -> Result<()> {
        let files = self.collect_source_files()?;
        tracing::info!("Indexing {} files", files.len());

        use rayon::prelude::*;
        let all_entries: Vec<FileEntry> = files
            .par_iter()
            .map(|path| {
                let extracted = extract_file(path, &self.config.extractor);
                let rel = path.strip_prefix(&self.repo_root).unwrap_or(path).to_path_buf();
                FileEntry {
                    path: rel,
                    symbols: extracted.symbols,
                    depends_on: vec![],
                    depended_by: vec![],
                }
            })
            .collect();

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

    fn extract_imports_for_file(&self, abs: &Path) -> Vec<RawImport> {
        self.extract_file(abs).imports
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

    fn make_rust_repo(root: &Path) {
        fs::create_dir(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "pub mod auth;\npub mod db;\n").unwrap();
        fs::write(root.join("src/auth.rs"), "pub fn login() {}").unwrap();
        fs::write(root.join("src/db.rs"), "pub struct User {}").unwrap();
    }

    #[test]
    fn full_index_populates_depends_on() {
        let tmp = TempDir::new().unwrap();
        make_rust_repo(tmp.path());

        let mut indexer = Indexer::new(tmp.path());
        indexer.full_index().unwrap();

        let entries = indexer.load_all_entries();
        let lib = entries.iter().find(|e| e.path == PathBuf::from("src/lib.rs")).unwrap();
        assert!(
            lib.depends_on.contains(&"src/auth.rs".to_string()),
            "lib.rs should depend on auth.rs, got: {:?}", lib.depends_on
        );
        assert!(
            lib.depends_on.contains(&"src/db.rs".to_string()),
            "lib.rs should depend on db.rs, got: {:?}", lib.depends_on
        );
    }

    #[test]
    fn full_index_populates_depended_by() {
        let tmp = TempDir::new().unwrap();
        make_rust_repo(tmp.path());

        let mut indexer = Indexer::new(tmp.path());
        indexer.full_index().unwrap();

        let entries = indexer.load_all_entries();
        let auth = entries.iter().find(|e| e.path == PathBuf::from("src/auth.rs")).unwrap();
        assert!(
            auth.depended_by.contains(&"src/lib.rs".to_string()),
            "auth.rs should be depended on by lib.rs, got: {:?}", auth.depended_by
        );
    }

    #[test]
    fn full_index_populates_graph_edges() {
        let tmp = TempDir::new().unwrap();
        make_rust_repo(tmp.path());

        let mut indexer = Indexer::new(tmp.path());
        indexer.full_index().unwrap();

        assert!(
            indexer.graph.has_dependency(
                &PathBuf::from("src/lib.rs"),
                &PathBuf::from("src/auth.rs"),
            ),
            "graph should have lib.rs → auth.rs edge"
        );
        assert!(
            indexer.graph.has_dependency(
                &PathBuf::from("src/lib.rs"),
                &PathBuf::from("src/db.rs"),
            ),
            "graph should have lib.rs → db.rs edge"
        );
    }

    #[test]
    fn graph_reverse_neighbors_returns_dependents() {
        let tmp = TempDir::new().unwrap();
        make_rust_repo(tmp.path());

        let mut indexer = Indexer::new(tmp.path());
        indexer.full_index().unwrap();

        let dependents = indexer.graph.reverse_neighbors(&PathBuf::from("src/auth.rs"));
        assert!(
            dependents.contains(&PathBuf::from("src/lib.rs")),
            "reverse neighbors of auth.rs should include lib.rs, got: {:?}", dependents
        );
    }

    #[test]
    fn sync_entries_from_graph_persists_manually_added_edge() {
        let tmp = TempDir::new().unwrap();
        // lib.rs has no imports — so heuristic depends_on will be empty
        fs::create_dir(tmp.path().join(".git")).unwrap();
        fs::create_dir_all(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src/lib.rs"), "// no imports\n").unwrap();
        fs::write(tmp.path().join("src/auth.rs"), "pub fn login() {}").unwrap();

        let mut indexer = Indexer::new(tmp.path());
        indexer.full_index().unwrap();

        // Verify heuristic left depends_on empty
        let entries = indexer.load_all_entries();
        let lib = entries.iter().find(|e| e.path == PathBuf::from("src/lib.rs")).unwrap();
        assert!(lib.depends_on.is_empty(), "heuristic should find no deps");

        // Simulate LSP discovering a new edge
        indexer.graph.add_dependency(
            &PathBuf::from("src/lib.rs"),
            &PathBuf::from("src/auth.rs"),
        );
        indexer.save_graph().unwrap();

        // sync_entries_from_graph should write the edge to the FileEntry JSON
        indexer.sync_entries_from_graph().unwrap();

        let entries = indexer.load_all_entries();
        let lib = entries.iter().find(|e| e.path == PathBuf::from("src/lib.rs")).unwrap();
        assert!(
            lib.depends_on.contains(&"src/auth.rs".to_string()),
            "after sync, depends_on should contain auth.rs, got: {:?}", lib.depends_on
        );
        let auth = entries.iter().find(|e| e.path == PathBuf::from("src/auth.rs")).unwrap();
        assert!(
            auth.depended_by.contains(&"src/lib.rs".to_string()),
            "after sync, auth.rs.depended_by should contain lib.rs, got: {:?}", auth.depended_by
        );
    }

    #[test]
    fn rebuild_index_from_map_updates_depends_on_for_changed_file() {
        use std::collections::HashMap;

        let tmp = TempDir::new().unwrap();
        make_rust_repo(tmp.path());

        let mut indexer = Indexer::new(tmp.path());
        indexer.full_index().unwrap();

        let mut entry_map: HashMap<PathBuf, FileEntry> = indexer
            .load_all_entries()
            .into_iter()
            .map(|fe| (fe.path.clone(), fe))
            .collect();

        let lib_path = tmp.path().join("src/lib.rs");
        fs::write(tmp.path().join("src/extra.rs"), "pub fn extra_fn() {}\n").unwrap();
        fs::write(&lib_path, "pub mod auth;\npub mod db;\nmod extra;\n").unwrap();

        let extracted = indexer.extract_file(&lib_path);
        entry_map.insert(
            PathBuf::from("src/lib.rs"),
            FileEntry {
                path: PathBuf::from("src/lib.rs"),
                symbols: extracted.symbols,
                depends_on: vec![],
                depended_by: vec![],
            },
        );
        entry_map.insert(
            PathBuf::from("src/extra.rs"),
            FileEntry {
                path: PathBuf::from("src/extra.rs"),
                symbols: indexer.extract_symbols(&tmp.path().join("src/extra.rs")),
                depends_on: vec![],
                depended_by: vec![],
            },
        );

        indexer.rebuild_index_from_map(&entry_map).unwrap();

        let entries = indexer.load_all_entries();
        let lib = entries
            .iter()
            .find(|e| e.path == PathBuf::from("src/lib.rs"))
            .unwrap();
        assert!(
            lib.depends_on.contains(&"src/extra.rs".to_string()),
            "daemon-style rebuild should resolve new depends_on, got {:?}",
            lib.depends_on
        );
    }
}
