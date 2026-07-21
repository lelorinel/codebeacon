//! Stable sub-file references for documentation / context entries.
//!
//! Copied from veld-anchor (independent copy; no shared crate with veld-lang).
//!
//! | Syntax                    | Kind    | Stability |
//! |---------------------------|---------|-----------|
//! | `path::SymbolName`        | Symbol  | ✓ follows symbol across edits |
//! | `path::## Heading`        | Heading | ✓ follows heading across edits |
//! | `path#N-M`                | Lines   | ✗ drifts when file is edited  |
//! | `path`                    | Whole   | — whole file                   |
//!
//! `resolve()` maps a `Reference` to the file slice content.
//! Callers should warn when a `Lines` anchor is used (prefer Symbol/Heading).
//! `ResolveError` is returned (not panicked) when an anchor cannot be found —
//! callers emit a warning and skip the context entry.

use crate::extractor::extract_symbols;
use std::path::{Path, PathBuf};

// ─── Public types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Anchor {
    /// No anchor — whole file.
    Whole,
    /// `::SymbolName` — code symbol (function, class, struct, …)
    Symbol(String),
    /// `::## Heading text` — markdown section heading (include the `#`s)
    Heading(String),
    /// `#start-end` — 1-based, inclusive line range. Fragile; emits a warning.
    Lines(u32, u32),
}

#[derive(Debug, Clone)]
pub struct Reference {
    /// Path exactly as written in the `.veld` file (relative or absolute).
    pub path: String,
    pub anchor: Anchor,
}

#[derive(Debug, Clone)]
pub struct ResolvedSlice {
    /// Human-readable label for the prompt context header.
    pub label: String,
    /// The extracted file content.
    pub content: String,
    /// 1-based start line in the original file.
    pub start_line: u32,
    /// 1-based end line in the original file (inclusive).
    pub end_line: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResolveError {
    FileNotFound(String),
    SymbolNotFound { file: String, symbol: String },
    HeadingNotFound { file: String, heading: String },
    LineRangeOutOfBounds { file: String, start: u32, end: u32, total: u32 },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::FileNotFound(p) =>
                write!(f, "context file not found: {p}"),
            ResolveError::SymbolNotFound { file, symbol } =>
                write!(f, "symbol '{symbol}' not found in {file}"),
            ResolveError::HeadingNotFound { file, heading } =>
                write!(f, "heading '{heading}' not found in {file}"),
            ResolveError::LineRangeOutOfBounds { file, start, end, total } =>
                write!(f, "line range {start}-{end} out of bounds in {file} (total: {total} lines)"),
        }
    }
}

// ─── Parsing ─────────────────────────────────────────────────────────────────

/// Parse a raw context string into a `Reference`.
///
/// Rules (in order):
/// 1. `path::something` → if `something` starts with `#` → `Heading(something)`, else `Symbol(something)`
/// 2. `path#N-M`       → `Lines(N, M)`
/// 3. anything else   → `Whole`
pub fn parse_reference(s: &str) -> Reference {
    // Rule 1: "::" anchor (prefer longest match → rfind)
    if let Some(sep) = s.rfind("::") {
        let path = s[..sep].to_string();
        let anchor_str = s[sep + 2..].trim();
        let anchor = if anchor_str.starts_with('#') {
            Anchor::Heading(anchor_str.to_string())
        } else {
            Anchor::Symbol(anchor_str.to_string())
        };
        return Reference { path, anchor };
    }

    // Rule 2: "#N-M" line range — only if "#" appears after at least one non-# char
    // (avoids misinterpreting markdown heading anchors like `docs.md#section`)
    if let Some(hash_pos) = s.rfind('#') {
        if hash_pos > 0 {
            let range_str = &s[hash_pos + 1..];
            if let Some((start_str, end_str)) = range_str.split_once('-') {
                if let (Ok(start), Ok(end)) = (start_str.parse::<u32>(), end_str.parse::<u32>()) {
                    if start >= 1 && end >= start {
                        return Reference {
                            path: s[..hash_pos].to_string(),
                            anchor: Anchor::Lines(start, end),
                        };
                    }
                }
            }
        }
    }

    Reference { path: s.to_string(), anchor: Anchor::Whole }
}

/// Returns true if this reference uses a fragile line-range anchor.
/// Callers should emit a deprecation warning when this is true.
pub fn is_fragile(r: &Reference) -> bool {
    matches!(r.anchor, Anchor::Lines(_, _))
}

/// Normalized markdown heading lines suitable for `::` anchors (e.g. `## Auth Flow`).
pub fn markdown_headings(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            let t = line.trim_start();
            if !t.starts_with('#') {
                return None;
            }
            let level = t.chars().take_while(|&c| c == '#').count();
            if level == 0 || level > 6 {
                return None;
            }
            let after_hashes = t[level..].trim_start();
            if after_hashes.is_empty() {
                return None;
            }
            Some(format!("{} {}", "#".repeat(level), after_hashes))
        })
        .collect()
}

