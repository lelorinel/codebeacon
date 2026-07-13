//! CWE-134 format string heuristic.

use crate::security::findings::{ProofStatus, SecurityFinding};
use regex::Regex;
use std::sync::OnceLock;

pub fn check_line(line: &str, line_no: usize) -> Vec<SecurityFinding> {
    let trimmed = line.trim();
    if !trimmed.contains("printf(")
        && !trimmed.contains("sprintf(")
        && !trimmed.contains("fprintf(")
        && !trimmed.contains("format!(")
    {
        return Vec::new();
    }

    let mut findings = Vec::new();
    for re in [printf_re(), sprintf_re(), fprintf_re(), format_macro_re()] {
        for caps in re.captures_iter(trimmed) {
            let Some(format_arg) = caps.get(1) else { continue };
            let format_arg = format_arg.as_str();
            if is_user_controlled_identifier(format_arg) {
                findings.push(SecurityFinding {
                    cwe: "CWE-134".into(),
                    line: line_no,
                    column: None,
                    site: trimmed.to_string(),
                    message: format!("format string may be user-controlled: {format_arg}"),
                    status: ProofStatus::PatternOnly,
                    witness: None,
                    fix_hint: Some("use a literal format string, not a variable".into()),
                });
            }
        }
    }
    findings
}

fn is_user_controlled_identifier(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('"')
        && !s.starts_with("r\"")
        && s.chars().next().is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && s.chars()
            .all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn printf_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"printf\s*\(\s*(\w+)\s*[\),]").unwrap())
}

fn sprintf_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"sprintf\s*\([^,]+,\s*(\w+)\s*[\),]").unwrap())
}

fn fprintf_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"fprintf\s*\([^,]+,\s*(\w+)\s*[\),]").unwrap())
}

fn format_macro_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"format!\s*\(\s*(\w+)\s*[\),]").unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_printf_variable() {
        let findings = check_line("printf(user_input);", 1);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].cwe, "CWE-134");
    }

    #[test]
    fn literal_format_is_safe() {
        assert!(check_line(r#"printf("%s", name);"#, 1).is_empty());
    }
}
