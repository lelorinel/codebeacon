mod config;
mod config_file;
mod daemon;
mod extractor;
mod graph;
mod imports;
mod indexer;
mod lsp;
mod mcp;
mod types;

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
        /// Repo root (default: auto-detected from .git)
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Start daemon + MCP server
    Serve {
        /// Repo root (default: auto-detected from .git)
        #[arg(long)]
        root: Option<PathBuf>,
        /// Enable file-system tools (read_file, write_file, edit_file, list_directory).
        /// Useful for local AI environments (e.g. LM Studio) that lack native file tools.
        #[arg(long)]
        fs_tools: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Handle --version manually (clap doesn't auto-add it with subcommands)
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
        Commands::Serve { root, fs_tools } => {
            // Root discovery, daemon spawning, and roots/list negotiation all
            // happen inside run_stdio_server (after the MCP handshake).
            mcp::run_stdio_server(root, fs_tools)?;
        }
    }

    Ok(())
}

fn resolve_roots(override_path: Option<PathBuf>) -> Result<Vec<PathBuf>> {
    // Priority: --root flag > CLAUDE_PROJECT_DIR env var > cwd
    let start = if let Some(p) = override_path {
        p
    } else if let Ok(env_root) = std::env::var("CLAUDE_PROJECT_DIR") {
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

#[cfg(test)]
mod tests {
    #[test]
    fn cli_has_expected_subcommands() {
        use clap::CommandFactory;
        let cmd = super::Cli::command();
        assert!(cmd.get_subcommands().any(|s| s.get_name() == "init"));
        assert!(cmd.get_subcommands().any(|s| s.get_name() == "serve"));
    }
}
