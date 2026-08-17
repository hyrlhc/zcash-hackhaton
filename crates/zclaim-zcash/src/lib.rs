//! The chain-facing side of ZClaim.
//!
//! Two jobs:
//!
//! 1. Turn a note the holder can decrypt, plus the commitment tree it sits in,
//!    into a [`zclaim_circuits::NoteWitness`].
//! 2. Stop a verifier from being fooled by a fabricated anchor.
//!
//! Job 2 is the one that is easy to get wrong. The circuit proves a note is in
//! *some* tree with a given root; it says nothing about whether that root ever
//! existed on Zcash. Without [`AnchorAuthenticator`], a prover can build a tree
//! containing a note it invented and produce a perfectly valid proof about it.

pub mod anchor;
#[cfg(feature = "lightwalletd")]
pub mod lightwalletd;
pub mod witness;

pub use anchor::{AnchorAuthenticator, AnchorSource, AuthenticatedAnchor, RootWindow};
pub use witness::TreeWitnessBuilder;

#[cfg(feature = "lightwalletd")]
pub use lightwalletd::{ChainInfo, LightwalletClient, TreeState, TESTNET_ENDPOINT};

/// Errors from the chain-facing layer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The anchor is not a root this verifier is willing to accept.
    #[error("anchor {anchor} is not a known {pool:?} note commitment tree root")]
    UnknownAnchor {
        /// Hex-encoded anchor.
        anchor: String,
        /// The pool it was claimed for.
        pool: zclaim_circuits::Pool,
    },
    /// The anchor is real but belongs to a different pool than claimed.
    #[error("anchor belongs to the {actual:?} pool, not {claimed:?}")]
    WrongPool {
        /// The pool the anchor really came from.
        actual: zclaim_circuits::Pool,
        /// The pool the statement claimed.
        claimed: zclaim_circuits::Pool,
    },
    /// The anchor is real but too old to accept.
    #[error("anchor at height {height} is older than the {limit}-block acceptance window")]
    AnchorTooOld {
        /// Height the anchor was observed at.
        height: u32,
        /// Configured window.
        limit: u32,
    },
    /// The commitment tree data was malformed.
    #[error("malformed tree data: {0}")]
    MalformedTree(String),
    /// Could not reach the light wallet server.
    #[error("could not reach the chain: {0}")]
    Transport(String),
    /// The server answered, with an error.
    #[error("{method} failed: {message}")]
    Rpc {
        /// The RPC that failed.
        method: String,
        /// What the server said.
        message: String,
    },
}
