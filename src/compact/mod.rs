//! Token-compressed MCP responses with path/symbol dictionary refs.

mod decode;
mod dict;
mod encode;
mod schema;
mod usage;

pub use decode::{expand_path, is_dict_ref, resolve_file_arg_with_root};
pub use dict::{
    build_dict_from_packages, read_dict, session_for_repo, write_dict, DictSession, PersistentDict,
    SymbolRef,
};
pub use encode::{
    encode_index_response, encode_package_response, encode_query_matches, path_ref_for,
};
pub use schema::{CompactFileEntry, CompactPackageDetail, CompactRepoIndex, CompactSymbolEntry};
pub use usage::{boost_hot_symbols, read_usage, record_usage, UsageLog};

use crate::config_file::CompactConfig;
use serde_json::Value;

/// Resolve compact mode: per-call `compact` arg overrides config (default true).
pub fn compact_mode(args: &Value, config: &CompactConfig) -> bool {
    args.get("compact")
        .and_then(|v| v.as_bool())
        .unwrap_or(config.enabled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_mode_defaults_true() {
        let cfg = CompactConfig::default();
        assert!(compact_mode(&Value::Null, &cfg));
    }

    #[test]
    fn compact_mode_config_false() {
        let cfg = CompactConfig { enabled: false };
        assert!(!compact_mode(&Value::Null, &cfg));
    }

    #[test]
    fn compact_mode_arg_overrides_config() {
        let cfg = CompactConfig { enabled: false };
        assert!(compact_mode(&serde_json::json!({"compact": true}), &cfg));
    }
}
