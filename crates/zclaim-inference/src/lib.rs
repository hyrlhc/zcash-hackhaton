//! Inference Guard.
//!
//! Every ZClaim proof is individually sound and individually leaks nothing
//! beyond its predicate. A *sequence* of them is a different matter. A verifier
//! that asks
//!
//! ```text
//! >= 1 ZEC    -> yes
//! >= 2 ZEC    -> yes
//! >= 4 ZEC    -> no
//! >= 3 ZEC    -> no
//! >= 2.5 ZEC  -> yes
//! ```
//!
//! is running a binary search on a number the system exists to hide. Each
//! answer is one bit; enough bits reconstruct the amount. No change to the
//! circuit can prevent this, because every individual proof is honest.
//!
//! The guard sits in front of the prover. It tracks, per (holder, verifier),
//! what the answers so far have already pinned down, and refuses to answer a
//! question that would narrow the surviving interval below a policy floor.
//!
//! # What this is not
//!
//! This is a **policy layer over query history**, not a cryptographic
//! guarantee. It binds a verifier that goes through this prover. It does not
//! bind a verifier who obtains proofs by another route, nor several verifiers
//! who collude by pooling their answers out of band — see [`Policy::granularity`]
//! and the threat model.

mod guard;
mod interval;

pub use guard::{Decision, Guard, Observation, Policy, SubjectKey};
pub use interval::Knowledge;

#[cfg(test)]
mod tests;
