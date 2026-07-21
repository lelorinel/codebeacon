use super::markers::{merge_marked_section, merge_mcp_json, remove_marked_section, remove_mcp_json};
use super::{
    copy_skill, mcp_json_block, write_hook_script, InstallCtx, CLAUDE_DISCOVERY_HOOK,
    CURSOR_HOOKS_EXAMPLE, HOOK_CONTEXT_SH, HOOK_SECURITY_SH,
};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
pub enum PlatformKind {
    Project,
    User,
    Both,
}

pub struct Platform {
    pub id: &'static str,
    pub description: &'static str,
    pub kind: PlatformKind,
    install_fn: fn(&InstallCtx) -> Result<()>,
    uninstall_fn: fn(bool, bool) -> Result<()>,
}

impl Platform {
    pub fn by_id(id: &str) -> Result<Self> {
        all_platforms()
            .into_iter()
            .find(|p| p.id == id)
            .with_context(|| format!("unknown platform: {id}"))
    }

    pub fn install(&self, ctx: &InstallCtx) -> Result<()> {
        (self.install_fn)(ctx)
    }

    pub fn uninstall(&self, project: bool, purge: bool) -> Result<()> {
        (self.uninstall_fn)(project, purge)
    }
}

pub fn all_platforms() -> Vec<Platform> {
    vec![
        Platform {
            id: "cursor",
            description: "Cursor IDE — .cursor/rules + mcp.json",
            kind: PlatformKind::Project,
            install_fn: install_cursor,
            uninstall_fn: uninstall_cursor,
        },
        Platform {
            id: "claude",
            description: "Claude Code — CLAUDE.md + discovery hook",
            kind: PlatformKind::Both,
            install_fn: install_claude,
            uninstall_fn: uninstall_claude,
        },
        Platform {
            id: "codex",
            description: "Codex — AGENTS.md + ~/.codex/config.toml MCP + hooks",
            kind: PlatformKind::Both,
            install_fn: install_codex,
            uninstall_fn: uninstall_codex,
        },
        Platform {
            id: "opencode",
            description: "OpenCode — skill + opencode.json MCP",
            kind: PlatformKind::User,
            install_fn: install_opencode,
            uninstall_fn: uninstall_opencode,
        },
        Platform {
            id: "hermes",
            description: "Hermes — skill + config.yaml MCP",
            kind: PlatformKind::User,
            install_fn: install_hermes,
            uninstall_fn: uninstall_hermes,
        },
        Platform {
            id: "agents",
            description: "Generic ~/.agents/skills/codebeacon",
            kind: PlatformKind::User,
            install_fn: install_agents,
            uninstall_fn: uninstall_agents,
        },
        Platform {
            id: "vscode",
            description: "VS Code — .vscode/mcp.json + copilot instructions",
            kind: PlatformKind::Project,
            install_fn: install_vscode,
            uninstall_fn: uninstall_vscode,
        },
    ]
}

fn home_dir() -> Result<PathBuf> {
    dirs_or_home()
}

fn dirs_or_home() -> Result<PathBuf> {
    if let Ok(h) = std::env::var("HOME") {
        return Ok(PathBuf::from(h));
    }
    anyhow::bail!("HOME not set")
}

fn write_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    println!("  wrote {}", path.display());
    Ok(())
}

fn merge_file_markdown(path: &Path, section: &str) -> Result<()> {
    let existing = fs::read_to_string(path).unwrap_or_default();
    let merged = merge_marked_section(&existing, section);
    write_file(path, &merged)
}

