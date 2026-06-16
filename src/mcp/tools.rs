use crate::config::codeindex_dir;
use crate::graph::{persistence as graph_persistence, DependencyGraph};
use crate::indexer::writer::{read_index, read_package};
use crate::lsp::pool::LspPool;
use crate::mcp::protocol::text_content;
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct ToolContext {
    pub repo_root: PathBuf,
    pub lsp_pool: Mutex<LspPool>,
}

impl ToolContext {
    fn codeindex(&self) -> PathBuf {
        codeindex_dir(&self.repo_root)
    }

    fn load_graph(&self) -> DependencyGraph {
        let path = self.codeindex().join("graph.bin");
        graph_persistence::load(&path).unwrap_or_default()
    }
}

pub fn dispatch(ctx: &ToolContext, name: &str, args: &Value) -> Result<Value> {
    match name {
        "get_context"     => handle_get_context(ctx, args),
        "drill_package"   => handle_drill_package(ctx, args),
        "find_references" => handle_find_references(ctx, args),
        "find_definition" => handle_find_definition(ctx, args),
        "get_dependents"  => handle_get_dependents(ctx, args),
        other => anyhow::bail!("unknown tool: {other}"),
    }
}

pub fn handle_get_context(ctx: &ToolContext, _args: &Value) -> Result<Value> {
    let index = read_index(&ctx.codeindex())?
        .context("No .codeindex/ found — run `codebeacon init` first")?;
    Ok(text_content(serde_json::to_string_pretty(&index)?))
}

pub fn handle_drill_package(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let name = args["name"].as_str().context("missing 'name'")?;
    let pkg = read_package(name, &ctx.codeindex())?
        .with_context(|| format!("package '{name}' not found"))?;
    Ok(text_content(serde_json::to_string_pretty(&pkg)?))
}

pub fn handle_find_references(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let symbol = args["symbol"].as_str().context("missing 'symbol'")?;

    // If caller provides file + position, use LSP for real usages
    if let (Some(file), Some(line), Some(character)) = (
        args["file"].as_str(),
        args["line"].as_u64(),
        args["character"].as_u64(),
    ) {
        let abs_path = if std::path::Path::new(file).is_absolute() {
            std::path::PathBuf::from(file)
        } else {
            ctx.repo_root.join(file)
        };
        if let Some(lang) = crate::config::detect_language(&abs_path) {
            let mut pool = ctx.lsp_pool.lock().unwrap();
            if let Some(client) = pool.get_or_start(&lang) {
                match client.references(&abs_path, line as u32, character as u32) {
                    Ok(result) => {
                        let refs = crate::lsp::parser::parse_references(&result);
                        if !refs.is_empty() {
                            let lines: Vec<String> = refs.iter().map(|r| {
                                let rel = r.file.strip_prefix(&ctx.repo_root).unwrap_or(&r.file);
                                format!("{}:{}", rel.display(), r.line)
                            }).collect();
                            return Ok(text_content(format!(
                                "References to '{}' (via LSP):\n{}", symbol, lines.join("\n")
                            )));
                        }
                    }
                    Err(e) => tracing::warn!("LSP references failed: {e}"),
                }
            }
        }
        // Fall through to index-based
    }

    // Try to resolve symbol position from index (auto-locate for LSP)
    // Walk index to find the symbol's definition location
    let packages_dir = ctx.codeindex().join("packages");
    let mut pkg_files: Vec<_> = std::fs::read_dir(&packages_dir)
        .into_iter().flatten().flatten()
        .filter_map(|e| e.path().to_str().map(str::to_string))
        .collect();
    pkg_files.sort();

    // Look up symbol in index to get its position, then try LSP references
    'outer: for pkg_path in &pkg_files {
        if let Ok(text) = std::fs::read_to_string(pkg_path) {
            if let Ok(pkg) = serde_json::from_str::<crate::types::PackageDetail>(&text) {
                for file in &pkg.files {
                    for sym in &file.symbols {
                        if sym.name == symbol {
                            let abs_path = ctx.repo_root.join(&file.path);
                            if let Some(lang) = crate::config::detect_language(&abs_path) {
                                let mut pool = ctx.lsp_pool.lock().unwrap();
                                if let Some(client) = pool.get_or_start(&lang) {
                                    match client.references(&abs_path, sym.line, sym.character) {
                                        Ok(result) => {
                                            let refs = crate::lsp::parser::parse_references(&result);
                                            if !refs.is_empty() {
                                                let lines: Vec<String> = refs.iter().map(|r| {
                                                    let rel = r.file.strip_prefix(&ctx.repo_root).unwrap_or(&r.file);
                                                    format!("{}:{}", rel.display(), r.line)
                                                }).collect();
                                                return Ok(text_content(format!(
                                                    "References to '{}' (via LSP from index):\n{}", symbol, lines.join("\n")
                                                )));
                                            }
                                        }
                                        Err(e) => tracing::warn!("LSP references (auto) failed: {e}"),
                                    }
                                }
                            }
                            break 'outer;
                        }
                    }
                }
            }
        }
    }

    // Final fallback: index substring search (old behaviour, clearly labelled)
    let mut found: Vec<String> = vec![];
    for pkg_path in &pkg_files {
        if let Ok(text) = std::fs::read_to_string(pkg_path) {
            if let Ok(pkg) = serde_json::from_str::<crate::types::PackageDetail>(&text) {
                for file in pkg.files {
                    for sym in &file.symbols {
                        if sym.name.contains(symbol) {
                            found.push(format!(
                                "{}:{} — {} [index fallback]",
                                file.path.display(), sym.line, sym.signature
                            ));
                        }
                    }
                }
            }
        }
    }

    if found.is_empty() {
        Ok(text_content(format!("No references found for '{symbol}'")))
    } else {
        Ok(text_content(found.join("\n")))
    }
}