/// Resolve a context file path against one or more base directories (project root, spec dir, …).
pub fn resolve_context_path(bases: &[&Path], path: &str) -> Option<PathBuf> {
    let rel = path.trim_start_matches("./");
    for base in bases {
        let candidate = resolve_path(base, rel);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

// ─── Resolution ──────────────────────────────────────────────────────────────

/// Resolve a `Reference` to a `ResolvedSlice`.
///
/// `base_dir` is the directory relative paths are resolved against
/// (typically the directory of the `.veld` file or the project root).
pub fn resolve(base_dir: &Path, r: &Reference) -> Result<ResolvedSlice, ResolveError> {
    resolve_with_bases(&[base_dir], r)
}

/// Like `resolve`, but tries each base directory until the context file is found.
pub fn resolve_with_bases(bases: &[&Path], r: &Reference) -> Result<ResolvedSlice, ResolveError> {
    let abs_path = resolve_context_path(bases, &r.path)
        .ok_or_else(|| ResolveError::FileNotFound(r.path.clone()))?;
    resolve_file_at(&abs_path, &r.path, &r.anchor)
}

fn resolve_file_at(
    abs_path: &Path,
    display_path: &str,
    anchor: &Anchor,
) -> Result<ResolvedSlice, ResolveError> {
    let path_str = abs_path.to_string_lossy().to_string();

    let content = std::fs::read_to_string(abs_path)
        .map_err(|_| ResolveError::FileNotFound(path_str.clone()))?;

    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len() as u32;

    match anchor {
        Anchor::Whole => Ok(ResolvedSlice {
            label: display_path.to_string(),
            content,
            start_line: 1,
            end_line: total,
        }),

        Anchor::Lines(start, end) => {
            if *start < 1 || *end > total || *start > *end {
                return Err(ResolveError::LineRangeOutOfBounds {
                    file: path_str,
                    start: *start,
                    end: *end,
                    total,
                });
            }
            let slice = lines[(*start - 1) as usize..*end as usize].join("\n");
            Ok(ResolvedSlice {
                label: format!("{} [lines {}-{}]", display_path, start, end),
                content: slice,
                start_line: *start,
                end_line: *end,
            })
        }

        Anchor::Symbol(sym) => resolve_symbol(abs_path, &path_str, display_path, sym, &lines),

        Anchor::Heading(heading) => resolve_heading(&path_str, display_path, heading, &lines),
    }
}

fn resolve_path(base_dir: &Path, path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base_dir.join(p)
    }
}

fn resolve_symbol(
    abs_path: &Path,
    path_str: &str,
    display_path: &str,
    sym: &str,
    lines: &[&str],
) -> Result<ResolvedSlice, ResolveError> {
    let symbols = extract_symbols(abs_path);
    let entry = symbols.iter().find(|s| s.name == sym).ok_or_else(|| {
        ResolveError::SymbolNotFound {
            file: path_str.to_string(),
            symbol: sym.to_string(),
        }
    })?;

    let start = entry.line as usize; // 1-based
    if start == 0 || start > lines.len() {
        return Err(ResolveError::SymbolNotFound {
            file: path_str.to_string(),
            symbol: sym.to_string(),
        });
    }

    // Determine end: find next symbol at same or lower nesting (i.e., next top-level symbol)
    let next_sym_line = symbols
        .iter()
        .find(|s| s.line > entry.line)
        .map(|s| s.line as usize)
        .unwrap_or(lines.len() + 1);

    // Also try brace-matching from the start line for brace-delimited languages
    let brace_end = brace_match_end(lines, start - 1);
    let end = if let Some(be) = brace_end {
        be.min(next_sym_line - 1)
    } else {
        (next_sym_line - 1).min(lines.len())
    };

    let start_u32 = start as u32;
    let end_u32 = end as u32;
    let slice = lines[start - 1..end].join("\n");

    Ok(ResolvedSlice {
        label: format!("{}::{}", display_path, sym),
        content: slice,
        start_line: start_u32,
        end_line: end_u32,
    })
}

fn resolve_heading(
    path_str: &str,
    display_path: &str,
    heading: &str,
    lines: &[&str],
) -> Result<ResolvedSlice, ResolveError> {
    let heading_level = heading.chars().take_while(|&c| c == '#').count();
    let heading_text = normalize_heading_label(heading.trim_start_matches('#').trim());

    let start_idx = lines
        .iter()
        .position(|l| heading_line_matches(l, heading_level, &heading_text))
        .ok_or_else(|| ResolveError::HeadingNotFound {
            file: path_str.to_string(),
            heading: heading.to_string(),
        })?;

    // Section ends at the next heading of same or higher level (fewer #s)
    let end_idx = lines[start_idx + 1..]
        .iter()
        .position(|l| {
            let level = l.chars().take_while(|&c| c == '#').count();
            level > 0 && level <= heading_level
        })
        .map(|rel| start_idx + 1 + rel)
        .unwrap_or(lines.len());

    let start_line = (start_idx + 1) as u32;
    let end_line = end_idx as u32;
    let slice = lines[start_idx..end_idx].join("\n");

    Ok(ResolvedSlice {
        label: format!("{}::{}", display_path, heading),
        content: slice,
        start_line,
        end_line,
    })
}

fn normalize_heading_label(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn heading_line_matches(line: &str, heading_level: usize, heading_text: &str) -> bool {
    let t = line.trim_start();
    if !t.starts_with('#') {
        return false;
    }
    let level = t.chars().take_while(|&c| c == '#').count();
    if level != heading_level {
        return false;
    }
    let text = normalize_heading_label(t[level..].trim());
    text.eq_ignore_ascii_case(heading_text)
}

/// Find the closing line of a brace-delimited block starting from `start_idx` (0-based).
/// Returns the 1-based line number of the closing `}`, or `None` if not brace-delimited.
fn brace_match_end(lines: &[&str], start_idx: usize) -> Option<usize> {
    let first = lines.get(start_idx)?;
    if !first.contains('{') {
        return None;
    }
    let mut depth: i32 = 0;
    for (i, line) in lines[start_idx..].iter().enumerate() {
        for ch in line.chars() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(start_idx + i + 1); // 1-based
                    }
                }
                _ => {}
            }
        }
    }
    None
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ── parse_reference ──────────────────────────────────────────────────────

    #[test]
    fn parse_whole_file() {
        let r = parse_reference("src/auth.ts");
        assert_eq!(r.path, "src/auth.ts");
        assert_eq!(r.anchor, Anchor::Whole);
    }

    #[test]
    fn parse_symbol_anchor() {
        let r = parse_reference("src/auth.ts::AuthService");
        assert_eq!(r.path, "src/auth.ts");
        assert_eq!(r.anchor, Anchor::Symbol("AuthService".to_string()));
    }

    #[test]
    fn markdown_headings_extracts_sections() {
        let md = "# Overview\n\n## Auth Flow\n\nJWT stuff.\n\n## Errors\n";
        let h = markdown_headings(md);
        assert!(h.contains(&"## Auth Flow".to_string()));
        assert!(h.contains(&"# Overview".to_string()));
        assert!(h.contains(&"## Errors".to_string()));
    }

    #[test]
    fn parse_heading_anchor() {
        let r = parse_reference("docs/api.md::## Authentication");
        assert_eq!(r.path, "docs/api.md");
        assert_eq!(r.anchor, Anchor::Heading("## Authentication".to_string()));
    }

    #[test]
    fn parse_h1_heading() {
        let r = parse_reference("README.md::# Overview");
        assert_eq!(r.anchor, Anchor::Heading("# Overview".to_string()));
    }

    #[test]
    fn parse_line_range() {
        let r = parse_reference("src/auth.ts#12-80");
        assert_eq!(r.path, "src/auth.ts");
        assert_eq!(r.anchor, Anchor::Lines(12, 80));
    }

    #[test]
    fn line_range_is_fragile() {
        let r = parse_reference("src/auth.ts#12-80");
        assert!(is_fragile(&r));
    }

    #[test]
    fn symbol_anchor_not_fragile() {
        let r = parse_reference("src/auth.ts::AuthService");
        assert!(!is_fragile(&r));
    }

    #[test]
    fn parse_prefers_double_colon_over_hash() {
        // If both :: and # appear, :: wins (rfind)
        let r = parse_reference("docs/file.md::## Section#1");
        assert_eq!(r.path, "docs/file.md");
        // anchor_str = "## Section#1" — starts with # so it's a Heading
        assert_eq!(r.anchor, Anchor::Heading("## Section#1".to_string()));
    }

    #[test]
    fn resolve_with_bases_uses_project_root() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        write_tmp(
            &tmp,
            "docs/design.md",
            "# Overview\n\n## Auth Flow\n\nJWT details.\n",
        );
        let src_dir = root.join("src");
        let r = parse_reference("docs/design.md::## Auth Flow");
        let slice = resolve_with_bases(&[src_dir.as_path(), root], &r).unwrap();
        assert!(slice.content.contains("JWT details"));
        assert_eq!(slice.start_line, 3);
    }

    #[test]
    fn resolve_heading_case_insensitive() {
        let tmp = TempDir::new().unwrap();
        write_tmp(&tmp, "doc.md", "## auth flow\n\nBody.\n");
        let r = Reference {
            path: "doc.md".to_string(),
            anchor: Anchor::Heading("## Auth Flow".to_string()),
        };
        let slice = resolve(tmp.path(), &r).unwrap();
        assert!(slice.content.contains("Body"));
    }

    // ── resolve — Lines ───────────────────────────────────────────────────────

    fn write_tmp(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let p = dir.path().join(name);
        fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn resolve_line_range_ok() {
        let tmp = TempDir::new().unwrap();
        write_tmp(&tmp, "f.ts", "line1\nline2\nline3\nline4\nline5\n");
        let r = Reference {
            path: "f.ts".to_string(),
            anchor: Anchor::Lines(2, 4),
        };
        let slice = resolve(tmp.path(), &r).unwrap();
        assert_eq!(slice.content, "line2\nline3\nline4");
        assert_eq!(slice.start_line, 2);
        assert_eq!(slice.end_line, 4);
        assert!(slice.label.contains("lines 2-4"));
    }

    #[test]
    fn resolve_line_range_out_of_bounds() {
        let tmp = TempDir::new().unwrap();
        write_tmp(&tmp, "f.ts", "a\nb\nc\n");
        let r = Reference {
            path: "f.ts".to_string(),
            anchor: Anchor::Lines(2, 10),
        };
        assert!(matches!(
            resolve(tmp.path(), &r),
            Err(ResolveError::LineRangeOutOfBounds { .. })
        ));
    }

    // ── resolve — Heading ────────────────────────────────────────────────────

    #[test]
    fn resolve_heading_section() {
        let tmp = TempDir::new().unwrap();
        write_tmp(&tmp, "doc.md", "# Overview\n\nIntro text.\n\n## Authentication\n\nAuth details here.\nMore auth.\n\n## Errors\n\nError codes.\n");
        let r = Reference {
            path: "doc.md".to_string(),
            anchor: Anchor::Heading("## Authentication".to_string()),
        };
        let slice = resolve(tmp.path(), &r).unwrap();
        assert!(slice.content.contains("Auth details here."));
        assert!(slice.content.contains("## Authentication"));
        assert!(!slice.content.contains("## Errors"));
    }

    #[test]
    fn resolve_heading_not_found() {
        let tmp = TempDir::new().unwrap();
        write_tmp(&tmp, "doc.md", "# Title\n\nContent.\n");
        let r = Reference {
            path: "doc.md".to_string(),
            anchor: Anchor::Heading("## Missing".to_string()),
        };
        assert!(matches!(
            resolve(tmp.path(), &r),
            Err(ResolveError::HeadingNotFound { .. })
        ));
    }

    #[test]
    fn resolve_heading_last_section_goes_to_eof() {
        let tmp = TempDir::new().unwrap();
        write_tmp(&tmp, "doc.md", "## First\n\nFirst content.\n\n## Last\n\nLast content here.\n");
        let r = Reference {
            path: "doc.md".to_string(),
            anchor: Anchor::Heading("## Last".to_string()),
        };
        let slice = resolve(tmp.path(), &r).unwrap();
        assert!(slice.content.contains("Last content here."));
    }

    // ── resolve — Symbol ────────────────────────────────────────────────────

    #[test]
    fn resolve_ts_symbol() {
        let tmp = TempDir::new().unwrap();
        write_tmp(&tmp, "auth.ts", "const helper = () => {}\n\nexport class AuthService {\n  login() {}\n  logout() {}\n}\n\nexport function otherFn() {}\n");
        let r = Reference {
            path: "auth.ts".to_string(),
            anchor: Anchor::Symbol("AuthService".to_string()),
        };
        let slice = resolve(tmp.path(), &r).unwrap();
        assert!(slice.content.contains("AuthService"));
        assert!(slice.content.contains("login"));
        // Should not include otherFn (after the class closes)
        assert!(!slice.content.contains("otherFn"));
    }

    #[test]
    fn resolve_symbol_not_found() {
        let tmp = TempDir::new().unwrap();
        write_tmp(&tmp, "auth.ts", "export function foo() {}\n");
        let r = Reference {
            path: "auth.ts".to_string(),
            anchor: Anchor::Symbol("Missing".to_string()),
        };
        assert!(matches!(
            resolve(tmp.path(), &r),
            Err(ResolveError::SymbolNotFound { .. })
        ));
    }

    // ── resolve — FileNotFound ───────────────────────────────────────────────

    #[test]
    fn resolve_file_not_found() {
        let tmp = TempDir::new().unwrap();
        let r = Reference {
            path: "nonexistent.ts".to_string(),
            anchor: Anchor::Whole,
        };
        assert!(matches!(
            resolve(tmp.path(), &r),
            Err(ResolveError::FileNotFound(_))
        ));
    }
}
