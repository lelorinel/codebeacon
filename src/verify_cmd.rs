use crate::config_file;
use crate::security::{verify_and_decide, VerifyReport};
use anyhow::Result;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct VerifyCliOutput {
    pub action: &'static str,
    pub report: VerifyReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Run security verification for the `codebeacon verify` CLI subcommand.
///
/// Security is always enabled for this command; mode and Z3 settings come from
/// `.codeindex.toml` when present.
pub fn run_verify(content: &str, path: Option<&Path>, root: Option<PathBuf>) -> Result<VerifyCliOutput> {
    let repos = crate::config::discover_repos(
        &root.clone().unwrap_or_else(|| std::env::current_dir().expect("cwd")),
    );
    let repo_root = if let Some(r) = root {
        r
    } else if repos.len() == 1 {
        repos[0].clone()
    } else if repos.is_empty() {
        std::env::current_dir()?
    } else {
        anyhow::bail!(
            "Multiple git repos in workspace. Use --root to select one for security policy."
        );
    };

    let cfg = config_file::load(&repo_root)?;
    let policy = cfg.security.to_policy(true);

    let verify_path = path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("fragment"));

    let (report, action) = verify_and_decide(&verify_path, content, &policy);
    let message = action.message().map(str::to_string);

    Ok(VerifyCliOutput {
        action: action.label(),
        report,
        message,
    })
}

pub fn print_human_output(out: &VerifyCliOutput) {
    match out.action {
        "allow" if out.report.findings.is_empty() => {
            println!(
                "No CWE-190 allocation sites found in `{}` ({} ms).",
                out.report.path, out.report.elapsed_ms
            );
        }
        "allow" => {
            println!(
                "All {} site(s) in `{}` are proven safe ({} Z3 call(s), {} ms).",
                out.report.sites_checked,
                out.report.path,
                out.report.z3_invocations,
                out.report.elapsed_ms
            );
        }
        "warn" | "block" => {
            if let Some(msg) = &out.message {
                print!("{msg}");
            }
        }
        _ => {}
    }
}

pub fn exit_code_for_action(action: &str) -> i32 {
    if action == "block" {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn git_repo() -> TempDir {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();
        tmp
    }

    #[test]
    fn verify_cli_blocks_vulnerable_malloc() {
        let tmp = git_repo();
        let out = run_verify(
            "int* p = malloc(n * sizeof(int));",
            Some(Path::new("alloc.c")),
            Some(tmp.path().to_path_buf()),
        )
        .unwrap();

        assert_eq!(out.report.sites_checked, 1);
        assert!(!out.report.findings.is_empty());

        #[cfg(feature = "security-z3")]
        assert_eq!(out.action, "block");

        #[cfg(not(feature = "security-z3"))]
        assert_eq!(out.action, "warn");
    }

    #[test]
    fn verify_cli_allows_safe_code() {
        let tmp = git_repo();
        let out = run_verify("return x + 1;", None, Some(tmp.path().to_path_buf())).unwrap();
        assert_eq!(out.action, "allow");
        assert!(out.report.findings.is_empty());
    }
}
