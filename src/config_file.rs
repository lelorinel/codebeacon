use crate::security::{PolicyMode, SecurityPolicy};
use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

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

    /// Seconds to spend enriching the heuristic index with LSP data in the
    /// background daemon. Set to 0 to disable LSP enrichment entirely. (default 60)
    #[serde(default = "default_lsp_enrich_timeout_secs")]
    pub lsp_enrich_timeout_secs: u64,

    #[serde(default)]
    pub lsp: LspConfig,

    #[serde(default)]
    pub security: SecurityConfig,

    #[serde(default)]
    pub extractor: ExtractorConfig,

    #[serde(default)]
    pub compact: CompactConfig,
}

fn default_lsp_concurrency() -> usize { 2 }
fn default_lsp_enrich_timeout_secs() -> u64 { 60 }

impl Default for CodeIndexConfig {
    fn default() -> Self {
        Self {
            extra_ignore_dirs: Vec::new(),
            ignore_globs: Vec::new(),
            languages: Vec::new(),
            lsp_concurrency: default_lsp_concurrency(),
            lsp_enrich_timeout_secs: default_lsp_enrich_timeout_secs(),
            lsp: LspConfig::default(),
            security: SecurityConfig::default(),
            extractor: ExtractorConfig::default(),
            compact: CompactConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExtractorConfig {
    /// regex | tree-sitter | auto (tree-sitter when feature enabled, else regex)
    #[serde(default = "default_extractor_mode")]
    pub mode: String,

    /// Per-file parse budget (ms); fallback to regex on timeout.
    #[serde(default = "default_parse_timeout_ms")]
    pub parse_timeout_ms: u64,

    /// Skip tree-sitter above this size (bytes); use regex.
    #[serde(default = "default_max_tree_sitter_bytes")]
    pub max_tree_sitter_bytes: usize,
}

fn default_extractor_mode() -> String {
    "auto".into()
}

fn default_parse_timeout_ms() -> u64 {
    50
}

fn default_max_tree_sitter_bytes() -> usize {
    512_000
}

impl Default for ExtractorConfig {
    fn default() -> Self {
        Self {
            mode: default_extractor_mode(),
            parse_timeout_ms: default_parse_timeout_ms(),
            max_tree_sitter_bytes: default_max_tree_sitter_bytes(),
        }
    }
}

impl ExtractorConfig {
    pub fn uses_tree_sitter(&self) -> bool {
        match self.mode.to_lowercase().as_str() {
            "regex" => false,
            "tree-sitter" | "treesitter" => cfg!(feature = "tree-sitter"),
            _ => cfg!(feature = "tree-sitter"), // auto
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SecurityConfig {
    #[serde(default)]
    pub enabled: bool,

    /// strict | balanced | permissive
    #[serde(default = "default_security_mode")]
    pub mode: String,

    #[serde(default = "default_z3_timeout_ms")]
    pub z3_timeout_ms: u64,

    #[serde(default)]
    pub block_on_unknown: bool,

    /// Enabled CWE ids (e.g. "190", "131"). Empty → Z3 defaults on, pattern CWEs off.
    #[serde(default)]
    pub enabled_cwes: Vec<String>,
}

fn default_security_mode() -> String {
    "balanced".into()
}

fn default_z3_timeout_ms() -> u64 {
    5_000
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: default_security_mode(),
            z3_timeout_ms: default_z3_timeout_ms(),
            block_on_unknown: false,
            enabled_cwes: Vec::new(),
        }
    }
}

impl SecurityConfig {
    pub fn to_policy(&self, cli_security: bool) -> SecurityPolicy {
        use crate::security::cwe::{default_enabled_cwes, normalize_cwe_id};
        use std::collections::HashSet;

        let enabled_cwes = if self.enabled_cwes.is_empty() {
            default_enabled_cwes()
        } else {
            self.enabled_cwes
                .iter()
                .map(|id| normalize_cwe_id(id))
                .collect::<HashSet<_>>()
        };

        SecurityPolicy {
            enabled: self.enabled || cli_security,
            mode: PolicyMode::parse(&self.mode).unwrap_or_default(),
            z3_timeout_ms: self.z3_timeout_ms,
            block_on_unknown: self.block_on_unknown,
            enabled_cwes,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompactConfig {
    /// When true (default), MCP index tools return token-compressed JSON with dict refs.
    #[serde(default = "default_compact_enabled")]
    pub enabled: bool,
}

fn default_compact_enabled() -> bool {
    true
}

impl Default for CompactConfig {
    fn default() -> Self {
        Self {
            enabled: default_compact_enabled(),
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
        assert_eq!(cfg.lsp_enrich_timeout_secs, 60);
        assert!(cfg.lsp.overrides.is_empty());
        assert!(cfg.compact.enabled);
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

    #[test]
    fn load_parses_security_section() {
        let tmp = TempDir::new().unwrap();
        let config_content = r#"
[security]
enabled = true
mode = "strict"
z3_timeout_ms = 3000
block_on_unknown = true
"#;
        fs::write(tmp.path().join(".codeindex.toml"), config_content).unwrap();
        let cfg = load(tmp.path()).unwrap();
        assert!(cfg.security.enabled);
        assert_eq!(cfg.security.mode, "strict");
        assert_eq!(cfg.security.z3_timeout_ms, 3000);
        assert!(cfg.security.block_on_unknown);
        let policy = cfg.security.to_policy(false);
        assert!(policy.enabled);
        assert_eq!(policy.z3_timeout_ms, 3000);
        assert!(policy.block_on_unknown);
        assert!(policy.cwe_enabled("190"));
    }

    #[test]
    fn load_parses_enabled_cwes() {
        let tmp = TempDir::new().unwrap();
        let config_content = r#"
[security]
enabled = true
enabled_cwes = ["190", "134", "78"]
"#;
        fs::write(tmp.path().join(".codeindex.toml"), config_content).unwrap();
        let cfg = load(tmp.path()).unwrap();
        let policy = cfg.security.to_policy(false);
        assert!(policy.cwe_enabled("134"));
        assert!(policy.cwe_enabled("78"));
        assert!(!policy.cwe_enabled("131"));
    }

    #[test]
    fn load_parses_extractor_section() {
        let tmp = TempDir::new().unwrap();
        let config_content = r#"
[extractor]
mode = "tree-sitter"
parse_timeout_ms = 100
max_tree_sitter_bytes = 256000
"#;
        fs::write(tmp.path().join(".codeindex.toml"), config_content).unwrap();
        let cfg = load(tmp.path()).unwrap();
        assert_eq!(cfg.extractor.mode, "tree-sitter");
        assert_eq!(cfg.extractor.parse_timeout_ms, 100);
        assert_eq!(cfg.extractor.max_tree_sitter_bytes, 256_000);
    }

    #[test]
    fn load_parses_compact_section() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join(".codeindex.toml"),
            "[compact]\nenabled = false\n",
        )
        .unwrap();
        let cfg = load(tmp.path()).unwrap();
        assert!(!cfg.compact.enabled);
    }

    #[test]
    fn cli_security_flag_enables_policy() {
        let cfg = SecurityConfig::default();
        assert!(!cfg.to_policy(false).enabled);
        assert!(cfg.to_policy(true).enabled);
    }
}
