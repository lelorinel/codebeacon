use crate::config::{detect_language, Language};
use regex::Regex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A raw, unresolved import extracted from source text.
#[derive(Debug, Clone, PartialEq)]
pub struct RawImport {
    /// The import target as written in source (e.g. `"auth"`, `"./utils"`, `"fmt"`).
    pub text: String,
    /// 1-based line number of the import statement (for LSP phase position lookup).
    pub line: u32,
    /// 0-based character offset of the import target start within the line.
    pub character: u32,
}

/// Extract raw import targets from a source file.
/// Returns an empty vec for unsupported languages or unreadable files.
pub fn extract_imports(path: &Path) -> Vec<RawImport> {
    let lang = match detect_language(path) {
        Some(l) => l,
        None => return vec![],
    };

    let code = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    match lang {
        Language::Rust => extract_rust_imports(&code),
        Language::TypeScript => extract_typescript_imports(&code),
        Language::Python => extract_python_imports(&code),
        Language::Go => extract_go_imports(&code),
        Language::CSharp => vec![],
    }
}

/// Resolve raw imports to repo-relative paths that exist in `known`.
/// Unknown / external / unresolvable imports are silently dropped.
pub fn resolve_imports(
    repo_root: &Path,
    from_file_rel: &Path,
    raw: &[RawImport],
    lang: &Language,
    known: &HashSet<PathBuf>,
) -> Vec<PathBuf> {
    let mut result = vec![];
    for imp in raw {
        match lang {
            Language::Rust => {
                if let Some(p) = resolve_rust(from_file_rel, &imp.text, known) {
                    if !result.contains(&p) {
                        result.push(p);
                    }
                }
            }
            Language::TypeScript => {
                if let Some(p) = resolve_typescript(from_file_rel, &imp.text, known) {
                    if !result.contains(&p) {
                        result.push(p);
                    }
                }
            }
            Language::Python => {
                for p in resolve_python(from_file_rel, &imp.text, known) {
                    if !result.contains(&p) {
                        result.push(p);
                    }
                }
            }
            Language::Go => {
                for p in resolve_go(repo_root, &imp.text, known) {
                    if !result.contains(&p) {
                        result.push(p);
                    }
                }
            }
            Language::CSharp => {}
        }
    }
    result
}

// ---------------------------------------------------------------------------
// extract — language implementations
// ---------------------------------------------------------------------------

