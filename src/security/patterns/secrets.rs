//! CWE-798 hardcoded credentials heuristic.

use crate::security::findings::{ProofStatus, SecurityFinding};
use regex::Regex;
use std::sync::OnceLock;

const MIN_SECRET_LEN: usize = 8;

pub fn check_line(line: &str, line_no: usize) -> Vec<SecurityFinding> {
    let trimmed = line.trim();
    let re = secret_re();
    let mut findings = Vec::new();

    for caps in re.captures_iter(trimmed) {
        let name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let value = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        if value.len() < MIN_SECRET_LEN {
            continue;
        }
        if is_placeholder(value) {
            continue;
        }
        findings.push(SecurityFinding {
            cwe: "CWE-798".into(),
            line: line_no,
            column: None,
            site: trimmed.to_string(),
            message: format!("hardcoded credential detected: {name}"),
            status: ProofStatus::PatternOnly,
            witness: None,
            fix_hint: Some("load secrets from environment or a secret manager".into()),
        });
    }

    findings
}

fn is_placeholder(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("changeme")
        || lower.contains("placeholder")
        || lower.contains("example")
        || lower == "password"
        || lower == "secret"
}

fn secret_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?i)(password|api_key|apikey|secret|token|auth_token)\s*=\s*["']([^"']+)["']"#,
        )
        .unwrap()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_api_key() {
        let findings = check_line(r#"api_key = "sk-abcdefghijklmnop";"#, 1);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].cwe, "CWE-798");
    }

    #[test]
    fn placeholder_is_ignored() {
        assert!(check_line(r#"password = "changeme";"#, 1).is_empty());
    }
}
