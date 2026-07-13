use crate::types::PackageDetail;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PackageConventions {
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub async_fn: bool,
    #[serde(default)]
    pub has_tests: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConventionsStore {
    #[serde(default)]
    pub packages: HashMap<String, PackageConventions>,
}

const PATTERNS: &[(&str, &str)] = &[
    ("error:anyhow", "anyhow"),
    ("error:thiserror", "thiserror"),
    ("error:Result", "Result<"),
    ("log:tracing", "tracing::"),
    ("log:log", "log::"),
    ("async", "async fn"),
    ("test", "#[test]"),
    ("test", "def test_"),
    ("export", "export "),
    ("pub", "pub fn"),
];

pub fn extract_package_conventions(pkg: &PackageDetail, repo_root: &Path) -> PackageConventions {
    let mut tag_counts: HashMap<String, u32> = HashMap::new();
    let mut async_fn = false;
    let mut has_tests = false;

    for file in &pkg.files {
        let abs = repo_root.join(&file.path);
        let content = std::fs::read_to_string(&abs).unwrap_or_default();
        for (tag, needle) in PATTERNS {
            if content.contains(needle) {
                *tag_counts.entry((*tag).to_string()).or_insert(0) += 1;
            }
        }
        if content.contains("async fn") {
            async_fn = true;
        }
        if content.contains("#[test]") || content.contains("def test_") {
            has_tests = true;
        }
    }

    let mut tags: Vec<(String, u32)> = tag_counts.into_iter().collect();
    tags.sort_by(|a, b| b.1.cmp(&a.1));
    let tags: Vec<String> = tags.into_iter().take(5).map(|(t, _)| t).collect();

    PackageConventions {
        tags,
        async_fn,
        has_tests,
    }
}

pub fn build_conventions_store(
    packages: &[PackageDetail],
    repo_root: &Path,
) -> ConventionsStore {
    let mut store = ConventionsStore::default();
    for pkg in packages {
        store.packages.insert(
            pkg.name.clone(),
            extract_package_conventions(pkg, repo_root),
        );
    }
    store
}

pub fn purpose_for_package(
    pkg: &PackageDetail,
    conv: Option<&PackageConventions>,
) -> String {
    let mut symbols: Vec<String> = pkg
        .files
        .iter()
        .flat_map(|f| f.symbols.iter().map(|s| s.name.clone()))
        .collect();
    symbols.sort();
    symbols.dedup();
    symbols.truncate(3);

    let sym_part = if symbols.is_empty() {
        String::new()
    } else {
        format!(": {}", symbols.join(", "))
    };

    let conv_part = conv
        .map(|c| {
            if c.tags.is_empty() {
                String::new()
            } else {
                format!(" ({})", c.tags.join(", "))
            }
        })
        .unwrap_or_default();

    format!("{}{}{}", pkg.name, sym_part, conv_part)
}

pub fn read_conventions(codeindex_dir: &Path) -> Result<ConventionsStore> {
    let path = codeindex_dir.join("conventions.json");
    if !path.exists() {
        return Ok(ConventionsStore::default());
    }
    let text = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text).unwrap_or_default())
}

pub fn write_conventions(store: &ConventionsStore, codeindex_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(codeindex_dir)?;
    let path = codeindex_dir.join("conventions.json");
    std::fs::write(path, serde_json::to_string_pretty(store)?)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct ConventionResponse {
    pub package: String,
    pub conventions: PackageConventions,
    pub example_signatures: Vec<String>,
}

pub fn package_conventions(
    pkg: &PackageDetail,
    store: &ConventionsStore,
) -> ConventionResponse {
    let conventions = store
        .packages
        .get(&pkg.name)
        .cloned()
        .unwrap_or_default();

    let mut example_signatures: Vec<String> = pkg
        .files
        .iter()
        .flat_map(|f| f.symbols.iter())
        .filter(|s| s.signature.contains("pub "))
        .take(2)
        .map(|s| s.signature.clone())
        .collect();
    if example_signatures.is_empty() {
        example_signatures = pkg
            .files
            .iter()
            .flat_map(|f| f.symbols.iter())
            .take(2)
            .map(|s| s.signature.clone())
            .collect();
    }

    ConventionResponse {
        package: pkg.name.clone(),
        conventions,
        example_signatures,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FileEntry, SymbolEntry, SymbolKind};
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn purpose_includes_symbols() {
        let pkg = PackageDetail {
            name: "auth".into(),
            files: vec![FileEntry {
                path: PathBuf::from("src/auth.rs"),
                symbols: vec![
                    SymbolEntry {
                        name: "login".into(),
                        signature: "fn login()".into(),
                        kind: SymbolKind::Function,
                        line: 1,
                        character: 0,
                    },
                ],
                depends_on: vec![],
                depended_by: vec![],
            }],
        };
        let p = purpose_for_package(&pkg, None);
        assert!(p.contains("login"));
    }
}