fn extract_rust_imports(code: &str) -> Vec<RawImport> {
    // mod foo; / pub mod foo; / pub(crate) mod foo;
    let mod_re = Regex::new(
        r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;",
    )
    .unwrap();
    // use crate::... / use super::...
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
    // import ... from './path' / export ... from '../path'
    // Side-effect: import './path'
    // Only capture paths starting with . (relative) or / (absolute repo paths).
    let from_re =
        Regex::new(r#"(?:import|export)\s[^;]*?from\s+['"]([./][^'"]+)['"]"#).unwrap();
    let side_re = Regex::new(r#"import\s+['"]([./][^'"]+)['"]"#).unwrap();

    let mut imports = vec![];
    for (idx, line) in code.lines().enumerate() {
        let line_num = (idx + 1) as u32;
        for re in &[&from_re, &side_re] {
            if let Some(caps) = re.captures(line) {
                if let Some(m) = caps.get(1) {
                    let text = m.as_str().to_string();
                    // Skip bare package imports that slipped through
                    if !text.starts_with('.') && !text.starts_with('/') {
                        continue;
                    }
                    if !imports.iter().any(|r: &RawImport| r.text == text && r.line == line_num) {
                        imports.push(RawImport {
                            text,
                            line: line_num,
                            character: m.start() as u32,
                        });
                    }
                }
            }
        }
    }
    imports
}

fn extract_python_imports(code: &str) -> Vec<RawImport> {
    // from .foo import ... / from foo.bar import ...
    let from_re = Regex::new(r"^from\s+(\.+\w*|\w[\w.]*)\s+import").unwrap();
    // import foo.bar (not `from`)
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
    // Single: import "path"
    let single_re = Regex::new(r#"^\s*import\s+"([^"]+)""#).unwrap();
    // Inside a group: \t"path"
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

// ---------------------------------------------------------------------------
// resolve — language implementations
// ---------------------------------------------------------------------------

fn resolve_rust(
    from_file_rel: &Path,
    text: &str,
    known: &HashSet<PathBuf>,
) -> Option<PathBuf> {
    if text.contains("::") {
        // `use crate::a::b::c` — strip "crate::" or "super::" prefix, convert to path
        let without_crate = text
            .trim_start_matches("crate::")
            .trim_start_matches("super::");
        let parts: Vec<&str> = without_crate.split("::").collect();

        // Try progressively shorter paths (last segment may be an item, not a file)
        for len in (1..=parts.len()).rev() {
            let seg = parts[..len].join("/");
            let candidates = vec![
                PathBuf::from(format!("src/{}.rs", seg)),
                PathBuf::from(format!("src/{}/mod.rs", seg)),
            ];
            if let Some(p) = try_candidates(&candidates, known) {
                return Some(p);
            }
        }
        None
    } else {
        // `mod foo;` — look in same directory as from_file_rel
        let dir = from_file_rel.parent().unwrap_or_else(|| Path::new(""));
        let candidates = vec![
            dir.join(format!("{}.rs", text)),
            dir.join(format!("{}/mod.rs", text)),
        ];
        try_candidates(&candidates, known)
    }
}

fn resolve_typescript(
    from_file_rel: &Path,
    text: &str,
    known: &HashSet<PathBuf>,
) -> Option<PathBuf> {
    // Only handle relative imports
    if !text.starts_with('.') {
        return None;
    }

    let base = from_file_rel
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let joined = base.join(text);
    // Normalize the path (resolve .. and .)
    let normalized = normalize_path(&joined);

    let candidates = vec![
        normalized.with_extension("ts"),
        normalized.with_extension("tsx"),
        normalized.join("index.ts"),
        normalized.join("index.tsx"),
        normalized.with_extension("js"),
    ];
    try_candidates(&candidates, known)
}

fn resolve_python(
    from_file_rel: &Path,
    text: &str,
    known: &HashSet<PathBuf>,
) -> Vec<PathBuf> {
    let mut result = vec![];

    if text.starts_with('.') {
        // Relative import: count leading dots
        let dots = text.chars().take_while(|c| *c == '.').count();
        let module = text.trim_start_matches('.');

        // Start from the file's directory, go up `dots - 1` levels
        let mut base = from_file_rel
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        for _ in 1..dots {
            base = base.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        }

        if module.is_empty() {
            // `from . import x` — the package itself
            let candidates = vec![
                base.join("__init__.py"),
            ];
            for c in &candidates {
                if known.contains(c) {
                    result.push(c.clone());
                }
            }
        } else {
            let mod_path = module.replace('.', "/");
            let candidates = vec![
                base.join(format!("{}.py", mod_path)),
                base.join(format!("{}/__init__.py", mod_path)),
            ];
            for c in &candidates {
                if known.contains(c) {
                    result.push(c.clone());
                }
            }
        }
    } else {
        // Absolute import: a.b.c → a/b/c.py | a/b/c/__init__.py
        // Also try src/ prefix
        let mod_path = text.replace('.', "/");
        let candidates = vec![
            PathBuf::from(format!("{}.py", mod_path)),
            PathBuf::from(format!("{}/__init__.py", mod_path)),
            PathBuf::from(format!("src/{}.py", mod_path)),
            PathBuf::from(format!("src/{}/__init__.py", mod_path)),
        ];
        for c in &candidates {
            if known.contains(c) {
                result.push(c.clone());
            }
        }
    }
    result
}

fn resolve_go(
    repo_root: &Path,
    text: &str,
    known: &HashSet<PathBuf>,
) -> Vec<PathBuf> {
    // Read go.mod to get the module prefix
    let module_name = read_go_module(repo_root);
    let pkg_rel = if let Some(ref module) = module_name {
        if text.starts_with(module.as_str()) {
            // Strip the module prefix + leading "/"
            let rest = &text[module.len()..];
            rest.trim_start_matches('/')
        } else {
            // External package — skip
            return vec![];
        }
    } else {
        // No go.mod — nothing we can resolve
        return vec![];
    };

    if pkg_rel.is_empty() {
        return vec![];
    }

    // All .go files in that directory that appear in known
    let dir = PathBuf::from(pkg_rel);
    known
        .iter()
        .filter(|p| {
            p.parent() == Some(dir.as_path())
                && p.extension().map(|e| e == "go").unwrap_or(false)
        })
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn try_candidates(candidates: &[PathBuf], known: &HashSet<PathBuf>) -> Option<PathBuf> {
    candidates.iter().find(|c| known.contains(*c)).cloned()
}

/// Read the module name from `<repo_root>/go.mod` (first `module` line).
fn read_go_module(repo_root: &Path) -> Option<String> {
    let content = std::fs::read_to_string(repo_root.join("go.mod")).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("module ") {
            let name = rest.trim().to_string();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

/// Lexically normalize a path by resolving `.` and `..` components.
fn normalize_path(path: &Path) -> PathBuf {
    let mut parts: Vec<std::ffi::OsString> = vec![];
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                parts.pop();
            }
            c => parts.push(c.as_os_str().to_owned()),
        }
    }
    parts.iter().collect()
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn known(paths: &[&str]) -> HashSet<PathBuf> {
        paths.iter().map(|p| PathBuf::from(p)).collect()
    }

    fn write_file(dir: &Path, rel: &str, content: &str) -> PathBuf {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();
        path
    }

    // -----------------------------------------------------------------------
    // extract_imports — Rust
    // -----------------------------------------------------------------------

    #[test]
    fn rust_extracts_mod_declaration() {
        let tmp = TempDir::new().unwrap();
        let path = write_file(tmp.path(), "src/lib.rs", "pub mod auth;\npub mod db;\n");
        let imports = extract_imports(&path);
        assert_eq!(imports.len(), 2);
        assert!(imports.iter().any(|r| r.text == "auth"));
        assert!(imports.iter().any(|r| r.text == "db"));
    }

    #[test]
    fn rust_extracts_use_crate_path() {
        let tmp = TempDir::new().unwrap();
        let path = write_file(tmp.path(), "src/main.rs", "use crate::config::Language;\n");
        let imports = extract_imports(&path);
        assert!(imports.iter().any(|r| r.text == "crate::config::Language"));
    }

    #[test]
    fn rust_records_correct_line_number() {
        let tmp = TempDir::new().unwrap();
        let path = write_file(tmp.path(), "src/lib.rs", "// comment\npub mod auth;\n");
        let imports = extract_imports(&path);
        let auth = imports.iter().find(|r| r.text == "auth").unwrap();
        assert_eq!(auth.line, 2);
    }

    // -----------------------------------------------------------------------
    // extract_imports — TypeScript
    // -----------------------------------------------------------------------

    #[test]
    fn typescript_extracts_relative_import() {
        let tmp = TempDir::new().unwrap();
        let path = write_file(
            tmp.path(),
            "src/app.ts",
            "import { foo } from './utils';\nimport bar from '../lib';\n",
        );
        let imports = extract_imports(&path);
        assert!(imports.iter().any(|r| r.text == "./utils"));
        assert!(imports.iter().any(|r| r.text == "../lib"));
    }

    #[test]
    fn typescript_skips_bare_package_import() {
        let tmp = TempDir::new().unwrap();
        let path = write_file(tmp.path(), "src/app.ts", "import React from 'react';\n");
        let imports = extract_imports(&path);
        // bare "react" should NOT appear (it's an external package)
        assert!(!imports.iter().any(|r| r.text == "react"));
    }

    // -----------------------------------------------------------------------
    // extract_imports — Python
    // -----------------------------------------------------------------------

    #[test]
    fn python_extracts_relative_from_import() {
        let tmp = TempDir::new().unwrap();
        let path = write_file(tmp.path(), "src/app.py", "from .utils import helper\n");
        let imports = extract_imports(&path);
        assert!(imports.iter().any(|r| r.text == ".utils"));
    }

    #[test]
    fn python_extracts_absolute_import() {
        let tmp = TempDir::new().unwrap();
        let path = write_file(tmp.path(), "src/app.py", "import auth.login\n");
        let imports = extract_imports(&path);
        assert!(imports.iter().any(|r| r.text == "auth.login"));
    }

    // -----------------------------------------------------------------------
    // extract_imports — Go
    // -----------------------------------------------------------------------

    #[test]
    fn go_extracts_single_import() {
        let tmp = TempDir::new().unwrap();
        let path = write_file(
            tmp.path(),
            "cmd/main.go",
            "import \"mymod/pkg/auth\"\n",
        );
        let imports = extract_imports(&path);
        assert!(imports.iter().any(|r| r.text == "mymod/pkg/auth"));
    }

    #[test]
    fn go_extracts_grouped_imports() {
        let tmp = TempDir::new().unwrap();
        let path = write_file(
            tmp.path(),
            "cmd/main.go",
            "import (\n\t\"mymod/pkg/auth\"\n\t\"fmt\"\n)\n",
        );
        let imports = extract_imports(&path);
        assert!(imports.iter().any(|r| r.text == "mymod/pkg/auth"));
        assert!(imports.iter().any(|r| r.text == "fmt"));
    }

    // -----------------------------------------------------------------------
    // resolve_imports — Rust
    // -----------------------------------------------------------------------

    #[test]
    fn rust_mod_resolves_to_sibling_rs() {
        let raw = vec![RawImport { text: "auth".into(), line: 1, character: 8 }];
        let k = known(&["src/auth.rs", "src/lib.rs"]);
        let result = resolve_imports(
            Path::new("/repo"),
            Path::new("src/lib.rs"),
            &raw,
            &Language::Rust,
            &k,
        );
        assert!(result.contains(&PathBuf::from("src/auth.rs")));
    }

    #[test]
    fn rust_mod_resolves_to_mod_rs() {
        let raw = vec![RawImport { text: "graph".into(), line: 1, character: 8 }];
        let k = known(&["src/graph/mod.rs", "src/lib.rs"]);
        let result = resolve_imports(
            Path::new("/repo"),
            Path::new("src/lib.rs"),
            &raw,
            &Language::Rust,
            &k,
        );
        assert!(result.contains(&PathBuf::from("src/graph/mod.rs")));
    }

    #[test]
    fn rust_use_crate_resolves_module_file() {
        let raw = vec![RawImport { text: "crate::config".into(), line: 1, character: 4 }];
        let k = known(&["src/config.rs"]);
        let result = resolve_imports(
            Path::new("/repo"),
            Path::new("src/indexer/mod.rs"),
            &raw,
            &Language::Rust,
            &k,
        );
        assert!(result.contains(&PathBuf::from("src/config.rs")));
    }

    #[test]
    fn rust_unknown_import_is_dropped() {
        let raw = vec![RawImport { text: "nonexistent".into(), line: 1, character: 8 }];
        let k = known(&["src/lib.rs"]);
        let result = resolve_imports(
            Path::new("/repo"),
            Path::new("src/lib.rs"),
            &raw,
            &Language::Rust,
            &k,
        );
        assert!(result.is_empty());
    }

    // -----------------------------------------------------------------------
    // resolve_imports — TypeScript
    // -----------------------------------------------------------------------

    #[test]
    fn typescript_relative_resolves_with_ts_extension() {
        let raw = vec![RawImport { text: "./utils".into(), line: 1, character: 21 }];
        let k = known(&["src/utils.ts", "src/app.ts"]);
        let result = resolve_imports(
            Path::new("/repo"),
            Path::new("src/app.ts"),
            &raw,
            &Language::TypeScript,
            &k,
        );
        assert!(result.contains(&PathBuf::from("src/utils.ts")));
    }

    #[test]
    fn typescript_relative_resolves_index_ts() {
        let raw = vec![RawImport { text: "./components".into(), line: 1, character: 21 }];
        let k = known(&["src/components/index.ts", "src/app.ts"]);
        let result = resolve_imports(
            Path::new("/repo"),
            Path::new("src/app.ts"),
            &raw,
            &Language::TypeScript,
            &k,
        );
        assert!(result.contains(&PathBuf::from("src/components/index.ts")));
    }

    #[test]
    fn typescript_bare_import_is_dropped() {
        let raw = vec![RawImport { text: "react".into(), line: 1, character: 7 }];
        let k = known(&["src/app.ts"]);
        let result = resolve_imports(
            Path::new("/repo"),
            Path::new("src/app.ts"),
            &raw,
            &Language::TypeScript,
            &k,
        );
        assert!(result.is_empty());
    }

    // -----------------------------------------------------------------------
    // resolve_imports — Python
    // -----------------------------------------------------------------------

    #[test]
    fn python_relative_resolves_sibling_py() {
        let raw = vec![RawImport { text: ".utils".into(), line: 1, character: 5 }];
        let k = known(&["src/utils.py", "src/app.py"]);
        let result = resolve_imports(
            Path::new("/repo"),
            Path::new("src/app.py"),
            &raw,
            &Language::Python,
            &k,
        );
        assert!(result.contains(&PathBuf::from("src/utils.py")));
    }

    #[test]
    fn python_absolute_resolves_from_repo_root() {
        let raw = vec![RawImport { text: "auth.login".into(), line: 1, character: 7 }];
        let k = known(&["auth/login.py"]);
        let result = resolve_imports(
            Path::new("/repo"),
            Path::new("main.py"),
            &raw,
            &Language::Python,
            &k,
        );
        assert!(result.contains(&PathBuf::from("auth/login.py")));
    }

    // -----------------------------------------------------------------------
    // resolve_imports — Go
    // -----------------------------------------------------------------------

    #[test]
    fn go_import_strips_module_prefix() {
        let tmp = TempDir::new().unwrap();
        // Write a go.mod so resolve_go can find the module name
        write_file(tmp.path(), "go.mod", "module mymod\n\ngo 1.21\n");
        let raw = vec![RawImport { text: "mymod/pkg/auth".into(), line: 1, character: 8 }];
        // go.mod: module mymod
        let k = known(&["pkg/auth/handler.go", "pkg/auth/models.go"]);
        let result = resolve_imports(
            tmp.path(),
            Path::new("cmd/main.go"),
            &raw,
            &Language::Go,
            &k,
        );
        assert!(result.contains(&PathBuf::from("pkg/auth/handler.go")));
        assert!(result.contains(&PathBuf::from("pkg/auth/models.go")));
    }

    #[test]
    fn go_external_import_is_dropped() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "go.mod", "module mymod\n\ngo 1.21\n");
        let raw = vec![RawImport { text: "fmt".into(), line: 1, character: 8 }];
        let k = known(&["cmd/main.go"]);
        let result = resolve_imports(
            tmp.path(),
            Path::new("cmd/main.go"),
            &raw,
            &Language::Go,
            &k,
        );
        assert!(result.is_empty());
    }
}
