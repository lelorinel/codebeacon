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

    #[serde(default)]
    pub intelligence: IntelligenceConfig,

    #[serde(default, rename = "loop")]
    pub loop_config: LoopConfig,

    #[serde(default)]
    pub locks: LocksConfig,

    #[serde(default)]
    pub docs: DocsConfig,
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
            intelligence: IntelligenceConfig::default(),
            loop_config: LoopConfig::default(),
            locks: LocksConfig::default(),
            docs: DocsConfig::default(),
        }
    }
}

/// Sidecar documentation index (`--docs` / `[docs]`).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct DocsConfig {
    /// Directory of markdown docs relative to repo root (or absolute).
    /// When set (or overridden by CLI `--docs`), docs tools and sidecar index are enabled.
    #[serde(default)]
    pub path: Option<String>,
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

#[derive(Debug, Clone, Deserialize)]
pub struct IntelligenceConfig {
    #[serde(default = "default_intelligence_enabled")]
    pub enabled: bool,
    #[serde(default = "default_focus_radius")]
    pub focus_default_radius: u32,
    #[serde(default = "default_impact_threshold")]
    pub change_impact_high_ref_threshold: u32,
    #[serde(default = "default_true")]
    pub conventions_enabled: bool,
    #[serde(default = "default_true")]
    pub git_context_enabled: bool,
}

fn default_intelligence_enabled() -> bool {
    true
}

fn default_focus_radius() -> u32 {
    2
}

fn default_impact_threshold() -> u32 {
    10
}

fn default_true() -> bool {
    true
}

