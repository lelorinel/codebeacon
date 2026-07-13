use crate::config_file::{IntelligenceConfig, LoopConfig, ReindexPolicy};
use crate::indexer::Indexer;
use crate::intelligence::{
    focus_context, index_status, resolve_rel_path, task_context, ChangeImpactResponse,
    FocusResponse, IndexStatusResponse, TaskContextResponse,
};
use crate::loop_coord::artifact::{write_iteration, IterationArtifact};
use crate::loop_coord::session::LoopSession;
use crate::loop_coord::signals::{compute_signals, LoopSignals};
use crate::query::RepoQueryCtx;
use anyhow::Result;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct LoopTickBundle {
    pub session_id: String,
    pub iteration: u32,
    pub goal: String,
    pub status: IndexStatusResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus: Option<FocusResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskContextResponse>,
    pub signals: LoopSignals,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impact: Option<ChangeImpactResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoopBeginResponse {
    pub session_id: String,
    pub goal: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tick: Option<LoopTickBundle>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoopEndResponse {
    pub session_id: String,
    pub goal: String,
    pub iterations: u32,
    pub touched_files: Vec<String>,
    pub stale_warning: Option<String>,
}

pub fn should_reindex(cfg: &LoopConfig, iteration: u32, stale_count: usize) -> bool {
    match cfg.reindex {
        ReindexPolicy::Never => false,
        ReindexPolicy::IfStale => stale_count > 0,
        ReindexPolicy::EveryN => {
            cfg.reindex_every_n > 0 && iteration % cfg.reindex_every_n == 0
        }
        ReindexPolicy::Always => true,
    }
}

pub fn run_catchup(repo_root: &Path) -> Result<bool> {
    let mut indexer = Indexer::new(repo_root);
    indexer.catchup_index()?;
    Ok(true)
}

pub fn loop_tick(
    repo_root: &Path,
    codeindex_dir: &Path,
    session: &mut LoopSession,
    loop_cfg: &LoopConfig,
    intel_cfg: &IntelligenceConfig,
    qctx: &RepoQueryCtx,
    file_override: Option<&str>,
) -> Result<LoopTickBundle> {
    session.ensure_open()?;
    session.bump_iteration();

    let mut status = index_status(repo_root, intel_cfg)?;
    let stale_before = status.stale_files.len();
    let reindex_recommended = stale_before > 0
        || matches!(loop_cfg.reindex, ReindexPolicy::Always | ReindexPolicy::EveryN);

    let mut reindexed = false;
    if should_reindex(loop_cfg, session.iteration, stale_before) {
        reindexed = run_catchup(repo_root)?;
        if reindexed {
            status = index_status(repo_root, intel_cfg)?;
        }
    }

    let focus_file = file_override
        .map(str::to_string)
        .or_else(|| session.primary_file().map(str::to_string));

    let focus = if loop_cfg.wants_prefetch("focus_context") {
        focus_file
            .as_ref()
            .map(|f| {
                focus_context(
                    qctx,
                    f,
                    loop_cfg.focus_radius(intel_cfg),
                    intel_cfg,
                )
            })
            .transpose()?
    } else {
        None
    };

    let task = if loop_cfg.wants_prefetch("task_context")
        && (session.iteration == 1 || loop_cfg.prefetch_on_tick.iter().any(|p| p == "task_context"))
    {
        Some(task_context(
            qctx,
            &session.goal,
            focus_file.as_deref(),
            10,
            intel_cfg,
        ))
    } else {
        None
    };

    let signals = compute_signals(
        loop_cfg,
        session.iteration,
        status.stale_files.len(),
        reindexed,
        reindex_recommended,
    );

    if loop_cfg.persist_sessions {
        let artifact = IterationArtifact {
            iteration: session.iteration,
            recorded_at: chrono::Utc::now().to_rfc3339(),
            stale_count: status.stale_files.len(),
            reindexed,
            recorded_files: None,
            symbol: None,
        };
        write_iteration(codeindex_dir, &session.id, &artifact)?;
    }

    Ok(LoopTickBundle {
        session_id: session.id.clone(),
        iteration: session.iteration,
        goal: session.goal.clone(),
        status,
        focus,
        task,
        signals,
        impact: None,
    })
}

pub fn loop_begin_with_tick(
    repo_root: &Path,
    codeindex_dir: &Path,
    goal: &str,
    active_files: Vec<String>,
    loop_cfg: &LoopConfig,
    intel_cfg: &IntelligenceConfig,
    qctx: &RepoQueryCtx,
    include_tick: bool,
) -> Result<(LoopSession, LoopBeginResponse)> {
    let mut session = crate::loop_coord::session::begin_session(goal, active_files);
    if loop_cfg.persist_sessions {
        crate::loop_coord::artifact::write_session(codeindex_dir, &session)?;
    }
    let tick = if include_tick {
        Some(loop_tick(
            repo_root,
            codeindex_dir,
            &mut session,
            loop_cfg,
            intel_cfg,
            qctx,
            None,
        )?)
    } else {
        None
    };
    if loop_cfg.persist_sessions {
        crate::loop_coord::artifact::write_session(codeindex_dir, &session)?;
    }
    let resp = LoopBeginResponse {
        session_id: session.id.clone(),
        goal: session.goal.clone(),
        tick,
    };
    Ok((session, resp))
}

pub fn loop_record(
    repo_root: &Path,
    codeindex_dir: &Path,
    session: &mut LoopSession,
    loop_cfg: &LoopConfig,
    intel_cfg: &IntelligenceConfig,
    qctx: &RepoQueryCtx,
    files: &[String],
    symbol: Option<&str>,
) -> Result<LoopRecordResponse> {
    session.ensure_open()?;
    session.record_files(files);

    let impact = symbol.map(|sym| {
        crate::intelligence::change_impact(qctx, sym, files.first().map(String::as_str), true, intel_cfg)
    }).transpose()?;

    if loop_cfg.persist_sessions {
        let artifact = IterationArtifact {
            iteration: session.iteration,
            recorded_at: chrono::Utc::now().to_rfc3339(),
            stale_count: 0,
            reindexed: false,
            recorded_files: Some(files.to_vec()),
            symbol: symbol.map(str::to_string),
        };
        write_iteration(codeindex_dir, &session.id, &artifact)?;
        crate::loop_coord::artifact::write_session(codeindex_dir, session)?;
    }

    let _ = repo_root;
    Ok(LoopRecordResponse {
        session_id: session.id.clone(),
        recorded_files: files.to_vec(),
        impact,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct LoopRecordResponse {
    pub session_id: String,
    pub recorded_files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impact: Option<ChangeImpactResponse>,
}

pub fn loop_end(
    repo_root: &Path,
    codeindex_dir: &Path,
    session: &mut LoopSession,
    loop_cfg: &LoopConfig,
    intel_cfg: &IntelligenceConfig,
) -> Result<LoopEndResponse> {
    session.ensure_open()?;
    session.close();
    let status = index_status(repo_root, intel_cfg)?;
    let stale_warning = status.stale_warning.clone();
    if loop_cfg.persist_sessions {
        crate::loop_coord::artifact::write_session(codeindex_dir, session)?;
    }
    Ok(LoopEndResponse {
        session_id: session.id.clone(),
        goal: session.goal.clone(),
        iterations: session.iteration,
        touched_files: session.touched_files.clone(),
        stale_warning,
    })
}

pub fn resolve_active_files(
    repo_root: &Path,
    file: Option<&str>,
    files: Option<Vec<String>>,
) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(f) = file {
        let abs = if std::path::Path::new(f).is_absolute() {
            std::path::PathBuf::from(f)
        } else {
            repo_root.join(f)
        };
        out.push(resolve_rel_path(repo_root, &abs));
    }
    if let Some(list) = files {
        for f in list {
            let abs = if std::path::Path::new(&f).is_absolute() {
                std::path::PathBuf::from(&f)
            } else {
                repo_root.join(&f)
            };
            let rel = resolve_rel_path(repo_root, &abs);
            if !out.contains(&rel) {
                out.push(rel);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reindex_policy_never() {
        let cfg = LoopConfig {
            reindex: ReindexPolicy::Never,
            ..LoopConfig::default()
        };
        assert!(!should_reindex(&cfg, 1, 3));
    }

    #[test]
    fn reindex_policy_if_stale() {
        let cfg = LoopConfig {
            reindex: ReindexPolicy::IfStale,
            ..LoopConfig::default()
        };
        assert!(should_reindex(&cfg, 1, 2));
        assert!(!should_reindex(&cfg, 1, 0));
    }

    #[test]
    fn reindex_policy_every_n() {
        let cfg = LoopConfig {
            reindex: ReindexPolicy::EveryN,
            reindex_every_n: 3,
            ..LoopConfig::default()
        };
        assert!(should_reindex(&cfg, 3, 0));
        assert!(!should_reindex(&cfg, 2, 0));
    }

    #[test]
    fn reindex_policy_always() {
        let cfg = LoopConfig {
            reindex: ReindexPolicy::Always,
            ..LoopConfig::default()
        };
        assert!(should_reindex(&cfg, 1, 0));
    }
}
