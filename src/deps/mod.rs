//! Dependency freshness: parse Cargo.toml / package.json / go.mod (offline by default).

use crate::config_file::DepsConfig;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct DepEntry {
    pub ecosystem: String,
    pub name: String,
    pub declared: String,
    pub locked: Option<String>,
    pub status: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DepFreshnessResponse {
    pub deps: Vec<DepEntry>,
    pub summary: String,
}

pub fn dep_freshness(repo_root: &Path, cfg: &DepsConfig) -> DepFreshnessResponse {
    if !cfg.enabled {
        return DepFreshnessResponse {
            deps: vec![],
            summary: "deps checks disabled".into(),
        };
    }

    let mut deps = Vec::new();
    deps.extend(parse_cargo(repo_root));
    deps.extend(parse_package_json(repo_root));
    deps.extend(parse_go_mod(repo_root));

    if cfg.check_registry {
        for d in &mut deps {
            if let Some(note) = registry_hint(&d.ecosystem, &d.name, &d.declared) {
                d.note = note;
                if d.status == "ok" {
                    d.status = "checked".into();
                }
            }
        }
    }

    let outdated = deps.iter().filter(|d| d.status == "drift" || d.status == "outdated").count();
    let summary = format!(
        "{} dependencies scanned, {} possible drift/outdated",
        deps.len(),
        outdated
    );

    DepFreshnessResponse { deps, summary }
}

fn parse_cargo(root: &Path) -> Vec<DepEntry> {
    let path = root.join("Cargo.toml");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    let lock = parse_cargo_lock_versions(root);
    let mut out = Vec::new();
    let mut in_deps = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_deps = t == "[dependencies]" || t == "[dev-dependencies]" || t == "[build-dependencies]";
            continue;
        }
        if !in_deps || t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some((name, ver)) = parse_toml_dep_line(t) {
            let locked = lock.get(&name).cloned();
            let status = match &locked {
                Some(l) if !version_compatible(&ver, l) => "drift".to_string(),
                Some(_) => "ok".to_string(),
                None => "unlocked".to_string(),
            };
            out.push(DepEntry {
                ecosystem: "cargo".into(),
                name,
                declared: ver.to_string(),
                locked,
                status,
                note: String::new(),
            });
        }
    }
    out
}

fn parse_toml_dep_line(line: &str) -> Option<(String, String)> {
    let line = line.split('#').next()?.trim();
    if let Some((name, rest)) = line.split_once('=') {
        let name = name.trim().trim_matches('"').to_string();
        let rest = rest.trim();
        if rest.starts_with('{') {
            // path/git deps — skip version freshness
            if rest.contains("path") || rest.contains("git") {
                return Some((name, "path/git".into()));
            }
            if let Some(v) = extract_quoted_field(rest, "version") {
                return Some((name, v));
            }
            return None;
        }
        let ver = rest.trim_matches('"').to_string();
        return Some((name, ver));
    }
    None
}

fn extract_quoted_field(s: &str, field: &str) -> Option<String> {
    let key = format!("{field} =");
    let idx = s.find(&key)?;
    let rest = &s[idx + key.len()..];
    let rest = rest.trim_start();
    if let Some(stripped) = rest.strip_prefix('"') {
        let end = stripped.find('"')?;
        return Some(stripped[..end].to_string());
    }
    None
}

fn parse_cargo_lock_versions(root: &Path) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let text = match std::fs::read_to_string(root.join("Cargo.lock")) {
        Ok(t) => t,
        Err(_) => return map,
    };
    let mut name = None;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("name = ") {
            name = t
                .trim_start_matches("name = ")
                .trim()
                .trim_matches('"')
                .to_string()
                .into();
        } else if t.starts_with("version = ") {
            if let Some(n) = name.take() {
                let ver = t
                    .trim_start_matches("version = ")
                    .trim()
                    .trim_matches('"')
                    .to_string();
                map.entry(n).or_insert(ver);
            }
        }
    }
    map
}

