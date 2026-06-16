use crate::config::codeindex_dir;
use crate::graph::{persistence as graph_persistence, DependencyGraph};
use crate::indexer::writer::{read_index, read_package};
use crate::mcp::protocol::text_content;
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::PathBuf;

pub struct ToolContext {
    pub repo_root: PathBuf,
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
        .context("No .codeindex/ found — run `lcp init` first")?;
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
    let mut found: Vec<String> = vec![];

    let packages_dir = ctx.codeindex().join("packages");
    for entry in std::fs::read_dir(packages_dir).into_iter().flatten().flatten() {
        if let Ok(text) = std::fs::read_to_string(entry.path()) {
            if let Ok(pkg) = serde_json::from_str::<crate::types::PackageDetail>(&text) {
                for file in pkg.files {
                    for sym in &file.symbols {
                        if sym.name.contains(symbol) {
                            found.push(format!("{}:{} — {}", file.path.display(), sym.line, sym.signature));
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
    let packages_dir = ctx.codeindex().join("packages");
    for entry in std::fs::read_dir(packages_dir).into_iter().flatten().flatten() {
        if let Ok(text) = std::fs::read_to_string(entry.path()) {
            if let Ok(pkg) = serde_json::from_str::<crate::types::PackageDetail>(&text) {
                for file in pkg.files {
                    for sym in &file.symbols {
                        if sym.name == symbol {
                            return Ok(text_content(format!(
                                "{}:{} — {}",
                                file.path.display(), sym.line, sym.signature
                            )));
                        }
                    }
                }
            }
        }
    }
    Ok(text_content(format!("Definition of '{symbol}' not found")))
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
                symbols: vec![SymbolEntry { name: "login".into(), signature: "fn login()".into(), kind: SymbolKind::Function, line: 1 }],
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
        let ctx = ToolContext { repo_root: tmp.path().to_path_buf() };
        let result = handle_get_context(&ctx, &serde_json::json!({"files": []})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("auth"));
    }

    #[test]
    fn drill_package_returns_package_detail() {
        let tmp = TempDir::new().unwrap();
        setup_codeindex(&tmp);
        let ctx = ToolContext { repo_root: tmp.path().to_path_buf() };
        let result = handle_drill_package(&ctx, &serde_json::json!({"name": "auth"})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("login"));
    }

    #[test]
    fn get_dependents_returns_list() {
        let tmp = TempDir::new().unwrap();
        let mut g = crate::graph::DependencyGraph::new();
        g.add_dependency(&PathBuf::from("src/api.rs"), &PathBuf::from("src/auth.rs"));
        crate::graph::persistence::save(&g, &tmp.path().join(".codeindex/graph.bin")).unwrap();
        let ctx = ToolContext { repo_root: tmp.path().to_path_buf() };
        let result = handle_get_dependents(&ctx, &serde_json::json!({"file": "src/auth.rs"})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("api.rs"));
    }
}
