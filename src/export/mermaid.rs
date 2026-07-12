//! Mermaid dependency graph export (lightweight substitute for graph.html).

use crate::config::codeindex_dir;
use crate::graph::persistence as graph_persistence;
use crate::indexer::writer::read_package;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const MAX_NODES: usize = 80;

pub struct MermaidOptions {
    pub root: PathBuf,
    pub output: PathBuf,
    pub package_filter: Option<String>,
}

impl MermaidOptions {
    pub fn resolve(
        root: Option<PathBuf>,
        output: Option<PathBuf>,
        package_filter: Option<String>,
    ) -> Result<Self> {
        let root = match root {
            Some(p) => p,
            None => crate::config::find_repo_root(&std::env::current_dir()?)
                .context("not in a git repo — use --root")?,
        };
        let output = output.unwrap_or_else(|| root.join(".codebeacon/dep-graph.mmd"));
        Ok(Self {
            root,
            output,
            package_filter,
        })
    }
}

fn mermaid_id(path: &Path) -> String {
    path.to_string_lossy()
        .replace(['/', '\\', '.', '-'], "_")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

fn label(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub fn export_mermaid(opts: &MermaidOptions) -> Result<String> {
    let codeindex = codeindex_dir(&opts.root);
    let graph_path = codeindex.join("graph.bin");
    let graph = graph_persistence::load(&graph_path).unwrap_or_default();

    let files_in_package: Option<HashSet<PathBuf>> = if let Some(ref pkg_name) = opts.package_filter {
        let pkg = read_package(pkg_name, &codeindex)?
            .with_context(|| format!("package '{pkg_name}' not found"))?;
        Some(pkg.files.iter().map(|f| f.path.clone()).collect())
    } else {
        None
    };

    let all_files = graph.all_files();
    let mut nodes: Vec<PathBuf> = if let Some(ref allowed) = files_in_package {
        all_files
            .into_iter()
            .filter(|f| allowed.contains(f))
            .collect()
    } else {
        all_files
    };
    nodes.sort();

    let truncated = nodes.len() > MAX_NODES;
    if truncated {
        nodes.truncate(MAX_NODES);
    }
    let node_set: HashSet<_> = nodes.iter().cloned().collect();

    let mut mmd = String::from("graph TD\n");
    for node in &nodes {
        let id = mermaid_id(node);
        mmd.push_str(&format!("  {id}[\"{label}\"]\n", label = label(node)));
    }

    for from in &nodes {
        for to in graph.neighbors(from) {
            if node_set.contains(&to) {
                mmd.push_str(&format!(
                    "  {} --> {}\n",
                    mermaid_id(from),
                    mermaid_id(&to)
                ));
            }
        }
    }

    if truncated {
        mmd.push_str(&format!(
            "\n%% Truncated: showing {MAX_NODES} of {} nodes. Use --package to narrow.\n",
            graph.node_count()
        ));
    }

    if let Some(parent) = opts.output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&opts.output, &mmd)?;
    Ok(mmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mermaid_export_valid_syntax() {
        let root = PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/simple_rust"
        ));
        if !codeindex_dir(&root).join("graph.bin").exists() {
            let mut indexer = crate::indexer::Indexer::new(&root);
            indexer.full_index().unwrap();
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let opts = MermaidOptions {
            root,
            output: tmp.path().join("graph.mmd"),
            package_filter: None,
        };
        let mmd = export_mermaid(&opts).unwrap();
        assert!(mmd.starts_with("graph TD"));
        assert!(mmd.contains("-->"));
    }
}
