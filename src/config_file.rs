use serde::Deserialize;
use std::path::Path;
use anyhow::Result;

#[derive(Debug, Deserialize)]
pub struct CodeIndexConfig {
    #[serde(default)]
    pub extra_ignore_dirs: Vec<String>,

    #[serde(default)]
    pub ignore_globs: Vec<String>,

    /// If non-empty, only these languages are indexed.
    /// Valid values (case-insensitive): "rust", "go", "python", "typescript", "csharp"
    #[serde(default)]
    pub languages: Vec<String>,

    /// Number of concurrent LSP workers per language (default 2)
    #[serde(default = "default_lsp_concurrency")]
    pub lsp_concurrency: usize,

    #[serde(default)]
    pub lsp: LspConfig,
}

fn default_lsp_concurrency() -> usize { 2 }

impl Default for CodeIndexConfig {
    fn default() -> Self {
        Self {
            extra_ignore_dirs: Vec::new(),
            ignore_globs: Vec::new(),
            languages: Vec::new(),
            lsp_concurrency: default_lsp_concurrency(),
            lsp: LspConfig::default(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct LspConfig {
    /// Override LSP binary per language, e.g. {"csharp": "OmniSharp"}
    #[serde(default)]
    pub overrides: std::collections::HashMap<String, String>,
}

/// Load .codeindex.toml from repo_root. Returns Default if file is absent.
pub fn load(repo_root: &Path) -> Result<CodeIndexConfig> {
    let path = repo_root.join(".codeindex.toml");
    if !path.exists() {
        return Ok(CodeIndexConfig::default());
    }
    let text = std::fs::read_to_string(&path)?;
    let cfg: CodeIndexConfig = toml::from_str(&text)?;
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn load_returns_default_when_no_file() {
        let tmp = TempDir::new().unwrap();
        let cfg = load(tmp.path()).unwrap();
        assert!(cfg.extra_ignore_dirs.is_empty());
        assert!(cfg.ignore_globs.is_empty());
        assert!(cfg.languages.is_empty());
        assert_eq!(cfg.lsp_concurrency, 2);
        assert!(cfg.lsp.overrides.is_empty());
    }

    #[test]
    fn load_parses_config_file() {
        let tmp = TempDir::new().unwrap();
        let config_content = r#"
extra_ignore_dirs = ["my_build", "tmp"]
ignore_globs = ["**/*.generated.cs"]
languages = ["rust", "go"]
lsp_concurrency = 4

[lsp]
overrides = { csharp = "OmniSharp" }
"#;
        fs::write(tmp.path().join(".codeindex.toml"), config_content).unwrap();
        let cfg = load(tmp.path()).unwrap();
        assert_eq!(cfg.extra_ignore_dirs, vec!["my_build", "tmp"]);
        assert_eq!(cfg.ignore_globs, vec!["**/*.generated.cs"]);
        assert_eq!(cfg.languages, vec!["rust", "go"]);
        assert_eq!(cfg.lsp_concurrency, 4);
        assert_eq!(cfg.lsp.overrides.get("csharp").map(String::as_str), Some("OmniSharp"));
    }

    #[test]
    fn load_uses_default_lsp_concurrency_when_not_set() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(".codeindex.toml"), "extra_ignore_dirs = []\n").unwrap();
        let cfg = load(tmp.path()).unwrap();
        assert_eq!(cfg.lsp_concurrency, 2);
    }
}
