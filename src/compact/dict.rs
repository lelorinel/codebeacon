use crate::types::{PackageDetail, SymbolEntry};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SymbolRef {
    pub p: String,
    pub n: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub g: String,
    pub l: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PersistentDict {
    pub rev: u64,
    pub paths: HashMap<String, String>,
    pub symbols: HashMap<String, SymbolRef>,
}

impl Default for DictSession {
    fn default() -> Self {
        Self {
            rev: 0,
            paths: HashMap::new(),
            path_to_id: HashMap::new(),
            symbols: HashMap::new(),
            next_path_id: 1,
            next_sym_id: 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DictSession {
    pub rev: u64,
    pub paths: HashMap<String, String>,
    pub path_to_id: HashMap<String, String>,
    pub symbols: HashMap<String, SymbolRef>,
    next_path_id: u32,
    next_sym_id: u32,
}

impl DictSession {
    pub fn from_persistent(dict: &PersistentDict) -> Self {
        let path_to_id = dict
            .paths
            .iter()
            .map(|(id, path)| (path.clone(), id.clone()))
            .collect();
        let next_path_id = dict
            .paths
            .keys()
            .filter_map(|k| k.strip_prefix('p').and_then(|n| n.parse::<u32>().ok()))
            .max()
            .map(|n| n + 1)
            .unwrap_or(1);
        let next_sym_id = dict
            .symbols
            .keys()
            .filter_map(|k| k.strip_prefix('s').and_then(|n| n.parse::<u32>().ok()))
            .max()
            .map(|n| n + 1)
            .unwrap_or(1);
        Self {
            rev: dict.rev,
            paths: dict.paths.clone(),
            path_to_id,
            symbols: dict.symbols.clone(),
            next_path_id,
            next_sym_id,
        }
    }

    pub fn path_id(&mut self, path: &str) -> String {
        if let Some(id) = self.path_to_id.get(path) {
            return id.clone();
        }
        let id = format!("p{}", self.next_path_id);
        self.next_path_id += 1;
        self.paths.insert(id.clone(), path.to_string());
        self.path_to_id.insert(path.to_string(), id.clone());
        id
    }

    pub fn symbol_id(&mut self, path_id: &str, sym: &SymbolEntry) -> String {
        let _key = format!("{}:{}:{}", path_id, sym.name, sym.line);
        if let Some((id, _)) = self.symbols.iter().find(|(_, r)| {
            r.p == path_id && r.n == sym.name && r.l == sym.line
        }) {
            return id.clone();
        }
        let id = format!("s{}", self.next_sym_id);
        self.next_sym_id += 1;
        self.symbols.insert(
            id.clone(),
            SymbolRef {
                p: path_id.to_string(),
                n: sym.name.clone(),
                g: sym.signature.clone(),
                l: sym.line,
            },
        );
        id
    }

    pub fn delta_since(&self, base: &PersistentDict) -> Option<PersistentDict> {
        let mut new_paths = HashMap::new();
        for (id, path) in &self.paths {
            if base.paths.get(id) != Some(path) {
                new_paths.insert(id.clone(), path.clone());
            }
        }
        let mut new_symbols = HashMap::new();
        for (id, sym) in &self.symbols {
            if base.symbols.get(id) != Some(sym) {
                new_symbols.insert(id.clone(), sym.clone());
            }
        }
        if new_paths.is_empty() && new_symbols.is_empty() {
            None
        } else {
            Some(PersistentDict {
                rev: self.rev,
                paths: new_paths,
                symbols: new_symbols,
            })
        }
    }

    pub fn to_persistent(&self) -> PersistentDict {
        PersistentDict {
            rev: self.rev,
            paths: self.paths.clone(),
            symbols: self.symbols.clone(),
        }
    }
}

pub fn build_dict_from_packages(packages: &[PackageDetail], prev_rev: u64) -> PersistentDict {
    let mut paths_sorted: Vec<String> = packages
        .iter()
        .flat_map(|p| p.files.iter().map(|f| f.path.to_string_lossy().into_owned()))
        .collect();
    paths_sorted.sort();
    paths_sorted.dedup();

    let mut paths = HashMap::new();
    for (i, path) in paths_sorted.iter().enumerate() {
        paths.insert(format!("p{}", i + 1), path.clone());
    }

    let mut symbols = HashMap::new();
    let mut sym_idx = 1u32;
    for pkg in packages {
        for file in &pkg.files {
            let path_str = file.path.to_string_lossy();
            let path_id = paths
                .iter()
                .find(|(_, p)| p.as_str() == path_str)
                .map(|(id, _)| id.clone())
                .unwrap_or_else(|| "p0".to_string());
            for sym in &file.symbols {
                let id = format!("s{sym_idx}");
                sym_idx += 1;
                symbols.insert(
                    id,
                    SymbolRef {
                        p: path_id.clone(),
                        n: sym.name.clone(),
                        g: sym.signature.clone(),
                        l: sym.line,
                    },
                );
            }
        }
    }

    PersistentDict {
        rev: prev_rev.saturating_add(1),
        paths,
        symbols,
    }
}

pub fn read_dict(codeindex_dir: &Path) -> Result<Option<PersistentDict>> {
    let path = codeindex_dir.join("dict.json");
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&text)?))
}

pub fn write_dict(dict: &PersistentDict, codeindex_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(codeindex_dir)?;
    let path = codeindex_dir.join("dict.json");
    let json = serde_json::to_string_pretty(dict)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn session_for_repo(codeindex_dir: &Path) -> DictSession {
    match read_dict(codeindex_dir) {
        Ok(Some(dict)) => DictSession::from_persistent(&dict),
        _ => DictSession::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FileEntry, PackageDetail, SymbolEntry, SymbolKind};
    use std::path::PathBuf;

    fn file(path: &str, sym: &str) -> FileEntry {
        FileEntry {
            path: PathBuf::from(path),
            symbols: vec![SymbolEntry {
                name: sym.into(),
                signature: format!("fn {sym}()"),
                kind: SymbolKind::Function,
                line: 1,
                character: 0,
            }],
            depends_on: vec![],
            depended_by: vec![],
        }
    }

    #[test]
    fn build_dict_deterministic_paths() {
        let packages = vec![
            PackageDetail {
                name: "auth".into(),
                files: vec![file("src/auth.rs", "login"), file("src/db.rs", "find")],
            },
        ];
        let dict = build_dict_from_packages(&packages, 0);
        assert_eq!(dict.paths.get("p1").map(String::as_str), Some("src/auth.rs"));
        assert_eq!(dict.paths.get("p2").map(String::as_str), Some("src/db.rs"));
        assert_eq!(dict.rev, 1);
    }

    #[test]
    fn session_path_id_stable() {
        let mut session = DictSession::default();
        let a = session.path_id("src/a.rs");
        let b = session.path_id("src/a.rs");
        assert_eq!(a, b);
        assert_eq!(a, "p1");
    }
}
