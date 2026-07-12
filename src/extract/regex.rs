use crate::config::{detect_language, Language};
use crate::imports::RawImport;
use crate::types::{SymbolEntry, SymbolKind};
use regex::Regex;
use std::path::Path;

type Rule = (Regex, SymbolKind);

fn rules_for(lang: &Language) -> Vec<Rule> {
    match lang {
        Language::Rust => rust_rules(),
        Language::Go => go_rules(),
        Language::Python => python_rules(),
        Language::TypeScript => typescript_rules(),
        Language::CSharp => csharp_rules(),
    }
}

fn compile(patterns: &[(&str, SymbolKind)]) -> Vec<Rule> {
    patterns
        .iter()
        .map(|(pat, kind)| (Regex::new(pat).expect("invalid regex"), kind.clone()))
        .collect()
}

fn rust_rules() -> Vec<Rule> {
    compile(&[
        (r#"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+"[^"]*"\s+)?fn\s+(\w+)"#, SymbolKind::Function),
        (r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?trait\s+(\w+)", SymbolKind::Trait),
        (r"^\s*(?:pub(?:\([^)]*\))?\s+)?struct\s+(\w+)", SymbolKind::Struct),
        (r"^\s*(?:pub(?:\([^)]*\))?\s+)?enum\s+(\w+)", SymbolKind::Enum),
        (r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)", SymbolKind::Module),
        (r"^\s*(?:pub(?:\([^)]*\))?\s+)?type\s+(\w+)", SymbolKind::Other),
        (r"^\s*(?:pub(?:\([^)]*\))?\s+)?const\s+(\w+)", SymbolKind::Variable),
        (r"^\s*macro_rules!\s*(\w+)", SymbolKind::Other),
    ])
}

fn go_rules() -> Vec<Rule> {
    compile(&[
        (r"^\s*type\s+(\w+)\s+struct", SymbolKind::Struct),
        (r"^\s*type\s+(\w+)\s+interface", SymbolKind::Trait),
        (r"^\s*func\s+\([^)]*\)\s+(\w+)", SymbolKind::Function),
        (r"^\s*func\s+(\w+)", SymbolKind::Function),
        (r"^\s*type\s+(\w+)", SymbolKind::Other),
    ])
}

fn python_rules() -> Vec<Rule> {
    compile(&[
        (r"^(?:async\s+)?def\s+(\w+)", SymbolKind::Function),
        (r"^class\s+(\w+)", SymbolKind::Struct),
    ])
}

fn typescript_rules() -> Vec<Rule> {
    compile(&[
        (r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s+(?:\*\s+)?(\w+)", SymbolKind::Function),
        (r"^\s*(?:export\s+)?(?:default\s+)?(?:abstract\s+)?class\s+(\w+)", SymbolKind::Struct),
        (r"^\s*(?:export\s+)?(?:default\s+)?interface\s+(\w+)", SymbolKind::Trait),
        (r"^\s*(?:export\s+)?type\s+(\w+)", SymbolKind::Other),
        (r"^\s*(?:export\s+)?(?:const|let|var)\s+(\w+)", SymbolKind::Variable),
    ])
}

fn csharp_rules() -> Vec<Rule> {
    compile(&[
        (r"^\s*(?:\[.*\]\s*)*(?:(?:public|private|protected|internal|static|virtual|override|abstract|sealed|readonly|async|unsafe|partial|new|extern)\s+)*class\s+(\w+)", SymbolKind::Struct),
        (r"^\s*(?:\[.*\]\s*)*(?:(?:public|private|protected|internal|static|virtual|override|abstract|sealed|readonly|async|unsafe|partial|new|extern)\s+)*interface\s+(\w+)", SymbolKind::Trait),
        (r"^\s*(?:\[.*\]\s*)*(?:(?:public|private|protected|internal|static|virtual|override|abstract|sealed|readonly|async|unsafe|partial|new|extern)\s+)*struct\s+(\w+)", SymbolKind::Struct),
        (r"^\s*(?:\[.*\]\s*)*(?:(?:public|private|protected|internal|static|virtual|override|abstract|sealed|readonly|async|unsafe|partial|new|extern)\s+)*enum\s+(\w+)", SymbolKind::Enum),
        (r"^\s*(?:\[.*\]\s*)*(?:(?:public|private|protected|internal|static|virtual|override|abstract|sealed|readonly|async|unsafe|partial|new|extern)\s+)*record\s+(\w+)", SymbolKind::Struct),
        (r"^\s*(?:\[.*\]\s*)*(?:public|private|protected|internal|static|virtual|override|abstract|sealed|readonly|async|unsafe|partial|new|extern)\s+(?:\w+(?:<[^>]*>)?\s+)*(\w+)\s*\(", SymbolKind::Function),
    ])
}

/// Extract symbols from source using line-anchored regex patterns.
pub fn extract_symbols_from_code(code: &str, lang: &Language) -> Vec<SymbolEntry> {
    let rules = rules_for(lang);
    let mut symbols = Vec::new();

    for (line_num, line) in code.lines().enumerate() {
        let match_line = match lang {
            Language::Python => line,
            _ => line.trim(),
        };
        if match_line.is_empty() {
            continue;
        }

        for (re, kind) in &rules {
            if let Some(caps) = re.captures(match_line) {
                if let Some(name) = caps.get(1) {
                    symbols.push(SymbolEntry {
                        name: name.as_str().to_string(),
                        signature: match_line.to_string(),
                        kind: kind.clone(),
                        line: (line_num + 1) as u32,
                        character: 0,
                    });
                }
                break;
            }
        }
    }

    symbols
}

/// Extract symbols from a file path using regex.
pub fn extract_symbols(path: &Path) -> Vec<SymbolEntry> {
    let lang = match detect_language(path) {
        Some(l) => l,
        None => return vec![],
    };

    let code = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    extract_symbols_from_code(&code, &lang)
}

/// Extract raw imports from source using regex.
pub fn extract_imports_from_code(code: &str, lang: &Language) -> Vec<RawImport> {
    match lang {
        Language::Rust => extract_rust_imports(code),
        Language::TypeScript => extract_typescript_imports(code),
        Language::Python => extract_python_imports(code),
        Language::Go => extract_go_imports(code),
        Language::CSharp => extract_csharp_imports(code),
    }
}

/// Extract raw imports from a file path using regex.
pub fn extract_imports(path: &Path) -> Vec<RawImport> {
    let lang = match detect_language(path) {
        Some(l) => l,
        None => return vec![],
    };

    let code = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    extract_imports_from_code(&code, &lang)
}

/// Combined regex extraction from source text.
pub fn extract_from_parts(code: &str, lang: &Language) -> (Vec<SymbolEntry>, Vec<RawImport>) {
    (
        extract_symbols_from_code(code, lang),
        extract_imports_from_code(code, lang),
    )
}

/// Combined regex extraction of symbols and imports.
pub fn extract_all(path: &Path) -> (Vec<SymbolEntry>, Vec<RawImport>) {
    let lang = match detect_language(path) {
        Some(l) => l,
        None => return (vec![], vec![]),
    };

    let code = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (vec![], vec![]),
    };

    (
        extract_symbols_from_code(&code, &lang),
        extract_imports_from_code(&code, &lang),
    )
}

fn extract_rust_imports(code: &str) -> Vec<RawImport> {
    let mod_re = Regex::new(
        r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;",
    )
    .unwrap();
    let use_re = Regex::new(
        r"^\s*(?:pub(?:\([^)]*\))?\s+)?use\s+((?:crate|super)::[\w:]+)",
    )
    .unwrap();

    let mut imports = vec![];
    for (idx, line) in code.lines().enumerate() {
        let line_num = (idx + 1) as u32;
        if let Some(caps) = mod_re.captures(line) {
            if let Some(m) = caps.get(1) {
                imports.push(RawImport {
                    text: m.as_str().to_string(),
                    line: line_num,
                    character: m.start() as u32,
                });
            }
        }
        if let Some(caps) = use_re.captures(line) {
            if let Some(m) = caps.get(1) {
                imports.push(RawImport {
                    text: m.as_str().to_string(),
                    line: line_num,
                    character: m.start() as u32,
                });
            }
        }
    }
    imports
}

fn extract_typescript_imports(code: &str) -> Vec<RawImport> {
    let from_re =
        Regex::new(r#"(?:import|export)\s[\s\S]*?from\s+['"]([./][^'"]+)['"]"#).unwrap();
    let side_re = Regex::new(r#"import\s+['"]([./][^'"]+)['"]"#).unwrap();

    let mut imports = vec![];
    for re in [&from_re, &side_re] {
        for caps in re.captures_iter(code) {
            if let Some(m) = caps.get(1) {
                let text = m.as_str().to_string();
                if !text.starts_with('.') && !text.starts_with('/') {
                    continue;
                }
                let line = code[..m.start()].matches('\n').count() as u32 + 1;
                let line_start = code[..m.start()].rfind('\n').map(|i| i + 1).unwrap_or(0);
                let character = (m.start() - line_start) as u32;
                if !imports.iter().any(|r: &RawImport| r.text == text && r.line == line) {
                    imports.push(RawImport {
                        text,
                        line,
                        character,
                    });
                }
            }
        }
    }
    imports
}

fn extract_python_imports(code: &str) -> Vec<RawImport> {
    let from_re = Regex::new(r"^from\s+(\.+\w*|\w[\w.]*)\s+import").unwrap();
    let import_re = Regex::new(r"^import\s+([\w.]+)").unwrap();

    let mut imports = vec![];
    for (idx, line) in code.lines().enumerate() {
        let line_num = (idx + 1) as u32;
        let trimmed = line.trim_start();
        if let Some(caps) = from_re.captures(trimmed) {
            if let Some(m) = caps.get(1) {
                imports.push(RawImport {
                    text: m.as_str().to_string(),
                    line: line_num,
                    character: m.start() as u32,
                });
                continue;
            }
        }
        if let Some(caps) = import_re.captures(trimmed) {
            if let Some(m) = caps.get(1) {
                imports.push(RawImport {
                    text: m.as_str().to_string(),
                    line: line_num,
                    character: m.start() as u32,
                });
            }
        }
    }
    imports
}

fn extract_go_imports(code: &str) -> Vec<RawImport> {
    let single_re = Regex::new(r#"^\s*import\s+"([^"]+)""#).unwrap();
    let group_item_re = Regex::new(r#"^\s+"([^"]+)""#).unwrap();

    let mut imports = vec![];
    let mut in_group = false;
    for (idx, line) in code.lines().enumerate() {
        let line_num = (idx + 1) as u32;
        let trimmed = line.trim();
        if trimmed == "import (" {
            in_group = true;
            continue;
        }
        if in_group {
            if trimmed == ")" {
                in_group = false;
                continue;
            }
            if let Some(caps) = group_item_re.captures(line) {
                if let Some(m) = caps.get(1) {
                    imports.push(RawImport {
                        text: m.as_str().to_string(),
                        line: line_num,
                        character: m.start() as u32,
                    });
                }
            }
            continue;
        }
        if let Some(caps) = single_re.captures(line) {
            if let Some(m) = caps.get(1) {
                imports.push(RawImport {
                    text: m.as_str().to_string(),
                    line: line_num,
                    character: m.start() as u32,
                });
            }
        }
    }
    imports
}

fn extract_csharp_imports(code: &str) -> Vec<RawImport> {
    let using_re = Regex::new(r"^\s*using\s+(?:static\s+)?([\w.]+)\s*;").unwrap();
    let mut imports = vec![];
    for (idx, line) in code.lines().enumerate() {
        let line_num = (idx + 1) as u32;
        if let Some(caps) = using_re.captures(line.trim()) {
            if let Some(m) = caps.get(1) {
                let ns = m.as_str();
                if ns == "System" || ns.starts_with("System.") {
                    continue;
                }
                imports.push(RawImport {
                    text: m.as_str().to_string(),
                    line: line_num,
                    character: m.start() as u32,
                });
            }
        }
    }
    imports
}