fn install_cursor(ctx: &InstallCtx) -> Result<()> {
    let root = if ctx.project {
        super::project_root()?
    } else {
        super::project_root()?
    };

    let rules_dir = root.join(".cursor/rules");
    fs::create_dir_all(&rules_dir)?;
    let rule_content = include_str!("../../assets/cursor/codebeacon.mdc");
    write_file(&rules_dir.join("codebeacon.mdc"), rule_content)?;

    let mcp_path = root.join(".cursor/mcp.json");
    let block = mcp_json_block(ctx.exe, ctx.security, ctx.fs_tools);
    let existing = fs::read_to_string(&mcp_path).unwrap_or_default();
    let merged = merge_mcp_json(&existing, &block)?;
    write_file(&mcp_path, &merged)?;

    write_hook_script(
        &root.join(".cursor/hooks/codebeacon-context.sh"),
        HOOK_CONTEXT_SH,
    )?;
    write_hook_script(
        &root.join(".cursor/hooks/codebeacon-security.sh"),
        HOOK_SECURITY_SH,
    )?;
    let dest = root.join(".cursor/hooks.json.example");
    write_file(&dest, CURSOR_HOOKS_EXAMPLE)?;
    println!("  hint: cp .cursor/hooks.json.example .cursor/hooks.json to enable hooks");
    Ok(())
}

fn uninstall_cursor(project: bool, _purge: bool) -> Result<()> {
    let root = super::project_root()?;
    if project {
        let _ = fs::remove_file(root.join(".cursor/rules/codebeacon.mdc"));
        if root.join(".cursor/mcp.json").exists() {
            let existing = fs::read_to_string(root.join(".cursor/mcp.json"))?;
            let merged = remove_mcp_json(&existing)?;
            write_file(&root.join(".cursor/mcp.json"), &merged)?;
        }
    }
    Ok(())
}

const CLAUDE_SECTION: &str = r#"## Codebeacon

Use Codebeacon MCP tools for code navigation — **not** grep/Read for exploration:

1. `get_context` — start here
2. `drill_package`, `find_definition`, `find_references`
3. `get_dependents` before risky edits
4. `query_context`, `shortest_path`, `hotspots` for graph queries
5. `init_workspace` if no index exists

Discovery hook reminds you when `.codeindex/` exists. Security: `verify_security` when enabled.
"#;

fn install_claude(ctx: &InstallCtx) -> Result<()> {
    let root = super::project_root()?;
    if ctx.project {
        merge_file_markdown(&root.join("CLAUDE.md"), CLAUDE_SECTION)?;
    }

    let claude_dir = home_dir()?.join(".claude");
    fs::create_dir_all(&claude_dir)?;

    write_hook_script(
        &claude_dir.join("hooks/codebeacon-context.sh"),
        HOOK_CONTEXT_SH,
    )?;

    let discovery_hint = claude_dir.join("hooks/claude-discovery-hook.json.example");
    write_file(&discovery_hint, CLAUDE_DISCOVERY_HOOK)?;
    println!(
        "  hint: merge {} into ~/.claude/settings.json PreToolUse hooks",
        discovery_hint.display()
    );

    let skill_dest = home_dir()?.join(".claude/skills/codebeacon");
    copy_skill(&skill_dest)?;
    Ok(())
}

fn uninstall_claude(project: bool, purge: bool) -> Result<()> {
    let root = super::project_root()?;
    if project {
        let path = root.join("CLAUDE.md");
        if path.exists() {
            let existing = fs::read_to_string(&path)?;
            write_file(&path, &remove_marked_section(&existing))?;
        }
    }
    if purge {
        let _ = fs::remove_dir_all(home_dir()?.join(".claude/skills/codebeacon"));
        let _ = fs::remove_file(home_dir()?.join(".claude/hooks/codebeacon-context.sh"));
    }
    Ok(())
}

const AGENTS_SECTION: &str = r#"## Codebeacon

Prefer Codebeacon MCP over grep: `get_context` first, then `drill_package` / `find_definition` / `find_references`.
Graph: `query_context`, `shortest_path`, `hotspots`. Impact: `get_dependents`. Security: `verify_security`.
"#;