pub fn handle_find_definition(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let symbol = args["symbol"].as_str().context("missing 'symbol'")?;

    // If caller provides file + position, try LSP first
    if let (Some(file), Some(line), Some(character)) = (
        args["file"].as_str(),
        args["line"].as_u64(),
        args["character"].as_u64(),
    ) {
        let abs_path = if std::path::Path::new(file).is_absolute() {
            std::path::PathBuf::from(file)
        } else {
            ctx.repo_root.join(file)
        };
        if let Some(lang) = crate::config::detect_language(&abs_path) {
            let mut pool = ctx.lsp_pool.lock().unwrap();
            if let Some(client) = pool.get_or_start(&lang) {
                match client.definition(&abs_path, line as u32, character as u32) {
                    Ok(result) => {
                        if let Some((def_path, def_line)) = crate::lsp::parser::parse_definition(&result) {
                            let rel = def_path.strip_prefix(&ctx.repo_root).unwrap_or(&def_path);
                            return Ok(text_content(format!(
                                "{}:{} (via LSP)", rel.display(), def_line
                            )));
                        }
                    }
                    Err(e) => tracing::warn!("LSP definition failed: {e}"),
                }
            }
        }
        // Fall through to index-based if LSP failed
    }

    // Index-based fallback: find ALL matching symbols, sorted deterministically
    let packages_dir = ctx.codeindex().join("packages");
    let mut found: Vec<String> = vec![];
    let mut pkg_files: Vec<_> = std::fs::read_dir(&packages_dir)
        .into_iter().flatten().flatten()
        .filter_map(|e| e.path().to_str().map(str::to_string))
        .collect();
    pkg_files.sort(); // deterministic order

    for pkg_path in pkg_files {
        if let Ok(text) = std::fs::read_to_string(&pkg_path) {
            if let Ok(pkg) = serde_json::from_str::<crate::types::PackageDetail>(&text) {
                for file in pkg.files {
                    for sym in &file.symbols {
                        if sym.name == symbol {
                            found.push(format!(
                                "{}:{} — {}",
                                file.path.display(), sym.line, sym.signature
                            ));
                        }
                    }
                }
            }
        }
    }

    if found.is_empty() {
        Ok(text_content(format!("Definition of '{symbol}' not found")))
    } else {
        Ok(text_content(found.join("\n")))
    }
}