impl Default for IntelligenceConfig {
    fn default() -> Self {
        Self {
            enabled: default_intelligence_enabled(),
            focus_default_radius: default_focus_radius(),
            change_impact_high_ref_threshold: default_impact_threshold(),
            conventions_enabled: default_true(),
            git_context_enabled: default_true(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReindexPolicy {
    Never,
    IfStale,
    EveryN,
    Always,
}

impl Default for ReindexPolicy {
    fn default() -> Self {
        ReindexPolicy::IfStale
    }
}

impl ReindexPolicy {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "never" => ReindexPolicy::Never,
            "if_stale" | "if-stale" => ReindexPolicy::IfStale,
            "every_n" | "every-n" => ReindexPolicy::EveryN,
            "always" => ReindexPolicy::Always,
            _ => ReindexPolicy::IfStale,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoopConfig {
    #[serde(default = "default_loop_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub reindex: ReindexPolicy,
    #[serde(default = "default_reindex_every_n")]
    pub reindex_every_n: u32,
    #[serde(default = "default_stale_warn_threshold")]
    pub stale_warn_threshold: u32,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
    #[serde(default = "default_true")]
    pub persist_sessions: bool,
    #[serde(default = "default_prefetch_on_tick")]
    pub prefetch_on_tick: Vec<String>,
    #[serde(default = "default_focus_radius")]
    pub default_focus_radius: u32,
}

fn default_loop_enabled() -> bool {
    true
}

fn default_reindex_every_n() -> u32 {
    3
}

fn default_stale_warn_threshold() -> u32 {
    5
}

fn default_max_iterations() -> u32 {
    50
}

fn default_prefetch_on_tick() -> Vec<String> {
    vec!["index_status".into(), "focus_context".into()]
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            enabled: default_loop_enabled(),
            reindex: ReindexPolicy::IfStale,
            reindex_every_n: default_reindex_every_n(),
            stale_warn_threshold: default_stale_warn_threshold(),
            max_iterations: default_max_iterations(),
            persist_sessions: default_true(),
            prefetch_on_tick: default_prefetch_on_tick(),
            default_focus_radius: default_focus_radius(),
        }
    }
}

impl LoopConfig {
    pub fn wants_prefetch(&self, name: &str) -> bool {
        self.prefetch_on_tick.iter().any(|p| p == name)
    }

    pub fn focus_radius(&self, intelligence: &IntelligenceConfig) -> u32 {
        if self.default_focus_radius > 0 {
            self.default_focus_radius
        } else {
            intelligence.focus_default_radius
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LocksConfig {
    /// When true (default), MCP exposes claim_path / release_path / await_path / sessions.
    #[serde(default = "default_locks_enabled")]
    pub enabled: bool,
    /// Claim TTL in seconds (same block_key renews the lease).
    #[serde(default = "default_lock_ttl_secs")]
    pub ttl_secs: u64,
    /// Optional path prefixes allowlist (empty = any relative path under workspace).
    #[serde(default)]
    pub allow: Vec<String>,
}

fn default_locks_enabled() -> bool {
    true
}

fn default_lock_ttl_secs() -> u64 {
    600
}

impl Default for LocksConfig {
    fn default() -> Self {
        Self {
            enabled: default_locks_enabled(),
            ttl_secs: default_lock_ttl_secs(),
            allow: Vec::new(),
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

/// Path string to store in `[docs] path` (prefer relative to repo root).
pub fn docs_path_for_config(repo_root: &Path, cli_docs: &Path) -> String {
    if cli_docs.is_absolute() {
        if let Ok(rel) = cli_docs.strip_prefix(repo_root) {
            let s = rel.display().to_string();
            return if s.is_empty() { ".".into() } else { s };
        }
        return cli_docs.display().to_string();
    }
    let s = cli_docs.to_string_lossy();
    let trimmed = s.trim_start_matches("./");
    if trimmed.is_empty() {
        ".".into()
    } else {
        trimmed.to_string()
    }
}

fn escape_toml_basic(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Insert or update `[docs] path` in an existing TOML text (preserves other content).
pub fn upsert_docs_path_toml(existing: &str, path_value: &str) -> String {
    let assignment = format!("path = \"{}\"", escape_toml_basic(path_value));
    let lines: Vec<&str> = existing.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    let mut found_docs = false;
    let mut wrote_path = false;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed == "[docs]" || trimmed.starts_with("[docs.") {
            found_docs = true;
            out.push(line.to_string());
            i += 1;
            // Copy/replace within this table until next [section]
            while i < lines.len() {
                let l = lines[i];
                let t = l.trim();
                if t.starts_with('[') && !t.starts_with("[[") {
                    break;
                }
                if t.starts_with("path ") || t.starts_with("path=") {
                    if !wrote_path {
                        out.push(assignment.clone());
                        wrote_path = true;
                    }
                    // skip old path line
                } else {
                    out.push(l.to_string());
                }
                i += 1;
            }
            if !wrote_path {
                out.push(assignment.clone());
                wrote_path = true;
            }
            continue;
        }
        out.push(line.to_string());
        i += 1;
    }

    if !found_docs {
        if !out.is_empty() && !out.last().map(|s| s.is_empty()).unwrap_or(true) {
            out.push(String::new());
        }
        out.push("[docs]".into());
        out.push(assignment);
    }

    let mut s = out.join("\n");
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// Persist `[docs] path` into `.codeindex.toml` (create file if missing).
pub fn persist_docs_path(repo_root: &Path, cli_docs: &Path) -> Result<()> {
    let path_value = docs_path_for_config(repo_root, cli_docs);
    let config_path = repo_root.join(".codeindex.toml");
    if !config_path.exists() {
        let body = format!(
            "# Written by `codebeacon init --docs`\n\n[docs]\npath = \"{}\"\n",
            escape_toml_basic(&path_value)
        );
        std::fs::write(&config_path, body)?;
        return Ok(());
    }
    let text = std::fs::read_to_string(&config_path)?;
    let updated = upsert_docs_path_toml(&text, &path_value);
    std::fs::write(&config_path, updated)?;
    Ok(())
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
        assert!(cfg.intelligence.enabled);
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

    #[test]
    fn load_parses_loop_section() {
        let tmp = TempDir::new().unwrap();
        let config_content = r#"
[loop]
enabled = true
reindex = "if_stale"
reindex_every_n = 5
max_iterations = 20
"#;
        fs::write(tmp.path().join(".codeindex.toml"), config_content).unwrap();
        let cfg = load(tmp.path()).unwrap();
        assert!(cfg.loop_config.enabled);
        assert_eq!(cfg.loop_config.reindex, ReindexPolicy::IfStale);
        assert_eq!(cfg.loop_config.reindex_every_n, 5);
        assert_eq!(cfg.loop_config.max_iterations, 20);
    }

    #[test]
    fn load_parses_locks_section() {
        let tmp = TempDir::new().unwrap();
        let config_content = r#"
[locks]
enabled = false
ttl_secs = 120
allow = ["src", "generated"]
"#;
        fs::write(tmp.path().join(".codeindex.toml"), config_content).unwrap();
        let cfg = load(tmp.path()).unwrap();
        assert!(!cfg.locks.enabled);
        assert_eq!(cfg.locks.ttl_secs, 120);
        assert_eq!(cfg.locks.allow, vec!["src", "generated"]);
    }

    #[test]
    fn locks_default_enabled() {
        let cfg = CodeIndexConfig::default();
        assert!(cfg.locks.enabled);
        assert_eq!(cfg.locks.ttl_secs, 600);
    }

    #[test]
    fn load_parses_docs_section() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join(".codeindex.toml"),
            "[docs]\npath = \"ai-docs\"\n",
        )
        .unwrap();
        let cfg = load(tmp.path()).unwrap();
        assert_eq!(cfg.docs.path.as_deref(), Some("ai-docs"));
    }

    #[test]
    fn persist_docs_creates_toml() {
        let tmp = TempDir::new().unwrap();
        persist_docs_path(tmp.path(), Path::new("../")).unwrap();
        let text = fs::read_to_string(tmp.path().join(".codeindex.toml")).unwrap();
        assert!(text.contains("[docs]"));
        assert!(text.contains("path = \"../\""));
        let cfg = load(tmp.path()).unwrap();
        assert_eq!(cfg.docs.path.as_deref(), Some("../"));
    }

    #[test]
    fn upsert_docs_preserves_other_sections() {
        let existing = "extra_ignore_dirs = [\"tmp\"]\n\n[compact]\nenabled = false\n";
        let out = upsert_docs_path_toml(existing, "docs");
        assert!(out.contains("extra_ignore_dirs"));
        assert!(out.contains("[compact]"));
        assert!(out.contains("enabled = false"));
        assert!(out.contains("[docs]"));
        assert!(out.contains("path = \"docs\""));
    }

    #[test]
    fn upsert_docs_updates_existing_path() {
        let existing = "[docs]\npath = \"old\"\n\n[locks]\nenabled = true\n";
        let out = upsert_docs_path_toml(existing, "../handbook");
        assert!(out.contains("path = \"../handbook\""));
        assert!(!out.contains("path = \"old\""));
        assert!(out.contains("[locks]"));
        let cfg: CodeIndexConfig = toml::from_str(&out).unwrap();
        assert_eq!(cfg.docs.path.as_deref(), Some("../handbook"));
    }

    #[test]
    fn docs_path_for_config_strips_dot_slash() {
        assert_eq!(
            docs_path_for_config(Path::new("/repo"), Path::new("./docs")),
            "docs"
        );
        assert_eq!(
            docs_path_for_config(Path::new("/repo"), Path::new("../")),
            "../"
        );
    }
}
