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

use anyhow::Result;
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

#[cfg(test)]
mod tests {
    #[test]
    fn cli_has_expected_subcommands() {
        use clap::CommandFactory;
        let cmd = super::Cli::command();
        for name in [
            "init", "serve", "verify", "install", "report", "query", "path",
            "explain", "dependents", "export", "hook",
        ] {
            assert!(
                cmd.get_subcommands().any(|s| s.get_name() == name),
                "missing subcommand: {name}"
            );
        }
    }
}
