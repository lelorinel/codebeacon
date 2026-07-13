//! Shared site types and utilities for security extractors.

use regex::Regex;
use std::sync::OnceLock;

pub mod allocation;
pub mod buffer_copy;
pub mod division;
pub mod subtraction;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TwoVarMulSite {
    pub raw: String,
    pub line: usize,
    pub kind: AllocKind,
    pub var_a: String,
    pub var_b: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShiftSite {
    pub raw: String,
    pub line: usize,
    pub kind: AllocKind,
    pub var: String,
    pub shift: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtractionSite {
    pub raw: String,
    pub line: usize,
    pub var: String,
    pub subtractor: u64,
    pub subtractor_source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DivisionSite {
    pub raw: String,
    pub line: usize,
    pub dividend_var: String,
    pub divisor_var: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferCopySite {
    pub raw: String,
    pub line: usize,
    pub kind: BufferCopyKind,
    pub var: String,
    pub elem_size: u64,
    pub elem_size_source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BufferCopyKind {
    Memcpy,
    Memset,
}

/// Outcome of analyzing one potential security site on a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractedSite {
    SymbolicMul(AllocationSite),
    SymbolicTwoVarMul(TwoVarMulSite),
    SymbolicShift(ShiftSite),
    SymbolicSub(SubtractionSite),
    SymbolicDiv(DivisionSite),
    SymbolicBufferCopy(BufferCopySite),
    Inconclusive {
        raw: String,
        line: usize,
        reason: String,
    },
    PatternOnly {
        raw: String,
        line: usize,
    },
}

/// Markers for line quick-reject across all security checks.
const SECURITY_MARKERS: &[&str] = &[
    "malloc(",
    "calloc(",
    "realloc(",
    "Vec::with_capacity(",
    "with_capacity(",
    "memcpy(",
    "memset(",
    "printf(",
    "sprintf(",
    "fprintf(",
    "format!(",
    "system(",
    "exec(",
    "popen(",
    "subprocess",
    "pickle",
    "yaml",
    "unserialize",
    "readObject",
    "password",
    "api_key",
    "secret",
    "open(",
    "Path::new",
];

/// Returns true when the line may contain any security-relevant construct.
pub fn security_markers_present(line: &str) -> bool {
    SECURITY_MARKERS.iter().any(|m| line.contains(m))
        || line.contains('*')
        || line.contains(" << ")
        || line.contains(" - ")
        || line.contains(" / ")
        || line.contains("../")
}

pub fn is_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars().next().is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && s.chars()
            .all(|c| c == '_' || c.is_ascii_alphanumeric())
}

pub fn is_constant_identifier(s: &str) -> bool {
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

pub(crate) fn parse_var_times_size(
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
            line: line_no,
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
            line: line_no,
            reason: "element size is zero".into(),
        });
    }

    Some(ExtractedSite::SymbolicMul(AllocationSite {
        raw: trimmed.to_string(),
        line: line_no,
        kind,
        var: var.to_string(),
        elem_size,
        elem_size_source,
    }))
}

pub(crate) fn is_constant_only_allocation(trimmed: &str) -> bool {
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

pub(crate) fn try_unresolved_sizeof(trimmed: &str, line_no: usize) -> Option<ExtractedSite> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"sizeof\s*\(\s*([^)]+)\s*\)").unwrap());
    let type_name = re.captures(trimmed)?.get(1)?.as_str();
    if resolve_sizeof(type_name).is_none() {
        return Some(ExtractedSite::Inconclusive {
            raw: trimmed.to_string(),
            line: line_no,
            reason: format!("cannot resolve sizeof({type_name})"),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_markers_detect_allocation() {
        assert!(security_markers_present("malloc(n * 4)"));
        assert!(!security_markers_present("return x + 1;"));
    }

    #[test]
    fn resolve_sizeof_primitives() {
        assert_eq!(resolve_sizeof("int"), Some(4));
        assert_eq!(resolve_sizeof("uint64_t"), Some(8));
        assert_eq!(resolve_sizeof("struct Foo"), None);
    }
}
