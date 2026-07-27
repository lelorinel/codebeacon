//! Diff-aware review bundle for PR / commit / working tree changes.

use crate::config_file::IntelligenceConfig;
use crate::intelligence::callgraph::affected_functions;
use crate::intelligence::focus::focus_context;
use crate::intelligence::impact::change_impact;
use crate::intelligence::{fragile_files, FragileFile};
use crate::query::RepoQueryCtx;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct ReviewBundle {
    pub base: String,
    pub changed_files: Vec<String>,
    pub changed_symbols: Vec<ChangedSymbol>,
    pub impacts: Vec<crate::intelligence::ChangeImpactResponse>,
    pub fragile_overlap: Vec<FragileFile>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangedSymbol {
    pub name: String,
    pub file: String,
    pub line: u32,
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub file: String,
    pub ranges: Vec<(u32, u32)>,
}

pub fn review_bundle(
    ctx: &RepoQueryCtx,
    cfg: &IntelligenceConfig,
    base: Option<&str>,
    pr: Option<u64>,
    commit: Option<&str>,
) -> Result<ReviewBundle> {
    let (label, diff_text) = if let Some(n) = pr {
        let text = pr_diff(ctx.root.as_path(), n)?;
        (format!("pr#{n}"), text)
    } else if let Some(sha) = commit {
        let text = git_show(ctx.root.as_path(), sha)?;
        (sha.to_string(), text)
    } else {
        let b = base.unwrap_or("HEAD");
        let text = git_diff(ctx.root.as_path(), b)?;
        (b.to_string(), text)
    };

    let hunks = parse_unified_diff(&diff_text);
    let changed_files: Vec<String> = hunks.iter().map(|h| h.file.clone()).collect();
    let changed_symbols = symbols_in_hunks(ctx, &hunks);

    let mut impacts = Vec::new();
    let mut seen = HashSet::new();
    for sym in changed_symbols.iter().take(20) {
        if !seen.insert(sym.name.clone()) {
            continue;
        }
        if let Ok(impact) = change_impact(ctx, &sym.name, Some(&sym.file), true, cfg) {
            impacts.push(impact);
        }
    }

    let fragile = fragile_files(ctx, 20, cfg.git_context_enabled);
    let fragile_overlap: Vec<_> = fragile
        .files
        .into_iter()
        .filter(|f| changed_files.iter().any(|c| c == &f.path || c.ends_with(&f.path)))
        .collect();

    let summary = format!(
        "Review {}: {} files, {} symbols, {} impacts, {} fragile overlaps",
        label,
        changed_files.len(),
        changed_symbols.len(),
        impacts.len(),
        fragile_overlap.len()
    );

    // Touch focus_context so review can include neighbor hints for first file
    if let Some(first) = changed_files.first() {
        let _ = focus_context(ctx, first, cfg.focus_default_radius, cfg);
        let _ = affected_functions(ctx, changed_symbols.first().map(|s| s.name.as_str()).unwrap_or(""), 1);
    }

    Ok(ReviewBundle {
        base: label,
        changed_files,
        changed_symbols,
        impacts,
        fragile_overlap,
        summary,
    })
}

fn pr_diff(root: &Path, pr: u64) -> Result<String> {
    let out = Command::new("gh")
        .args(["pr", "diff", &pr.to_string()])
        .current_dir(root)
        .output()
        .context("failed to run gh pr diff")?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    // Fallback: try git against merge-base of PR head if gh fails
    bail!(
        "gh pr diff failed: {}",
        String::from_utf8_lossy(&out.stderr)
    )
}

fn git_diff(root: &Path, base: &str) -> Result<String> {
    let range = if base == "HEAD" || base.contains("...") {
        base.to_string()
    } else {
        format!("{base}...HEAD")
    };
    let out = Command::new("git")
        .args(["-C", &root.to_string_lossy(), "diff", "-U0", &range])
        .output()
        .context("git diff failed")?;
    if !out.status.success() {
        // working tree vs HEAD
        let out2 = Command::new("git")
            .args(["-C", &root.to_string_lossy(), "diff", "-U0", "HEAD"])
            .output()
            .context("git diff HEAD failed")?;
        return Ok(String::from_utf8_lossy(&out2.stdout).into_owned());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn git_show(root: &Path, sha: &str) -> Result<String> {
    let out = Command::new("git")
        .args(["-C", &root.to_string_lossy(), "show", "--format=", "-U0", sha])
        .output()
        .context("git show failed")?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn parse_unified_diff(diff: &str) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();
    let mut current: Option<DiffHunk> = None;
    let hunk_re = regex::Regex::new(r"@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@").unwrap();

    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("+++ b/") {
            if let Some(h) = current.take() {
                hunks.push(h);
            }
            current = Some(DiffHunk {
                file: rest.to_string(),
                ranges: vec![],
            });
            continue;
        }
        if let Some(rest) = line.strip_prefix("+++ ") {
            if rest != "/dev/null" {
                if let Some(h) = current.take() {
                    hunks.push(h);
                }
                let file = rest.trim_start_matches("b/");
                current = Some(DiffHunk {
                    file: file.to_string(),
                    ranges: vec![],
                });
            }
            continue;
        }
        if let Some(caps) = hunk_re.captures(line) {
            if let Some(h) = current.as_mut() {
                let start: u32 = caps[1].parse().unwrap_or(1);
                let len: u32 = caps
                    .get(2)
                    .map(|m| m.as_str().parse().unwrap_or(1))
                    .unwrap_or(1);
                let end = start.saturating_add(len.max(1)).saturating_sub(1);
                h.ranges.push((start, end.max(start)));
            }
        }
    }
    if let Some(h) = current {
        hunks.push(h);
    }
    hunks
}

fn symbols_in_hunks(ctx: &RepoQueryCtx, hunks: &[DiffHunk]) -> Vec<ChangedSymbol> {
    let mut out = Vec::new();
    for h in hunks {
        for pkg in ctx.packages.values() {
            for file in &pkg.files {
                let path = file.path.to_string_lossy();
                if path != h.file.as_str() && !path.ends_with(&h.file) && !h.file.ends_with(path.as_ref())
                {
                    continue;
                }
                for sym in &file.symbols {
                    let hit = h.ranges.iter().any(|(a, b)| sym.line >= *a && sym.line <= *b)
                        || h.ranges.is_empty();
                    if hit {
                        out.push(ChangedSymbol {
                            name: sym.name.clone(),
                            file: path.to_string(),
                            line: sym.line,
                            kind: format!("{:?}", sym.kind),
                        });
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_diff_extracts_file_and_range() {
        let diff = r#"
diff --git a/src/auth.rs b/src/auth.rs
--- a/src/auth.rs
+++ b/src/auth.rs
@@ -3,0 +3,2 @@
+pub fn login() {}
"#;
        let hunks = parse_unified_diff(diff);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].file, "src/auth.rs");
        assert!(!hunks[0].ranges.is_empty());
    }
}
