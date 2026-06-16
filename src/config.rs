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
