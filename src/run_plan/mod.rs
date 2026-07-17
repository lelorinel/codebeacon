//! `codebeacon run-plan` — parallel agents over plan markdown docs.

mod brief;
mod discover;
mod spawn;

use crate::locks::{reset_stable_locks, SharedLockStore};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub use discover::{discover_plans, PlanDoc};
pub use spawn::{resolve_agent_bin, RunPlanProvider};

#[derive(Debug, Clone)]
pub struct RunPlanOpts {
    pub plans_dir: PathBuf,
    pub prompt: String,
    pub workspace: PathBuf,
    pub parallel: usize,
    pub model: String,
    pub provider: RunPlanProvider,
    pub dry_run: bool,
    pub ttl_secs: u64,
}

pub fn run(opts: RunPlanOpts) -> Result<()> {
    let plans = discover_plans(&opts.plans_dir)?;
    if plans.is_empty() {
        bail!(
            "no *.md plans found in {}",
            opts.plans_dir.display()
        );
    }

    let run_id = format!(
        "{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );
    let run_dir = crate::config::codeindex_dir(&opts.workspace)
        .join("run-plan")
        .join(&run_id);
    std::fs::create_dir_all(&run_dir)
        .with_context(|| format!("create run dir {}", run_dir.display()))?;

    let locks_path = reset_stable_locks(&opts.workspace).map_err(|e| anyhow::anyhow!(e))?;
    let store = SharedLockStore::open(locks_path, opts.ttl_secs, vec![]);

    for plan in &plans {
        store
            .register_session(&plan.block_key, None)
            .map_err(|e| anyhow::anyhow!(e))?;
    }

    let briefs: Vec<(PlanDoc, PathBuf, PathBuf)> = plans
        .into_iter()
        .map(|plan| {
            let brief_path = run_dir.join(format!("{}.md", plan.block_key));
            let signal_path = run_dir.join("signals").join(format!("DONE.{}", plan.block_key));
            brief::write_brief(
                &brief_path,
                &plan,
                &opts.prompt,
                &plan.block_key,
                &signal_path,
            )?;
            Ok((plan, brief_path, signal_path))
        })
        .collect::<Result<Vec<_>>>()?;

    println!(
        "[codebeacon] run-plan: {} plan(s), provider={}, parallel={}, dry_run={}, run={}",
        briefs.len(),
        opts.provider.as_str(),
        if opts.parallel == 0 {
            "all".to_string()
        } else {
            opts.parallel.to_string()
        },
        opts.dry_run,
        run_id
    );

    let mcp_config = if opts.provider == RunPlanProvider::Claude {
        let cfg = run_dir.join("mcp.json");
        spawn::write_claude_mcp_config(&cfg, &opts.workspace)?;
        Some(cfg)
    } else {
        None
    };

    let wave = if opts.parallel == 0 {
        briefs.len().max(1)
    } else {
        opts.parallel.max(1)
    };

    for chunk in briefs.chunks(wave) {
        spawn::run_wave(spawn::SpawnWaveOpts {
            chunk,
            workspace: &opts.workspace,
            model: &opts.model,
            provider: opts.provider,
            dry_run: opts.dry_run,
            store: &store,
            mcp_config: mcp_config.as_deref(),
        })?;
    }

    let sessions = store.list_sessions().map_err(|e| anyhow::anyhow!(e))?;
    let mut ok_n = 0usize;
    let mut fail_n = 0usize;
    for s in &sessions {
        match s.status {
            crate::locks::SessionStatus::Done => ok_n += 1,
            crate::locks::SessionStatus::Failed | crate::locks::SessionStatus::TimedOut => {
                fail_n += 1
            }
            _ => {}
        }
        println!(
            "  {} → {:?} {}",
            s.block_key,
            s.status,
            if s.summary.is_empty() {
                String::new()
            } else {
                format!("— {}", s.summary)
            }
        );
    }
    println!("[codebeacon] run-plan done: {ok_n} ok, {fail_n} failed/timed_out");
    if fail_n > 0 {
        bail!("run-plan finished with failures");
    }
    Ok(())
}

/// Resolve workspace root (explicit or cwd).
pub fn resolve_workspace(root: Option<&Path>) -> PathBuf {
    root.map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}