fn parse_package_json(root: &Path) -> Vec<DepEntry> {
    let path = root.join("package.json");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    let v: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let lock = parse_package_lock(root);
    let mut out = Vec::new();
    for key in ["dependencies", "devDependencies"] {
        if let Some(obj) = v.get(key).and_then(|x| x.as_object()) {
            for (name, ver) in obj {
                let declared = ver.as_str().unwrap_or("").to_string();
                let locked = lock.get(name).cloned();
                let status = if locked.is_some() { "ok" } else { "unlocked" };
                out.push(DepEntry {
                    ecosystem: "npm".into(),
                    name: name.clone(),
                    declared,
                    locked,
                    status: status.into(),
                    note: String::new(),
                });
            }
        }
    }
    out
}

fn parse_package_lock(root: &Path) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for lock_name in ["package-lock.json", "pnpm-lock.yaml", "yarn.lock"] {
        let path = root.join(lock_name);
        if !path.exists() {
            continue;
        }
        if lock_name == "package-lock.json" {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(packages) = v.get("packages").and_then(|p| p.as_object()) {
                        for (k, meta) in packages {
                            let name = k.trim_start_matches("node_modules/");
                            if name.is_empty() || name.contains('/') {
                                continue;
                            }
                            if let Some(ver) = meta.get("version").and_then(|x| x.as_str()) {
                                map.insert(name.to_string(), ver.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    map
}

fn parse_go_mod(root: &Path) -> Vec<DepEntry> {
    let path = root.join("go.mod");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    let mut out = Vec::new();
    let mut in_require = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("require (") {
            in_require = true;
            continue;
        }
        if in_require && t == ")" {
            in_require = false;
            continue;
        }
        if t.starts_with("require ") && !t.contains('(') {
            let parts: Vec<_> = t.trim_start_matches("require ").split_whitespace().collect();
            if parts.len() >= 2 {
                out.push(DepEntry {
                    ecosystem: "go".into(),
                    name: parts[0].to_string(),
                    declared: parts[1].to_string(),
                    locked: None,
                    status: "ok".into(),
                    note: String::new(),
                });
            }
            continue;
        }
        if in_require {
            let parts: Vec<_> = t.split_whitespace().collect();
            if parts.len() >= 2 {
                out.push(DepEntry {
                    ecosystem: "go".into(),
                    name: parts[0].to_string(),
                    declared: parts[1].to_string(),
                    locked: None,
                    status: "ok".into(),
                    note: String::new(),
                });
            }
        }
    }
    out
}

fn version_compatible(declared: &str, locked: &str) -> bool {
    if declared == "path/git" || declared.is_empty() {
        return true;
    }
    let d = declared.trim_start_matches(['^', '~', '=', '>', '<', ' ']);
    locked.starts_with(d) || locked.starts_with(&format!("{d}.")) || d.starts_with(locked)
}

fn registry_hint(ecosystem: &str, name: &str, declared: &str) -> Option<String> {
    // Soft network probe — failures are ignored.
    match ecosystem {
        "cargo" => {
            let url = format!("https://crates.io/api/v1/crates/{name}");
            let out = std::process::Command::new("curl")
                .args(["-fsSL", "--max-time", "3", &url])
                .output()
                .ok()?;
            if !out.status.success() {
                return None;
            }
            let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
            let latest = v
                .pointer("/crate/max_version")
                .and_then(|x| x.as_str())?;
            if !version_compatible(declared, latest) {
                Some(format!("latest on crates.io: {latest}"))
            } else {
                Some(format!("latest {latest}"))
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parses_cargo_toml_deps() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            r#"
[package]
name = "x"
version = "0.1.0"

[dependencies]
serde = "1"
anyhow = { version = "1.0" }
"#,
        )
        .unwrap();
        let deps = parse_cargo(tmp.path());
        assert!(deps.iter().any(|d| d.name == "serde"));
        assert!(deps.iter().any(|d| d.name == "anyhow"));
    }
}
