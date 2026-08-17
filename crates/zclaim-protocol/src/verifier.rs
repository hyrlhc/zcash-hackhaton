//! The verifier: asks a question, then decides whether to believe the answer.
//!
//! "The proof verified" is not the same as "the claim is true". A Halo2 proof
//! establishes that *some* note in *some* tree satisfies the predicate. Turning
//! that into a statement about Zcash takes three more checks, all of them here.

use std::collections::HashMap;

use ff::PrimeField;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zclaim_circuits::{Pool, Statement};
use zclaim_core::{Predicate, ProofRequest, VerifierDomain};
use zclaim_proof::{Proof, VerifyingKey};
use zclaim_zcash::{AnchorAuthenticator, AnchorSource, AuthenticatedAnchor};

/// What a holder hands over: the proof and the statement it is about.
///
/// This is the whole wire message. It carries no witness material — the proof
/// is a transcript of commitments and openings, and the statement is by
/// construction what the verifier is allowed to see.
#[derive(Debug, Serialize, Deserialize)]
pub struct Presentation {
    /// The Halo2 proof, hex-encoded on the wire.
    #[serde(with = "proof_hex")]
    pub proof: Proof,
    /// The public statement the proof is checked against.
    pub statement: Statement,
}

mod proof_hex {
    use super::Proof;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(p: &Proof, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&p.to_hex())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Proof, D::Error> {
        let s = String::deserialize(d)?;
        Proof::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

/// Why a presentation was not believed.
#[derive(Debug, thiserror::Error)]
pub enum Rejection {
    /// The statement does not answer the question this verifier asked.
    #[error("the answer does not match the request: {0}")]
    WrongQuestion(String),
    /// The anchor is not a root this verifier can confirm against the chain.
    #[error("anchor not authenticated: {0}")]
    Anchor(#[from] zclaim_zcash::Error),
    /// The proof did not verify.
    #[error("proof rejected: {0}")]
    Proof(#[from] zclaim_proof::Error),
    /// This payment has already been claimed at this verifier.
    #[error("this payment was already claimed (nullifier {nullifier})")]
    AlreadyClaimed {
        /// The nullifier that had been seen before.
        nullifier: String,
    },
}

/// A believed claim.
#[derive(Clone, Debug)]
pub struct Acceptance {
    /// The predicate that is now known to hold.
    pub predicate: Predicate,
    /// The chain root the payment was proved against.
    pub anchor: AuthenticatedAnchor,
    /// The payment's tag at this verifier. Stable, and meaningless elsewhere.
    pub nullifier: String,
    /// The holder's pseudonym at this verifier. Stable across their payments
    /// here, and meaningless elsewhere.
    pub holder_tag: String,
}

/// An application that asks ZClaim questions.
///
/// Holds the state a verifier legitimately needs: which roots it trusts, and
/// which claims it has already honoured. Note what it cannot hold — anything
/// that would let it recognise a holder at another verifier.
pub struct Verifier<S> {
    domain: VerifierDomain,
    pool: Pool,
    anchors: AnchorAuthenticator<S>,
    claimed: HashMap<String, Predicate>,
}

impl<S: AnchorSource> Verifier<S> {
    /// Builds a verifier for one application domain and one shielded pool.
    pub fn new(domain: VerifierDomain, pool: Pool, anchors: AnchorAuthenticator<S>) -> Self {
        Verifier {
            domain,
            pool,
            anchors,
            claimed: HashMap::new(),
        }
    }

    /// The domain this verifier scopes its questions to.
    pub fn domain(&self) -> &VerifierDomain {
        &self.domain
    }

    /// The anchor authenticator this verifier consults.
    pub fn anchors(&self) -> &AnchorAuthenticator<S> {
        &self.anchors
    }

    /// The anchor authenticator, mutably, so the verifier can keep following
    /// the chain as blocks arrive.
    pub fn anchors_mut(&mut self) -> &mut AnchorAuthenticator<S> {
        &mut self.anchors
    }

    /// Publishes a request with a fresh single-use challenge.
    pub fn request(
        &self,
        predicate: Predicate,
        purpose: impl Into<String>,
        expiry_height: u32,
        rng: &mut impl RngCore,
    ) -> ProofRequest {
        let mut nonce = [0u8; 32];
        rng.fill_bytes(&mut nonce);

        ProofRequest {
            domain: self.domain.clone(),
            nonce,
            predicate,
            purpose: purpose.into(),
            expiry_height,
        }
    }

    /// Runs every check, in the order that fails cheapest first.
    pub fn accept(
        &mut self,
        request: &ProofRequest,
        presentation: &Presentation,
    ) -> Result<Acceptance, Rejection> {
        // 1. The statement must be the one this request asked for. Skipping
        //    this would let a holder answer a question nobody posed.
        self.check_answers_the_request(request, &presentation.statement)?;

        // 2. The anchor must be a root Zcash produced, in the right pool,
        //    recently. Without this the proof is about a tree the prover made
        //    up, and says nothing about any payment.
        let anchor = self
            .anchors
            .authenticate(presentation.statement.anchor, presentation.statement.pool)?;

        // 3. The proof itself.
        presentation
            .proof
            .verify(VerifyingKey::shared(), &presentation.statement)?;

        // 4. One payment, one claim. The nullifier is stable here and unrelated
        //    anywhere else, so this is replay protection that costs no privacy.
        let nullifier = hex::encode(presentation.statement.nullifier.to_repr());
        if self.claimed.contains_key(&nullifier) {
            return Err(Rejection::AlreadyClaimed { nullifier });
        }
        self.claimed
            .insert(nullifier.clone(), request.predicate.clone());

        Ok(Acceptance {
            predicate: request.predicate.clone(),
            anchor,
            nullifier,
            holder_tag: hex::encode(presentation.statement.holder_tag.to_repr()),
        })
    }

    /// Whether a payment has already been claimed here.
    pub fn has_claimed(&self, nullifier: &str) -> bool {
        self.claimed.contains_key(nullifier)
    }

    /// Recomputes the statement this request demands and compares it against
    /// what arrived, field by field.
    ///
    /// The nullifier and holder tag are excluded: those are outputs of the
    /// proof, and the circuit derives them from witness material the verifier
    /// does not have. Everything else the verifier fixed itself.
    fn check_answers_the_request(
        &self,
        request: &ProofRequest,
        statement: &Statement,
    ) -> Result<(), Rejection> {
        let expected = Statement::from_request(
            statement.anchor,
            self.pool,
            request,
            statement.nullifier,
            statement.holder_tag,
        )
        .map_err(|e| Rejection::WrongQuestion(e.to_string()))?;

        if &expected != statement {
            return Err(Rejection::WrongQuestion(
                "the statement does not match the published request".into(),
            ));
        }

        if statement.pool != self.pool {
            return Err(Rejection::WrongQuestion(format!(
                "expected a {:?} anchor, got {:?}",
                self.pool, statement.pool
            )));
        }

        Ok(())
    }
}