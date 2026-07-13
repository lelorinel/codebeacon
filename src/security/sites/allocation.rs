//! Allocation site extraction (CWE-190, CWE-131, shift overflow).

use super::{
    is_constant_identifier, is_constant_only_allocation, is_identifier, parse_var_times_size,
    try_unresolved_sizeof, AllocKind, ExtractedSite, ShiftSite, TwoVarMulSite,
};
use regex::Regex;
use std::sync::OnceLock;

pub fn extract_sites(line: &str, line_no: usize) -> Vec<ExtractedSite> {
    let trimmed = line.trim();
    if !looks_like_allocation_line(trimmed) {
        return Vec::new();
    }

    let mut results = Vec::new();

    for extractor in [
        try_malloc,
        try_calloc,
        try_realloc,
        try_vec_with_capacity,
        try_dot_with_capacity,
        try_shift_malloc,
        try_shift_with_capacity,
    ] {
        if let Some(site) = extractor(trimmed, line_no) {
            results.push(site);
        }
    }

    if results.is_empty() && looks_like_allocation_line(trimmed) {
        if is_constant_only_allocation(trimmed) {
            return results;
        }
        if let Some(site) = try_unresolved_sizeof(trimmed, line_no) {
            results.push(site);
        } else {
            results.push(ExtractedSite::PatternOnly {
                raw: trimmed.to_string(),
                line: line_no,
            });
        }
    }

    results
}

fn looks_like_allocation_line(trimmed: &str) -> bool {
    const MARKERS: &[&str] = &[
        "malloc(",
        "calloc(",
        "realloc(",
        "Vec::with_capacity(",
        "with_capacity(",
    ];
    MARKERS.iter().any(|m| trimmed.contains(m))
        && (trimmed.contains('*') || trimmed.contains("<<"))
}

fn try_malloc(trimmed: &str, line_no: usize) -> Option<ExtractedSite> {
    if let Some(caps) = malloc_sizeof_re().captures(trimmed) {
        let var = caps.get(1)?.as_str();
        let type_name = caps.get(2)?.as_str();
        return parse_var_times_size(trimmed, line_no, AllocKind::Malloc, var, type_name, None);
    }

    if let Some(caps) = malloc_const_re().captures(trimmed) {
        let var = caps.get(1)?.as_str();
        let lit = caps.get(2)?.as_str();
        let elem = lit.parse::<u64>().ok()?;
        return parse_var_times_size(
            trimmed,
            line_no,
            AllocKind::Malloc,
            var,
            "",
            Some((elem, lit.to_string())),
        );
    }

    if let Some(caps) = malloc_two_var_re().captures(trimmed) {
        let a = caps.get(1)?.as_str();
        let b = caps.get(2)?.as_str();
        if is_identifier(a)
            && is_identifier(b)
            && !is_constant_identifier(a)
            && !is_constant_identifier(b)
        {
            return Some(ExtractedSite::SymbolicTwoVarMul(TwoVarMulSite {
                raw: trimmed.to_string(),
                line: line_no,
                kind: AllocKind::Malloc,
                var_a: a.to_string(),
                var_b: b.to_string(),
            }));
        }
    }

    None
}

fn try_calloc(trimmed: &str, line_no: usize) -> Option<ExtractedSite> {
    if let Some(caps) = calloc_sizeof_re().captures(trimmed) {
        let var = caps.get(1)?.as_str();
        let type_name = caps.get(2)?.as_str();
        return parse_var_times_size(trimmed, line_no, AllocKind::Calloc, var, type_name, None);
    }

    if let Some(caps) = calloc_const_re().captures(trimmed) {
        let var = caps.get(1)?.as_str();
        let lit = caps.get(2)?.as_str();
        let elem = lit.parse::<u64>().ok()?;
        return parse_var_times_size(
            trimmed,
            line_no,
            AllocKind::Calloc,
            var,
            "",
            Some((elem, lit.to_string())),
        );
    }

    if let Some(caps) = calloc_two_var_re().captures(trimmed) {
        let a = caps.get(1)?.as_str();
        let b = caps.get(2)?.as_str();
        if is_identifier(a)
            && is_identifier(b)
            && !is_constant_identifier(a)
            && !is_constant_identifier(b)
        {
            return Some(ExtractedSite::SymbolicTwoVarMul(TwoVarMulSite {
                raw: trimmed.to_string(),
                line: line_no,
                kind: AllocKind::Calloc,
                var_a: a.to_string(),
                var_b: b.to_string(),
            }));
        }
    }

    None
}

