mod config;
mod daemon;
mod graph;
mod indexer;
mod lsp;
mod mcp;
mod types;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "lcp", about = "Language Context Protocol — hierarchical code index for AI")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build full index for current repo
    Init {
        /// Repo root (default: auto-detected from .git)
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Start daemon + MCP server
    Serve {
        /// Repo root (default: auto-detected from .git)
        #[arg(long)]
        root: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { root } => {
            let root = resolve_root(root)?;
            let root_uri = format!("file://{}", root.display());
            let mut pool = lsp::pool::LspPool::new(&root_uri);
            let mut indexer = indexer::Indexer::new(&root);
            indexer.full_index(&mut pool)?;
            println!("Index written to {}/.codeindex/", root.display());
        }
        Commands::Serve { root } => {
            let root = resolve_root(root)?;
            let root_clone = root.clone();
            tokio::spawn(async move {
                if let Err(e) = daemon::start(root_clone).await {
                    tracing::error!("Daemon error: {e}");
                }
            });
            mcp::run_stdio_server(root)?;
        }
    }

    Ok(())
}

fn resolve_root(override_path: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = override_path { return Ok(p); }
    let cwd = std::env::current_dir()?;
    config::find_repo_root(&cwd)
        .ok_or_else(|| anyhow::anyhow!("Could not find repo root (no .git directory found). Use --root to specify."))
}

#[cfg(test)]
mod tests {
    #[test]
    fn cli_help_exits_cleanly() {
        use clap::CommandFactory;
        let cmd = super::Cli::command();
        assert!(cmd.get_subcommands().any(|s| s.get_name() == "init"));
        assert!(cmd.get_subcommands().any(|s| s.get_name() == "serve"));
    }
}
