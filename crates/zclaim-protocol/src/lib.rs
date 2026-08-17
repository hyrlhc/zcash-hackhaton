//! The ZClaim exchange, from both ends.
//!
//! The lower crates each do one job — build a statement, prove it, check a root,
//! track what a verifier has learned. This crate is where those become a
//! protocol, and where the two roles are kept honestly separate:
//!
//! - A [`Verifier`] asks a question and decides whether to believe the answer.
//!   Believing it means four independent checks, not one: the anchor is a real
//!   chain root, the proof verifies against the statement, the statement is the
//!   one this verifier asked for, and the claim has not been made before.
//!
//! - A [`Holder`] decides whether to answer at all. This is where the Inference
//!   Guard lives, and it lives here deliberately. A guard on the verifier's side
//!   would be the adversary policing itself; the party with something to lose
//!   has to be the party that can say no.

pub mod holder;
pub mod verifier;

pub use holder::{Holder, Refusal, Response};
pub use verifier::{Acceptance, Presentation, Rejection, Verifier};

/// Errors that are neither a refusal nor a rejection: something is misconfigured.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The request named a merchant whose address could not be parsed.
    #[error(transparent)]
    Statement(#[from] zclaim_circuits::Error),
    /// Proving failed for a reason other than an unsatisfiable predicate.
    #[error(transparent)]
    Proof(#[from] zclaim_proof::Error),
}

#[cfg(test)]
mod tests;