fn try_realloc(trimmed: &str, line_no: usize) -> Option<ExtractedSite> {
    if let Some(caps) = realloc_sizeof_re().captures(trimmed) {
        let var = caps.get(1)?.as_str();
        let type_name = caps.get(2)?.as_str();
        return parse_var_times_size(trimmed, line_no, AllocKind::Realloc, var, type_name, None);
    }

    if let Some(caps) = realloc_const_re().captures(trimmed) {
        let var = caps.get(1)?.as_str();
        let lit = caps.get(2)?.as_str();
        let elem = lit.parse::<u64>().ok()?;
        return parse_var_times_size(
            trimmed,
            line_no,
            AllocKind::Realloc,
            var,
            "",
            Some((elem, lit.to_string())),
        );
    }

    None
}

fn try_vec_with_capacity(trimmed: &str, line_no: usize) -> Option<ExtractedSite> {
    let caps = vec_with_capacity_re().captures(trimmed)?;
    let var = caps.get(1)?.as_str();
    let rhs = caps.get(2)?.as_str();
    parse_with_capacity_rhs(trimmed, line_no, AllocKind::WithCapacity, var, rhs)
}

fn try_dot_with_capacity(trimmed: &str, line_no: usize) -> Option<ExtractedSite> {
    if trimmed.contains("Vec::with_capacity(") {
        return None;
    }
    let caps = dot_with_capacity_re().captures(trimmed)?;
    let var = caps.get(1)?.as_str();
    let rhs = caps.get(2)?.as_str();
    parse_with_capacity_rhs(trimmed, line_no, AllocKind::WithCapacity, var, rhs)
}

fn parse_with_capacity_rhs(
    trimmed: &str,
    line_no: usize,
    kind: AllocKind,
    var: &str,
    rhs: &str,
) -> Option<ExtractedSite> {
    let rhs = rhs.trim();
    if rhs.parse::<u64>().is_ok() {
        let elem = rhs.parse::<u64>().ok()?;
        return parse_var_times_size(trimmed, line_no, kind, var, "", Some((elem, rhs.to_string())));
    }
    if is_identifier(rhs) && !is_constant_identifier(rhs) && is_identifier(var) {
        return Some(ExtractedSite::SymbolicTwoVarMul(TwoVarMulSite {
            raw: trimmed.to_string(),
            line: line_no,
            kind,
            var_a: var.to_string(),
            var_b: rhs.to_string(),
        }));
    }
    None
}

fn try_shift_malloc(trimmed: &str, line_no: usize) -> Option<ExtractedSite> {
    let caps = malloc_shift_re().captures(trimmed)?;
    let var = caps.get(1)?.as_str();
    let shift_lit = caps.get(2)?.as_str();
    if !is_identifier(var) || is_constant_identifier(var) {
        return None;
    }
    let shift = shift_lit.parse::<u32>().ok()?;
    Some(ExtractedSite::SymbolicShift(ShiftSite {
        raw: trimmed.to_string(),
        line: line_no,
        kind: AllocKind::Malloc,
        var: var.to_string(),
        shift,
    }))
}

