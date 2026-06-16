use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoIndex {
    pub repo: String,
    pub generated_at: String,
    pub packages: Vec<PackageSummary>,
    pub hot_symbols: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageSummary {
    pub name: String,
    pub purpose: String,
    pub files: usize,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageDetail {
    pub name: String,
    pub files: Vec<FileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub symbols: Vec<SymbolEntry>,
    pub depends_on: Vec<String>,
    pub depended_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolEntry {
    pub name: String,
    pub signature: String,
    pub kind: SymbolKind,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Trait,
    Module,
    Variable,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceLocation {
    pub file: PathBuf,
    pub line: u32,
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinitionLocation {
    pub file: PathBuf,
    pub line: u32,
    pub signature: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_index_serializes_round_trip() {
        let idx = RepoIndex {
            repo: "my-project".into(),
            generated_at: "2026-06-16T10:00:00Z".into(),
            packages: vec![PackageSummary {
                name: "auth".into(),
                purpose: "user auth".into(),
                files: 3,
                score: 0.9,
            }],
            hot_symbols: vec!["Login".into()],
        };
        let json = serde_json::to_string(&idx).unwrap();
        let back: RepoIndex = serde_json::from_str(&json).unwrap();
        assert_eq!(back.repo, "my-project");
        assert_eq!(back.packages[0].name, "auth");
        assert!((back.packages[0].score - 0.9).abs() < 0.001);
    }
}
