//! Generate CODEBEACON_REPORT.md (Graphify GRAPH_REPORT parity).

use crate::config_file::load as load_config;
use crate::graph::path::hotspots;
use crate::query::RepoQueryCtx;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub struct ReportOptions {
    pub root: PathBuf,
    pub output: PathBuf,
}

impl ReportOptions {
    pub fn resolve(root: Option<PathBuf>, output: Option<PathBuf>) -> Result<Self> {
        let root = match root {
            Some(p) => p,
            None => crate::config::find_repo_root(&std::env::current_dir()?)
                .context("not in a git repo — use --root")?,
        };
        let output = output.unwrap_or_else(|| {
            let default = root.join("CODEBEACON_REPORT.md");
            if default.parent().map(|p| p.exists()).unwrap_or(false) {
                default
            } else {
                root.join(".codebeacon/CODEBEACON_REPORT.md")
            }
        });
        Ok(Self { root, output })
    }
}

pub fn generate(opts: &ReportOptions) -> Result<String> {
    let ctx = RepoQueryCtx::load(&opts.root)?;
    let config = load_config(&opts.root).unwrap_or_default();
    let (extracted, inferred) = ctx.edge_provenance();
    let hs = hotspots(&ctx.graph, 10);

    let mut md = String::new();
    md.push_str("# Codebeacon Report\n\n");
    md.push_str(&format!("> Generated for **{}** at {}\n\n", ctx.index.repo, ctx.index.generated_at));

    md.push_str("## Summary\n\n");
    md.push_str("| Metric | Value |\n|--------|-------|\n");
    md.push_str(&format!("| Packages | {} |\n", ctx.index.packages.len()));
    md.push_str(&format!("| Files | {} |\n", ctx.count_files()));
    md.push_str(&format!("| Symbols | {} |\n", ctx.count_symbols()));
    md.push_str(&format!("| Graph nodes | {} |\n", ctx.graph.node_count()));
    md.push_str(&format!("| Graph edges | {} |\n", ctx.graph.edge_count()));
    md.push('\n');

    md.push_str("## Hotspots\n\n");
    md.push_str("Files with the most reverse dependencies (god nodes):\n\n");
    if hs.is_empty() {
        md.push_str("_No graph data._\n\n");
    } else {
        md.push_str("| Rank | File | Dependents |\n|------|------|------------|\n");
        for (i, (path, count)) in hs.iter().enumerate() {
            md.push_str(&format!("| {} | `{}` | {} |\n", i + 1, path.display(), count));
        }
        md.push('\n');
    }

    md.push_str("## Packages\n\n");
    md.push_str("| Package | Files | Score | Purpose |\n|---------|-------|-------|--------|\n");
    for pkg in &ctx.index.packages {
        md.push_str(&format!(
            "| {} | {} | {:.2} | {} |\n",
            pkg.name, pkg.files, pkg.score, pkg.purpose
        ));
    }
    md.push('\n');

    md.push_str("## Hot Symbols\n\n");
    if ctx.index.hot_symbols.is_empty() {
        md.push_str("_None indexed._\n\n");
    } else {
        for sym in &ctx.index.hot_symbols {
            md.push_str(&format!("- `{sym}`\n"));
        }
        md.push('\n');
    }

    md.push_str("## Suggested Questions\n\n");
    for (path, count) in hs.iter().take(5) {
        if *count > 0 {
            md.push_str(&format!(
                "- What breaks if I change `{}`? ({} dependents)\n",
                path.display(),
                count
            ));
        }
    }
    md.push_str("- How does authentication flow through the dependency graph?\n");
    md.push_str("- Which packages have the highest relevance scores?\n\n");

    md.push_str("## Edge Provenance\n\n");
    md.push_str("| Type | Count | Source |\n|------|-------|--------|\n");
    md.push_str(&format!(
        "| EXTRACTED | {} | import-resolved edges |\n",
        extracted
    ));
    if inferred > 0 {
        md.push_str(&format!(
            "| INFERRED | {} | LSP-enriched edges |\n",
            inferred
        ));
    } else if config.lsp_enrich_timeout_secs > 0 {
        md.push_str("| INFERRED | — | LSP enrichment may add edges in background daemon |\n");
    }
    md.push('\n');

    md.push_str("## Security\n\n");
    if config.security.enabled {
        md.push_str(&format!(
            "Security policy **enabled** (mode: `{}`). Use `codebeacon verify` or MCP `verify_security` before risky edits.\n\n"
        , config.security.mode));
    } else {
        md.push_str("Security policy not enabled. Install with `--security` or set `[security] enabled = true` in `.codeindex.toml`.\n\n");
    }

    md.push_str("## LCP Differentiators\n\n");
    md.push_str("- **Live index** — daemon updates `.codeindex/` on save (not batch `graph.json`)\n");
    md.push_str("- **LSP precision** — `find_definition` / `find_references` MCP tools\n");
    md.push_str("- **Impact analysis** — `get_dependents` / `codebeacon dependents`\n");
    md.push_str("- **Multi-repo** — MCP `repo` argument for workspaces\n");
    md.push_str("- **Z3 security gate** — CWE-190 formal verification (`codebeacon verify`)\n\n");

    md.push_str("## Commands\n\n");
    md.push_str("```bash\n");
    md.push_str(&format!("codebeacon query \"auth\" --root {}\n", opts.root.display()));
    md.push_str(&format!("codebeacon path src/auth.rs src/db.rs --root {}\n", opts.root.display()));
    md.push_str(&format!("codebeacon explain login --root {}\n", opts.root.display()));
    md.push_str(&format!("codebeacon export mermaid --root {}\n", opts.root.display()));
    md.push_str("```\n");

    if let Some(parent) = opts.output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&opts.output, &md)?;
    Ok(md)
}

pub fn generate_or_read(root: &Path) -> Result<String> {
    let report_path = root.join("CODEBEACON_REPORT.md");
    if report_path.exists() {
        return Ok(std::fs::read_to_string(&report_path)?);
    }
    let alt = root.join(".codebeacon/CODEBEACON_REPORT.md");
    if alt.exists() {
        return Ok(std::fs::read_to_string(&alt)?);
    }
    let opts = ReportOptions {
        root: root.to_path_buf(),
        output: report_path,
    };
    generate(&opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_generates_markdown() {
        let root = PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/simple_rust"
        ));
        if !crate::config::codeindex_dir(&root).join("index.json").exists() {
            let mut indexer = crate::indexer::Indexer::new(&root);
            indexer.full_index().unwrap();
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let out = tmp.path().join("report.md");
        let opts = ReportOptions {
            root: root.clone(),
            output: out.clone(),
        };
        let md = generate(&opts).unwrap();
        assert!(md.contains("Hotspots"));
        assert!(md.contains("Packages"));
        assert!(out.exists());
    }
}
