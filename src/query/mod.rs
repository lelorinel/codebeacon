//! Shared query logic for CLI and MCP (no LLM — term overlap + graph proximity).

use crate::config::codeindex_dir;
use crate::graph::path::{format_path_hops, hotspots as graph_hotspots, shortest_path};
use crate::graph::{persistence as graph_persistence, DependencyGraph};
use crate::graph::bfs::score_files;
use crate::indexer::writer::{read_index, read_package};
use crate::types::{PackageDetail, RepoIndex, SymbolEntry};
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Loaded index + graph for a single repo.
pub struct RepoQueryCtx {
    pub root: PathBuf,
    pub index: RepoIndex,
    pub graph: DependencyGraph,
    pub packages: HashMap<String, PackageDetail>,
}

impl RepoQueryCtx {
    pub fn load(root: &Path) -> Result<Self> {
        let codeindex = codeindex_dir(root);
        let index = read_index(&codeindex)?
            .context("no index found — run `codebeacon init` first")?;
        let graph_path = codeindex.join("graph.bin");
        let graph = graph_persistence::load(&graph_path).unwrap_or_default();

        let mut packages = HashMap::new();
        for pkg in &index.packages {
            if let Some(detail) = read_package(&pkg.name, &codeindex)? {
                packages.insert(pkg.name.clone(), detail);
            }
        }

        Ok(Self {
            root: root.to_path_buf(),
            index,
            graph,
            packages,
        })
    }

