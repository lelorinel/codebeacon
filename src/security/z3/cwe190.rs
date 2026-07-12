//! CWE-190 integer overflow encoding for allocation size expressions.

use crate::security::sites::AllocationSite;

/// Result of a Z3 proof attempt on one allocation site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Z3Outcome {
    Vulnerable { witness: String },
    Safe,
    Unknown { reason: String },
    /// Feature disabled or site out of Z3 scope — caller keeps PatternOnly.
    Skipped,
}

#[cfg(feature = "security-z3")]
pub fn prove_cwe190(site: &AllocationSite, timeout_ms: u64) -> Z3Outcome {
    if site.elem_size == 0 {
        return Z3Outcome::Unknown {
            reason: "element size is zero".into(),
        };
    }

    prove_cwe190_inner(site, timeout_ms)
}

#[cfg(not(feature = "security-z3"))]
pub fn prove_cwe190(_site: &AllocationSite, _timeout_ms: u64) -> Z3Outcome {
    Z3Outcome::Skipped
}

#[cfg(feature = "security-z3")]
fn prove_cwe190_inner(site: &AllocationSite, timeout_ms: u64) -> Z3Outcome {
    use z3::ast::{BV, Bool};
    use z3::{Config, Context, SatResult, Solver};

    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    let solver = Solver::new(&ctx);

    let mut params = z3::Params::new(&ctx);
    params.set_u32("timeout", timeout_ms as u32);
    solver.set_params(&params);

    let bits = 64u32;
    let n = BV::new_const(&ctx, site.var.as_str(), bits);
    let elem = BV::from_u64(&ctx, site.elem_size, bits);
    let product = n.bvmul(&elem);
    let zero = BV::from_u64(&ctx, 0, bits);
    let n_gt_0 = n.bvugt(&zero);
    let wrap = product.bvult(&n);
    let overflow = Bool::and(&ctx, &[&n_gt_0, &wrap]);

    solver.assert(&overflow);

    match solver.check() {
        SatResult::Sat => {
            let model = solver.get_model().unwrap();
            let n_val = model.eval(&n, true).unwrap();
            let n_u64 = n_val.as_u64().unwrap_or(0);
            let witness = format!(
                "{}={} ({}*{} overflows {}-bit)",
                site.var, n_u64, site.var, site.elem_size, bits
            );
            Z3Outcome::Vulnerable { witness }
        }
        SatResult::Unsat => Z3Outcome::Safe,
        SatResult::Unknown => Z3Outcome::Unknown {
            reason: "solver returned unknown (timeout or resource limit)".into(),
        },
    }
}

#[cfg(feature = "security-z3")]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::sites::{AllocKind, AllocationSite};

    fn site(var: &str, elem_size: u64) -> AllocationSite {
        AllocationSite {
            raw: format!("malloc({var} * {elem_size})"),
            line: 1,
            kind: AllocKind::Malloc,
            var: var.to_string(),
            elem_size,
            elem_size_source: elem_size.to_string(),
        }
    }

    #[test]
    fn malloc_n_times_four_is_vulnerable() {
        let outcome = prove_cwe190(&site("n", 4), 5_000);
        assert!(
            matches!(outcome, Z3Outcome::Vulnerable { .. }),
            "expected SAT, got {outcome:?}"
        );
        if let Z3Outcome::Vulnerable { witness } = outcome {
            assert!(witness.contains("n="));
            assert!(witness.contains('4'));
        }
    }

    #[test]
    fn elem_size_zero_is_unknown() {
        let outcome = prove_cwe190(&site("n", 0), 5_000);
        assert!(matches!(outcome, Z3Outcome::Unknown { .. }));
    }

    #[test]
    fn forced_no_overflow_is_safe() {
        use z3::ast::{BV, Bool};
        use z3::{Config, Context, SatResult, Solver};

        let cfg = Config::new();
        let ctx = Context::new(&cfg);
        let solver = Solver::new(&ctx);
        let n = BV::new_const(&ctx, "n", 64);
        let elem = BV::from_u64(&ctx, 4, 64);
        let product = n.bvmul(&elem);
        let zero = BV::from_u64(&ctx, 0, 64);
        let n_gt_0 = n.bvugt(&zero);
        let wrap = product.bvult(&n);
        let overflow = Bool::and(&ctx, &[&n_gt_0, &wrap]);
        solver.assert(&overflow);
        // Force n < 1 — no positive n can overflow.
        solver.assert(&n.bvule(&zero));
        assert_eq!(solver.check(), SatResult::Unsat);
    }
}
