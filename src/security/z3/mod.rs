//! Z3 SMT backend for security proofs (optional `security-z3` feature).

mod cwe190;

pub use cwe190::{prove_cwe190, Z3Outcome};
