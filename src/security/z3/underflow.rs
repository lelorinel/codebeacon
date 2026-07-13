//! CWE-191 integer underflow encoding.

use crate::security::sites::SubtractionSite;
use super::Z3Outcome;

pub fn prove_underflow(site: &SubtractionSite, timeout_ms: u64) -> Z3Outcome {
    #[cfg(feature = "security-z3")]
    {
        use z3::ast::BV;
        use z3::{Config, Context, SatResult, Solver};

        let cfg = Config::new();
        let ctx = Context::new(&cfg);
        let solver = Solver::new(&ctx);

        let mut params = z3::Params::new(&ctx);
        params.set_u32("timeout", timeout_ms as u32);
        solver.set_params(&params);

        let bits = 64u32;
        let n = BV::new_const(&ctx, site.var.as_str(), bits);
        let k = BV::from_u64(&ctx, site.subtractor, bits);
        let underflow = n.bvult(&k);
        solver.assert(&underflow);

        match solver.check() {
            SatResult::Sat => {
                let model = solver.get_model().unwrap();
                let n_val = model.eval(&n, true).unwrap().as_u64().unwrap_or(0);
                Z3Outcome::Vulnerable {
                    witness: format!(
                        "{}={} ({}-{} underflows 64-bit)",
                        site.var, n_val, site.var, site.subtractor
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
#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::sites::SubtractionSite;

    #[test]
    fn subtraction_underflow_is_vulnerable() {
        let site = SubtractionSite {
            raw: "malloc(n - 4)".into(),
            line: 1,
            var: "n".into(),
            subtractor: 4,
            subtractor_source: "4".into(),
        };
        let outcome = prove_underflow(&site, 5_000);
        assert!(matches!(outcome, Z3Outcome::Vulnerable { .. }));
    }
}
