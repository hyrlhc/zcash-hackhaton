//! The ZClaim predicate circuit.
//!
//! Proves a condition about a real Zcash shielded note without revealing the
//! note. Every cryptographic constraint is Zcash's own code: `NoteCommit^Orchard`
//! and `MerkleCRH^Orchard` come from the `orchard` crate under its
//! `unstable-voting-circuits` feature, which exists so third parties can build
//! circuits over Orchard-protocol notes.
//!
//! Ironwood (NU6.3) reuses the Orchard protocol's note machinery, so this one
//! circuit covers both the Orchard and Ironwood pools. The pools differ only in
//! which note commitment tree the anchor comes from, which is the verifier's
//! job to authenticate — see [`Statement`].

mod circuit;
pub mod statement;
#[cfg(any(test, feature = "test-fixtures"))]
pub mod testing;
pub mod wire;
mod witness;

pub use circuit::{ZClaimCircuit, K};
pub use statement::{MerchantBinding, Pool, Statement, NUM_INSTANCES};
pub use wire::{MerchantBindingWire, PoolWire, StatementWire};
pub use witness::{holder_tag, note_nullifier, NoteWitness};

/// Errors from statement construction.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The statement could not be built from the request.
    #[error("invalid statement: {0}")]
    Statement(String),
}

#[cfg(test)]
mod tests;