    fn tokenize(question: &str) -> Vec<String> {
        question
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '/')
            .filter(|t| t.len() >= 2)
            .map(str::to_string)
            .collect()
    }

    fn term_overlap(haystack: &str, terms: &[String]) -> f32 {
        if terms.is_empty() {
            return 0.0;
        }
        let h = haystack.to_lowercase();
        let hits = terms.iter().filter(|t| h.contains(t.as_str())).count();
        hits as f32 / terms.len() as f32
    }

    /// Ranked search over packages, symbols, and files.
    pub fn query(&self, question: &str, limit: usize) -> Vec<QueryMatch> {
        self.query_with_active(question, limit, None)
    }

    /// Like `query`, but boosts proximity to `active_files` when provided.
    pub fn query_with_active(
        &self,
        question: &str,
        limit: usize,
        active_files: Option<&[PathBuf]>,
    ) -> Vec<QueryMatch> {
        let terms = Self::tokenize(question);
        let bfs_scores = match active_files {
            Some(files) if !files.is_empty() => score_files(&self.graph, files),
            _ => score_files(&self.graph, &[]),
        };
        let mut matches: Vec<QueryMatch> = Vec::new();

        for pkg in &self.index.packages {
            let pkg_score = Self::term_overlap(&pkg.name, &terms)
                + Self::term_overlap(&pkg.purpose, &terms);
            if pkg_score > 0.0 {
                matches.push(QueryMatch {
                    kind: MatchKind::Package,
                    name: pkg.name.clone(),
                    detail: format!("{} files, score {:.2}", pkg.files, pkg.score),
                    score: pkg_score + pkg.score * 0.1,
                    hint: format!("drill_package name={}", pkg.name),
                });
            }
        }

        for (pkg_name, pkg) in &self.packages {
            for file in &pkg.files {
                let path_str = file.path.to_string_lossy().into_owned();
                let file_score = Self::term_overlap(&path_str, &terms);
                let prox = bfs_scores.get(&file.path).copied().unwrap_or(0.1);
                if file_score > 0.0 {
                    matches.push(QueryMatch {
                        kind: MatchKind::File,
                        name: path_str.clone(),
                        detail: format!("package {pkg_name}"),
                        score: file_score + prox * 0.2,
                        hint: format!("explain {path_str}"),
                    });
                }
                for sym in &file.symbols {
                    let sym_score = Self::term_overlap(&sym.name, &terms)
                        + Self::term_overlap(&sym.signature, &terms);
                    if sym_score > 0.0 {
                        matches.push(QueryMatch {
                            kind: MatchKind::Symbol,
                            name: sym.name.clone(),
                            detail: format!("{path_str}:{} — {}", sym.line, sym.signature),
                            score: sym_score + prox * 0.3,
                            hint: format!("find_definition symbol={}", sym.name),
                        });
                    }
                }
            }
        }

        for sym in &self.index.hot_symbols {
            let s = Self::term_overlap(sym, &terms);
            if s > 0.0 {
                matches.push(QueryMatch {
                    kind: MatchKind::HotSymbol,
                    name: sym.clone(),
                    detail: "hot symbol".into(),
                    score: s + 0.15,
                    hint: format!("find_definition symbol={sym}"),
                });
            }
        }

        matches.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        matches.truncate(limit);
        matches
    }

    pub fn format_query(&self, question: &str, limit: usize) -> String {
        let matches = self.query(question, limit);
        if matches.is_empty() {
            return format!("No matches for \"{question}\".");
        }
        let mut out = format!("Query: \"{question}\"\n\n");
        for (i, m) in matches.iter().enumerate() {
            out.push_str(&format!(
                "{}. [{:?}] {} — {} (score {:.2})\n   → {}\n",
                i + 1,
                m.kind,
                m.name,
                m.detail,
                m.score,
                m.hint
            ));
        }
        out
    }

    /// Resolve a user hint to a graph node (file path).
    pub fn resolve_to_file(&self, hint: &str) -> Option<PathBuf> {
        if let Some(p) = self.graph.resolve_node(hint) {
            return Some(p);
        }
        let hint_lower = hint.to_lowercase();

        for (name, pkg) in &self.packages {
            if name.eq_ignore_ascii_case(hint) || hint_lower == name.to_lowercase() {
                return pkg.files.first().map(|f| f.path.clone());
            }
        }

        for pkg in self.packages.values() {
            for file in &pkg.files {
                if file.path.to_string_lossy().contains(hint) {
                    return Some(file.path.clone());
                }
                for sym in &file.symbols {
                    if sym.name.eq_ignore_ascii_case(hint) {
                        return Some(file.path.clone());
                    }
                }
            }
        }
        None
    }

    pub fn path_between(&self, from: &str, to: &str) -> Result<String> {
        let from_file = self
            .resolve_to_file(from)
            .with_context(|| format!("could not resolve '{from}' to a file in the graph"))?;
        let to_file = self
            .resolve_to_file(to)
            .with_context(|| format!("could not resolve '{to}' to a file in the graph"))?;

        match shortest_path(&self.graph, &from_file, &to_file) {
            Some(path) => Ok(format_path_hops(&path)),
            None => bail!(
                "no dependency path from {} to {}",
                from_file.display(),
                to_file.display()
            ),
        }
    }

    pub fn explain(&self, name: &str) -> Result<String> {
        let hint = name.trim();

        for (pkg_name, pkg) in &self.packages {
            if pkg_name.eq_ignore_ascii_case(hint) {
                return Ok(self.format_package_explain(pkg));
            }
            for file in &pkg.files {
                let path_str = file.path.to_string_lossy();
                if path_str == hint
                    || path_str.ends_with(hint)
                    || hint.ends_with(path_str.as_ref())
                {
                    return Ok(self.format_file_explain(file));
                }
                if let Some(sym) = file
                    .symbols
                    .iter()
                    .find(|s| s.name.eq_ignore_ascii_case(hint))
                {
                    return Ok(self.format_symbol_explain(sym, &file.path, pkg_name));
                }
            }
        }

        if let Some(p) = self.resolve_to_file(hint) {
            for pkg in self.packages.values() {
                if let Some(file) = pkg.files.iter().find(|f| f.path == p) {
                    return Ok(self.format_file_explain(file));
                }
            }
        }

        bail!("'{name}' not found in index")
    }

    fn format_symbol_explain(
        &self,
        sym: &SymbolEntry,
        file: &PathBuf,
        package: &str,
    ) -> String {
        format!(
            "Symbol: {}\nPackage: {}\nFile: {}\nLine: {}\nKind: {:?}\nSignature: {}",
            sym.name, package, file.display(), sym.line, sym.kind, sym.signature
        )
    }

    fn format_package_explain(&self, pkg: &PackageDetail) -> String {
        let mut out = format!("Package: {}\nFiles: {}\n\n", pkg.name, pkg.files.len());
        for f in &pkg.files {
            out.push_str(&format!("  {} ({} symbols)\n", f.path.display(), f.symbols.len()));
            if !f.depends_on.is_empty() {
                out.push_str(&format!("    imports: {}\n", f.depends_on.join(", ")));
            }
        }
        out
    }

    fn format_file_explain(&self, file: &crate::types::FileEntry) -> String {
        let deps = self.graph.neighbors(&file.path);
        let dependents = self.graph.reverse_neighbors(&file.path);
        let mut out = format!(
            "File: {}\nSymbols: {}\nDirect imports (graph): {}\nDependents: {}\n",
            file.path.display(),
            file.symbols.len(),
            deps.len(),
            dependents.len()
        );
        if !file.depends_on.is_empty() {
            out.push_str(&format!("Indexed imports: {}\n", file.depends_on.join(", ")));
        }
        if !dependents.is_empty() {
            let names: Vec<_> = dependents.iter().map(|p| p.display().to_string()).collect();
            out.push_str(&format!("Dependents: {}\n", names.join(", ")));
        }
        for sym in &file.symbols {
            out.push_str(&format!("  - {} ({:?}) L{}\n", sym.name, sym.kind, sym.line));
        }
        out
    }

    pub fn dependents_of(&self, file_hint: &str) -> Result<String> {
        let file = self
            .resolve_to_file(file_hint)
            .with_context(|| format!("could not resolve '{file_hint}'"))?;
        let deps = self.graph.reverse_neighbors(&file);
        if deps.is_empty() {
            return Ok(format!("No files depend on {}", file.display()));
        }
        let mut lines: Vec<String> = deps
            .iter()
            .map(|p| format!("  {}", p.display()))
            .collect();
        lines.sort();
        Ok(format!(
            "Files that depend on {}:\n{}",
            file.display(),
            lines.join("\n")
        ))
    }

    pub fn hotspots_text(&self, limit: usize) -> String {
        let hs = graph_hotspots(&self.graph, limit);
        if hs.is_empty() {
            return "No hotspots (empty graph).".into();
        }
        let mut out = format!("Top {limit} hotspots (by dependent count):\n\n");
        for (i, (path, count)) in hs.iter().enumerate() {
            out.push_str(&format!(
                "{}. {} — {} dependents\n",
                i + 1,
                path.display(),
                count
            ));
        }
        out
    }

    /// Like `hotspots_text`, but maps each path through `path_ref` (compact dict IDs).
    pub fn hotspots_compact_text(
        &self,
        limit: usize,
        path_ref: impl Fn(&str) -> String,
    ) -> String {
        let hs = graph_hotspots(&self.graph, limit);
        if hs.is_empty() {
            return "No hotspots (empty graph).".into();
        }
        let mut out = format!("Top {limit} hotspots (by dependent count):\n\n");
        for (i, (path, count)) in hs.iter().enumerate() {
            let path_str = path.to_string_lossy();
            out.push_str(&format!(
                "{}. {} — {} dependents\n",
                i + 1,
                path_ref(&path_str),
                count
            ));
        }
        out
    }

    pub fn count_files(&self) -> usize {
        self.packages.values().map(|p| p.files.len()).sum()
    }

    pub fn count_symbols(&self) -> usize {
        self.packages
            .values()
            .flat_map(|p| p.files.iter())
            .map(|f| f.symbols.len())
            .sum()
    }

    pub fn edge_provenance(&self) -> (usize, usize) {
        let total = self.graph.edge_count();
        // Import-resolved edges are EXTRACTED; LSP may add INFERRED edges beyond
        // what import extraction found. Without per-edge metadata, report all as EXTRACTED
        // and note LSP enrichment separately when graph has more edges than import-only.
        (total, 0)
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum MatchKind {
    Package,
    File,
    Symbol,
    HotSymbol,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct QueryMatch {
    pub kind: MatchKind,
    pub name: String,
    pub detail: String,
    pub score: f32,
    pub hint: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_root() -> PathBuf {
        PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/simple_rust"
        ))
    }

    fn load_fixture() -> RepoQueryCtx {
        let root = fixture_root();
        if !codeindex_dir(&root).join("index.json").exists() {
            let mut indexer = crate::indexer::Indexer::new(&root);
            indexer.full_index().unwrap();
        }
        RepoQueryCtx::load(&root).unwrap()
    }

    #[test]
    fn query_auth_returns_matches() {
        let ctx = load_fixture();
        let matches = ctx.query("auth", 10);
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|m| m.name.contains("auth") || m.name == "login"));
    }

    #[test]
    fn path_auth_to_db() {
        let ctx = load_fixture();
        let out = ctx.path_between("src/auth.rs", "src/db.rs").unwrap();
        assert!(out.contains("auth.rs"));
        assert!(out.contains("db.rs"));
        assert!(out.contains("--imports-->"));
    }

    #[test]
    fn explain_symbol_login() {
        let ctx = load_fixture();
        let out = ctx.explain("login").unwrap();
        assert!(out.contains("login"));
        assert!(out.contains("auth.rs"));
    }

    #[test]
    fn hotspots_db_is_top() {
        let ctx = load_fixture();
        let hs = graph_hotspots(&ctx.graph, 5);
        assert!(!hs.is_empty());
        assert!(hs[0].0.to_string_lossy().contains("db.rs"));
    }
}
