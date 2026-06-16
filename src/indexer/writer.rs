use crate::types::{PackageDetail, RepoIndex};
use anyhow::Result;
use std::path::Path;

pub fn write_package(pkg: &PackageDetail, codeindex_dir: &Path) -> Result<()> {
    let dir = codeindex_dir.join("packages");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", pkg.name));
    let json = serde_json::to_string_pretty(pkg)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn write_index(index: &RepoIndex, codeindex_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(codeindex_dir)?;
    let path = codeindex_dir.join("index.json");
    let json = serde_json::to_string_pretty(index)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn read_index(codeindex_dir: &Path) -> Result<Option<RepoIndex>> {
    let path = codeindex_dir.join("index.json");
    if !path.exists() { return Ok(None); }
    let text = std::fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&text)?))
}

pub fn read_package(name: &str, codeindex_dir: &Path) -> Result<Option<PackageDetail>> {
    let path = codeindex_dir.join("packages").join(format!("{name}.json"));
    if !path.exists() { return Ok(None); }
    let text = std::fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&text)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn sample_package() -> PackageDetail {
        PackageDetail {
            name: "auth".into(),
            files: vec![FileEntry {
                path: PathBuf::from("src/auth.rs"),
                symbols: vec![SymbolEntry {
                    name: "login".into(),
                    signature: "fn login() -> Token".into(),
                    kind: SymbolKind::Function,
                    line: 5,
                }],
                depends_on: vec!["db::find_user".into()],
                depended_by: vec![],
            }],
        }
    }

    #[test]
    fn write_and_read_package() {
        let tmp = TempDir::new().unwrap();
        let pkg = sample_package();
        write_package(&pkg, tmp.path()).unwrap();

        let path = tmp.path().join("packages").join("auth.json");
        assert!(path.exists());
        let back: PackageDetail = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(back.name, "auth");
        assert_eq!(back.files[0].symbols[0].name, "login");
    }

    #[test]
    fn write_and_read_index() {
        let tmp = TempDir::new().unwrap();
        let idx = RepoIndex {
            repo: "test-repo".into(),
            generated_at: "2026-06-16T00:00:00Z".into(),
            packages: vec![PackageSummary { name: "auth".into(), purpose: String::new(), files: 1, score: 0.9 }],
            hot_symbols: vec!["login".into()],
        };
        write_index(&idx, tmp.path()).unwrap();
        let path = tmp.path().join("index.json");
        assert!(path.exists());
        let back: RepoIndex = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(back.repo, "test-repo");
    }
}
