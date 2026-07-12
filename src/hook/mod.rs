//! Git post-commit hook for incremental re-indexing.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

const HOOK_START: &str = "# codebeacon-hook-start";
const HOOK_END: &str = "# codebeacon-hook-end";

pub struct HookOptions {
    pub root: PathBuf,
}

impl HookOptions {
    pub fn resolve(root: Option<PathBuf>) -> Result<Self> {
        let root = match root {
            Some(p) => p,
            None => crate::config::find_repo_root(&std::env::current_dir()?)
                .context("not in a git repo — use --root")?,
        };
        Ok(Self { root })
    }
}

fn hooks_dir(root: &Path) -> PathBuf {
    root.join(".git/hooks")
}

fn hook_script(exe: &Path, root: &Path) -> String {
    format!(
        r#"#!/bin/sh
{HOOK_START}
# Re-index after commit (Codebeacon)
"{}" init --root "{}" 2>/dev/null || true
{HOOK_END}
"#,
        exe.display(),
        root.display()
    )
}

pub fn install(opts: &HookOptions) -> Result<()> {
    let exe = std::env::current_exe().context("could not resolve codebeacon binary")?;
    let hook_path = hooks_dir(&opts.root).join("post-commit");
    let script = hook_script(&exe, &opts.root);

    let existing = fs::read_to_string(&hook_path).unwrap_or_default();
    if existing.contains(HOOK_START) {
        let start = existing.find(HOOK_START).unwrap();
        let end = existing.find(HOOK_END).unwrap() + HOOK_END.len();
        let mut out = existing[..start].to_string();
        out.push_str(&script);
        if end < existing.len() {
            out.push_str(&existing[end..]);
        }
        fs::write(&hook_path, out)?;
    } else if existing.is_empty() {
        fs::write(&hook_path, &script)?;
    } else {
        fs::write(&hook_path, format!("{existing}\n{script}"))?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755))?;
    }

    println!("Installed post-commit hook at {}", hook_path.display());
    Ok(())
}

pub fn uninstall(opts: &HookOptions) -> Result<()> {
    let hook_path = hooks_dir(&opts.root).join("post-commit");
    if !hook_path.exists() {
        println!("No post-commit hook found.");
        return Ok(());
    }
    let existing = fs::read_to_string(&hook_path)?;
    if !existing.contains(HOOK_START) {
        println!("No Codebeacon section in post-commit hook.");
        return Ok(());
    }
    let start = existing.find(HOOK_START).unwrap();
    let end = existing.find(HOOK_END).unwrap() + HOOK_END.len();
    let out = format!("{}{}", &existing[..start], &existing[end..]);
    if out.trim().is_empty() {
        fs::remove_file(&hook_path)?;
    } else {
        fs::write(&hook_path, out.trim_end())?;
    }
    println!("Removed Codebeacon section from post-commit hook.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_script_contains_marker() {
        let s = hook_script(Path::new("/bin/codebeacon"), Path::new("/repo"));
        assert!(s.contains(HOOK_START));
        assert!(s.contains("init --root"));
    }
}
