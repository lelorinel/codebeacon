//! Test gap analysis: match prod functions to test functions by naming heuristics.

use crate::query::RepoQueryCtx;
use crate::types::SymbolKind;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize)]
pub struct TestGap {
    pub symbol: String,
    pub file: String,
    pub line: u32,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestGapsResponse {
    pub gaps: Vec<TestGap>,
    pub tested_count: usize,
    pub untested_count: usize,
    pub test_fn_count: usize,
}

pub fn test_gaps(
    ctx: &RepoQueryCtx,
    package: Option<&str>,
    file_filter: Option<&str>,
    limit: usize,
) -> TestGapsResponse {
    let mut prod = Vec::new();
    let mut tests = Vec::new();

    for (pkg_name, pkg) in &ctx.packages {
        if let Some(p) = package {
            if pkg_name != p {
                continue;
            }
        }
        for file in &pkg.files {
            let path = file.path.to_string_lossy();
            if let Some(f) = file_filter {
                if path != f && !path.ends_with(f) {
                    continue;
                }
            }
            let is_test_file = path.contains("test") || path.contains("spec") || path.contains("__tests__");
            for sym in &file.symbols {
                if sym.kind != SymbolKind::Function {
                    continue;
                }
                if is_test_file || is_test_name(&sym.name) {
                    tests.push((sym.name.clone(), path.to_string()));
                } else {
                    prod.push((sym.name.clone(), path.to_string(), sym.line));
                }
            }
        }
    }

    let test_targets: HashSet<String> = tests
        .iter()
        .flat_map(|(name, _)| target_names(name))
        .collect();

    let mut gaps = Vec::new();
    let mut tested = 0usize;
    for (name, file, line) in &prod {
        if is_covered(name, &test_targets) {
            tested += 1;
        } else {
            gaps.push(TestGap {
                symbol: name.clone(),
                file: file.clone(),
                line: *line,
                reason: "no matching test function found".into(),
            });
        }
    }

    gaps.truncate(limit);
    let untested = gaps.len();

    TestGapsResponse {
        gaps,
        tested_count: tested,
        untested_count: untested,
        test_fn_count: tests.len(),
    }
}

fn is_test_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with("test_")
        || (lower.starts_with("test")
            && name.len() > 4
            && name
                .chars()
                .nth(4)
                .map(|c| c.is_uppercase())
                .unwrap_or(false))
        || lower.ends_with("_test")
        || lower.starts_with("it_")
        || lower == "it"
        || lower == "describe"
}

fn target_names(test_name: &str) -> Vec<String> {
    let mut out = vec![test_name.to_string()];
    let lower = test_name.to_lowercase();
    for prefix in ["test_", "test", "it_", "should_"] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            if rest.len() >= 2 {
                out.push(rest.to_string());
            }
        }
    }
    if let Some(rest) = lower.strip_suffix("_test") {
        if rest.len() >= 2 {
            out.push(rest.to_string());
        }
    }
    out
}

fn is_covered(prod: &str, targets: &HashSet<String>) -> bool {
    let lower = prod.to_lowercase();
    if targets.contains(prod) || targets.contains(&lower) {
        return true;
    }
    targets.iter().any(|t| t == &lower || lower.contains(t) || t.contains(&lower))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_target_from_test_login() {
        let t = target_names("test_login");
        assert!(t.iter().any(|x| x == "login"));
    }

    #[test]
    fn detects_test_name() {
        assert!(is_test_name("test_login"));
        assert!(!is_test_name("login"));
    }
}