fn try_shift_with_capacity(trimmed: &str, line_no: usize) -> Option<ExtractedSite> {
    let caps = with_capacity_shift_re().captures(trimmed)?;
    let var = caps.get(1)?.as_str();
    let shift_lit = caps.get(2)?.as_str();
    if !is_identifier(var) || is_constant_identifier(var) {
        return None;
    }
    let shift = shift_lit.parse::<u32>().ok()?;
    Some(ExtractedSite::SymbolicShift(ShiftSite {
        raw: trimmed.to_string(),
        line: line_no,
        kind: AllocKind::WithCapacity,
        var: var.to_string(),
        shift,
    }))
}

fn malloc_sizeof_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"malloc\s*\(\s*(\w+)\s*\*\s*sizeof\s*\(\s*([^)]+)\s*\)").unwrap()
    })
}

fn malloc_const_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"malloc\s*\(\s*(\w+)\s*\*\s*(\d+)\s*\)").unwrap())
}

fn malloc_two_var_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"malloc\s*\(\s*(\w+)\s*\*\s*(\w+)\s*\)").unwrap())
}

fn calloc_sizeof_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"calloc\s*\(\s*(\w+)\s*,\s*sizeof\s*\(\s*([^)]+)\s*\)").unwrap()
    })
}

fn calloc_const_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"calloc\s*\(\s*(\w+)\s*,\s*(\d+)\s*\)").unwrap())
}

fn calloc_two_var_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"calloc\s*\(\s*(\w+)\s*,\s*(\w+)\s*\)").unwrap())
}

fn realloc_sizeof_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"realloc\s*\([^,]+,\s*(\w+)\s*\*\s*sizeof\s*\(\s*([^)]+)\s*\)").unwrap()
    })
}

fn realloc_const_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"realloc\s*\([^,]+,\s*(\w+)\s*\*\s*(\d+)\s*\)").unwrap())
}

fn vec_with_capacity_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"Vec::with_capacity\s*\(\s*(\w+)\s*\*\s*(\w+)\s*\)").unwrap()
    })
}

fn dot_with_capacity_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\.with_capacity\s*\(\s*(\w+)\s*\*\s*(\w+)\s*\)").unwrap())
}

fn malloc_shift_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"malloc\s*\(\s*(\w+)\s*<<\s*(\d+)\s*\)").unwrap())
}

fn with_capacity_shift_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?:Vec::with_capacity|\.with_capacity)\s*\(\s*(\w+)\s*<<\s*(\d+)\s*\)")
            .unwrap()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::sites::AllocationSite;

    fn symbolic_mul(line: &str) -> Option<AllocationSite> {
        extract_sites(line, 1).into_iter().find_map(|s| match s {
            ExtractedSite::SymbolicMul(site) => Some(site),
            _ => None,
        })
    }

    fn two_var(line: &str) -> Option<TwoVarMulSite> {
        extract_sites(line, 1).into_iter().find_map(|s| match s {
            ExtractedSite::SymbolicTwoVarMul(site) => Some(site),
            _ => None,
        })
    }

    #[test]
    fn parses_malloc_sizeof_int() {
        let site = symbolic_mul("int* p = malloc(n * sizeof(int));").unwrap();
        assert_eq!(site.var, "n");
        assert_eq!(site.elem_size, 4);
    }

    #[test]
    fn two_variables_is_symbolic() {
        let site = two_var("void* p = malloc(a * b);").unwrap();
        assert_eq!(site.var_a, "a");
        assert_eq!(site.var_b, "b");
    }

    #[test]
    fn calloc_two_var() {
        let site = two_var("void* p = calloc(n, m);").unwrap();
        assert_eq!(site.var_a, "n");
        assert_eq!(site.var_b, "m");
    }

    #[test]
    fn shift_malloc() {
        let site = extract_sites("p = malloc(n << 4);", 1)
            .into_iter()
            .find_map(|s| match s {
                ExtractedSite::SymbolicShift(s) => Some(s),
                _ => None,
            })
            .unwrap();
        assert_eq!(site.var, "n");
        assert_eq!(site.shift, 4);
    }

    #[test]
    fn constant_only_malloc_emits_nothing() {
        assert!(extract_sites("int* p = malloc(100 * sizeof(int));", 1).is_empty());
    }
}
