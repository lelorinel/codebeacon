use crate::security::findings::{ProofStatus, SecurityFinding, VerifyReport};
use crate::security::policy::{decide, GateAction};
use crate::security::sites::{extract_sites, ExtractedSite};
use crate::security::z3::{prove_cwe190, Z3Outcome};
use std::path::Path;
use std::time::Instant;

use super::policy::SecurityPolicy;

/// Run verification and policy decision — same path as the MCP security gate.
pub fn verify_and_decide(
    path: &Path,
    content: &str,
    policy: &SecurityPolicy,
) -> (VerifyReport, GateAction) {
    let report = verify_fragment(path, content, policy);
    let action = decide(&report, policy);
    (report, action)
}

/// Verify a code fragment (typically the `new_string` from an edit).
pub fn verify_fragment(
    path: &Path,
    content: &str,
    policy: &SecurityPolicy,
) -> VerifyReport {
    let start = Instant::now();
    let mut findings = Vec::new();
    let mut sites_checked = 0usize;
    let mut z3_invocations = 0usize;

    if !policy.enabled {
        return VerifyReport {
            path: path.display().to_string(),
            findings,
            sites_checked,
            z3_invocations,
            elapsed_ms: start.elapsed().as_millis() as u64,
        };
    }

    for (line_idx, line) in content.lines().enumerate() {
        let line_no = line_idx + 1;
        for extracted in extract_sites(line, line_no) {
            sites_checked += 1;
            match extracted {
                ExtractedSite::Symbolic(site) => {
                    let site_raw = site.raw.clone();
                    let (status, witness, z3_called) = match prove_cwe190(&site, policy.z3_timeout_ms)
                    {
                        Z3Outcome::Vulnerable { witness } => {
                            (ProofStatus::ProvenVulnerable, Some(witness), true)
                        }
                        Z3Outcome::Safe => (ProofStatus::ProvenSafe, None, true),
                        Z3Outcome::Unknown { .. } => (ProofStatus::Unknown, None, true),
                        Z3Outcome::Skipped => (ProofStatus::PatternOnly, None, false),
                    };
                    if z3_called {
                        z3_invocations += 1;
                    }
                    findings.push(make_finding(line_no, &site_raw, status, witness));
                }
                ExtractedSite::Inconclusive { raw, .. } => {
                    findings.push(make_finding(line_no, &raw, ProofStatus::Unknown, None));
                }
                ExtractedSite::PatternOnly(raw) => {
                    findings.push(make_finding(line_no, &raw, ProofStatus::PatternOnly, None));
                }
            }
        }
    }

    VerifyReport {
        path: path.display().to_string(),
        findings,
        sites_checked,
        z3_invocations,
        elapsed_ms: start.elapsed().as_millis() as u64,
    }
}

fn make_finding(
    line: usize,
    site: &str,
    status: ProofStatus,
    witness: Option<String>,
) -> SecurityFinding {
    SecurityFinding {
        cwe: "CWE-190".into(),
        line,
        column: None,
        site: site.to_string(),
        message: "integer overflow risk in allocation size computation".into(),
        status,
        witness,
        fix_hint: Some("guard: if (n > SIZE_MAX / element_size) return NULL;".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_malloc_overflow_candidate() {
        let code = "int* p = malloc(n * sizeof(int));";
        let policy = SecurityPolicy {
            enabled: true,
            ..Default::default()
        };
        let report = verify_fragment(Path::new("a.c"), code, &policy);
        assert_eq!(report.sites_checked, 1);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].cwe, "CWE-190");

        #[cfg(feature = "security-z3")]
        {
            assert_eq!(report.findings[0].status, ProofStatus::ProvenVulnerable);
            assert!(report.findings[0].witness.is_some());
            assert!(report.z3_invocations > 0);
        }

        #[cfg(not(feature = "security-z3"))]
        {
            assert_eq!(report.findings[0].status, ProofStatus::PatternOnly);
            assert_eq!(report.z3_invocations, 0);
        }
    }

    #[test]
    fn skips_safe_looking_line() {
        let code = "return x + 1;";
        let policy = SecurityPolicy {
            enabled: true,
            ..Default::default()
        };
        let report = verify_fragment(Path::new("a.c"), code, &policy);
        assert_eq!(report.sites_checked, 0);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn constant_only_malloc_no_finding() {
        let code = "int* p = malloc(100 * sizeof(int));";
        let policy = SecurityPolicy {
            enabled: true,
            ..Default::default()
        };
        let report = verify_fragment(Path::new("a.c"), code, &policy);
        assert_eq!(report.sites_checked, 0);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn two_variables_is_unknown() {
        let code = "void* p = malloc(a * b);";
        let policy = SecurityPolicy {
            enabled: true,
            ..Default::default()
        };
        let report = verify_fragment(Path::new("a.c"), code, &policy);
        assert_eq!(report.sites_checked, 1);
        assert_eq!(report.findings[0].status, ProofStatus::Unknown);
        assert_eq!(report.z3_invocations, 0);
    }

    #[cfg(feature = "security-z3")]
    #[test]
    fn proven_vulnerable_has_witness() {
        let code = "char* buf = malloc(count * 4);";
        let policy = SecurityPolicy {
            enabled: true,
            ..Default::default()
        };
        let report = verify_fragment(Path::new("a.c"), code, &policy);
        assert_eq!(report.findings[0].status, ProofStatus::ProvenVulnerable);
        assert!(report.findings[0].witness.as_ref().unwrap().contains("count="));
    }
}
