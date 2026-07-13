use crate::config::codeindex_dir;
use crate::indexer::writer::read_index;
use crate::intelligence::git::{git_status, is_git_repo, GitStatusSummary};
use crate::config_file::IntelligenceConfig;
use anyhow::Result;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct IndexStatusResponse {
    pub indexed_at: String,
    pub graph_mtime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<GitStatusSummary>,
    pub stale_files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_warning: Option<String>,
}

pub fn index_status(repo_root: &Path, cfg: &IntelligenceConfig) -> Result<IndexStatusResponse> {
    let codeindex = codeindex_dir(repo_root);
    let index = read_index(&codeindex)?;
    let indexed_at = index
        .as_ref()
        .map(|i| i.generated_at.clone())
        .unwrap_or_else(|| "never".into());

    let graph_path = codeindex.join("graph.bin");
    let graph_mtime = std::fs::metadata(&graph_path).ok().and_then(|m| {
        m.modified().ok().map(|t| {
            chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()
        })
    });

    let git = if cfg.git_context_enabled && is_git_repo(repo_root) {
        git_status(repo_root)
    } else {
        None
    };

    let mut stale_files = Vec::new();
    if let Some(ref g) = git {
        stale_files.extend(g.modified.clone());
    }

    let stale_warning = if stale_files.is_empty() {
        None
    } else {
        Some(format!(
            "{} file(s) changed in working tree since last index build",
            stale_files.len()
        ))
    };

    Ok(IndexStatusResponse {
        indexed_at,
        graph_mtime,
        git,
        stale_files,
        stale_warning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::writer::write_index;
    use crate::types::{PackageSummary, RepoIndex};
    use tempfile::TempDir;

    #[test]
    fn index_status_reads_generated_at() {
        let tmp = TempDir::new().unwrap();
        let codeindex = tmp.path().join(".codeindex");
        std::fs::create_dir_all(&codeindex).unwrap();
        let index = RepoIndex {
            repo: "test".into(),
            generated_at: "2026-01-01T00:00:00Z".into(),
            packages: vec![PackageSummary {
                name: "auth".into(),
                purpose: String::new(),
                files: 1,
                score: 1.0,
            }],
            hot_symbols: vec![],
        };
        write_index(&index, &codeindex).unwrap();

        let cfg = IntelligenceConfig {
            git_context_enabled: false,
            ..Default::default()
        };
        let out = index_status(tmp.path(), &cfg).unwrap();
        assert_eq!(out.indexed_at, "2026-01-01T00:00:00Z");
        assert!(out.git.is_none());
        assert!(out.stale_warning.is_none());
    }
}
