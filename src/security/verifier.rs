use crate::security::findings::{ProofStatus, SecurityFinding, VerifyReport};
use crate::security::patterns;
use crate::security::policy::{decide, GateAction};
use crate::security::sites::allocation;
use crate::security::sites::buffer_copy;
use crate::security::sites::division;
use crate::security::sites::subtraction;
use crate::security::sites::{security_markers_present, ExtractedSite};
use crate::security::z3::{
    prove_buffer_copy_overflow, prove_div_zero, prove_mul_overflow, prove_shift_overflow,
    prove_two_var_mul, prove_underflow, Z3Outcome,
};
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
        if !security_markers_present(line) {
            continue;
        }

        if policy.any_cwe_enabled(&["190", "131"]) {
            for extracted in allocation::extract_sites(line, line_no) {
                sites_checked += 1;
                process_allocation_site(
                    extracted,
                    policy,
                    &mut findings,
                    &mut z3_invocations,
                );
            }
        }

        if policy.cwe_enabled("191") {
            for extracted in subtraction::extract_sites(line, line_no) {
                sites_checked += 1;
                process_subtraction_site(extracted, policy, &mut findings, &mut z3_invocations);
            }
        }

        if policy.cwe_enabled("369") {
            for extracted in division::extract_sites(line, line_no) {
                sites_checked += 1;
                process_division_site(extracted, policy, &mut findings, &mut z3_invocations);
            }
        }

        if policy.cwe_enabled("680") {
            for extracted in buffer_copy::extract_sites(line, line_no) {
                sites_checked += 1;
                process_buffer_copy_site(extracted, policy, &mut findings, &mut z3_invocations);
            }
        }

        let pattern_findings = patterns::check_line(line, line_no, policy);
        sites_checked += pattern_findings.len();
        findings.extend(pattern_findings);
    }

    VerifyReport {
        path: path.display().to_string(),
        findings,
        sites_checked,
        z3_invocations,
        elapsed_ms: start.elapsed().as_millis() as u64,
    }
}

fn process_allocation_site(
    extracted: ExtractedSite,
    policy: &SecurityPolicy,
    findings: &mut Vec<SecurityFinding>,
    z3_invocations: &mut usize,
) {
    match extracted {
        ExtractedSite::SymbolicMul(site) if policy.cwe_enabled("190") => {
            let site_raw = site.raw.clone();
            let (status, witness, z3_called) =
                z3_status(prove_mul_overflow(&site, policy.z3_timeout_ms));
            if z3_called {
                *z3_invocations += 1;
            }
            findings.push(make_finding(
                site.line,
                &site_raw,
                "CWE-190",
                "integer overflow risk in allocation size computation",
                status,
                witness,
                Some("guard: if (n > SIZE_MAX / element_size) return NULL;".into()),
            ));
        }
        ExtractedSite::SymbolicTwoVarMul(site) if policy.cwe_enabled("131") => {
            let site_raw = site.raw.clone();
            let (status, witness, z3_called) =
                z3_status(prove_two_var_mul(&site, policy.z3_timeout_ms));
            if z3_called {
                *z3_invocations += 1;
            }
            findings.push(make_finding(
                site.line,
                &site_raw,
                "CWE-131",
                "incorrect buffer size from two-variable multiplication",
                status,
                witness,
                Some("guard both factors before multiplying for allocation size".into()),
            ));
        }
        ExtractedSite::SymbolicShift(site) if policy.cwe_enabled("190") => {
            let site_raw = site.raw.clone();
            let (status, witness, z3_called) =
                z3_status(prove_shift_overflow(&site, policy.z3_timeout_ms));
            if z3_called {
                *z3_invocations += 1;
            }
            findings.push(make_finding(
                site.line,
                &site_raw,
                "CWE-190",
                "integer overflow risk in shift-based allocation size",
                status,
                witness,
                Some("guard shift amount and check for overflow before allocation".into()),
            ));
        }
        ExtractedSite::Inconclusive { raw, line, .. } => {
            findings.push(make_finding(
                line,
                &raw,
                "CWE-190",
                "allocation size could not be verified",
                ProofStatus::Unknown,
                None,
                None,
            ));
        }
        ExtractedSite::PatternOnly { raw, line } => {
            findings.push(make_finding(
                line,
                &raw,
                "CWE-190",
                "allocation pattern detected but not structured",
                ProofStatus::PatternOnly,
                None,
                None,
            ));
        }
        _ => {}
    }
}

fn process_subtraction_site(
    extracted: ExtractedSite,
    policy: &SecurityPolicy,
    findings: &mut Vec<SecurityFinding>,
    z3_invocations: &mut usize,
) {
    if let ExtractedSite::SymbolicSub(site) = extracted {
        let site_raw = site.raw.clone();
        let (status, witness, z3_called) = z3_status(prove_underflow(&site, policy.z3_timeout_ms));
        if z3_called {
            *z3_invocations += 1;
        }
        findings.push(make_finding(
            site.line,
            &site_raw,
            "CWE-191",
            "integer underflow risk in allocation size computation",
            status,
            witness,
            Some("guard: if (n < subtractor) return NULL;".into()),
        ));
    }
}

