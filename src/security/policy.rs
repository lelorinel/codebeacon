use crate::security::cwe::{default_enabled_cwes, normalize_cwe_id};
use crate::security::findings::{ProofStatus, SecurityFinding, VerifyReport};
use std::collections::HashSet;

/// How aggressively the gate treats inconclusive results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PolicyMode {
    /// Block proven exploits and unknowns; warn on pattern-only.
    Strict,
    /// Block proven exploits; warn on unknown/pattern-only but allow write.
    #[default]
    Balanced,
    /// Block only Z3-proven exploits; pass everything else.
    Permissive,
}

impl PolicyMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "strict" => Some(Self::Strict),
            "balanced" => Some(Self::Balanced),
            "permissive" => Some(Self::Permissive),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SecurityPolicy {
    pub enabled: bool,
    pub mode: PolicyMode,
    /// Per-site Z3 solver budget.
    pub z3_timeout_ms: u64,
    /// When true, Unknown is treated like block even in Balanced mode.
    pub block_on_unknown: bool,
    /// Enabled CWE identifiers (numeric strings, e.g. "190").
    pub enabled_cwes: HashSet<String>,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: PolicyMode::Balanced,
            z3_timeout_ms: 5_000,
            block_on_unknown: false,
            enabled_cwes: default_enabled_cwes(),
        }
    }
}

impl SecurityPolicy {
    pub fn cwe_enabled(&self, id: &str) -> bool {
        self.enabled_cwes.contains(&normalize_cwe_id(id))
    }

    pub fn any_cwe_enabled(&self, ids: &[&str]) -> bool {
        ids.iter().any(|id| self.cwe_enabled(id))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateAction {
    Allow,
    Warn { message: String },
    Block { message: String },
}

impl GateAction {
    pub fn allows_write(&self) -> bool {
        !matches!(self, Self::Block { .. })
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Warn { .. } => "warn",
            Self::Block { .. } => "block",
        }
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Allow => None,
            Self::Warn { message } | Self::Block { message } => Some(message),
        }
    }
}

/// Merge per-finding decisions into a single gate outcome for a write/edit.
pub fn decide(report: &VerifyReport, policy: &SecurityPolicy) -> GateAction {
    if !policy.enabled {
        return GateAction::Allow;
    }

    let mut blocks = Vec::new();
    let mut warns = Vec::new();

    for finding in &report.findings {
        match decide_finding(finding, policy) {
            GateAction::Block { message } => blocks.push(message),
            GateAction::Warn { message } => warns.push(message),
            GateAction::Allow => {}
        }
    }

    if !blocks.is_empty() {
        return GateAction::Block {
            message: format_gate_message("BLOCKED", &blocks, &warns, report),
        };
    }

    if !warns.is_empty() {
        return GateAction::Warn {
            message: format_gate_message("WARNING", &[], &warns, report),
        };
    }

    GateAction::Allow
}

fn decide_finding(finding: &SecurityFinding, policy: &SecurityPolicy) -> GateAction {
    let detail = format_finding_line(finding);

    match finding.status {
        ProofStatus::ProvenVulnerable => GateAction::Block { message: detail },
        ProofStatus::ProvenSafe => GateAction::Allow,
        ProofStatus::Unknown => match (policy.mode, policy.block_on_unknown) {
            (PolicyMode::Strict, _) | (_, true) => GateAction::Block {
                message: format!("{detail} (inconclusive — blocking inconclusive result)"),
            },
            (PolicyMode::Balanced, false) | (PolicyMode::Permissive, false) => {
                GateAction::Warn { message: detail }
            }
        },
        ProofStatus::PatternOnly => match policy.mode {
            PolicyMode::Strict => GateAction::Warn {
                message: format!("{detail} (pattern only — verify with Z3)"),
            },
            PolicyMode::Balanced => GateAction::Warn {
                message: format!("{detail} (pattern only — Z3 not run)"),
            },
            PolicyMode::Permissive => GateAction::Allow,
        },
    }
}

fn format_finding_line(f: &SecurityFinding) -> String {
    let witness = f
        .witness
        .as_deref()
        .map(|w| format!(" witness={w}"))
        .unwrap_or_default();
    format!(
        "line {} {} [{}] {}{}",
        f.line, f.cwe, status_label(&f.status), f.message, witness
    )
}

