use serde::{Deserialize, Serialize};

/// Result of a formal check on one code site (e.g. one `malloc(n * size)` expression).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProofStatus {
    /// Z3 returned SAT — exploit input exists (witness required).
    ProvenVulnerable,
    /// Z3 returned UNSAT — no exploit input exists for this property encoding.
    ProvenSafe,
    /// Z3 timed out, encoding failed, or site was out of scope.
    Unknown,
    /// AST/heuristic pattern matched but Z3 was not run or did not complete.
    PatternOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityFinding {
    pub cwe: String,
    pub line: usize,
    pub column: Option<usize>,
    pub site: String,
    pub message: String,
    pub status: ProofStatus,
    /// Concrete exploit input when status is ProvenVulnerable (e.g. `n = 1073741825`).
    pub witness: Option<String>,
    /// Suggested fix when available.
    pub fix_hint: Option<String>,
}

impl SecurityFinding {
    pub fn is_blocking_candidate(&self) -> bool {
        matches!(self.status, ProofStatus::ProvenVulnerable)
    }

    pub fn needs_attention(&self) -> bool {
        !matches!(self.status, ProofStatus::ProvenSafe)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VerifyReport {
    pub path: String,
    pub findings: Vec<SecurityFinding>,
    pub sites_checked: usize,
    pub z3_invocations: usize,
    pub elapsed_ms: u64,
}

impl VerifyReport {
    pub fn has_findings(&self) -> bool {
        self.findings.iter().any(|f| f.needs_attention())
    }

    pub fn blocking_findings(&self) -> Vec<&SecurityFinding> {
        self.findings
            .iter()
            .filter(|f| f.is_blocking_candidate())
            .collect()
    }
}