fn process_division_site(
    extracted: ExtractedSite,
    policy: &SecurityPolicy,
    findings: &mut Vec<SecurityFinding>,
    z3_invocations: &mut usize,
) {
    if let ExtractedSite::SymbolicDiv(site) = extracted {
        let site_raw = site.raw.clone();
        let (status, witness, z3_called) = z3_status(prove_div_zero(&site, policy.z3_timeout_ms));
        if z3_called {
            *z3_invocations += 1;
        }
        findings.push(make_finding(
            site.line,
            &site_raw,
            "CWE-369",
            "divide-by-zero risk in allocation size computation",
            status,
            witness,
            Some("guard: if (divisor == 0) return NULL;".into()),
        ));
    }
}

fn process_buffer_copy_site(
    extracted: ExtractedSite,
    policy: &SecurityPolicy,
    findings: &mut Vec<SecurityFinding>,
    z3_invocations: &mut usize,
) {
    if let ExtractedSite::SymbolicBufferCopy(site) = extracted {
        let site_raw = site.raw.clone();
        let (status, witness, z3_called) =
            z3_status(prove_buffer_copy_overflow(&site, policy.z3_timeout_ms));
        if z3_called {
            *z3_invocations += 1;
        }
        findings.push(make_finding(
            site.line,
            &site_raw,
            "CWE-680",
            "integer overflow risk in buffer copy size computation",
            status,
            witness,
            Some("guard copy size before memcpy/memset".into()),
        ));
    }
}

fn z3_status(outcome: Z3Outcome) -> (ProofStatus, Option<String>, bool) {
    match outcome {
        Z3Outcome::Vulnerable { witness } => (ProofStatus::ProvenVulnerable, Some(witness), true),
        Z3Outcome::Safe => (ProofStatus::ProvenSafe, None, true),
        Z3Outcome::Unknown { .. } => (ProofStatus::Unknown, None, true),
        Z3Outcome::Skipped => (ProofStatus::PatternOnly, None, false),
    }
}

fn make_finding(
    line: usize,
    site: &str,
    cwe: &str,
    message: &str,
    status: ProofStatus,
    witness: Option<String>,
    fix_hint: Option<String>,
) -> SecurityFinding {
    SecurityFinding {
        cwe: cwe.to_string(),
        line,
        column: None,
        site: site.to_string(),
        message: message.to_string(),
        status,
        witness,
        fix_hint,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> SecurityPolicy {
        SecurityPolicy {
            enabled: true,
            ..Default::default()
        }
    }

    #[test]
    fn detects_malloc_overflow_candidate() {
        let code = "int* p = malloc(n * sizeof(int));";
        let report = verify_fragment(Path::new("a.c"), code, &policy());
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
        let report = verify_fragment(Path::new("a.c"), "return x + 1;", &policy());
        assert_eq!(report.sites_checked, 0);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn constant_only_malloc_no_finding() {
        let report = verify_fragment(
            Path::new("a.c"),
            "int* p = malloc(100 * sizeof(int));",
            &policy(),
        );
        assert_eq!(report.sites_checked, 0);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn two_variables_uses_cwe131() {
        let code = "void* p = malloc(a * b);";
        let report = verify_fragment(Path::new("a.c"), code, &policy());
        assert_eq!(report.sites_checked, 1);
        assert_eq!(report.findings[0].cwe, "CWE-131");

        #[cfg(feature = "security-z3")]
        assert_eq!(report.findings[0].status, ProofStatus::ProvenVulnerable);
    }

    #[test]
    fn subtraction_cwe191() {
        let report = verify_fragment(Path::new("a.c"), "p = malloc(n - 4);", &policy());
        assert_eq!(report.findings[0].cwe, "CWE-191");
    }

    #[test]
    fn division_cwe369() {
        let report = verify_fragment(
            Path::new("a.c"),
            "p = malloc(total / count);",
            &policy(),
        );
        assert_eq!(report.findings[0].cwe, "CWE-369");
    }

    #[test]
    fn memcpy_cwe680() {
        let report = verify_fragment(
            Path::new("a.c"),
            "memcpy(dst, src, n * 4);",
            &policy(),
        );
        assert_eq!(report.findings[0].cwe, "CWE-680");
    }

    #[test]
    fn pattern_cwe_disabled_by_default() {
        let report = verify_fragment(Path::new("a.c"), "printf(user_input);", &policy());
        assert!(report.findings.is_empty());
    }

    #[test]
    fn pattern_cwe_enabled_when_configured() {
        let mut p = policy();
        p.enabled_cwes.insert("134".into());
        let report = verify_fragment(Path::new("a.c"), "printf(user_input);", &p);
        assert_eq!(report.findings[0].cwe, "CWE-134");
        assert_eq!(report.findings[0].status, ProofStatus::PatternOnly);
    }

    #[cfg(feature = "security-z3")]
    #[test]
    fn proven_vulnerable_has_witness() {
        let report = verify_fragment(
            Path::new("a.c"),
            "char* buf = malloc(count * 4);",
            &policy(),
        );
        assert_eq!(report.findings[0].status, ProofStatus::ProvenVulnerable);
        assert!(report.findings[0].witness.as_ref().unwrap().contains("count="));
    }
}
