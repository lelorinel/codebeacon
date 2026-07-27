//! Architecture / layer boundary checks from `.codeindex.toml` [architecture].

use crate::config_file::ArchitectureConfig;
use crate::query::RepoQueryCtx;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct ArchViolation {
    pub from_file: String,
    pub to_file: String,
    pub from_layer: String,
    pub to_layer: String,
    pub rule: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchCheckResponse {
    pub enabled: bool,
    pub violations: Vec<ArchViolation>,
    pub checked_edges: usize,
    pub summary: String,
}

pub fn arch_check(ctx: &RepoQueryCtx, cfg: &ArchitectureConfig) -> ArchCheckResponse {
    if !cfg.enabled {
        return ArchCheckResponse {
            enabled: false,
            violations: vec![],
            checked_edges: 0,
            summary: "architecture checks disabled".into(),
        };
    }

    let mut violations = Vec::new();
    let mut checked = 0usize;

    for pkg in ctx.packages.values() {
        for file in &pkg.files {
            let from_layer = layer_for(&file.path, cfg);
            for dep in &file.depends_on {
                checked += 1;
                let to_path = Path::new(dep);
                let to_layer = layer_for(to_path, cfg);
                let (Some(fl), Some(tl)) = (&from_layer, &to_layer) else {
                    continue;
                };
                if fl == tl {
                    continue;
                }
                if is_denied(fl, tl, cfg) {
                    violations.push(ArchViolation {
                        from_file: file.path.to_string_lossy().into_owned(),
                        to_file: dep.clone(),
                        from_layer: fl.clone(),
                        to_layer: tl.clone(),
                        rule: format!("deny {fl} → {tl}"),
                    });
                    continue;
                }
                if has_allow_rules(cfg) && !is_allowed(fl, tl, cfg) {
                    violations.push(ArchViolation {
                        from_file: file.path.to_string_lossy().into_owned(),
                        to_file: dep.clone(),
                        from_layer: fl.clone(),
                        to_layer: tl.clone(),
                        rule: format!("not in allow list: {fl} → {tl}"),
                    });
                }
            }
        }
    }

    let summary = if violations.is_empty() {
        format!("ok — {checked} edges checked, no violations")
    } else {
        format!("{} violation(s) in {checked} edges", violations.len())
    };

    ArchCheckResponse {
        enabled: true,
        violations,
        checked_edges: checked,
        summary,
    }
}

fn layer_for(path: &Path, cfg: &ArchitectureConfig) -> Option<String> {
    let s = path.to_string_lossy();
    for m in &cfg.map {
        for pat in &m.packages {
            if glob_match(pat, &s) {
                return Some(m.layer.clone());
            }
        }
    }
    None
}

fn glob_match(pat: &str, path: &str) -> bool {
    if pat.ends_with("/**") {
        let prefix = &pat[..pat.len() - 3];
        return path.starts_with(prefix) || path.contains(&format!("/{prefix}"));
    }
    if pat.contains('*') {
        let parts: Vec<&str> = pat.split('*').collect();
        if parts.len() == 2 {
            return path.starts_with(parts[0]) && path.ends_with(parts[1]);
        }
    }
    path.contains(pat) || path == pat
}

fn is_denied(from: &str, to: &str, cfg: &ArchitectureConfig) -> bool {
    cfg.deny
        .iter()
        .any(|d| d.from == from && d.to.iter().any(|t| t == to))
}

fn is_allowed(from: &str, to: &str, cfg: &ArchitectureConfig) -> bool {
    cfg.allow
        .iter()
        .any(|a| a.from == from && a.to.iter().any(|t| t == to))
}

fn has_allow_rules(cfg: &ArchitectureConfig) -> bool {
    !cfg.allow.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_file::{ArchAllow, ArchDeny, ArchMap};

    #[test]
    fn glob_prefix() {
        assert!(glob_match("src/domain/**", "src/domain/user.rs"));
    }

    #[test]
    fn deny_domain_to_infra() {
        let cfg = ArchitectureConfig {
            enabled: true,
            layers: vec!["domain".into(), "infra".into()],
            map: vec![
                ArchMap {
                    layer: "domain".into(),
                    packages: vec!["src/domain/**".into()],
                },
                ArchMap {
                    layer: "infra".into(),
                    packages: vec!["src/infra/**".into()],
                },
            ],
            allow: vec![],
            deny: vec![ArchDeny {
                from: "domain".into(),
                to: vec!["infra".into()],
            }],
        };
        assert!(is_denied("domain", "infra", &cfg));
        assert!(!is_denied("infra", "domain", &cfg));
        let _ = ArchAllow {
            from: "app".into(),
            to: vec!["domain".into()],
        };
    }
}
