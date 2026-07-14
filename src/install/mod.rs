pub mod markers;
pub mod platforms;

use anyhow::{Context, Result};
use platforms::{all_platforms, Platform};
use std::path::{Path, PathBuf};

pub struct InstallOptions {
    pub platform: Option<String>,
    pub project: bool,
    pub security: bool,
    pub fs_tools: bool,
    pub list_only: bool,
    /// Non-interactive yes: auto-run `init` when no index exists.
    pub yes: bool,
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
        })?;
    }

    if opts.project {
        println!("\nHint: git add .cursor/ .vscode/ CLAUDE.md AGENTS.md .mcp.json  # as applicable");
    }

    maybe_init_after_install(opts)?;

    Ok(())
}

/// True when `.codeindex/index.json` exists under `root`.
pub fn index_present(root: &Path) -> bool {
    crate::config::codeindex_dir(root)
        .join("index.json")
        .is_file()
}

/// Empty / y / yes → run init; n / no / other → skip.
pub fn parse_init_reply(s: &str) -> bool {
    let t = s.trim().to_ascii_lowercase();
    t.is_empty() || t == "y" || t == "yes"
}

fn stdin_interactive() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

fn maybe_init_after_install(opts: &InstallOptions) -> Result<()> {
    let root = project_root()?;
    if index_present(&root) {
        return Ok(());
    }

    let should_init = if opts.yes {
        true
    } else if stdin_interactive() {
        eprint!("No .codeindex/index.json — run init now? [Y/n] ");
        use std::io::Write;
        let _ = std::io::stderr().flush();
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .context("reading init prompt")?;
        parse_init_reply(&line)
    } else {
        println!(
            "No index found. Run `codebeacon init` when ready (pass --yes to auto-init)."
        );
        false
    };

    if !should_init {
        return Ok(());
    }

    println!("Running init in {}...", root.display());
    let mut indexer = crate::indexer::Indexer::new(&root);
    indexer.full_index()?;
    println!("Index written to {}/.codeindex/", root.display());
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
}

pub fn skill_content() -> &'static str {
    include_str!("../../assets/skill/SKILL.md")
}

/// Skill tree embedded at compile time so install works from crates.io / release binaries.
const SKILL_FILES: &[(&str, &str)] = &[
    ("SKILL.md", include_str!("../../assets/skill/SKILL.md")),
    (
        "references/loop.md",
        include_str!("../../assets/skill/references/loop.md"),
    ),
    (
        "references/mcp-tools.md",
        include_str!("../../assets/skill/references/mcp-tools.md"),
    ),
    (
        "references/security.md",
        include_str!("../../assets/skill/references/security.md"),
    ),
];

pub const HOOK_CONTEXT_SH: &str = include_str!("../../assets/hooks/codebeacon-context.sh");
pub const HOOK_SECURITY_SH: &str = include_str!("../../assets/hooks/codebeacon-security.sh");
pub const CURSOR_HOOKS_EXAMPLE: &str =
    include_str!("../../assets/hooks/cursor-hooks.json.example");
pub const CLAUDE_DISCOVERY_HOOK: &str =
    include_str!("../../assets/hooks/claude-discovery-hook.json");

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

pub fn copy_skill(dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    for (rel, content) in SKILL_FILES {
        let path = dest.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, content)?;
    }
    Ok(())
}

pub fn write_hook_script(dest: &Path, content: &str) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(dest, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))?;
    }
    println!("  wrote {}", dest.display());
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
            yes: false,
        })
        .unwrap();
    }

    #[test]
    fn parse_init_reply_defaults_yes() {
        assert!(parse_init_reply(""));
        assert!(parse_init_reply("\n"));
        assert!(parse_init_reply("y"));
        assert!(parse_init_reply("Y"));
        assert!(parse_init_reply("yes"));
        assert!(!parse_init_reply("n"));
        assert!(!parse_init_reply("no"));
        assert!(!parse_init_reply("nope"));
    }

    #[test]
    fn index_present_checks_index_json() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!index_present(dir.path()));
        let ci = crate::config::codeindex_dir(dir.path());
        std::fs::create_dir_all(&ci).unwrap();
        assert!(!index_present(dir.path()));
        std::fs::write(ci.join("index.json"), "{}").unwrap();
        assert!(index_present(dir.path()));
    }

    #[test]
    fn copy_skill_writes_embedded_files() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("skills/codebeacon");
        copy_skill(&dest).unwrap();
        assert!(dest.join("SKILL.md").is_file());
        assert!(dest.join("references/loop.md").is_file());
        assert!(dest.join("references/mcp-tools.md").is_file());
        assert!(dest.join("references/security.md").is_file());
        assert!(std::fs::read_to_string(dest.join("SKILL.md"))
            .unwrap()
            .contains("get_context"));
    }
}
