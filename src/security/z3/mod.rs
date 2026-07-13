//! Z3 SMT backend for security proofs (optional `security-z3` feature).

mod cwe190;
mod div_zero;
mod overflow;
mod underflow;

pub use cwe190::prove_cwe190;
pub use div_zero::prove_div_zero;
pub use overflow::{
    prove_buffer_copy_overflow, prove_mul_overflow, prove_shift_overflow, prove_two_var_mul,
};
pub use underflow::prove_underflow;

/// Result of a Z3 proof attempt on one security site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Z3Outcome {
    Vulnerable { witness: String },
    Safe,
    Unknown { reason: String },
    /// Feature disabled or site out of Z3 scope — caller keeps PatternOnly.
    Skipped,
}
