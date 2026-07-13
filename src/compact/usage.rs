use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageLog {
    #[serde(default)]
    pub drill_package: HashMap<String, u64>,
    #[serde(default)]
    pub find_definition: HashMap<String, u64>,
    #[serde(default)]
    pub find_references: HashMap<String, u64>,
    #[serde(default)]
    pub query_context: HashMap<String, u64>,
}

impl UsageLog {
    pub fn record(&mut self, tool: &str, key: &str) {
        let map = match tool {
            "drill_package" => &mut self.drill_package,
            "find_definition" => &mut self.find_definition,
            "find_references" => &mut self.find_references,
            "query_context" => &mut self.query_context,
            _ => return,
        };
        *map.entry(key.to_string()).or_insert(0) += 1;
    }

    pub fn score(&self, tool: &str, key: &str) -> u64 {
        let map = match tool {
            "drill_package" => &self.drill_package,
            "find_definition" => &self.find_definition,
            "find_references" => &self.find_references,
            "query_context" => &self.query_context,
            _ => return 0,
        };
        map.get(key).copied().unwrap_or(0)
    }
}

pub fn read_usage(codeindex_dir: &Path) -> Result<UsageLog> {
    let path = codeindex_dir.join("usage.json");
    if !path.exists() {
        return Ok(UsageLog::default());
    }
    let text = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text).unwrap_or_default())
}

pub fn write_usage(log: &UsageLog, codeindex_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(codeindex_dir)?;
    let path = codeindex_dir.join("usage.json");
    let json = serde_json::to_string_pretty(log)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn record_usage(codeindex_dir: &Path, tool: &str, key: &str) -> Result<()> {
    let mut log = read_usage(codeindex_dir)?;
    log.record(tool, key);
    write_usage(&log, codeindex_dir)
}

/// Re-rank hot symbol candidates by usage frequency (stable tie-break: alpha).
pub fn boost_hot_symbols(mut symbols: Vec<String>, usage: &UsageLog, limit: usize) -> Vec<String> {
    symbols.sort_by(|a, b| {
        let sa = usage.score("find_definition", a) + usage.score("query_context", a);
        let sb = usage.score("find_definition", b) + usage.score("query_context", b);
        sb.cmp(&sa).then_with(|| a.cmp(b))
    });
    symbols.truncate(limit);
    symbols
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boost_hot_symbols_by_usage() {
        let mut usage = UsageLog::default();
        usage.find_definition.insert("login".into(), 10);
        usage.find_definition.insert("logout".into(), 1);
        let boosted = boost_hot_symbols(vec!["logout".into(), "login".into()], &usage, 10);
        assert_eq!(boosted[0], "login");
    }
}
