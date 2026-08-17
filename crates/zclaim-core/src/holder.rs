//! The holder's long-term secret.
//!
//! A ZClaim proof answers a question about a note. Without something else in
//! the statement, anyone who obtains that note's witness could answer the same
//! question — the proof would be a bearer token that survives being copied.
//! The holder key is what a proof is additionally bound to, so that answering
//! requires an ongoing secret rather than a one-time leak.
//!
//! The key never leaves the prover. What appears in a statement is a
//! domain-scoped tag derived from it, which is computed inside the circuit —
//! see `zclaim_circuits::holder_tag`.

use ff::FromUniformBytes;
use pasta_curves::pallas;

/// Domain separator for holder key derivation.
const HOLDER_PERSONALIZATION: &[u8; 16] = b"ZClaim_Holder__1";

/// A holder's secret, as a Pallas base field element.
///
/// Deliberately not `Serialize`, `Display` or transparently `Debug`: the only
/// way this value should ever leave the process is through wallet storage the
/// caller controls.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct HolderKey(pallas::Base);

impl HolderKey {
    /// Derives a holder key from 32 bytes of wallet entropy.
    ///
    /// Hashing rather than reducing the seed directly means every 32-byte seed
    /// is usable, including the ones that are not canonical field elements.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let hash = blake2b_simd::Params::new()
            .hash_length(64)
            .personal(HOLDER_PERSONALIZATION)
            .hash(seed);

        let mut wide = [0u8; 64];
        wide.copy_from_slice(hash.as_bytes());
        HolderKey(pallas::Base::from_uniform_bytes(&wide))
    }

    /// The secret field element the circuit witnesses.
    ///
    /// Callers must treat the result as key material: it must not be logged,
    /// serialised into a request, or sent to a verifier.
    pub fn secret(&self) -> pallas::Base {
        self.0
    }

    /// Wraps a field element that is already a holder secret.
    pub fn from_field(secret: pallas::Base) -> Self {
        HolderKey(secret)
    }
}

impl std::fmt::Debug for HolderKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HolderKey(redacted)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derivation_is_deterministic() {
        assert_eq!(HolderKey::from_seed(&[7; 32]), HolderKey::from_seed(&[7; 32]));
    }

    #[test]
    fn different_seeds_give_different_keys() {
        assert_ne!(HolderKey::from_seed(&[7; 32]), HolderKey::from_seed(&[8; 32]));
    }

    /// A seed that is not a canonical field element must still yield a key,
    /// rather than failing for the wallet that happened to generate it.
    #[test]
    fn a_non_canonical_seed_is_still_usable() {
        let _ = HolderKey::from_seed(&[0xFF; 32]);
    }

    #[test]
    fn the_secret_is_not_printed() {
        let debug = format!("{:?}", HolderKey::from_seed(&[7; 32]));
        assert_eq!(debug, "HolderKey(redacted)");
    }
}
