mod loop_coord;
mod intelligence;
mod compact;
mod config;
mod config_file;
mod daemon;
mod export;
mod extract;
mod extractor;
mod graph;
mod hook;
mod imports;
mod indexer;
mod install;
mod lsp;
mod mcp;
mod query;
mod report;
mod security;
mod types;
mod verify_cmd;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "codebeacon", about = "Codebeacon — hierarchical code index for AI coding assistants")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build full index for current repo
    Init {
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Start daemon + MCP server
    Serve {
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long)]
        fs_tools: bool,
        #[arg(long)]
        security: bool,
    },
    /// Verify a code fragment against the security policy (CWE checks)
    Verify {
        #[arg(long)]
        content: String,
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Install Codebeacon skill, rules, and MCP config for AI platforms
    Install {
        #[arg(long)]
        platform: Option<String>,
        #[arg(long)]
        project: bool,
        #[arg(long)]
        security: bool,
        #[arg(long)]
        fs_tools: bool,
        #[arg(long)]
        list: bool,
    },
    /// Remove Codebeacon integration files
    Uninstall {
        #[arg(long)]
        platform: Option<String>,
        #[arg(long)]
        project: bool,
        #[arg(long)]
        purge: bool,
    },
    /// Generate CODEBEACON_REPORT.md
    Report {
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Search packages, symbols, and files by keywords
    Query {
        question: String,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long)]
        compact: Option<bool>,
    },
    /// Shortest dependency path between two files/symbols/packages
    Path {
        from: String,
        to: String,
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Explain a symbol, package, or file
    Explain {
        name: String,
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// List files that depend on the given file
    Dependents {
        file: String,
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Subgraph around a file for edit-time context
    Focus {
        file: String,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long)]
        radius: Option<u32>,
        #[arg(long)]
        compact: Option<bool>,
    },
    /// Index freshness vs working tree
    Status {
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Blast radius before changing a symbol
    Impact {
        symbol: String,
        #[arg(long)]
        file: Option<String>,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long)]
        exact: Option<bool>,
        #[arg(long)]
        compact: Option<bool>,
    },
    /// Public exports for a package
    Api {
        package: String,
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Git history and dependency context for a file
    Why {
        file: String,
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Loop context coordinator for iterative agent work
    Loop {
        #[command(subcommand)]
        command: LoopCommands,
    },
    /// Export dependency graph
    Export {
        #[command(subcommand)]
        command: ExportCommands,
    },
    /// Install or remove git post-commit re-index hook
    Hook {
        #[command(subcommand)]
        command: HookCommands,
    },
}

#[derive(Subcommand)]
enum LoopCommands {
    /// Start a loop session (optionally runs first tick)
    Begin {
        goal: String,
        #[arg(long)]
        file: Option<String>,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long)]
        no_tick: bool,
        #[arg(long)]
        compact: Option<bool>,
    },
    /// Next loop iteration context bundle
    Tick {
        #[arg(long)]
        session: String,
        #[arg(long)]
        file: Option<String>,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long)]
        compact: Option<bool>,
    },
    /// Record files touched after an edit
    Record {
        #[arg(long)]
        session: String,
        #[arg(long, required = true)]
        files: Vec<String>,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Close loop session
    End {
        #[arg(long)]
        session: String,
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Emit AGENT_LOOP_TICK_codebeacon sentinel on interval (Cursor /loop integration)
    Watch {
        #[arg(long)]
        session: String,
        #[arg(long, default_value = "5m")]
        interval: String,
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// begin + first tick + watch (one-shot setup)
    Run {
        goal: String,
        #[arg(long)]
        file: Option<String>,
        #[arg(long)]
        interval: Option<String>,
        #[arg(long)]
        root: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ExportCommands {
    /// Export dependency graph as Mermaid diagram
    Mermaid {
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        package: Option<String>,
    },
}

#[derive(Subcommand)]
enum HookCommands {
    /// Install git post-commit hook
    Install {
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Remove Codebeacon section from post-commit hook
    Uninstall {
        #[arg(long)]
        root: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    if std::env::args().any(|a| a == "--version" || a == "-V") {
        println!("codebeacon {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { root } => {
            let repos = resolve_roots(root)?;
            for repo in &repos {
                tracing::info!("Indexing repo: {}", repo.display());
                let mut indexer = indexer::Indexer::new(repo);
                indexer.full_index()?;
                println!("Index written to {}/.codeindex/", repo.display());
            }
        }
        Commands::Serve { root, fs_tools, security } => {
            mcp::run_stdio_server(root, fs_tools, security)?;
        }
        Commands::Verify {
            content,
            path,
            json,
            root,
        } => {
            let out = verify_cmd::run_verify(&content, path.as_deref(), root)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                verify_cmd::print_human_output(&out);
            }
            let code = verify_cmd::exit_code_for_action(out.action);
            if code != 0 {
                std::process::exit(code);
            }
        }
        Commands::Install {
            platform,
            project,
            security,
            fs_tools,
            list,
        } => {
            install::run_install(&install::InstallOptions {
                platform,
                project,
                security,
                fs_tools,
                list_only: list,
            })?;
        }
        Commands::Uninstall {
            platform,
            project,
            purge,
        } => {
            install::run_uninstall(&install::UninstallOptions {
                platform,
                project,
                purge,
            })?;
        }
        Commands::Report { root, output } => {
            let opts = report::ReportOptions::resolve(root, output)?;
            let md = report::generate(&opts)?;
            println!("Report written to {}", opts.output.display());
            if std::env::var("CODEBEACON_REPORT_STDOUT").is_ok() {
                println!("{md}");
            }
        }
        Commands::Query { question, root, compact } => {
            let repo = resolve_single_root(root)?;
            let cfg = config_file::load(&repo)?;
            let ctx = query::RepoQueryCtx::load(&repo)?;
            let use_compact = compact.unwrap_or(cfg.compact.enabled);
            if use_compact {
                let mut session = crate::compact::session_for_repo(&config::codeindex_dir(&repo));
                let matches = ctx.query(&question, 10);
                let compact_matches = crate::compact::encode_query_matches(&matches, &mut session);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "question": question,
                        "matches": compact_matches,
                    }))?
                );
            } else {
                print!("{}", ctx.format_query(&question, 10));
            }
        }
        Commands::Path { from, to, root } => {
            let repo = resolve_single_root(root)?;
            let ctx = query::RepoQueryCtx::load(&repo)?;
            println!("{}", ctx.path_between(&from, &to)?);
        }
        Commands::Explain { name, root } => {
            let repo = resolve_single_root(root)?;
            let ctx = query::RepoQueryCtx::load(&repo)?;
            print!("{}", ctx.explain(&name)?);
        }
        Commands::Dependents { file, root } => {
            let repo = resolve_single_root(root)?;
            let ctx = query::RepoQueryCtx::load(&repo)?;
            print!("{}", ctx.dependents_of(&file)?);
        }
        Commands::Focus {
            file,
            root,
            radius,
            compact,
        } => {
            let repo = resolve_single_root(root)?;
            let cfg = config_file::load(&repo)?;
            let qctx = query::RepoQueryCtx::load(&repo)?;
            let abs = repo.join(&file);
            let rel = intelligence::resolve_rel_path(&repo, &abs);
            let r = radius.unwrap_or(cfg.intelligence.focus_default_radius);
            let out = intelligence::focus_context(&qctx, &rel, r, &cfg.intelligence)?;
            let use_compact = compact.unwrap_or(cfg.compact.enabled);
            if use_compact {
                let mut session = crate::compact::session_for_repo(&config::codeindex_dir(&repo));
                let compact_out = crate::compact::encode_focus_response(&out, &mut session);
                println!("{}", serde_json::to_string_pretty(&compact_out)?);
            } else {
                println!("{}", serde_json::to_string_pretty(&out)?);
            }
        }
        Commands::Status { root } => {
            let repo = resolve_single_root(root)?;
            let cfg = config_file::load(&repo)?;
            let out = intelligence::index_status(&repo, &cfg.intelligence)?;
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Commands::Impact {
            symbol,
            file,
            root,
            exact,
            compact,
        } => {
            let repo = resolve_single_root(root)?;
            let cfg = config_file::load(&repo)?;
            let qctx = query::RepoQueryCtx::load(&repo)?;
            let file_rel = file.as_deref();
            let out = intelligence::change_impact(
                &qctx,
                &symbol,
                file_rel,
                exact.unwrap_or(true),
                &cfg.intelligence,
            )?;
            let use_compact = compact.unwrap_or(cfg.compact.enabled);
            if use_compact {
                let mut session = crate::compact::session_for_repo(&config::codeindex_dir(&repo));
                let compact_out = crate::compact::encode_change_impact(&out, &mut session);
                println!("{}", serde_json::to_string_pretty(&compact_out)?);
            } else {
                println!("{}", serde_json::to_string_pretty(&out)?);
            }
        }
        Commands::Api { package, root } => {
            let repo = resolve_single_root(root)?;
            let qctx = query::RepoQueryCtx::load(&repo)?;
            let pkg = qctx
                .packages
                .get(&package)
                .with_context(|| format!("package '{package}' not found"))?;
            let out = intelligence::api_surface(pkg);
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Commands::Why { file, root } => {
            let repo = resolve_single_root(root)?;
            let cfg = config_file::load(&repo)?;
            let qctx = query::RepoQueryCtx::load(&repo)?;
            let abs = repo.join(&file);
            let rel = intelligence::resolve_rel_path(&repo, &abs);
            let (recent_commits, blame_first_line) = if cfg.intelligence.git_context_enabled {
                (
                    intelligence::git::git_log_file(&repo, &rel, 3),
                    intelligence::git::git_blame_first_line(&repo, &rel),
                )
            } else {
                (vec![], None)
            };
            let out = intelligence::why_file(&qctx, &rel, recent_commits, blame_first_line);
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Commands::Loop { command } => run_loop_command(command)?,
        Commands::Export { command } => match command {
            ExportCommands::Mermaid {
                root,
                output,
                package,
            } => {
                let opts = export::mermaid::MermaidOptions::resolve(root, output, package)?;
                export::mermaid::export_mermaid(&opts)?;
                println!("Mermaid diagram written to {}", opts.output.display());
            }
        },
        Commands::Hook { command } => match command {
            HookCommands::Install { root } => {
                let opts = hook::HookOptions::resolve(root)?;
                hook::install(&opts)?;
            }
            HookCommands::Uninstall { root } => {
                let opts = hook::HookOptions::resolve(root)?;
                hook::uninstall(&opts)?;
            }
        },
    }

    Ok(())
}

fn resolve_roots(override_path: Option<PathBuf>) -> Result<Vec<PathBuf>> {
    if let Some(p) = override_path {
        if !p.is_dir() {
            anyhow::bail!("--root '{}' is not a directory", p.display());
        }
        return Ok(vec![p]);
    }
    let start = if let Ok(env_root) = std::env::var("CLAUDE_PROJECT_DIR") {
        PathBuf::from(env_root)
    } else {
        std::env::current_dir()?
    };
    let repos = config::discover_repos(&start);
    if repos.is_empty() {
        anyhow::bail!(
            "Could not find any git repo at or under '{}'.\n\
             Make sure the directory (or one of its children) contains a .git folder,\n\
             or use --root to point at a git repo / workspace directory.",
            start.display()
        );
    }
    Ok(repos)
}

fn resolve_single_root(override_path: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = override_path {
        return Ok(p);
    }
    let repos = resolve_roots(None)?;
    Ok(repos[0].clone())
}

fn run_loop_command(command: LoopCommands) -> Result<()> {
    match command {
        LoopCommands::Begin {
            goal,
            file,
            root,
            no_tick,
            compact,
        } => {
            let repo = resolve_single_root(root)?;
            let cfg = config_file::load(&repo)?;
            let qctx = query::RepoQueryCtx::load(&repo)?;
            let codeindex = config::codeindex_dir(&repo);
            let active = loop_coord::resolve_active_files(&repo, file.as_deref(), None);
            let (_session, resp) = loop_coord::loop_begin_with_tick(
                &repo,
                &codeindex,
                &goal,
                active,
                &cfg.loop_config,
                &cfg.intelligence,
                &qctx,
                !no_tick,
            )?;
            let use_compact = compact.unwrap_or(cfg.compact.enabled);
            if use_compact {
                if let Some(ref tick) = resp.tick {
                    let mut session = crate::compact::session_for_repo(&codeindex);
                    let c = crate::compact::encode_loop_tick(tick, &mut session);
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "session_id": resp.session_id,
                            "goal": resp.goal,
                            "tick": c,
                        }))?
                    );
                } else {
                    println!("{}", serde_json::to_string_pretty(&resp)?);
                }
            } else {
                println!("{}", serde_json::to_string_pretty(&resp)?);
            }
        }
        LoopCommands::Tick {
            session,
            file,
            root,
            compact,
        } => {
            let repo = resolve_single_root(root)?;
            let cfg = config_file::load(&repo)?;
            let qctx = query::RepoQueryCtx::load(&repo)?;
            let codeindex = config::codeindex_dir(&repo);
            let mut loop_session = loop_coord::read_session(&codeindex, &session)?;
            let file_rel = file.as_deref();
            let out = loop_coord::loop_tick(
                &repo,
                &codeindex,
                &mut loop_session,
                &cfg.loop_config,
                &cfg.intelligence,
                &qctx,
                file_rel,
            )?;
            loop_coord::write_session(&codeindex, &loop_session)?;
            let use_compact = compact.unwrap_or(cfg.compact.enabled);
            if use_compact {
                let mut dict = crate::compact::session_for_repo(&codeindex);
                let c = crate::compact::encode_loop_tick(&out, &mut dict);
                println!("{}", serde_json::to_string_pretty(&c)?);
            } else {
                println!("{}", serde_json::to_string_pretty(&out)?);
            }
        }
        LoopCommands::Record {
            session,
            files,
            symbol,
            root,
        } => {
            let repo = resolve_single_root(root)?;
            let cfg = config_file::load(&repo)?;
            let qctx = query::RepoQueryCtx::load(&repo)?;
            let codeindex = config::codeindex_dir(&repo);
            let mut loop_session = loop_coord::read_session(&codeindex, &session)?;
            let out = loop_coord::loop_record(
                &repo,
                &codeindex,
                &mut loop_session,
                &cfg.loop_config,
                &cfg.intelligence,
                &qctx,
                &files,
                symbol.as_deref(),
            )?;
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        LoopCommands::End { session, root } => {
            let repo = resolve_single_root(root)?;
            let cfg = config_file::load(&repo)?;
            let codeindex = config::codeindex_dir(&repo);
            let mut loop_session = loop_coord::read_session(&codeindex, &session)?;
            let out = loop_coord::loop_end(
                &repo,
                &codeindex,
                &mut loop_session,
                &cfg.loop_config,
                &cfg.intelligence,
            )?;
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        LoopCommands::Watch {
            session,
            interval,
            root,
        } => {
            let repo = resolve_single_root(root)?;
            let secs = parse_interval_secs(&interval)?;
            let exe = std::env::current_exe()?;
            let root_s = repo.display();
            let prompt = serde_json::json!({
                "session_id": session,
                "prompt": format!("Call codebeacon loop tick --session {session} and continue the task."),
            });
            let prompt_escaped = prompt.to_string().replace('\'', "'\\''");
            println!(
                "Watching session {session} every {interval} ({secs}s). Press Ctrl+C to stop."
            );
            let script = format!(
                "while true; do sleep {secs}; echo 'AGENT_LOOP_TICK_codebeacon {prompt_escaped}'; done"
            );
            let status = std::process::Command::new("sh")
                .arg("-c")
                .arg(&script)
                .env("CODEBEACON_LOOP_ROOT", root_s.to_string())
                .status()
                .context("failed to run watch loop")?;
            if !status.success() {
                anyhow::bail!("watch loop exited with {}", status);
            }
        }
        LoopCommands::Run {
            goal,
            file,
            interval,
            root,
        } => {
            let repo = resolve_single_root(root.clone())?;
            let cfg = config_file::load(&repo)?;
            let qctx = query::RepoQueryCtx::load(&repo)?;
            let codeindex = config::codeindex_dir(&repo);
            let active = loop_coord::resolve_active_files(&repo, file.as_deref(), None);
            let (_session, resp) = loop_coord::loop_begin_with_tick(
                &repo,
                &codeindex,
                &goal,
                active,
                &cfg.loop_config,
                &cfg.intelligence,
                &qctx,
                true,
            )?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
            if let Some(iv) = interval {
                run_loop_command(LoopCommands::Watch {
                    session: resp.session_id,
                    interval: iv,
                    root,
                })?;
            }
        }
    }
    Ok(())
}

fn parse_interval_secs(s: &str) -> Result<u64> {
    let s = s.trim().to_lowercase();
    if let Some(num) = s.strip_suffix('s') {
        return Ok(num.parse().context("invalid seconds")?);
    }
    if let Some(num) = s.strip_suffix('m') {
        return Ok(num.parse::<u64>().context("invalid minutes")? * 60);
    }
    if let Some(num) = s.strip_suffix('h') {
        return Ok(num.parse::<u64>().context("invalid hours")? * 3600);
    }
    if let Some(num) = s.strip_suffix('d') {
        return Ok(num.parse::<u64>().context("invalid days")? * 86400);
    }
    s.parse::<u64>().context("interval must be like 30s, 5m, 2h")
}

#[cfg(test)]
mod tests {
    #[test]
    fn cli_has_expected_subcommands() {
        use clap::CommandFactory;
        let cmd = super::Cli::command();
        for name in [
            "init", "serve", "verify", "install", "report", "query", "path",
            "explain", "dependents", "focus", "status", "impact", "api", "why", "loop",
            "export", "hook",
        ] {
            assert!(
                cmd.get_subcommands().any(|s| s.get_name() == name),
                "missing subcommand: {name}"
            );
        }
    }
}