fn install_codex(ctx: &InstallCtx) -> Result<()> {
    let root = super::project_root()?;
    let args = super::mcp_args(ctx.security, ctx.fs_tools);

    // User-level Codex MCP (primary): ~/.codex/config.toml
    // https://developers.openai.com/codex/mcp — key must be mcp_servers
    let codex_home = home_dir()?.join(".codex");
    fs::create_dir_all(&codex_home)?;
    let user_cfg = codex_home.join("config.toml");
    let existing = fs::read_to_string(&user_cfg).unwrap_or_default();
    write_file(
        &user_cfg,
        &super::markers::merge_codex_mcp_toml(&existing, ctx.exe, &args),
    )?;
    println!("  wrote MCP [mcp_servers.codebeacon] → {}", user_cfg.display());

    if ctx.project {
        merge_file_markdown(&root.join("AGENTS.md"), AGENTS_SECTION)?;

        let rel_hook = ".codex/hooks/codebeacon-context.sh";
        write_hook_script(&root.join(rel_hook), HOOK_CONTEXT_SH)?;
        let hooks_json = format!(
            r#"{{
  "hooks": {{
    "preToolUse": [
      {{
        "command": "{rel_hook}",
        "matcher": "Grep|Read|Glob"
      }}
    ]
  }}
}}"#
        );
        write_file(&root.join(".codex/hooks.json"), &hooks_json)?;

        // Project-scoped MCP (trusted projects only)
        let proj_codex = root.join(".codex");
        fs::create_dir_all(&proj_codex)?;
        let proj_cfg = proj_codex.join("config.toml");
        let existing = fs::read_to_string(&proj_cfg).unwrap_or_default();
        write_file(
            &proj_cfg,
            &super::markers::merge_codex_mcp_toml(&existing, ctx.exe, &args),
        )?;
        println!(
            "  wrote project MCP → {} (requires trusted project in Codex)",
            proj_cfg.display()
        );
    }

    let skill_dest = home_dir()?.join(".codex/skills/codebeacon");
    copy_skill(&skill_dest)?;
    Ok(())
}

fn uninstall_codex(project: bool, purge: bool) -> Result<()> {
    let root = super::project_root()?;
    let user_cfg = home_dir()?.join(".codex/config.toml");
    if user_cfg.exists() {
        let existing = fs::read_to_string(&user_cfg)?;
        write_file(
            &user_cfg,
            &super::markers::remove_codex_mcp_toml(&existing),
        )?;
    }
    if project {
        let path = root.join("AGENTS.md");
        if path.exists() {
            let existing = fs::read_to_string(&path)?;
            write_file(&path, &remove_marked_section(&existing))?;
        }
        let _ = fs::remove_file(root.join(".codex/hooks.json"));
        let proj_cfg = root.join(".codex/config.toml");
        if proj_cfg.exists() {
            let existing = fs::read_to_string(&proj_cfg)?;
            write_file(
                &proj_cfg,
                &super::markers::remove_codex_mcp_toml(&existing),
            )?;
        }
    }
    if purge {
        let _ = fs::remove_dir_all(home_dir()?.join(".codex/skills/codebeacon"));
    }
    Ok(())
}

fn install_opencode(ctx: &InstallCtx) -> Result<()> {
    let config_dir = home_dir()?.join(".config/opencode");
    fs::create_dir_all(&config_dir)?;

    let skill_dest = config_dir.join("skills/codebeacon");
    copy_skill(&skill_dest)?;

    let opencode_json = config_dir.join("opencode.json");
    let block = mcp_json_block(ctx.exe, ctx.security, ctx.fs_tools);
    let existing = fs::read_to_string(&opencode_json).unwrap_or_default();
    let merged = merge_mcp_json(&existing, &block)?;
    write_file(&opencode_json, &merged)?;
    Ok(())
}

