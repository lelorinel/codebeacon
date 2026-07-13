//! CWE-369 divide-by-zero encoding.

use crate::security::sites::DivisionSite;
use super::Z3Outcome;

pub fn prove_div_zero(site: &DivisionSite, timeout_ms: u64) -> Z3Outcome {
    #[cfg(feature = "security-z3")]
    {
        use z3::ast::{Ast, BV};
        use z3::{Config, Context, SatResult, Solver};

        let cfg = Config::new();
        let ctx = Context::new(&cfg);
        let solver = Solver::new(&ctx);

        let mut params = z3::Params::new(&ctx);
        params.set_u32("timeout", timeout_ms as u32);
        solver.set_params(&params);

        let bits = 64u32;
        let divisor = BV::new_const(&ctx, site.divisor_var.as_str(), bits);
        let zero = BV::from_u64(&ctx, 0, bits);
        solver.assert(&divisor._eq(&zero));

        match solver.check() {
            SatResult::Sat => Z3Outcome::Vulnerable {
                witness: format!("{}=0 (division by zero)", site.divisor_var),
            },
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
    use crate::security::sites::DivisionSite;

    #[test]
    fn division_by_zero_is_vulnerable() {
        let site = DivisionSite {
            raw: "malloc(total / count)".into(),
            line: 1,
            dividend_var: "total".into(),
            divisor_var: "count".into(),
        };
        let outcome = prove_div_zero(&site, 5_000);
        assert!(matches!(outcome, Z3Outcome::Vulnerable { .. }));
    }
}