fn status_label(status: &ProofStatus) -> &'static str {
    match status {
        ProofStatus::ProvenVulnerable => "PROVEN",
        ProofStatus::ProvenSafe => "SAFE",
        ProofStatus::Unknown => "UNKNOWN",
        ProofStatus::PatternOnly => "PATTERN",
    }
}

fn format_gate_message(
    header: &str,
    blocks: &[String],
    warns: &[String],
    report: &VerifyReport,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{header}: security verification found issues in `{}`\n",
        report.path
    ));
    out.push_str(&format!(
        "checked {} site(s), {} Z3 call(s), {} ms\n\n",
        report.sites_checked, report.z3_invocations, report.elapsed_ms
    ));

    if !blocks.is_empty() {
        out.push_str("Blocking:\n");
        for (i, line) in blocks.iter().enumerate() {
            out.push_str(&format!("  {}. {}\n", i + 1, line));
        }
    }

    if !warns.is_empty() {
        out.push_str("Warnings:\n");
        for (i, line) in warns.iter().enumerate() {
            out.push_str(&format!("  {}. {}\n", i + 1, line));
        }
    }

    if !blocks.is_empty() {
        out.push_str(
            "\nFix the issue or run `verify_security` after editing. \
             Strict mode also blocks inconclusive (UNKNOWN) results.\n",
        );
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::findings::ProofStatus;

    fn finding(status: ProofStatus, witness: Option<&str>) -> SecurityFinding {
        SecurityFinding {
            cwe: "CWE-190".into(),
            line: 3,
            column: None,
            site: "malloc(n * sizeof(int))".into(),
            message: "integer overflow in allocation size".into(),
            status,
            witness: witness.map(str::to_string),
            fix_hint: None,
        }
    }

    fn report(findings: Vec<SecurityFinding>) -> VerifyReport {
        VerifyReport {
            path: "src/auth.c".into(),
            findings,
            sites_checked: 1,
            z3_invocations: 1,
            elapsed_ms: 12,
        }
    }

    #[test]
    fn proven_vulnerable_always_blocks() {
        let policy = SecurityPolicy {
            enabled: true,
            mode: PolicyMode::Permissive,
            ..Default::default()
        };
        let action = decide(
            &report(vec![finding(ProofStatus::ProvenVulnerable, Some("n=1073741825"))]),
            &policy,
        );
        assert!(!action.allows_write());
    }

    #[test]
    fn proven_safe_allows() {
        let policy = SecurityPolicy {
            enabled: true,
            ..Default::default()
        };
        let action = decide(&report(vec![finding(ProofStatus::ProvenSafe, None)]), &policy);
        assert!(action.allows_write());
    }

    #[test]
    fn unknown_strict_blocks() {
        let policy = SecurityPolicy {
            enabled: true,
            mode: PolicyMode::Strict,
            ..Default::default()
        };
        let action = decide(&report(vec![finding(ProofStatus::Unknown, None)]), &policy);
        assert!(!action.allows_write());
    }

    #[test]
    fn unknown_balanced_warns_by_default() {
        let policy = SecurityPolicy {
            enabled: true,
            mode: PolicyMode::Balanced,
            block_on_unknown: false,
            ..Default::default()
        };
        let action = decide(&report(vec![finding(ProofStatus::Unknown, None)]), &policy);
        assert!(matches!(action, GateAction::Warn { .. }));
        assert!(action.allows_write());
    }

    #[test]
    fn unknown_balanced_can_block_when_configured() {
        let policy = SecurityPolicy {
            enabled: true,
            mode: PolicyMode::Balanced,
            block_on_unknown: true,
            ..Default::default()
        };
        let action = decide(&report(vec![finding(ProofStatus::Unknown, None)]), &policy);
        assert!(!action.allows_write());
    }

    #[test]
    fn pattern_only_permissive_allows() {
        let policy = SecurityPolicy {
            enabled: true,
            mode: PolicyMode::Permissive,
            ..Default::default()
        };
        let action = decide(&report(vec![finding(ProofStatus::PatternOnly, None)]), &policy);
        assert!(action.allows_write());
    }
}
