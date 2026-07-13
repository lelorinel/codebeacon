use crate::config_file::LoopConfig;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct LoopSignals {
    pub stale_count: usize,
    pub reindex_recommended: bool,
    pub reindexed: bool,
    pub should_pause: bool,
    pub should_stop: bool,
    pub hints: Vec<String>,
}

pub fn compute_signals(
    cfg: &LoopConfig,
    iteration: u32,
    stale_count: usize,
    reindexed: bool,
    reindex_recommended: bool,
) -> LoopSignals {
    let should_pause = stale_count as u32 >= cfg.stale_warn_threshold && !reindexed;
    let should_stop = iteration >= cfg.max_iterations;
    let mut hints = Vec::new();
    if reindex_recommended && !reindexed {
        hints.push("index is stale — consider re-index or call loop_tick with reindex policy".into());
    }
    if should_pause {
        hints.push(format!(
            "{} stale file(s) exceed threshold ({})",
            stale_count, cfg.stale_warn_threshold
        ));
    }
    if should_stop {
        hints.push(format!(
            "max_iterations ({}) reached — call loop_end",
            cfg.max_iterations
        ));
    }
    LoopSignals {
        stale_count,
        reindex_recommended,
        reindexed,
        should_pause,
        should_stop,
        hints,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cfg() -> LoopConfig {
        LoopConfig {
            stale_warn_threshold: 5,
            max_iterations: 10,
            ..LoopConfig::default()
        }
    }

    #[test]
    fn should_pause_when_stale_exceeds_threshold() {
        let sig = compute_signals(&test_cfg(), 1, 6, false, true);
        assert!(sig.should_pause);
    }

    #[test]
    fn should_stop_at_max_iterations() {
        let sig = compute_signals(&test_cfg(), 10, 0, false, false);
        assert!(sig.should_stop);
    }

    #[test]
    fn no_pause_after_reindex() {
        let sig = compute_signals(&test_cfg(), 1, 6, true, false);
        assert!(!sig.should_pause);
    }
}
