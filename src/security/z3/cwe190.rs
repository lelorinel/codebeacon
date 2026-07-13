//! CWE-190 integer overflow encoding — delegates to shared overflow module.

use crate::security::sites::AllocationSite;
use super::overflow::prove_mul_overflow;
use super::Z3Outcome;

pub fn prove_cwe190(site: &AllocationSite, timeout_ms: u64) -> Z3Outcome {
    prove_mul_overflow(site, timeout_ms)
}