pub fn handle_get_dependents(ctx: &ToolContext, args: &Value) -> Result<Value> {
    let file = args["file"].as_str().context("missing 'file'")?;
    let graph = ctx.load_graph();
    // Try absolute path first, fall back to relative path as stored in graph
    let abs_path = ctx.repo_root.join(file);
    let rel_path = PathBuf::from(file);
    let dependents = {
        let by_abs = graph.reverse_neighbors(&abs_path);
        if !by_abs.is_empty() {
            by_abs
        } else {
            graph.reverse_neighbors(&rel_path)
        }
    };
    if dependents.is_empty() {
        return Ok(text_content(format!("No files depend on '{file}'")));
    }
    let lines: Vec<String> = dependents.iter()
        .map(|p| p.strip_prefix(&ctx.repo_root).unwrap_or(p).display().to_string())
        .collect();
    Ok(text_content(lines.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn setup_codeindex(tmp: &TempDir) {
        use crate::indexer::writer::{write_index, write_package};
        let idx = RepoIndex {
            repo: "test".into(),
            generated_at: "2026-06-16T00:00:00Z".into(),
            packages: vec![
                PackageSummary { name: "auth".into(), purpose: "auth".into(), files: 1, score: 0.9 },
            ],
            hot_symbols: vec!["login".into()],
        };
        let pkg = PackageDetail {
            name: "auth".into(),
            files: vec![FileEntry {
                path: PathBuf::from("src/auth.rs"),
                symbols: vec![SymbolEntry { name: "login".into(), signature: "fn login()".into(), kind: SymbolKind::Function, line: 1, character: 0 }],
                depends_on: vec![],
                depended_by: vec![],
            }],
        };
        let ci = tmp.path().join(".codeindex");
        write_index(&idx, &ci).unwrap();
        write_package(&pkg, &ci).unwrap();
    }

    #[test]
    fn get_context_returns_index_json() {
        let tmp = TempDir::new().unwrap();
        setup_codeindex(&tmp);
        let ctx = ToolContext { repo_root: tmp.path().to_path_buf(), lsp_pool: Mutex::new(LspPool::new("file:///tmp")) };
        let result = handle_get_context(&ctx, &serde_json::json!({"files": []})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("auth"));
    }

    #[test]
    fn drill_package_returns_package_detail() {
        let tmp = TempDir::new().unwrap();
        setup_codeindex(&tmp);
        let ctx = ToolContext { repo_root: tmp.path().to_path_buf(), lsp_pool: Mutex::new(LspPool::new("file:///tmp")) };
        let result = handle_drill_package(&ctx, &serde_json::json!({"name": "auth"})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("login"));
    }

    fn setup_codeindex_multi(tmp: &TempDir) {
        use crate::indexer::writer::{write_index, write_package};
        let idx = RepoIndex {
            repo: "test".into(),
            generated_at: "2026-06-16T00:00:00Z".into(),
            packages: vec![
                PackageSummary { name: "auth".into(), purpose: "auth".into(), files: 1, score: 0.9 },
                PackageSummary { name: "api".into(), purpose: "api".into(), files: 1, score: 0.8 },
            ],
            hot_symbols: vec!["validate".into()],
        };
        let pkg_auth = PackageDetail {
            name: "auth".into(),
            files: vec![FileEntry {
                path: PathBuf::from("src/auth.rs"),
                symbols: vec![SymbolEntry {
                    name: "validate".into(),
                    signature: "fn validate() -> bool".into(),
                    kind: SymbolKind::Function,
                    line: 3,
                    character: 0,
                }],
                depends_on: vec![],
                depended_by: vec![],
            }],
        };
        let pkg_api = PackageDetail {
            name: "api".into(),
            files: vec![FileEntry {
                path: PathBuf::from("src/api.rs"),
                symbols: vec![SymbolEntry {
                    name: "validate".into(),
                    signature: "fn validate() -> Result<()>".into(),
                    kind: SymbolKind::Function,
                    line: 10,
                    character: 0,
                }],
                depends_on: vec![],
                depended_by: vec![],
            }],
        };
        let ci = tmp.path().join(".codeindex");
        write_index(&idx, &ci).unwrap();
        write_package(&pkg_auth, &ci).unwrap();
        write_package(&pkg_api, &ci).unwrap();
    }

    #[test]
    fn find_definition_returns_all_matches_sorted() {
        let tmp = TempDir::new().unwrap();
        setup_codeindex_multi(&tmp);
        let ctx = ToolContext {
            repo_root: tmp.path().to_path_buf(),
            lsp_pool: Mutex::new(LspPool::new("file:///tmp")),
        };
        let result = handle_find_definition(&ctx, &serde_json::json!({"symbol": "validate"})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        // Both files should appear
        assert!(text.contains("src/auth.rs"), "expected auth.rs in: {text}");
        assert!(text.contains("src/api.rs"), "expected api.rs in: {text}");
        // Lines should appear in sorted order (api.rs comes before auth.rs alphabetically)
        let auth_pos = text.find("auth.rs").unwrap();
        let api_pos = text.find("api.rs").unwrap();
        assert!(api_pos < auth_pos, "expected api.rs to appear before auth.rs (sorted): {text}");
    }

    #[test]
    fn find_references_index_fallback() {
        let tmp = TempDir::new().unwrap();
        setup_codeindex(&tmp);
        let ctx = ToolContext {
            repo_root: tmp.path().to_path_buf(),
            lsp_pool: Mutex::new(LspPool::new("file:///tmp")),
        };
        // No file/line/character provided — falls through to index fallback
        let result = handle_find_references(&ctx, &serde_json::json!({"symbol": "login"})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("[index fallback]"),
            "expected '[index fallback]' label in: {text}"
        );
        assert!(text.contains("login"), "expected 'login' in: {text}");
    }

    #[test]
    fn get_dependents_returns_list() {
        let tmp = TempDir::new().unwrap();
        let mut g = crate::graph::DependencyGraph::new();
        g.add_dependency(&PathBuf::from("src/api.rs"), &PathBuf::from("src/auth.rs"));
        crate::graph::persistence::save(&g, &tmp.path().join(".codeindex/graph.bin")).unwrap();
        let ctx = ToolContext { repo_root: tmp.path().to_path_buf(), lsp_pool: Mutex::new(LspPool::new("file:///tmp")) };
        let result = handle_get_dependents(&ctx, &serde_json::json!({"file": "src/auth.rs"})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("api.rs"));
    }
}
