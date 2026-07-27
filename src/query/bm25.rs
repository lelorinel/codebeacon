//! BM25 ranking over indexed packages, files, and symbols.

use crate::query::tokenize::{tokenize, light_stem};
use crate::types::{PackageDetail, PackageSummary, RepoIndex};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

pub const K1: f32 = 1.2;
pub const B: f32 = 0.75;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchIndex {
    /// Document id → term → tf
    pub docs: Vec<SearchDoc>,
    /// term → document frequency
    pub df: HashMap<String, u32>,
    pub avg_dl: f32,
    pub n_docs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchDoc {
    pub kind: DocKind,
    pub name: String,
    pub detail: String,
    pub hint: String,
    pub path: Option<String>,
    pub term_tf: HashMap<String, u32>,
    pub dl: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocKind {
    Package,
    File,
    Symbol,
    HotSymbol,
}

impl SearchIndex {
    pub fn build(index: &RepoIndex, packages: &HashMap<String, PackageDetail>) -> Self {
        let mut docs = Vec::new();

        for pkg in &index.packages {
            docs.push(doc_from_text(
                DocKind::Package,
                pkg.name.clone(),
                format!("{} files, score {:.2}", pkg.files, pkg.score),
                format!("drill_package name={}", pkg.name),
                None,
                &format!("{} {}", pkg.name, pkg.purpose),
            ));
        }

        for (pkg_name, pkg) in packages {
            for file in &pkg.files {
                let path_str = file.path.to_string_lossy().into_owned();
                docs.push(doc_from_text(
                    DocKind::File,
                    path_str.clone(),
                    format!("package {pkg_name}"),
                    format!("explain {path_str}"),
                    Some(path_str.clone()),
                    &path_str,
                ));
                for sym in &file.symbols {
                    let text = format!("{} {} {}", sym.name, sym.signature, path_str);
                    docs.push(doc_from_text(
                        DocKind::Symbol,
                        sym.name.clone(),
                        format!("{path_str}:{} — {}", sym.line, sym.signature),
                        format!("find_definition symbol={}", sym.name),
                        Some(path_str.clone()),
                        &text,
                    ));
                }
            }
        }

        for sym in &index.hot_symbols {
            docs.push(doc_from_text(
                DocKind::HotSymbol,
                sym.clone(),
                "hot symbol".into(),
                format!("find_definition symbol={sym}"),
                None,
                sym,
            ));
        }

        let n_docs = docs.len() as u32;
        let avg_dl = if docs.is_empty() {
            0.0
        } else {
            docs.iter().map(|d| d.dl as f32).sum::<f32>() / n_docs as f32
        };

        let mut df: HashMap<String, u32> = HashMap::new();
        for doc in &docs {
            for term in doc.term_tf.keys() {
                *df.entry(term.clone()).or_insert(0) += 1;
            }
        }

        Self {
            docs,
            df,
            avg_dl,
            n_docs,
        }
    }

    pub fn score_query(&self, question: &str) -> Vec<(usize, f32)> {
        let terms = tokenize(question);
        if terms.is_empty() || self.n_docs == 0 {
            return vec![];
        }

        let mut scores: Vec<(usize, f32)> = Vec::new();
        for (i, doc) in self.docs.iter().enumerate() {
            let mut score = 0.0_f32;
            for term in &terms {
                let tf = *doc.term_tf.get(term).unwrap_or(&0) as f32;
                if tf == 0.0 {
                    // Prefix / contains fallback for short stems (auth ⊂ authentic)
                    let soft = doc
                        .term_tf
                        .keys()
                        .filter(|t| t.starts_with(term.as_str()) || term.starts_with(t.as_str()))
                        .map(|t| *doc.term_tf.get(t).unwrap_or(&0))
                        .max()
                        .unwrap_or(0) as f32;
                    if soft == 0.0 {
                        continue;
                    }
                    let df = soft_df(self, term);
                    score += idf(self.n_docs, df) * bm25_tf(soft, doc.dl, self.avg_dl);
                    continue;
                }
                let df = *self.df.get(term).unwrap_or(&1);
                score += idf(self.n_docs, df) * bm25_tf(tf, doc.dl, self.avg_dl);
            }
            if score > 0.0 {
                scores.push((i, score));
            }
        }
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores
    }

    pub fn save(&self, codeindex: &Path) -> anyhow::Result<()> {
        let path = codeindex.join("search.bin");
        let bytes = bincode::serialize(self)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    pub fn load(codeindex: &Path) -> Option<Self> {
        let path = codeindex.join("search.bin");
        let bytes = std::fs::read(path).ok()?;
        bincode::deserialize(&bytes).ok()
    }
}

fn soft_df(index: &SearchIndex, term: &str) -> u32 {
    index
        .df
        .iter()
        .filter(|(t, _)| t.starts_with(term) || term.starts_with(t.as_str()))
        .map(|(_, c)| *c)
        .max()
        .unwrap_or(1)
}

fn doc_from_text(
    kind: DocKind,
    name: String,
    detail: String,
    hint: String,
    path: Option<String>,
    text: &str,
) -> SearchDoc {
    let tokens = tokenize(text);
    let mut term_tf: HashMap<String, u32> = HashMap::new();
    for t in &tokens {
        *term_tf.entry(t.clone()).or_insert(0) += 1;
    }
    // Also index unstemmed short prefixes for soft match
    for t in tokenize(text) {
        if t.len() >= 3 {
            let prefix = light_stem(&t[..t.len().min(4)]);
            term_tf.entry(prefix).or_insert(0);
        }
    }
    let dl = tokens.len().max(1) as u32;
    SearchDoc {
        kind,
        name,
        detail,
        hint,
        path,
        term_tf,
        dl,
    }
}

fn idf(n: u32, df: u32) -> f32 {
    let n = n as f32;
    let df = df.max(1) as f32;
    ((n - df + 0.5) / (df + 0.5) + 1.0).ln()
}

fn bm25_tf(tf: f32, dl: u32, avg_dl: f32) -> f32 {
    let avg = if avg_dl <= 0.0 { 1.0 } else { avg_dl };
    let dl = dl as f32;
    tf * (K1 + 1.0) / (tf + K1 * (1.0 - B + B * dl / avg))
}

/// Rebuild search index from repo index + packages and persist.
pub fn write_search_index(
    codeindex: &Path,
    index: &RepoIndex,
    packages: &[PackageDetail],
) -> anyhow::Result<()> {
    let map: HashMap<String, PackageDetail> = packages
        .iter()
        .map(|p| (p.name.clone(), p.clone()))
        .collect();
    let search = SearchIndex::build(index, &map);
    search.save(codeindex)?;
    Ok(())
}

/// Helper used when only summaries are available during partial builds.
#[allow(dead_code)]
pub fn empty_with_packages(summaries: &[PackageSummary]) -> SearchIndex {
    let index = RepoIndex {
        repo: String::new(),
        generated_at: String::new(),
        packages: summaries.to_vec(),
        hot_symbols: vec![],
    };
    SearchIndex::build(&index, &HashMap::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FileEntry, SymbolEntry, SymbolKind};
    use std::path::PathBuf;

    #[test]
    fn bm25_ranks_login_symbol() {
        let mut packages = HashMap::new();
        packages.insert(
            "auth".into(),
            PackageDetail {
                name: "auth".into(),
                files: vec![FileEntry {
                    path: PathBuf::from("src/auth.rs"),
                    symbols: vec![SymbolEntry {
                        name: "login".into(),
                        signature: "fn login()".into(),
                        kind: SymbolKind::Function,
                        line: 1,
                        character: 0,
                    }],
                    depends_on: vec![],
                    depended_by: vec![],
                }],
            },
        );
        let index = RepoIndex {
            repo: "t".into(),
            generated_at: String::new(),
            packages: vec![PackageSummary {
                name: "auth".into(),
                purpose: "user authentication".into(),
                files: 1,
                score: 1.0,
            }],
            hot_symbols: vec!["login".into()],
        };
        let search = SearchIndex::build(&index, &packages);
        let scores = search.score_query("login");
        assert!(!scores.is_empty());
        let top = &search.docs[scores[0].0];
        assert!(top.name.contains("login") || top.name.contains("auth"));
    }
}
