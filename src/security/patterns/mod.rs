//! Pattern-only security checks (no Z3).

mod command_injection;
mod deserialize;
mod format_string;
mod path_traversal;
mod secrets;

use crate::security::findings::SecurityFinding;
use crate::security::policy::SecurityPolicy;

pub fn check_line(line: &str, line_no: usize, policy: &SecurityPolicy) -> Vec<SecurityFinding> {
    let mut findings = Vec::new();
    if policy.cwe_enabled("134") {
        findings.extend(format_string::check_line(line, line_no));
    }
    if policy.cwe_enabled("78") {
        findings.extend(command_injection::check_line(line, line_no));
    }
    if policy.cwe_enabled("798") {
        findings.extend(secrets::check_line(line, line_no));
    }
    if policy.cwe_enabled("502") {
        findings.extend(deserialize::check_line(line, line_no));
    }
    if policy.cwe_enabled("22") {
        findings.extend(path_traversal::check_line(line, line_no));
    }
    findings
}
