pub mod markers;
pub mod platforms;

use anyhow::{bail, Context, Result};
use platforms::{all_platforms, Platform};
use std::path::{Path, PathBuf};

const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

pub struct InstallOptions {
    pub platform: Option<String>,
    pub project: bool,
    pub security: bool,
    pub fs_tools: bool,
    pub list_only: bool,
}

pub fn run_install(opts: &InstallOptions) -> Result<()> {
    if opts.list_only {
        println!("Available platforms:");
        for p in all_platforms() {
            println!("  {} — {}", p.id, p.description);
        }
        return Ok(());
    }

    let platforms: Vec<Platform> = if let Some(ref id) = opts.platform {
        vec![Platform::by_id(id).with_context(|| format!("unknown platform '{id}'"))?]
    } else {
        all_platforms()
    };

    let exe = std::env::current_exe().context("could not resolve codebeacon binary path")?;
    let exe_str = exe.display().to_string();

    for platform in platforms {
        println!("Installing Codebeacon for {}...", platform.id);
        platform.install(&InstallCtx {
            exe: &exe_str,
            project: opts.project,
            security: opts.security,
            fs_tools: opts.fs_tools,
            manifest_dir: Path::new(MANIFEST_DIR),
        })?;
    }

    if opts.project {
        println!("\nHint: git add .cursor/ .vscode/ CLAUDE.md AGENTS.md .mcp.json  # as applicable");
    }

    Ok(())
}

pub struct UninstallOptions {
    pub platform: Option<String>,
    pub project: bool,
    pub purge: bool,
}

pub fn run_uninstall(opts: &UninstallOptions) -> Result<()> {
    let platforms: Vec<Platform> = if let Some(ref id) = opts.platform {
        vec![Platform::by_id(id).with_context(|| format!("unknown platform '{id}'"))?]
    } else {
        all_platforms()
    };

    for platform in platforms {
        println!("Uninstalling Codebeacon from {}...", platform.id);
        platform.uninstall(opts.project, opts.purge)?;
    }
    Ok(())
}

pub struct InstallCtx<'a> {
    pub exe: &'a str,
    pub project: bool,
    pub security: bool,
    pub fs_tools: bool,
    pub manifest_dir: &'a Path,
}

pub fn skill_content() -> &'static str {
    include_str!("../../assets/skill/SKILL.md")
}

pub fn mcp_args(security: bool, fs_tools: bool) -> Vec<String> {
    let mut args = vec!["serve".to_string()];
    if fs_tools {
        args.push("--fs-tools".to_string());
    }
    if security {
        args.push("--security".to_string());
    }
    args
}

pub fn mcp_json_block(exe: &str, security: bool, fs_tools: bool) -> String {
    let args = mcp_args(security, fs_tools);
    let args_json: Vec<_> = args.iter().map(|a| format!("\"{a}\"")).collect();
    format!(
        r#"{{
  "codebeacon": {{
    "command": "{exe}",
    "args": [{args}]
  }}
}}"#,
        exe = exe.replace('\\', "\\\\"),
        args = args_json.join(", ")
    )
}

pub fn copy_skill(dest: &Path, manifest_dir: &Path) -> Result<()> {
    let src = manifest_dir.join("assets/skill");
    if !src.exists() {
        bail!("skill assets not found at {}", src.display());
    }
    std::fs::create_dir_all(dest)?;
    copy_dir_recursive(&src, dest)?;
    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest_path = dest.join(entry.file_name());
        if ty.is_dir() {
            std::fs::create_dir_all(&dest_path)?;
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

pub fn project_root() -> Result<PathBuf> {
    std::env::current_dir().context("cwd")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_json_includes_security_flag() {
        let j = mcp_json_block("/usr/bin/codebeacon", true, false);
        assert!(j.contains("--security"));
        assert!(j.contains("codebeacon"));
    }

    #[test]
    fn install_list_does_not_fail() {
        run_install(&InstallOptions {
            platform: None,
            project: false,
            security: false,
            fs_tools: false,
            list_only: true,
        })
        .unwrap();
    }
}