fn uninstall_opencode(_project: bool, purge: bool) -> Result<()> {
    let config_dir = home_dir()?.join(".config/opencode");
    if config_dir.join("opencode.json").exists() {
        let existing = fs::read_to_string(config_dir.join("opencode.json"))?;
        write_file(&config_dir.join("opencode.json"), &remove_mcp_json(&existing)?)?;
    }
    if purge {
        let _ = fs::remove_dir_all(config_dir.join("skills/codebeacon"));
    }
    Ok(())
}

fn install_hermes(ctx: &InstallCtx) -> Result<()> {
    let hermes_dir = home_dir()?.join(".hermes");
    fs::create_dir_all(&hermes_dir)?;

    copy_skill(&hermes_dir.join("skills/codebeacon"))?;

    let config_path = hermes_dir.join("config.yaml");
    let mcp_section = format!(
        "\n# codebeacon-start\nmcp_servers:\n  codebeacon:\n    command: {}\n    args: {:?}\n# codebeacon-end\n",
        ctx.exe,
        super::mcp_args(ctx.security, ctx.fs_tools)
    );
    let existing = fs::read_to_string(&config_path).unwrap_or_default();
    if existing.contains("codebeacon-start") {
        let start = existing.find("# codebeacon-start").unwrap();
        let end = existing.find("# codebeacon-end").unwrap() + "# codebeacon-end".len();
        let mut out = existing[..start].to_string();
        out.push_str(&mcp_section);
        out.push_str(&existing[end..]);
        write_file(&config_path, &out)?;
    } else {
        write_file(&config_path, &(existing + &mcp_section))?;
    }
    Ok(())
}

fn uninstall_hermes(_project: bool, purge: bool) -> Result<()> {
    let config_path = home_dir()?.join(".hermes/config.yaml");
    if config_path.exists() {
        let existing = fs::read_to_string(&config_path)?;
        if let (Some(start), Some(end)) = (
            existing.find("# codebeacon-start"),
            existing.find("# codebeacon-end"),
        ) {
            let out = format!(
                "{}{}",
                &existing[..start],
                &existing[end + "# codebeacon-end".len()..]
            );
            write_file(&config_path, out.trim_end())?;
        }
    }
    if purge {
        let _ = fs::remove_dir_all(home_dir()?.join(".hermes/skills/codebeacon"));
    }
    Ok(())
}

fn install_agents(_ctx: &InstallCtx) -> Result<()> {
    let dest = home_dir()?.join(".agents/skills/codebeacon");
    copy_skill(&dest)
}

fn uninstall_agents(_project: bool, purge: bool) -> Result<()> {
    if purge {
        let _ = fs::remove_dir_all(home_dir()?.join(".agents/skills/codebeacon"));
    }
    Ok(())
}

fn install_vscode(ctx: &InstallCtx) -> Result<()> {
    let root = super::project_root()?;
    let vscode_dir = root.join(".vscode");
    fs::create_dir_all(&vscode_dir)?;

    let mcp_path = vscode_dir.join("mcp.json");
    let block = mcp_json_block(ctx.exe, ctx.security, ctx.fs_tools);
    let existing = fs::read_to_string(&mcp_path).unwrap_or_default();
    write_file(&mcp_path, &merge_mcp_json(&existing, &block)?)?;

    let copilot = root.join(".github/copilot-instructions.md");
    merge_file_markdown(&copilot, CLAUDE_SECTION)?;
    Ok(())
}

fn uninstall_vscode(project: bool, _purge: bool) -> Result<()> {
    if !project {
        return Ok(());
    }
    let root = super::project_root()?;
    if root.join(".vscode/mcp.json").exists() {
        let existing = fs::read_to_string(root.join(".vscode/mcp.json"))?;
        write_file(&root.join(".vscode/mcp.json"), &remove_mcp_json(&existing)?)?;
    }
    let copilot = root.join(".github/copilot-instructions.md");
    if copilot.exists() {
        let existing = fs::read_to_string(&copilot)?;
        write_file(&copilot, &remove_marked_section(&existing))?;
    }
    Ok(())
}
