//! CWE identifiers and default enablement sets.

use std::collections::HashSet;

/// Z3-backed checks enabled by default.
pub const DEFAULT_Z3_CWES: &[&str] = &["190", "131", "191", "369", "680"];

/// Pattern-only checks (opt-in via config).
pub const PATTERN_CWES: &[&str] = &["78", "134", "502", "798", "22"];

pub fn default_enabled_cwes() -> HashSet<String> {
    DEFAULT_Z3_CWES.iter().map(|s| (*s).to_string()).collect()
}

pub fn normalize_cwe_id(id: &str) -> String {
    id.trim()
        .trim_start_matches("CWE-")
        .trim_start_matches("cwe-")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_includes_z3_not_pattern() {
        let set = default_enabled_cwes();
        assert!(set.contains("190"));
        assert!(set.contains("131"));
        assert!(!set.contains("78"));
        assert!(!set.contains("134"));
    }

    #[test]
    fn normalize_strips_prefix() {
        assert_eq!(normalize_cwe_id("CWE-190"), "190");
        assert_eq!(normalize_cwe_id("190"), "190");
    }
}
