//! Extract structured allocation sites from single-line code fragments (CWE-190).

use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllocKind {
    Malloc,
    Calloc,
    Realloc,
    WithCapacity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllocationSite {
    pub raw: String,
    pub line: usize,
    pub kind: AllocKind,
    pub var: String,
    pub elem_size: u64,
    pub elem_size_source: String,
}

/// Outcome of analyzing one potential allocation site on a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractedSite {
    /// Symbolic variable present — candidate for Z3.
    Symbolic(AllocationSite),
    /// Heuristic match but Z3 cannot run (multi-var, unresolvable sizeof, etc.).
    Inconclusive {
        raw: String,
        reason: String,
    },
    /// Heuristic match, parser could not structure — Phase 1 fallback.
    PatternOnly(String),
}

/// Extract allocation sites from a single source line (1-based `line_no`).
pub fn extract_sites(line: &str, line_no: usize) -> Vec<ExtractedSite> {
    let trimmed = line.trim();
    if !looks_like_allocation_line(trimmed) {
        return Vec::new();
    }

    let mut results = Vec::new();

    if let Some(site) = try_malloc(trimmed, line_no) {
        results.push(site);
    }
    if let Some(site) = try_calloc(trimmed, line_no) {
        results.push(site);
    }
    if let Some(site) = try_realloc(trimmed, line_no) {
        results.push(site);
    }
    if let Some(site) = try_vec_with_capacity(trimmed, line_no) {
        results.push(site);
    }
    if let Some(site) = try_dot_with_capacity(trimmed, line_no) {
        results.push(site);
    }

    if results.is_empty() && looks_like_allocation_line(trimmed) {
        if is_constant_only_allocation(trimmed) {
            return results;
        }
        if let Some(site) = try_unresolved_sizeof(trimmed) {
            results.push(site);
        } else {
            results.push(ExtractedSite::PatternOnly(trimmed.to_string()));
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
    MARKERS.iter().any(|m| trimmed.contains(m)) && trimmed.contains('*')
}

fn try_malloc(trimmed: &str, line_no: usize) -> Option<ExtractedSite> {
    let re_sizeof = malloc_sizeof_re();
    if let Some(caps) = re_sizeof.captures(trimmed) {
        let var = caps.get(1)?.as_str();
        let type_name = caps.get(2)?.as_str();
        return parse_var_times_size(trimmed, line_no, AllocKind::Malloc, var, type_name, None);
    }

    let re_const = malloc_const_re();
    if let Some(caps) = re_const.captures(trimmed) {
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
        if is_identifier(a) && is_identifier(b) && !is_constant_identifier(a) && !is_constant_identifier(b) {
            return Some(ExtractedSite::Inconclusive {
                raw: trimmed.to_string(),
                reason: format!("multiple symbolic variables: {a} * {b}"),
            });
        }
    }

    None
}

fn try_calloc(trimmed: &str, line_no: usize) -> Option<ExtractedSite> {
    let re_sizeof = calloc_sizeof_re();
    if let Some(caps) = re_sizeof.captures(trimmed) {
        let var = caps.get(1)?.as_str();
        let type_name = caps.get(2)?.as_str();
        return parse_var_times_size(trimmed, line_no, AllocKind::Calloc, var, type_name, None);
    }

    let re_const = calloc_const_re();
    if let Some(caps) = re_const.captures(trimmed) {
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

    None
}

fn try_realloc(trimmed: &str, line_no: usize) -> Option<ExtractedSite> {
    let re_sizeof = realloc_sizeof_re();
    if let Some(caps) = re_sizeof.captures(trimmed) {
        let var = caps.get(1)?.as_str();
        let type_name = caps.get(2)?.as_str();
        return parse_var_times_size(trimmed, line_no, AllocKind::Realloc, var, type_name, None);
    }

    let re_const = realloc_const_re();
    if let Some(caps) = re_const.captures(trimmed) {
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
    let re = vec_with_capacity_re();
    let caps = re.captures(trimmed)?;
    let var = caps.get(1)?.as_str();
    let rhs = caps.get(2)?.as_str();
    parse_with_capacity_rhs(trimmed, line_no, AllocKind::WithCapacity, var, rhs)
}

fn try_dot_with_capacity(trimmed: &str, line_no: usize) -> Option<ExtractedSite> {
    if trimmed.contains("Vec::with_capacity(") {
        return None;
    }
    let re = dot_with_capacity_re();
    let caps = re.captures(trimmed)?;
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
    if is_identifier(rhs) {
        return Some(ExtractedSite::Inconclusive {
            raw: trimmed.to_string(),
            reason: format!("multiple symbolic variables: {var} * {rhs}"),
        });
    }
    None
}

fn parse_var_times_size(
    trimmed: &str,
    line_no: usize,
    kind: AllocKind,
    var: &str,
    type_name: &str,
    literal: Option<(u64, String)>,
) -> Option<ExtractedSite> {
    if is_constant_identifier(var) {
        return None;
    }

    if !is_identifier(var) {
        return Some(ExtractedSite::Inconclusive {
            raw: trimmed.to_string(),
            reason: format!("non-identifier variable: {var}"),
        });
    }

    let (elem_size, elem_size_source) = if let Some((size, src)) = literal {
        (size, src)
    } else {
        let resolved = resolve_sizeof(type_name)?;
        let src = format!("sizeof({type_name})");
        (resolved, src)
    };

    if elem_size == 0 {
        return Some(ExtractedSite::Inconclusive {
            raw: trimmed.to_string(),
            reason: "element size is zero".into(),
        });
    }

    Some(ExtractedSite::Symbolic(AllocationSite {
        raw: trimmed.to_string(),
        line: line_no,
        kind,
        var: var.to_string(),
        elem_size,
        elem_size_source,
    }))
}

fn is_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars().next().is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && s.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn is_constant_identifier(s: &str) -> bool {
    s.parse::<u64>().is_ok()
}

/// Resolve `sizeof(TYPE)` to a byte size for common C/Rust primitive names.
pub fn resolve_sizeof(type_name: &str) -> Option<u64> {
    match type_name.trim() {
        "char" | "signed char" | "unsigned char" | "uint8_t" | "int8_t" | "i8" | "u8" => Some(1),
        "short" | "unsigned short" | "int16_t" | "uint16_t" | "i16" | "u16" => Some(2),
        "int" | "unsigned int" | "int32_t" | "uint32_t" | "float" | "i32" | "u32" | "f32" => {
            Some(4)
        }
        "long"
        | "unsigned long"
        | "int64_t"
        | "uint64_t"
        | "double"
        | "long long"
        | "unsigned long long"
        | "i64"
        | "u64"
        | "f64"
        | "size_t"
        | "ssize_t"
        | "uintptr_t"
        | "intptr_t" => Some(8),
        _ => None,
    }
}

fn is_constant_only_allocation(trimmed: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?:malloc|calloc|realloc)\s*\(\s*(\d+)\s*\*\s*(sizeof\s*\([^)]+\)|\d+)\s*\)",
        )
        .unwrap()
    });
    let Some(caps) = re.captures(trimmed) else {
        return false;
    };
    let rhs = caps.get(2).map(|m| m.as_str()).unwrap_or("");
    if let Some(stripped) = rhs.strip_prefix("sizeof(").and_then(|s| s.strip_suffix(')')) {
        return resolve_sizeof(stripped).is_some();
    }
    rhs.parse::<u64>().is_ok()
}

