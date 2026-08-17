//! The holder: decides whether to answer, then answers.
//!
//! Two things happen here and the order matters. The guard runs *first*, on the
//! question alone, and can refuse. Only if it allows does the holder look at
//! the amount. Deciding on the amount first and consulting the guard afterwards
//! would leak: a refusal would then depend on the answer.

use ff::PrimeField;
use pasta_curves::pallas;
use rand::RngCore;
use zclaim_circuits::{NoteWitness, Pool, Statement};
use zclaim_core::{HolderKey, ProofRequest, Zatoshi};
use zclaim_inference::{Decision, Guard, Knowledge, SubjectKey};
use zclaim_proof::{Proof, ProvingKey};

use crate::{verifier::Presentation, Error};

/// Why the holder did not answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The Inference Guard judged the question too revealing.
    Guarded(Decision),
    /// The request has passed its expiry height.
    Expired(String),
    /// The claim is simply false. The holder says so rather than attempting a
    /// proof that could not verify.
    ClaimIsFalse,
}

impl Refusal {
    /// A line safe to show the verifier: it depends only on the question and on
    /// history the verifier already has.
    pub fn describe(&self) -> String {
        match self {
            Refusal::Guarded(Decision::Block { reason }) => reason.clone(),
            Refusal::Guarded(other) => format!("refused ({})", other.label()),
            Refusal::Expired(e) => e.clone(),
            Refusal::ClaimIsFalse => "the claim does not hold".to_string(),
        }
    }
}

/// What the holder returns for a request.
///
/// The answer variant is boxed: a presentation carries several kilobytes of
/// proof, and an unboxed enum would make every refusal that size too.
#[derive(Debug)]
pub enum Response {
    /// A proof, plus the guard's verdict so a wallet UI can surface it.
    Answer {
        /// The presentation to hand the verifier.
        presentation: Box<Presentation>,
        /// The guard's verdict. May be [`Decision::Warning`].
        decision: Decision,
    },
    /// No proof.
    Refused(Refusal),
}

/// A holder of one shielded payment, answering questions about it.
///
/// Owns exactly two secrets — the note witness and the holder key — and neither
/// ever leaves this struct.
pub struct Holder {
    witness: NoteWitness,
    anchor: pallas::Base,
    pool: Pool,
    guard: Guard,
}

impl Holder {
    /// Takes custody of a witness for a payment sitting under `anchor`.
    pub fn new(witness: NoteWitness, anchor: pallas::Base, pool: Pool, guard: Guard) -> Self {
        Holder {
            witness,
            anchor,
            pool,
            guard,
        }
    }

    /// The holder key in use. Exposed so a wallet can persist it; never send it.
    pub fn holder_key(&self) -> HolderKey {
        self.witness.holder
    }

    /// What the guard has recorded so far.
    pub fn guard(&self) -> &Guard {
        &self.guard
    }

    /// Runs the guard without answering, for a consent dialog.
    pub fn screen(&self, request: &ProofRequest) -> Decision {
        self.guard
            .evaluate(&self.subject(request), &request.predicate)
    }

    /// What the asking verifier has been able to work out so far.
    ///
    /// This is the holder's own estimate of the leak, and is what a wallet
    /// should show its user: "this shop can tell your payment was between 2 and
    /// 4 ZEC".
    pub fn exposure(&self, request: &ProofRequest) -> Knowledge {
        self.guard.knowledge(&self.subject(request))
    }

    /// Answers a request, or refuses it.
    pub fn respond(
        &mut self,
        request: &ProofRequest,
        chain_height: u32,
        pk: &ProvingKey,
        rng: impl RngCore,
    ) -> Result<Response, Error> {
        if let Err(e) = request.check_fresh(chain_height) {
            return Ok(Response::Refused(Refusal::Expired(e.to_string())));
        }

        let subject = self.subject(request);
        let decision = self.guard.evaluate(&subject, &request.predicate);
        if !decision.is_allowed() {
            return Ok(Response::Refused(Refusal::Guarded(decision)));
        }

        // The guard has cleared the question, so it is going to be answered one
        // way or the other. Being unable to prove a claim *is* the answer "no":
        // the verifier learns the same thing it would learn from a signed
        // denial. So the outcome is recorded before it is acted on, or the
        // guard's interval would drift away from what the verifier knows.
        let holds = self.claim_holds(request);
        self.guard.record(&subject, &request.predicate, holds);

        if !holds {
            return Ok(Response::Refused(Refusal::ClaimIsFalse));
        }

        let domain = request.domain_tag();
        let statement = Statement::from_request(
            self.anchor,
            self.pool,
            request,
            self.witness.nullifier(domain),
            self.witness.holder_tag(domain),
        )?;

        let proof = Proof::create(pk, &self.witness, &statement, rng)?;

        Ok(Response::Answer {
            presentation: Box::new(Presentation { proof, statement }),
            decision,
        })
    }

    /// Whether the predicate is true of the payment this holder is sitting on.
    fn claim_holds(&self, request: &ProofRequest) -> bool {
        request
            .predicate
            .amount
            .operator
            .holds(Zatoshi(self.witness.value), request.predicate.amount.value)
    }

    /// The guard's key for this verifier and this payment.
    ///
    /// The nullifier is already domain-scoped, so a holder answering two
    /// verifiers keeps two histories that cannot be joined.
    fn subject(&self, request: &ProofRequest) -> SubjectKey {
        SubjectKey {
            domain: request.domain.clone(),
            nullifier: hex::encode(self.witness.nullifier(request.domain_tag()).to_repr()),
        }
    }
}
