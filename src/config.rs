use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Go,
    Python,
    TypeScript,
    CSharp,
}

impl Language {
    pub fn lsp_binary(&self) -> &'static str {
        match self {
            Language::Rust => "rust-analyzer",
            Language::Go => "gopls",
            Language::Python => "pylsp",
            Language::TypeScript => "typescript-language-server",
            Language::CSharp => "csharp-ls",
        }
    }

    pub fn lsp_args(&self) -> &'static [&'static str] {
        match self {
            Language::TypeScript => &["--stdio"],
            Language::CSharp => &["--stdio"],
            _ => &[],
        }
    }

    pub fn language_id(&self) -> &'static str {
        match self {
            Language::Rust       => "rust",
            Language::Go         => "go",
            Language::Python     => "python",
            Language::TypeScript => "typescript",
            Language::CSharp     => "csharp",
        }
    }
}

pub fn find_repo_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Determine which git repos under `root` should be indexed.
///
/// Rules (in order):
/// 1. If `root` itself contains `.git` → single repo `[root]` (existing behaviour).
/// 2. Otherwise scan immediate children; those that contain `.git` are collected →
///    multi-repo workspace (e.g. an `examples/` directory with several repos).
/// 3. If no children are repos either → fall back to walking *upward* from `root`
///    for a `.git` ancestor (legacy single-repo fallback).
/// 4. If nothing found → empty vec (caller should emit a clear error).
/// Returns the workspace start directory by checking env vars in priority order,
/// then falling back to the current working directory.
///
/// Checked vars:
/// - `CLAUDE_PROJECT_DIR` — set by Claude Code
/// - `CURSOR_WORKSPACE`   — set by Cursor
///
/// VS Code, Zed, and Cline all launch MCP servers with `cwd` set to the
/// workspace folder, so the `cwd` fallback covers those clients.
pub fn workspace_start_from_env() -> PathBuf {
    for var in &["CLAUDE_PROJECT_DIR", "CURSOR_WORKSPACE"] {
        if let Ok(v) = std::env::var(var) {
            let p = PathBuf::from(&v);
            if p.exists() {
                return p;
            }
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn discover_repos(root: &Path) -> Vec<PathBuf> {
    // Rule 1: root is already a git repo
    if root.join(".git").exists() {
        return vec![root.to_path_buf()];
    }

    // Rule 2: scan first-level children for git repos
    let mut repos: Vec<PathBuf> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() { return None; }
            let name = path.file_name()?.to_string_lossy();
            // Skip hidden dirs (.git, .vscode, …) and common non-project dirs
            if name.starts_with('.') { return None; }
            if path.join(".git").exists() { Some(path) } else { None }
        })
        .collect();

    if !repos.is_empty() {
        repos.sort(); // deterministic order
        return repos;
    }

    // Rule 3: walk upward (legacy single-repo fallback)
    if let Some(ancestor) = find_repo_root(root) {
        return vec![ancestor];
    }

    // Rule 4: nothing found
    vec![]
}

pub fn detect_language(path: &Path) -> Option<Language> {
    match path.extension()?.to_str()? {
        "rs" => Some(Language::Rust),
        "go" => Some(Language::Go),
        "py" => Some(Language::Python),
        "ts" | "js" | "tsx" | "jsx" => Some(Language::TypeScript),
        "cs" => Some(Language::CSharp),
        _ => None,
    }
}

pub fn language_from_id(id: &str) -> Option<Language> {
    match id {
        "rust"       => Some(Language::Rust),
        "go"         => Some(Language::Go),
        "python"     => Some(Language::Python),
        "typescript" => Some(Language::TypeScript),
        "csharp"     => Some(Language::CSharp),
        _            => None,
    }
}

pub fn codeindex_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".codeindex")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn finds_repo_root_by_git_dir() {
        let tmp = TempDir::new().unwrap();
        let subdir = tmp.path().join("src/auth");
        fs::create_dir_all(&subdir).unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();

        let root = find_repo_root(&subdir).unwrap();
        assert_eq!(root, tmp.path());
    }

    #[test]
    fn returns_none_when_no_git_dir() {
        let tmp = TempDir::new().unwrap();
        let result = find_repo_root(tmp.path());
        assert!(result.is_none());
    }

    #[test]
    fn discover_repos_single_repo_root() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();
        let repos = discover_repos(tmp.path());
        assert_eq!(repos, vec![tmp.path().to_path_buf()]);
    }

    #[test]
    fn discover_repos_multi_child_repos() {
        let tmp = TempDir::new().unwrap();
        // tmp has no .git — it's the workspace root
        for name in &["alpha", "beta", "gamma"] {
            let sub = tmp.path().join(name);
            fs::create_dir_all(&sub).unwrap();
            fs::create_dir(sub.join(".git")).unwrap();
        }
        // A non-repo dir should NOT be included
        fs::create_dir_all(tmp.path().join("docs")).unwrap();

        let repos = discover_repos(tmp.path());
        assert_eq!(repos.len(), 3);
        // Sorted: alpha, beta, gamma
        assert!(repos[0].ends_with("alpha"));
        assert!(repos[1].ends_with("beta"));
        assert!(repos[2].ends_with("gamma"));
    }

    #[test]
    fn discover_repos_fallback_walk_upward() {
        let tmp = TempDir::new().unwrap();
        // Create a git root, then a child dir with no children that are repos
        fs::create_dir(tmp.path().join(".git")).unwrap();
        let sub = tmp.path().join("src/deep");
        fs::create_dir_all(&sub).unwrap();

        // discover_repos from 'src/deep' should find the ancestor git root
        let repos = discover_repos(&sub);
        assert_eq!(repos, vec![tmp.path().to_path_buf()]);
    }

    #[test]
    fn discover_repos_empty_when_no_git_anywhere() {
        let tmp = TempDir::new().unwrap();
        let repos = discover_repos(tmp.path());
        assert!(repos.is_empty());
    }

    #[test]
    fn detects_language_by_extension() {
        assert_eq!(detect_language(std::path::Path::new("foo.rs")), Some(Language::Rust));
        assert_eq!(detect_language(std::path::Path::new("foo.go")), Some(Language::Go));
        assert_eq!(detect_language(std::path::Path::new("foo.py")), Some(Language::Python));
        assert_eq!(detect_language(std::path::Path::new("foo.ts")), Some(Language::TypeScript));
        assert_eq!(detect_language(std::path::Path::new("foo.js")), Some(Language::TypeScript));
        assert_eq!(detect_language(std::path::Path::new("foo.cs")), Some(Language::CSharp));
        assert_eq!(detect_language(std::path::Path::new("foo.txt")), None);
    }
}
