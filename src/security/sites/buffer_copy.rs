//! Buffer copy site extraction (CWE-680).

use super::{
    is_constant_identifier, is_identifier, resolve_sizeof, BufferCopyKind, BufferCopySite,
    ExtractedSite,
};
use regex::Regex;
use std::sync::OnceLock;

pub fn extract_sites(line: &str, line_no: usize) -> Vec<ExtractedSite> {
    let trimmed = line.trim();
    if !trimmed.contains("memcpy(") && !trimmed.contains("memset(") {
        return Vec::new();
    }

    let mut results = Vec::new();
    if let Some(site) = try_memcpy(trimmed, line_no) {
        results.push(site);
    }
    if let Some(site) = try_memset(trimmed, line_no) {
        results.push(site);
    }
    results
}

fn try_memcpy(trimmed: &str, line_no: usize) -> Option<ExtractedSite> {
    if let Some(caps) = memcpy_sizeof_re().captures(trimmed) {
        let var = caps.get(1)?.as_str();
        let type_name = caps.get(2)?.as_str();
        return parse_var_times_size(trimmed, line_no, BufferCopyKind::Memcpy, var, type_name, None);
    }

    if let Some(caps) = memcpy_const_re().captures(trimmed) {
        let var = caps.get(1)?.as_str();
        let lit = caps.get(2)?.as_str();
        let elem = lit.parse::<u64>().ok()?;
        return parse_var_times_size(
            trimmed,
            line_no,
            BufferCopyKind::Memcpy,
            var,
            "",
            Some((elem, lit.to_string())),
        );
    }

    None
}

fn try_memset(trimmed: &str, line_no: usize) -> Option<ExtractedSite> {
    if let Some(caps) = memset_sizeof_re().captures(trimmed) {
        let var = caps.get(1)?.as_str();
        let type_name = caps.get(2)?.as_str();
        return parse_var_times_size(trimmed, line_no, BufferCopyKind::Memset, var, type_name, None);
    }

    if let Some(caps) = memset_const_re().captures(trimmed) {
        let var = caps.get(1)?.as_str();
        let lit = caps.get(2)?.as_str();
        let elem = lit.parse::<u64>().ok()?;
        return parse_var_times_size(
            trimmed,
            line_no,
            BufferCopyKind::Memset,
            var,
            "",
            Some((elem, lit.to_string())),
        );
    }

    None
}

fn parse_var_times_size(
    trimmed: &str,
    line_no: usize,
    kind: BufferCopyKind,
    var: &str,
    type_name: &str,
    literal: Option<(u64, String)>,
) -> Option<ExtractedSite> {
    if is_constant_identifier(var) || !is_identifier(var) {
        return None;
    }

    let (elem_size, elem_size_source) = if let Some((size, src)) = literal {
        (size, src)
    } else {
        let resolved = resolve_sizeof(type_name)?;
        (resolved, format!("sizeof({type_name})"))
    };

    if elem_size == 0 {
        return None;
    }

    Some(ExtractedSite::SymbolicBufferCopy(BufferCopySite {
        raw: trimmed.to_string(),
        line: line_no,
        kind,
        var: var.to_string(),
        elem_size,
        elem_size_source,
    }))
}

fn memcpy_sizeof_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"memcpy\s*\([^,]+,\s*[^,]+,\s*(\w+)\s*\*\s*sizeof\s*\(\s*([^)]+)\s*\)")
            .unwrap()
    })
}

fn memcpy_const_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"memcpy\s*\([^,]+,\s*[^,]+,\s*(\w+)\s*\*\s*(\d+)\s*\)").unwrap()
    })
}

fn memset_sizeof_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"memset\s*\([^,]+,\s*[^,]+,\s*(\w+)\s*\*\s*sizeof\s*\(\s*([^)]+)\s*\)")
            .unwrap()
    })
}

fn memset_const_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"memset\s*\([^,]+,\s*[^,]+,\s*(\w+)\s*\*\s*(\d+)\s*\)").unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_memcpy_const() {
        let sites = extract_sites("memcpy(dst, src, n * 4);", 1);
        assert_eq!(sites.len(), 1);
        match &sites[0] {
            ExtractedSite::SymbolicBufferCopy(s) => {
                assert_eq!(s.var, "n");
                assert_eq!(s.elem_size, 4);
            }
            _ => panic!("expected SymbolicBufferCopy"),
        }
    }
}
