//! Subtraction site extraction (CWE-191 integer underflow).

use super::{is_constant_identifier, is_identifier, ExtractedSite, SubtractionSite};
use regex::Regex;
use std::sync::OnceLock;

pub fn extract_sites(line: &str, line_no: usize) -> Vec<ExtractedSite> {
    let trimmed = line.trim();
    if !trimmed.contains(" - ") {
        return Vec::new();
    }

    let mut results = Vec::new();
    for extractor in [try_malloc_sub, try_realloc_sub, try_with_capacity_sub] {
        if let Some(site) = extractor(trimmed, line_no) {
            results.push(site);
        }
    }
    results
}

fn try_malloc_sub(trimmed: &str, line_no: usize) -> Option<ExtractedSite> {
    parse_sub(trimmed, line_no, malloc_sub_re())
}

fn try_realloc_sub(trimmed: &str, line_no: usize) -> Option<ExtractedSite> {
    parse_sub(trimmed, line_no, realloc_sub_re())
}

fn try_with_capacity_sub(trimmed: &str, line_no: usize) -> Option<ExtractedSite> {
    parse_sub(trimmed, line_no, with_capacity_sub_re())
}

fn parse_sub(trimmed: &str, line_no: usize, re: &Regex) -> Option<ExtractedSite> {
    let caps = re.captures(trimmed)?;
    let var = caps.get(1)?.as_str();
    let sub_lit = caps.get(2)?.as_str();

    if !is_identifier(var) || is_constant_identifier(var) {
        return None;
    }

    let subtractor = sub_lit.parse::<u64>().ok()?;
    if subtractor == 0 {
        return None;
    }

    Some(ExtractedSite::SymbolicSub(SubtractionSite {
        raw: trimmed.to_string(),
        line: line_no,
        var: var.to_string(),
        subtractor,
        subtractor_source: sub_lit.to_string(),
    }))
}

fn malloc_sub_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"malloc\s*\(\s*(\w+)\s*-\s*(\d+)\s*\)").unwrap())
}

fn realloc_sub_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"realloc\s*\([^,]+,\s*(\w+)\s*-\s*(\d+)\s*\)").unwrap()
    })
}

fn with_capacity_sub_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?:Vec::with_capacity|\.with_capacity)\s*\(\s*(\w+)\s*-\s*(\d+)\s*\)")
            .unwrap()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_malloc_subtraction() {
        let sites = extract_sites("p = malloc(n - 4);", 1);
        assert_eq!(sites.len(), 1);
        match &sites[0] {
            ExtractedSite::SymbolicSub(s) => {
                assert_eq!(s.var, "n");
                assert_eq!(s.subtractor, 4);
            }
            _ => panic!("expected SymbolicSub"),
        }
    }

    #[test]
    fn unrelated_line_emits_nothing() {
        assert!(extract_sites("return x + 1;", 1).is_empty());
    }
}
