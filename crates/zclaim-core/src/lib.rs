//! Shared vocabulary for ZClaim: what a verifier may ask, and how that question
//! is bound into a proof so the answer cannot be reused elsewhere.
//!
//! This crate has no dependency on the proving system. It exists so the
//! verifier SDK, the guard and the circuit all agree on exactly one canonical
//! encoding of a request — if they disagree, context binding silently breaks.

pub mod context;
pub mod holder;
pub mod predicate;

pub use context::{Context, DomainTag, ProofRequest, VerifierDomain};
pub use holder::HolderKey;
pub use predicate::{
    AmountPredicate, Comparison, Merchant, Predicate, Zatoshi, RAW_ADDRESS_LEN,
};

/// Errors surfaced by predicate parsing and request construction.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// A merchant address was not valid hex, or not the raw address length.
    #[error("invalid merchant address encoding: {0}")]
    InvalidMerchantAddress(String),
    /// The predicate JSON did not match the canonical schema.
    #[error("invalid predicate: {0}")]
    InvalidPredicate(String),
    /// The request has passed its expiry height.
    #[error("proof request expired at height {expiry}, chain is at {current}")]
    Expired {
        /// Height the request was valid until.
        expiry: u32,
        /// Height the verifier observed.
        current: u32,
    },
}
