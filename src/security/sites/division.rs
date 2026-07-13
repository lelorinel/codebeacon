//! Division site extraction (CWE-369 divide by zero).

use super::{is_constant_identifier, is_identifier, ExtractedSite, DivisionSite};
use regex::Regex;
use std::sync::OnceLock;

pub fn extract_sites(line: &str, line_no: usize) -> Vec<ExtractedSite> {
    let trimmed = line.trim();
    if !trimmed.contains(" / ") {
        return Vec::new();
    }

    let mut results = Vec::new();
    for extractor in [try_malloc_div, try_realloc_div, try_with_capacity_div] {
        if let Some(site) = extractor(trimmed, line_no) {
            results.push(site);
        }
    }
    results
}

fn try_malloc_div(trimmed: &str, line_no: usize) -> Option<ExtractedSite> {
    parse_div(trimmed, line_no, malloc_div_re())
}

fn try_realloc_div(trimmed: &str, line_no: usize) -> Option<ExtractedSite> {
    parse_div(trimmed, line_no, realloc_div_re())
}

fn try_with_capacity_div(trimmed: &str, line_no: usize) -> Option<ExtractedSite> {
    parse_div(trimmed, line_no, with_capacity_div_re())
}

fn parse_div(trimmed: &str, line_no: usize, re: &Regex) -> Option<ExtractedSite> {
    let caps = re.captures(trimmed)?;
    let dividend = caps.get(1)?.as_str();
    let divisor = caps.get(2)?.as_str();

    if !is_identifier(dividend)
        || !is_identifier(divisor)
        || is_constant_identifier(dividend)
        || is_constant_identifier(divisor)
    {
        return None;
    }

    Some(ExtractedSite::SymbolicDiv(DivisionSite {
        raw: trimmed.to_string(),
        line: line_no,
        dividend_var: dividend.to_string(),
        divisor_var: divisor.to_string(),
    }))
}

fn malloc_div_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"malloc\s*\(\s*(\w+)\s*/\s*(\w+)\s*\)").unwrap())
}

fn realloc_div_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"realloc\s*\([^,]+,\s*(\w+)\s*/\s*(\w+)\s*\)").unwrap())
}

fn with_capacity_div_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?:Vec::with_capacity|\.with_capacity)\s*\(\s*(\w+)\s*/\s*(\w+)\s*\)")
            .unwrap()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_malloc_division() {
        let sites = extract_sites("p = malloc(total / count);", 1);
        assert_eq!(sites.len(), 1);
        match &sites[0] {
            ExtractedSite::SymbolicDiv(s) => {
                assert_eq!(s.dividend_var, "total");
                assert_eq!(s.divisor_var, "count");
            }
            _ => panic!("expected SymbolicDiv"),
        }
    }
}
