//! Discover plan markdown files in a directory (flat, v1).

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct PlanDoc {
    pub path: PathBuf,
    pub block_key: String,
    pub body: String,
}

pub fn discover_plans(dir: &Path) -> Result<Vec<PlanDoc>> {
    if !dir.is_dir() {
        bail!("plans dir is not a directory: {}", dir.display());
    }
    let mut out = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(dir)
        .with_context(|| format!("read plans dir {}", dir.display()))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("plan")
            .to_string();
        let block_key = sanitize_block_key(&stem);
        let body = fs::read_to_string(&path)
            .with_context(|| format!("read plan {}", path.display()))?;
        out.push(PlanDoc {
            path,
            block_key,
            body,
        });
    }
    Ok(out)
}

fn sanitize_block_key(stem: &str) -> String {
    stem.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | ' ' => '_',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn discovers_flat_md() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("auth.md"), "# auth\n").unwrap();
        fs::write(tmp.path().join("ui.md"), "# ui\n").unwrap();
        fs::write(tmp.path().join("notes.txt"), "skip").unwrap();
        let plans = discover_plans(tmp.path()).unwrap();
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].block_key, "auth");
        assert_eq!(plans[1].block_key, "ui");
    }
}
