//! Shared overflow encodings for multiplication and bit-shift.

use crate::security::sites::{AllocationSite, BufferCopySite, ShiftSite, TwoVarMulSite};
use super::Z3Outcome;

#[cfg(feature = "security-z3")]
use z3::ast::{BV, Bool};
#[cfg(feature = "security-z3")]
use z3::{Config, Context, SatResult, Solver};

pub fn prove_mul_overflow(site: &AllocationSite, timeout_ms: u64) -> Z3Outcome {
    if site.elem_size == 0 {
        return Z3Outcome::Unknown {
            reason: "element size is zero".into(),
        };
    }
    #[cfg(feature = "security-z3")]
    {
        prove_mul_inner(&site.var, site.elem_size, &site.var, timeout_ms, |n_u64| {
            format!(
                "{}={} ({}*{} overflows 64-bit)",
                site.var, n_u64, site.var, site.elem_size
            )
        })
    }
    #[cfg(not(feature = "security-z3"))]
    {
        let _ = (site, timeout_ms);
        Z3Outcome::Skipped
    }
}

pub fn prove_buffer_copy_overflow(site: &BufferCopySite, timeout_ms: u64) -> Z3Outcome {
    if site.elem_size == 0 {
        return Z3Outcome::Unknown {
            reason: "element size is zero".into(),
        };
    }
    #[cfg(feature = "security-z3")]
    {
        prove_mul_inner(&site.var, site.elem_size, &site.var, timeout_ms, |n_u64| {
            format!(
                "{}={} ({}*{} overflows 64-bit)",
                site.var, n_u64, site.var, site.elem_size
            )
        })
    }
    #[cfg(not(feature = "security-z3"))]
    {
        let _ = (site, timeout_ms);
        Z3Outcome::Skipped
    }
}

pub fn prove_two_var_mul(site: &TwoVarMulSite, timeout_ms: u64) -> Z3Outcome {
    #[cfg(feature = "security-z3")]
    {
        let cfg = Config::new();
        let ctx = Context::new(&cfg);
        let solver = Solver::new(&ctx);
        set_timeout(&solver, &ctx, timeout_ms);

        let bits = 64u32;
        let a = BV::new_const(&ctx, site.var_a.as_str(), bits);
        let b = BV::new_const(&ctx, site.var_b.as_str(), bits);
        let product = a.bvmul(&b);
        let zero = BV::from_u64(&ctx, 0, bits);
        let a_gt_0 = a.bvugt(&zero);
        let b_gt_0 = b.bvugt(&zero);
        let wrap = product.bvult(&a);
        let overflow = Bool::and(&ctx, &[&a_gt_0, &b_gt_0, &wrap]);
        solver.assert(&overflow);

        match solver.check() {
            SatResult::Sat => {
                let model = solver.get_model().unwrap();
                let a_val = model.eval(&a, true).unwrap().as_u64().unwrap_or(0);
                let b_val = model.eval(&b, true).unwrap().as_u64().unwrap_or(0);
                Z3Outcome::Vulnerable {
                    witness: format!(
                        "{}={}, {}={} ({}*{} overflows 64-bit)",
                        site.var_a, a_val, site.var_b, b_val, site.var_a, site.var_b
                    ),
                }
            }
            SatResult::Unsat => Z3Outcome::Safe,
            SatResult::Unknown => Z3Outcome::Unknown {
                reason: "solver returned unknown (timeout or resource limit)".into(),
            },
        }
    }
    #[cfg(not(feature = "security-z3"))]
    {
        let _ = (site, timeout_ms);
        Z3Outcome::Skipped
    }
}

pub fn prove_shift_overflow(site: &ShiftSite, timeout_ms: u64) -> Z3Outcome {
    #[cfg(feature = "security-z3")]
    {
        let cfg = Config::new();
        let ctx = Context::new(&cfg);
        let solver = Solver::new(&ctx);
        set_timeout(&solver, &ctx, timeout_ms);

        let bits = 64u32;
        let n = BV::new_const(&ctx, site.var.as_str(), bits);
        let shift = BV::from_u64(&ctx, site.shift as u64, bits);
        let shifted = n.bvshl(&shift);
        let zero = BV::from_u64(&ctx, 0, bits);
        let n_gt_0 = n.bvugt(&zero);
        let wrap = shifted.bvult(&n);
        let overflow = Bool::and(&ctx, &[&n_gt_0, &wrap]);
        solver.assert(&overflow);

        match solver.check() {
            SatResult::Sat => {
                let model = solver.get_model().unwrap();
                let n_val = model.eval(&n, true).unwrap().as_u64().unwrap_or(0);
                Z3Outcome::Vulnerable {
                    witness: format!(
                        "{}={} ({}<<{} overflows 64-bit)",
                        site.var, n_val, site.var, site.shift
                    ),
                }
            }
            SatResult::Unsat => Z3Outcome::Safe,
            SatResult::Unknown => Z3Outcome::Unknown {
                reason: "solver returned unknown (timeout or resource limit)".into(),
            },
        }
    }
    #[cfg(not(feature = "security-z3"))]
    {
        let _ = (site, timeout_ms);
        Z3Outcome::Skipped
    }
}

#[cfg(feature = "security-z3")]
fn prove_mul_inner(
    var_name: &str,
    elem_size: u64,
    witness_var: &str,
    timeout_ms: u64,
    witness_fmt: impl FnOnce(u64) -> String,
) -> Z3Outcome {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    let solver = Solver::new(&ctx);
    set_timeout(&solver, &ctx, timeout_ms);

    let bits = 64u32;
    let n = BV::new_const(&ctx, var_name, bits);
    let elem = BV::from_u64(&ctx, elem_size, bits);
    let product = n.bvmul(&elem);
    let zero = BV::from_u64(&ctx, 0, bits);
    let n_gt_0 = n.bvugt(&zero);
    let wrap = product.bvult(&n);
    let overflow = Bool::and(&ctx, &[&n_gt_0, &wrap]);
    solver.assert(&overflow);

    match solver.check() {
        SatResult::Sat => {
            let model = solver.get_model().unwrap();
            let n_val = model.eval(&n, true).unwrap().as_u64().unwrap_or(0);
            let _ = witness_var;
            Z3Outcome::Vulnerable {
                witness: witness_fmt(n_val),
            }
        }
        SatResult::Unsat => Z3Outcome::Safe,
        SatResult::Unknown => Z3Outcome::Unknown {
            reason: "solver returned unknown (timeout or resource limit)".into(),
        },
    }
}

#[cfg(feature = "security-z3")]
fn set_timeout(solver: &Solver, ctx: &Context, timeout_ms: u64) {
    let mut params = z3::Params::new(ctx);
    params.set_u32("timeout", timeout_ms as u32);
    solver.set_params(&params);
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
        let outcome = prove_mul_overflow(&site("n", 4), 5_000);
        assert!(matches!(outcome, Z3Outcome::Vulnerable { .. }));
    }
}
