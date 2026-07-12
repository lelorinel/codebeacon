pub mod regex;

#[cfg(feature = "tree-sitter")]
mod tree_sitter;

#[cfg(not(feature = "tree-sitter"))]
mod tree_sitter {
    use crate::config::Language;
    use crate::imports::RawImport;
    use crate::types::SymbolEntry;
    use std::path::Path;

    pub fn extract(
        _path: &Path,
        _code: &str,
        _lang: &Language,
        _timeout_ms: u64,
    ) -> Result<(Vec<SymbolEntry>, Vec<RawImport>), String> {
        Err("tree-sitter feature not enabled".into())
    }
}

use crate::config::{detect_language, Language};
use crate::config_file::ExtractorConfig;
use crate::imports::RawImport;
use crate::types::SymbolEntry;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractEngine {
    Regex,
    TreeSitter,
}

#[derive(Debug, Clone)]
pub struct ExtractResult {
    pub symbols: Vec<SymbolEntry>,
    pub imports: Vec<RawImport>,
    pub engine: ExtractEngine,
}

/// Unified extraction entry point: tree-sitter when enabled/configured, else regex.
pub fn extract_file(path: &Path, config: &ExtractorConfig) -> ExtractResult {
    let lang = match detect_language(path) {
        Some(l) => l,
        None => {
            return ExtractResult {
                symbols: vec![],
                imports: vec![],
                engine: ExtractEngine::Regex,
            };
        }
    };

    let code = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => {
            return ExtractResult {
                symbols: vec![],
                imports: vec![],
                engine: ExtractEngine::Regex,
            };
        }
    };

    extract_from_source(path, &code, &lang, config)
}

/// Extract from in-memory source (used by tests and incremental daemon path).
pub fn extract_from_source(
    path: &Path,
    code: &str,
    lang: &Language,
    config: &ExtractorConfig,
) -> ExtractResult {
    if should_use_tree_sitter(config, code.len()) {
        match tree_sitter::extract(path, code, lang, config.parse_timeout_ms) {
            Ok((symbols, imports)) => {
                tracing::debug!(
                    path = %path.display(),
                    engine = "tree-sitter",
                    symbols = symbols.len(),
                    imports = imports.len(),
                    "extract"
                );
                return ExtractResult {
                    symbols,
                    imports,
                    engine: ExtractEngine::TreeSitter,
                };
            }
            Err(reason) => {
                tracing::debug!(
                    path = %path.display(),
                    reason = %reason,
                    "tree-sitter fallback to regex"
                );
            }
        }
    }

    let (symbols, imports) = regex::extract_from_parts(code, lang);
    tracing::debug!(
        path = %path.display(),
        engine = "regex",
        symbols = symbols.len(),
        imports = imports.len(),
        "extract"
    );
    ExtractResult {
        symbols,
        imports,
        engine: ExtractEngine::Regex,
    }
}

fn should_use_tree_sitter(config: &ExtractorConfig, byte_len: usize) -> bool {
    if !config.uses_tree_sitter() {
        return false;
    }
    if byte_len > config.max_tree_sitter_bytes {
        tracing::debug!(
            bytes = byte_len,
            max = config.max_tree_sitter_bytes,
            "file too large for tree-sitter"
        );
        return false;
    }
    true
}
