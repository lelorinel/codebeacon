use anyhow::Result;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct GitStatusSummary {
    pub dirty_count: usize,
    pub modified: Vec<String>,
    pub untracked: Vec<String>,
}

pub fn git_status(repo_root: &Path) -> Option<GitStatusSummary> {
    let out = Command::new("git")
        .args(["-C", &repo_root.to_string_lossy(), "status", "--porcelain"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut modified = Vec::new();
    let mut untracked = Vec::new();
    for line in text.lines() {
        if line.len() < 4 {
            continue;
        }
        let path = line[3..].trim();
        if line.starts_with("??") {
            untracked.push(path.to_string());
        } else {
            modified.push(path.to_string());
        }
    }
    let dirty_count = modified.len() + untracked.len();
    Some(GitStatusSummary {
        dirty_count,
        modified,
        untracked,
    })
}

pub fn git_log_file(repo_root: &Path, rel_file: &str, limit: u32) -> Vec<String> {
    let out = Command::new("git")
        .args([
            "-C",
            &repo_root.to_string_lossy(),
            "log",
            &format!("-{limit}"),
            "--format=%h %s",
            "--",
            rel_file,
        ])
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect(),
        _ => vec![],
    }
}

pub fn git_blame_first_line(repo_root: &Path, rel_file: &str) -> Option<String> {
    let out = Command::new("git")
        .args([
            "-C",
            &repo_root.to_string_lossy(),
            "blame",
            "-L",
            "1,1",
            "--",
            rel_file,
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout);
    let trimmed = line.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Commit count per file in the last 30 days (capped file list).
pub fn git_churn(repo_root: &Path, files: &[String], limit: usize) -> Result<Vec<(String, u32)>> {
    let mut counts = Vec::new();
    for file in files.iter().take(limit) {
        let out = Command::new("git")
            .args([
                "-C",
                &repo_root.to_string_lossy(),
                "log",
                "--since=30 days ago",
                "--format=",
                "--",
                file,
            ])
            .output()?;
        let n = if out.status.success() {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .count() as u32
        } else {
            0
        };
        counts.push((file.clone(), n));
    }
    Ok(counts)
}

pub fn is_git_repo(repo_root: &Path) -> bool {
    repo_root.join(".git").exists()
}