fn try_unresolved_sizeof(trimmed: &str) -> Option<ExtractedSite> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"sizeof\s*\(\s*([^)]+)\s*\)").unwrap());
    let type_name = re.captures(trimmed)?.get(1)?.as_str();
    if resolve_sizeof(type_name).is_none() {
        return Some(ExtractedSite::Inconclusive {
            raw: trimmed.to_string(),
            reason: format!("cannot resolve sizeof({type_name})"),
        });
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;

    fn symbolic(line: &str) -> Option<AllocationSite> {
        extract_sites(line, 1)
            .into_iter()
            .find_map(|s| match s {
                ExtractedSite::Symbolic(site) => Some(site),
                _ => None,
            })
    }

    #[test]
    fn parses_malloc_sizeof_int() {
        let site = symbolic("int* p = malloc(n * sizeof(int));").unwrap();
        assert_eq!(site.var, "n");
        assert_eq!(site.elem_size, 4);
        assert_eq!(site.elem_size_source, "sizeof(int)");
        assert_eq!(site.kind, AllocKind::Malloc);
    }

    #[test]
    fn parses_malloc_var_times_const() {
        let site = symbolic("char* buf = malloc(count * 4);").unwrap();
        assert_eq!(site.var, "count");
        assert_eq!(site.elem_size, 4);
        assert_eq!(site.elem_size_source, "4");
    }

    #[test]
    fn parses_calloc_sizeof() {
        let site = symbolic("void* p = calloc(n, sizeof(int));").unwrap();
        assert_eq!(site.var, "n");
        assert_eq!(site.elem_size, 4);
        assert_eq!(site.kind, AllocKind::Calloc);
    }

    #[test]
    fn parses_calloc_const() {
        let site = symbolic("void* p = calloc(items, 8);").unwrap();
        assert_eq!(site.var, "items");
        assert_eq!(site.elem_size, 8);
    }

    #[test]
    fn parses_realloc_sizeof() {
        let site = symbolic("p = realloc(p, n * sizeof(int));").unwrap();
        assert_eq!(site.var, "n");
        assert_eq!(site.kind, AllocKind::Realloc);
    }

    #[test]
    fn parses_vec_with_capacity() {
        let site = symbolic("let v = Vec::with_capacity(n * 4);").unwrap();
        assert_eq!(site.var, "n");
        assert_eq!(site.elem_size, 4);
        assert_eq!(site.kind, AllocKind::WithCapacity);
    }

    #[test]
    fn parses_dot_with_capacity() {
        let site = symbolic("v.with_capacity(n * 8);").unwrap();
        assert_eq!(site.var, "n");
        assert_eq!(site.elem_size, 8);
    }

    #[test]
    fn constant_only_malloc_emits_nothing() {
        let sites = extract_sites("int* p = malloc(100 * sizeof(int));", 1);
        assert!(sites.is_empty());
    }

    #[test]
    fn two_variables_is_inconclusive() {
        let sites = extract_sites("void* p = malloc(a * b);", 1);
        assert_eq!(sites.len(), 1);
        assert!(matches!(
            sites[0],
            ExtractedSite::Inconclusive { .. }
        ));
    }

    #[test]
    fn unknown_sizeof_is_inconclusive() {
        let sites = extract_sites(
            "malloc(foo(a, b) * sizeof(struct Unknown));",
            1,
        );
        assert_eq!(sites.len(), 1);
        assert!(matches!(
            sites[0],
            ExtractedSite::Inconclusive { .. }
        ));
    }

    #[test]
    fn unrelated_line_emits_nothing() {
        let sites = extract_sites("return x + 1;", 1);
        assert!(sites.is_empty());
    }

    #[test]
    fn heuristic_malloc_two_vars_is_inconclusive() {
        let sites = extract_sites("malloc(weird * stuff);", 1);
        assert_eq!(sites.len(), 1);
        assert!(matches!(sites[0], ExtractedSite::Inconclusive { .. }));
    }

    #[test]
    fn resolve_sizeof_primitives() {
        assert_eq!(resolve_sizeof("int"), Some(4));
        assert_eq!(resolve_sizeof("uint64_t"), Some(8));
        assert_eq!(resolve_sizeof("struct Foo"), None);
    }
}
