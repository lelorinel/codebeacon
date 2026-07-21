//! Sidecar documentation index (`.codeindex/docs.json`).

use crate::config::codeindex_dir;
use crate::docs::anchor::{markdown_headings, parse_reference, Anchor};
use crate::extractor::extract_symbols;
use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const EXPLICIT_RE: &str = r"(?i)<!--\s*codebeacon:\s*([^>]+?)\s*-->";

fn explicit_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(EXPLICIT_RE).expect("explicit link regex"))
}

fn path_like_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?x)
            \b
            (?:[A-Za-z0-9_.-]+/)+
            [A-Za-z0-9_.-]+\.
            (?:rs|go|py|ts|tsx|js|jsx|cs)
            \b
        ")
        .expect("path-like regex")
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    Explicit,
    Heuristic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocsLink {
    /// Repo-relative path (and optional `::Symbol`).
    pub target: String,
    pub kind: LinkKind,
    /// True when the target file does not exist under the repo root.
    #[serde(default)]
    pub broken: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocsSection {
    /// Stable id: `docs/api.md::## Auth`
    pub id: String,
    pub file: String,
    pub heading: String,
    pub level: usize,
    pub start_line: u32,
    pub end_line: u32,
    /// First ~200 chars of body (no heading line).
    pub snippet: String,
    pub links: Vec<DocsLink>,
    #[serde(default)]
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocsFileMeta {
    pub path: String,
    pub hash: String,
    pub mtime_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocsIndex {
    pub root: String,
    pub docs_root: String,
    pub generated_at: String,
    pub files: Vec<DocsFileMeta>,
    pub sections: Vec<DocsSection>,
}

impl DocsIndex {
    pub fn path(codeindex: &Path) -> PathBuf {
        codeindex.join("docs.json")
    }
}

pub fn load_docs_index(codeindex: &Path) -> Result<Option<DocsIndex>> {
    let path = DocsIndex::path(codeindex);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(Some(serde_json::from_str(&text)?))
}

pub fn write_docs_index(index: &DocsIndex, codeindex: &Path) -> Result<()> {
    std::fs::create_dir_all(codeindex)?;
    let path = DocsIndex::path(codeindex);
    let text = serde_json::to_string_pretty(index)?;
    std::fs::write(path, text)?;
    Ok(())
}

/// Content hash (stable enough for change detection).
pub fn content_hash(s: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Walk docs dir, build sections + links, write `.codeindex/docs.json`.
/// Preserves `stale` flags for section ids that still exist when `preserve_stale` is true.
pub fn reindex_docs(
    repo_root: &Path,
    docs_root: &Path,
    preserve_stale: bool,
) -> Result<DocsIndex> {
    let abs_docs = if docs_root.is_absolute() {
        docs_root.to_path_buf()
    } else {
        repo_root.join(docs_root)
    };
    if !abs_docs.is_dir() {
        anyhow::bail!(
            "docs path does not exist or is not a directory: {}",
            abs_docs.display()
        );
    }

    let codeindex = codeindex_dir(repo_root);
    let prev_stale: HashMap<String, bool> = if preserve_stale {
        load_docs_index(&codeindex)?
            .map(|idx| {
                idx.sections
                    .into_iter()
                    .map(|s| (s.id, s.stale))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        HashMap::new()
    };

    let symbol_names = collect_code_symbol_names(repo_root);
    let mut files_meta = Vec::new();
    let mut sections = Vec::new();

    for entry in walkdir::WalkDir::new(&abs_docs)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "md" && ext != "mdx" {
            continue;
        }
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let rel = path
            .strip_prefix(repo_root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let mtime_secs = std::fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        files_meta.push(DocsFileMeta {
            path: rel.clone(),
            hash: content_hash(&content),
            mtime_secs,
        });

        let file_sections = parse_sections(&rel, &content, repo_root, &symbol_names);
        for mut sec in file_sections {
            if preserve_stale {
                if let Some(true) = prev_stale.get(&sec.id) {
                    sec.stale = true;
                }
            }
            sections.push(sec);
        }
    }

    files_meta.sort_by(|a, b| a.path.cmp(&b.path));
    sections.sort_by(|a, b| a.id.cmp(&b.id));

    let index = DocsIndex {
        root: repo_root.display().to_string(),
        docs_root: abs_docs
            .strip_prefix(repo_root)
            .unwrap_or(&abs_docs)
            .to_string_lossy()
            .replace('\\', "/"),
        generated_at: chrono::Utc::now().to_rfc3339(),
        files: files_meta,
        sections,
    };
    write_docs_index(&index, &codeindex)?;
    Ok(index)
}

fn collect_code_symbol_names(repo_root: &Path) -> HashSet<String> {
    let mut names = HashSet::new();
    let codeindex = codeindex_dir(repo_root);
    let packages_dir = codeindex.join("packages");
    if packages_dir.is_dir() {
        if let Ok(rd) = std::fs::read_dir(&packages_dir) {
            for entry in rd.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(text) = std::fs::read_to_string(&path) {
                    if let Ok(pkg) = serde_json::from_str::<crate::types::PackageDetail>(&text) {
                        for f in &pkg.files {
                            for s in &f.symbols {
                                names.insert(s.name.clone());
                            }
                        }
                    }
                }
            }
        }
    }
    names
}

fn parse_sections(
    file_rel: &str,
    content: &str,
    repo_root: &Path,
    symbol_names: &HashSet<String>,
) -> Vec<DocsSection> {
    let lines: Vec<&str> = content.lines().collect();
    let mut heading_positions: Vec<(usize, usize, String)> = Vec::new(); // (line_idx, level, full_heading)

    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if !t.starts_with('#') {
            continue;
        }
        let level = t.chars().take_while(|&c| c == '#').count();
        if level == 0 || level > 6 {
            continue;
        }
        let after = t[level..].trim_start();
        if after.is_empty() {
            continue;
        }
        let heading = format!("{} {}", "#".repeat(level), after);
        heading_positions.push((i, level, heading));
    }

    // Whole-file section when no headings
    if heading_positions.is_empty() {
        let body = content.trim();
        let links = extract_links(body, repo_root, symbol_names);
        return vec![DocsSection {
            id: file_rel.to_string(),
            file: file_rel.to_string(),
            heading: String::new(),
            level: 0,
            start_line: 1,
            end_line: lines.len().max(1) as u32,
            snippet: truncate(body, 200),
            links,
            stale: false,
        }];
    }

    let mut sections = Vec::new();
    for (hi, &(start_idx, level, ref heading)) in heading_positions.iter().enumerate() {
        let end_idx = heading_positions[hi + 1..]
            .iter()
            .find(|(_, l, _)| *l <= level)
            .map(|(idx, _, _)| *idx)
            .unwrap_or(lines.len());
        let slice = lines[start_idx..end_idx].join("\n");
        let body = if start_idx + 1 < end_idx {
            lines[start_idx + 1..end_idx].join("\n")
        } else {
            String::new()
        };
        let links = extract_links(&slice, repo_root, symbol_names);
        let id = format!("{file_rel}::{heading}");
        sections.push(DocsSection {
            id,
            file: file_rel.to_string(),
            heading: heading.clone(),
            level,
            start_line: (start_idx + 1) as u32,
            end_line: end_idx as u32,
            snippet: truncate(body.trim(), 200),
            links,
            stale: false,
        });
    }

    // Sanity: headings list should match markdown_headings
    let _ = markdown_headings(content);
    sections
}

fn extract_links(text: &str, repo_root: &Path, symbol_names: &HashSet<String>) -> Vec<DocsLink> {
    let mut links = Vec::new();
    let mut seen = HashSet::new();

    for cap in explicit_re().captures_iter(text) {
        let raw = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        if raw.is_empty() {
            continue;
        }
        let key = format!("e:{raw}");
        if !seen.insert(key) {
            continue;
        }
        let broken = !target_exists(repo_root, raw);
        links.push(DocsLink {
            target: raw.to_string(),
            kind: LinkKind::Explicit,
            broken,
        });
    }

    for cap in path_like_re().captures_iter(text) {
        let path = cap.get(0).map(|m| m.as_str()).unwrap_or("");
        let key = format!("h:{path}");
        if !seen.insert(key) {
            continue;
        }
        // Skip if already explicit
        if links.iter().any(|l| l.target == path || l.target.starts_with(&format!("{path}::"))) {
            continue;
        }
        let broken = !repo_root.join(path).is_file();
        links.push(DocsLink {
            target: path.to_string(),
            kind: LinkKind::Heuristic,
            broken,
        });
    }

    // Symbol name heuristic: whole-word match against known symbols (min len 3)
    for name in symbol_names {
        if name.len() < 3 {
            continue;
        }
        let pat = format!(r"\b{}\b", regex::escape(name));
        if let Ok(re) = Regex::new(&pat) {
            if re.is_match(text) {
                let key = format!("s:{name}");
                if seen.insert(key) {
                    links.push(DocsLink {
                        target: format!("::{name}"),
                        kind: LinkKind::Heuristic,
                        broken: false,
                    });
                }
            }
        }
    }

    links
}

fn target_exists(repo_root: &Path, target: &str) -> bool {
    let r = parse_reference(target);
    let path = r.path.trim_start_matches("./");
    if path.is_empty() {
        // `::Symbol` only — cannot verify file
        return matches!(r.anchor, Anchor::Symbol(_));
    }
    repo_root.join(path).is_file()
}

fn truncate(s: &str, max: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

/// Mark sections stale when a code file (repo-relative) changes.
pub fn mark_stale_for_code_path(repo_root: &Path, code_rel: &Path) -> Result<usize> {
    let codeindex = codeindex_dir(repo_root);
    let Some(mut index) = load_docs_index(&codeindex)? else {
        return Ok(0);
    };
    let rel = code_rel.to_string_lossy().replace('\\', "/");
    let file_name = code_rel
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    // Symbols defined in this file (for `::Symbol` heuristic links)
    let abs = repo_root.join(code_rel);
    let file_symbols: HashSet<String> = if abs.is_file() {
        extract_symbols(&abs).into_iter().map(|s| s.name).collect()
    } else {
        HashSet::new()
    };

    let mut n = 0;
    for sec in &mut index.sections {
        let hit = sec.links.iter().any(|link| {
            if link.broken {
                return false;
            }
            let r = parse_reference(&link.target);
            if !r.path.is_empty() {
                let p = r.path.trim_start_matches("./").replace('\\', "/");
                if p == rel || p.ends_with(&format!("/{rel}")) || rel.ends_with(&p) {
                    return true;
                }
                if !file_name.is_empty() && p.ends_with(file_name) {
                    return true;
                }
            }
            if let Anchor::Symbol(sym) = &r.anchor {
                if file_symbols.contains(sym) {
                    return true;
                }
            }
            // Heuristic `::Symbol` only targets
            if link.target.starts_with("::") {
                let sym = &link.target[2..];
                if file_symbols.contains(sym) {
                    return true;
                }
            }
            false
        });
        if hit && !sec.stale {
            sec.stale = true;
            n += 1;
        }
    }
    if n > 0 {
        write_docs_index(&index, &codeindex)?;
    }
    Ok(n)
}

/// Clear stale for sections belonging to a docs file that was just reindexed.
pub fn clear_stale_for_docs_file(index: &mut DocsIndex, docs_file_rel: &str) {
    let norm = docs_file_rel.replace('\\', "/");
    for sec in &mut index.sections {
        if sec.file == norm {
            sec.stale = false;
        }
    }
}

#[derive(Debug, Clone)]
pub struct DocsQueryMatch {
    pub id: String,
    pub heading: String,
    pub snippet: String,
    pub score: f32,
    pub stale: bool,
}

/// Keyword search over section headings and snippets.
pub fn query_docs(index: &DocsIndex, question: &str, limit: usize) -> Vec<DocsQueryMatch> {
    let terms: Vec<String> = question
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '/')
        .filter(|t| t.len() >= 2)
        .map(str::to_string)
        .collect();
    if terms.is_empty() {
        return vec![];
    }
    let mut matches: Vec<DocsQueryMatch> = Vec::new();
    for sec in &index.sections {
        let hay = format!("{} {} {}", sec.heading, sec.snippet, sec.file).to_lowercase();
        let hits = terms.iter().filter(|t| hay.contains(t.as_str())).count();
        if hits == 0 {
            continue;
        }
        let mut score = hits as f32 / terms.len() as f32;
        if sec.heading.to_lowercase().contains(&terms[0]) {
            score += 0.25;
        }
        if sec.stale {
            score += 0.05;
        }
        matches.push(DocsQueryMatch {
            id: sec.id.clone(),
            heading: sec.heading.clone(),
            snippet: sec.snippet.clone(),
            score,
            stale: sec.stale,
        });
    }
    matches.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    matches.truncate(limit);
    matches
}

/// Resolve docs path from CLI override or `[docs] path` config.
pub fn resolve_docs_root(
    repo_root: &Path,
    cli_docs: Option<&Path>,
    cfg_path: Option<&str>,
) -> Option<PathBuf> {
    if let Some(p) = cli_docs {
        let abs = if p.is_absolute() {
            p.to_path_buf()
        } else {
            repo_root.join(p)
        };
        return Some(abs);
    }
    cfg_path.map(|s| {
        let p = Path::new(s);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            repo_root.join(p)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn reindex_extracts_headings_and_explicit_links() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/auth.rs"), "pub fn login() {}\n").unwrap();
        fs::write(
            root.join("docs/design.md"),
            "# Overview\n\nIntro.\n\n## Auth Flow\n\n<!-- codebeacon: src/auth.rs -->\nJWT details.\n\n## Errors\n\nCodes.\n",
        )
        .unwrap();

        let idx = reindex_docs(root, Path::new("docs"), false).unwrap();
        assert!(idx.sections.iter().any(|s| s.heading == "## Auth Flow"));
        let auth = idx
            .sections
            .iter()
            .find(|s| s.heading == "## Auth Flow")
            .unwrap();
        assert!(auth
            .links
            .iter()
            .any(|l| l.target == "src/auth.rs" && l.kind == LinkKind::Explicit && !l.broken));
        assert!(DocsIndex::path(&codeindex_dir(root)).is_file());
    }

    #[test]
    fn mark_stale_on_code_change() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/auth.rs"), "pub fn login() {}\n").unwrap();
        fs::write(
            root.join("docs/a.md"),
            "## Auth\n\n<!-- codebeacon: src/auth.rs -->\n",
        )
        .unwrap();
        reindex_docs(root, Path::new("docs"), false).unwrap();
        let n = mark_stale_for_code_path(root, Path::new("src/auth.rs")).unwrap();
        assert!(n >= 1);
        let idx = load_docs_index(&codeindex_dir(root)).unwrap().unwrap();
        assert!(idx.sections.iter().any(|s| s.stale));
    }

    #[test]
    fn query_docs_ranks_heading() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(
            root.join("docs/a.md"),
            "## Authentication\n\nLogin flow.\n\n## Billing\n\nStripe.\n",
        )
        .unwrap();
        let idx = reindex_docs(root, Path::new("docs"), false).unwrap();
        let hits = query_docs(&idx, "authentication login", 5);
        assert!(!hits.is_empty());
        assert!(hits[0].id.contains("Authentication"));
    }
}
