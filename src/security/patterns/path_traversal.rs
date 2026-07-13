//! CWE-22 path traversal heuristic.

use crate::security::findings::{ProofStatus, SecurityFinding};
use regex::Regex;
use std::sync::OnceLock;

pub fn check_line(line: &str, line_no: usize) -> Vec<SecurityFinding> {
    let trimmed = line.trim();
    let mut findings = Vec::new();

    if trimmed.contains("../") {
        if let Some(caps) = path_concat_re().captures(trimmed) {
            let var = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            if is_identifier(var) {
                findings.push(make_finding(trimmed, line_no, var, "path concatenation with ../"));
            }
        } else {
            findings.push(make_finding(
                trimmed,
                line_no,
                "path",
                "literal path traversal sequence",
            ));
        }
    }

    if let Some(caps) = path_new_re().captures(trimmed) {
        if let Some(var_match) = caps.get(1) {
            let var = var_match.as_str();
            if is_identifier(var) {
                findings.push(make_finding(
                    trimmed,
                    line_no,
                    var,
                    "user-controlled path in Path::new",
                ));
            }
        }
    }

    if let Some(caps) = open_var_re().captures(trimmed) {
        if let Some(var_match) = caps.get(1) {
            let var = var_match.as_str();
            if is_identifier(var) {
                findings.push(make_finding(
                    trimmed,
                    line_no,
                    var,
                    "user-controlled path in open()",
                ));
            }
        }
    }

    findings
}

fn make_finding(site: &str, line_no: usize, var: &str, detail: &str) -> SecurityFinding {
    SecurityFinding {
        cwe: "CWE-22".into(),
        line: line_no,
        column: None,
        site: site.to_string(),
        message: format!("{detail}: {var}"),
        status: ProofStatus::PatternOnly,
        witness: None,
        fix_hint: Some("validate and canonicalize paths before file access".into()),
    }
}

fn is_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars().next().is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && s.chars()
            .all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn path_concat_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(\w+)\s*\+\s*["']\.\./"#).unwrap())
}

fn path_new_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"Path::new\s*\(\s*&?(\w+)\s*\)").unwrap())
}

fn open_var_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"open\s*\(\s*(\w+)\s*[\),]").unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_path_traversal_concat() {
        let findings = check_line(r#"open(base + "../etc/passwd")"#, 1);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].cwe, "CWE-22");
    }
}
