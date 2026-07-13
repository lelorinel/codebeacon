//! CWE-78 OS command injection heuristic.

use crate::security::findings::{ProofStatus, SecurityFinding};
use regex::Regex;
use std::sync::OnceLock;

pub fn check_line(line: &str, line_no: usize) -> Vec<SecurityFinding> {
    let trimmed = line.trim();
    if !trimmed.contains("system(")
        && !trimmed.contains("exec(")
        && !trimmed.contains("popen(")
        && !trimmed.contains("subprocess")
    {
        return Vec::new();
    }

    let mut findings = Vec::new();
    for re in [system_re(), exec_re(), popen_re(), subprocess_re()] {
        for caps in re.captures_iter(trimmed) {
            let Some(arg_match) = caps.get(1) else { continue };
            let arg = arg_match.as_str();
            if is_user_controlled(arg) {
                findings.push(SecurityFinding {
                    cwe: "CWE-78".into(),
                    line: line_no,
                    column: None,
                    site: trimmed.to_string(),
                    message: format!("command may include user-controlled input: {arg}"),
                    status: ProofStatus::PatternOnly,
                    witness: None,
                    fix_hint: Some("avoid passing user input to shell commands".into()),
                });
            }
        }
    }
    findings
}

fn is_user_controlled(s: &str) -> bool {
    !s.starts_with('"')
        && !s.starts_with('\'')
        && s.chars().next().is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
}

fn system_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"system\s*\(\s*(\w+)\s*\)").unwrap())
}

fn exec_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"exec\s*\(\s*(\w+)\s*[\),]").unwrap())
}

fn popen_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"popen\s*\(\s*(\w+)\s*[\),]").unwrap())
}

fn subprocess_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"subprocess\.(?:call|run|Popen)\s*\(\s*(\w+)\s*[\),]").unwrap()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_system_variable() {
        let findings = check_line("system(cmd);", 1);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].cwe, "CWE-78");
    }
}
