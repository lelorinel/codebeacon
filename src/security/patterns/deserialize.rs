//! CWE-502 unsafe deserialization heuristic.

use crate::security::findings::{ProofStatus, SecurityFinding};

pub fn check_line(line: &str, line_no: usize) -> Vec<SecurityFinding> {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();

    let (matched, label) = if lower.contains("pickle.loads") || lower.contains("pickle.load(") {
        (true, "pickle.loads")
    } else if lower.contains("yaml.unsafe_load") || lower.contains("yaml.load(") {
        (true, "yaml.load")
    } else if lower.contains("unserialize(") {
        (true, "unserialize")
    } else if lower.contains("readobject(") {
        (true, "readObject")
    } else if lower.contains("marshal.loads") {
        (true, "marshal.loads")
    } else {
        (false, "")
    };

    if !matched {
        return Vec::new();
    }

    vec![SecurityFinding {
        cwe: "CWE-502".into(),
        line: line_no,
        column: None,
        site: trimmed.to_string(),
        message: format!("unsafe deserialization via {label}"),
        status: ProofStatus::PatternOnly,
        witness: None,
        fix_hint: Some("deserialize only trusted data with safe parsers".into()),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_pickle() {
        let findings = check_line("data = pickle.loads(raw)", 1);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].cwe, "CWE-502");
    }
}
