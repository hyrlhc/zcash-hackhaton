//! Context binding.
//!
//! A proof that is valid anywhere is a bearer token. ZClaim binds every proof to
//! the exact question that was asked, by whom, and for which single use. The
//! binding is a field element the circuit constrains as a public input, so a
//! proof produced for one request cannot satisfy another.

use ff::FromUniformBytes;
use pasta_curves::pallas;
use serde::{Deserialize, Serialize};

use crate::predicate::Predicate;

/// Domain separator for the context hash. Changing this invalidates every
/// previously issued request, which is the intended behaviour on a break.
const CONTEXT_PERSONALIZATION: &[u8; 16] = b"ZClaim_Context_1";

/// Domain separator for the verifier tag. Distinct from the context separator
/// so the two hashes can never collide on the same input.
const DOMAIN_PERSONALIZATION: &[u8; 16] = b"ZClaim_Domain__1";

/// Identifies the application asking for a proof.
///
/// Two applications must never share a domain: the domain is what makes a
/// holder's nullifier unlinkable between them.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VerifierDomain(pub String);

impl VerifierDomain {
    /// Wraps an application identifier, e.g. a hostname.
    pub fn new(s: impl Into<String>) -> Self {
        VerifierDomain(s.into())
    }

    /// The field element that scopes nullifiers and holder tags to this
    /// verifier.
    ///
    /// Note what is *absent*: the nonce, the predicate, everything that varies
    /// between requests. That is the point. A nullifier scoped to the full
    /// context would change on every request and could never be used to spot a
    /// repeated claim; scoped to the domain alone it is stable for this
    /// verifier and unrelated to what any other verifier sees.
    pub fn tag(&self) -> DomainTag {
        let hash = blake2b_simd::Params::new()
            .hash_length(64)
            .personal(DOMAIN_PERSONALIZATION)
            .hash(self.0.as_bytes());

        let mut wide = [0u8; 64];
        wide.copy_from_slice(hash.as_bytes());
        DomainTag(pallas::Base::from_uniform_bytes(&wide))
    }
}

/// A verifier's scope, as a field element the circuit consumes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DomainTag(pub pallas::Base);

impl DomainTag {
    /// The underlying field element.
    pub fn inner(&self) -> pallas::Base {
        self.0
    }
}

/// Everything a verifier publishes when it asks for a proof.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofRequest {
    /// Which application is asking.
    pub domain: VerifierDomain,
    /// Single-use challenge. Must be freshly random per request.
    pub nonce: [u8; 32],
    /// The statement to prove.
    pub predicate: Predicate,
    /// What the answer will be used for. Bound in, so a proof issued for
    /// "age gate" cannot be replayed at "loyalty tier".
    pub purpose: String,
    /// Block height after which this request must be refused.
    pub expiry_height: u32,
}

impl ProofRequest {
    /// Derives the field element the circuit binds as its `context` public input.
    ///
    /// Every field of the request feeds the hash, so changing any of them —
    /// including the threshold — yields a different context and therefore
    /// requires a fresh proof. That property is what lets the Inference Guard
    /// see each probe as a distinct, countable event.
    pub fn context(&self) -> Context {
        let mut h = blake2b_simd::Params::new()
            .hash_length(64)
            .personal(CONTEXT_PERSONALIZATION)
            .to_state();

        let mut field = |bytes: &[u8]| {
            h.update(&(bytes.len() as u64).to_le_bytes());
            h.update(bytes);
        };

        field(self.domain.0.as_bytes());
        field(&self.nonce);
        field(self.predicate.canonical_json().as_bytes());
        field(self.purpose.as_bytes());
        field(&self.expiry_height.to_le_bytes());

        let mut wide = [0u8; 64];
        wide.copy_from_slice(h.finalize().as_bytes());
        Context(pallas::Base::from_uniform_bytes(&wide))
    }

    /// The verifier scope this request belongs to.
    pub fn domain_tag(&self) -> DomainTag {
        self.domain.tag()
    }

    /// Rejects a request whose expiry has passed.
    pub fn check_fresh(&self, current_height: u32) -> Result<(), crate::Error> {
        if current_height > self.expiry_height {
            Err(crate::Error::Expired {
                expiry: self.expiry_height,
                current: current_height,
            })
        } else {
            Ok(())
        }
    }
}

/// The context field element bound into a proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Context(pub pallas::Base);

impl Context {
    /// The underlying field element.
    pub fn inner(&self) -> pallas::Base {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::predicate::{AmountPredicate, Comparison, Merchant, Zatoshi};

    fn request(threshold: u64, domain: &str, nonce: u8) -> ProofRequest {
        ProofRequest {
            domain: VerifierDomain::new(domain),
            nonce: [nonce; 32],
            predicate: Predicate {
                merchant: Merchant {
                    label: "quantum-cafe".into(),
                    address: "aa".repeat(43),
                },
                amount: AmountPredicate {
                    operator: Comparison::Gte,
                    value: Zatoshi(threshold),
                },
            },
            purpose: "loyalty-tier".into(),
            expiry_height: 1_000_000,
        }
    }

    #[test]
    fn context_is_deterministic() {
        assert_eq!(
            request(100_000_000, "cafe.example", 1).context(),
            request(100_000_000, "cafe.example", 1).context()
        );
    }

    #[test]
    fn context_separates_verifier_domains() {
        assert_ne!(
            request(100_000_000, "app-a.example", 1).context(),
            request(100_000_000, "app-b.example", 1).context(),
            "the same question from two apps must not share a context"
        );
    }

    #[test]
    fn context_separates_nonces() {
        assert_ne!(
            request(100_000_000, "cafe.example", 1).context(),
            request(100_000_000, "cafe.example", 2).context(),
            "a replayed proof must not satisfy a fresh challenge"
        );
    }

    #[test]
    fn context_separates_thresholds() {
        assert_ne!(
            request(100_000_000, "cafe.example", 1).context(),
            request(200_000_000, "cafe.example", 1).context()
        );
    }

    #[test]
    fn context_separates_purposes() {
        let mut a = request(100_000_000, "cafe.example", 1);
        let mut b = a.clone();
        a.purpose = "age-gate".into();
        b.purpose = "loyalty-tier".into();
        assert_ne!(a.context(), b.context());
    }

    /// Length-prefixing matters: without it, `("ab", "c")` and `("a", "bc")`
    /// would hash identically and two different requests would share a context.
    #[test]
    fn field_boundaries_are_unambiguous() {
        let mut a = request(100_000_000, "app", 1);
        let mut b = request(100_000_000, "ap", 1);
        a.purpose = "x".into();
        b.purpose = "px".into();
        assert_ne!(a.context(), b.context());
    }

    /// The nullifier scope must be stable across requests, or a verifier could
    /// never recognise a repeated claim.
    #[test]
    fn the_domain_tag_ignores_everything_but_the_domain() {
        let a = request(100_000_000, "cafe.example", 1);
        let b = request(500_000_000, "cafe.example", 9);
        assert_eq!(a.domain_tag(), b.domain_tag());
        assert_ne!(
            a.context(),
            b.context(),
            "a different question is still a different context"
        );
    }

    #[test]
    fn domain_tags_separate_verifiers() {
        assert_ne!(
            VerifierDomain::new("app-a.example").tag(),
            VerifierDomain::new("app-b.example").tag()
        );
    }

    #[test]
    fn expired_requests_are_refused() {
        let r = request(100_000_000, "cafe.example", 1);
        assert!(r.check_fresh(999_999).is_ok());
        assert!(r.check_fresh(1_000_000).is_ok());
        assert!(r.check_fresh(1_000_001).is_err());
    }
}
