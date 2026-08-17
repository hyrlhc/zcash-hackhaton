//! End-to-end proving tests: real Halo2 proofs, not constraint satisfaction.

use pasta_curves::pallas;
use rand::rngs::OsRng;
use zclaim_circuits::{
    statement::Pool,
    testing::{
        address_for, binding_for, holder, merchant_named, payment_witness, OTHER_CAFE,
        QUANTUM_CAFE, ZEC,
    },
    NoteWitness, Statement,
};
use zclaim_core::{
    AmountPredicate, Comparison, DomainTag, Predicate, ProofRequest, VerifierDomain, Zatoshi,
};

use crate::{Proof, ProvingKey, VerifyingKey};

const HOLDER: u8 = 0x11;
const OTHER_HOLDER: u8 = 0x22;

fn cafe_domain() -> DomainTag {
    VerifierDomain::new("quantum-cafe.example").tag()
}

fn honest(value_zat: u64, threshold_zat: u64, direction: i8) -> (NoteWitness, Statement) {
    let merchant = address_for(QUANTUM_CAFE);
    let domain = cafe_domain();
    let (witness, anchor) = payment_witness(merchant, value_zat, 0x09, holder(HOLDER));

    let statement = Statement {
        anchor,
        pool: Pool::Ironwood,
        merchant: binding_for(&merchant),
        threshold: threshold_zat,
        direction,
        domain_tag: domain.inner(),
        nullifier: witness.nullifier(domain),
        holder_tag: witness.holder_tag(domain),
        context: pallas::Base::from(0xDEAD_BEEF),
    };
    (witness, statement)
}

/// The demo, end to end: 2.7 ZEC paid, `>= 1 ZEC` proved, verifier accepts.
#[test]
fn honest_proof_verifies() {
    let (witness, statement) = honest(27 * ZEC / 10, ZEC, 1);

    let proof = Proof::create(ProvingKey::shared(), &witness, &statement, OsRng)
        .expect("an honest witness must prove");

    assert!(proof.verify(VerifyingKey::shared(), &statement).is_ok());
}

/// A payment that does not clear the bar yields nothing a verifier will accept.
///
/// Note what is *not* asserted: that proving fails. `halo2_proofs::create_proof`
/// does not evaluate the constraint system the way `MockProver` does, so a
/// prover with an unsatisfying witness may well get bytes back. Those bytes are
/// simply not a proof. The security property lives entirely in verification,
/// which is why this test checks verification rather than proving.
#[test]
fn a_false_claim_does_not_verify() {
    let (witness, statement) = honest(7 * ZEC / 10, ZEC, 1);

    let outcome = Proof::create(ProvingKey::shared(), &witness, &statement, OsRng);

    if let Ok(proof) = outcome {
        assert!(
            proof.verify(VerifyingKey::shared(), &statement).is_err(),
            "0.7 ZEC must not yield an accepted proof of >= 1 ZEC"
        );
    }
}

/// A proof of `>= 1 ZEC` is not a proof of `>= 3 ZEC`. This is what stops a
/// verifier from re-reading one answer as a stronger one.
#[test]
fn a_proof_does_not_upgrade_to_a_stronger_threshold() {
    let (witness, statement) = honest(27 * ZEC / 10, ZEC, 1);
    let proof = Proof::create(ProvingKey::shared(), &witness, &statement, OsRng).unwrap();

    let mut stronger = statement.clone();
    stronger.threshold = 3 * ZEC;

    assert!(proof.verify(VerifyingKey::shared(), &stronger).is_err());
}

/// Replay across applications must fail. This is the property that makes a
/// ZClaim proof not a bearer token.
#[test]
fn a_proof_does_not_replay_into_another_application() {
    let (witness, statement) = honest(27 * ZEC / 10, ZEC, 1);
    let proof = Proof::create(ProvingKey::shared(), &witness, &statement, OsRng).unwrap();

    let mut other_app = statement.clone();
    other_app.context = pallas::Base::from(0x00B0_B0B0);

    assert!(proof.verify(VerifyingKey::shared(), &other_app).is_err());
}

#[test]
fn a_proof_does_not_transfer_to_another_merchant() {
    let (witness, statement) = honest(27 * ZEC / 10, ZEC, 1);
    let proof = Proof::create(ProvingKey::shared(), &witness, &statement, OsRng).unwrap();

    let mut other = statement.clone();
    other.merchant = binding_for(&address_for(OTHER_CAFE));

    assert!(proof.verify(VerifyingKey::shared(), &other).is_err());
}

/// Holder binding, at the level that matters: someone who has the note witness
/// but not the holder secret cannot produce a proof under the holder's tag.
#[test]
fn a_leaked_witness_does_not_let_a_thief_prove_as_the_holder() {
    let (witness, statement) = honest(27 * ZEC / 10, ZEC, 1);

    let mut thief = witness.clone();
    thief.holder = holder(OTHER_HOLDER);

    let forged = Proof::create(ProvingKey::shared(), &thief, &statement, OsRng);
    if let Ok(proof) = forged {
        assert!(
            proof.verify(VerifyingKey::shared(), &statement).is_err(),
            "the holder's tag must be unreachable without their secret"
        );
    }
}

#[test]
fn a_tampered_proof_is_rejected() {
    let (witness, statement) = honest(27 * ZEC / 10, ZEC, 1);
    let proof = Proof::create(ProvingKey::shared(), &witness, &statement, OsRng).unwrap();

    let mut bytes = proof.as_bytes().to_vec();
    bytes[0] ^= 0x01;

    assert!(Proof::from_bytes(bytes)
        .verify(VerifyingKey::shared(), &statement)
        .is_err());
}

#[test]
fn proofs_survive_hex_transport() {
    let (witness, statement) = honest(27 * ZEC / 10, ZEC, 1);
    let proof = Proof::create(ProvingKey::shared(), &witness, &statement, OsRng).unwrap();

    let round_tripped = Proof::from_hex(&proof.to_hex()).unwrap();
    assert_eq!(proof, round_tripped);
    assert!(round_tripped
        .verify(VerifyingKey::shared(), &statement)
        .is_ok());
}

/// A proof carries no witness material. The amount, in particular, must not be
/// recoverable by scanning the bytes for it.
#[test]
fn the_proof_does_not_contain_the_amount() {
    let amount = 27 * ZEC / 10;
    let (witness, statement) = honest(amount, ZEC, 1);
    let proof = Proof::create(ProvingKey::shared(), &witness, &statement, OsRng).unwrap();

    let bytes = proof.as_bytes();
    for encoding in [
        amount.to_le_bytes().to_vec(),
        amount.to_be_bytes().to_vec(),
    ] {
        assert!(
            !bytes.windows(encoding.len()).any(|w| w == encoding),
            "the hidden amount must not appear verbatim in the proof"
        );
    }
}

/// The whole path a verifier SDK walks: publish a request, build the statement
/// from it, prove, verify.
#[test]
fn request_to_verification_round_trip() {
    let merchant = address_for(QUANTUM_CAFE);
    let (witness, anchor) = payment_witness(merchant, 27 * ZEC / 10, 0x09, holder(HOLDER));

    let request = ProofRequest {
        domain: VerifierDomain::new("quantum-cafe.example"),
        nonce: [0x42; 32],
        predicate: Predicate {
            merchant: merchant_named("quantum-cafe", QUANTUM_CAFE),
            amount: AmountPredicate {
                operator: Comparison::Gte,
                value: Zatoshi::ZEC,
            },
        },
        purpose: "loyalty-tier".into(),
        expiry_height: 3_500_000,
    };

    let domain = request.domain_tag();
    let statement = Statement::from_request(
        anchor,
        Pool::Ironwood,
        &request,
        witness.nullifier(domain),
        witness.holder_tag(domain),
    )
    .expect("the request is well formed");

    let proof = Proof::create(ProvingKey::shared(), &witness, &statement, OsRng).unwrap();
    assert!(proof.verify(VerifyingKey::shared(), &statement).is_ok());

    // A different nonce is a different question and needs a different proof.
    let mut replayed = request.clone();
    replayed.nonce = [0x43; 32];
    let replayed_statement = Statement::from_request(
        anchor,
        Pool::Ironwood,
        &replayed,
        witness.nullifier(domain),
        witness.holder_tag(domain),
    )
    .unwrap();

    assert!(proof
        .verify(VerifyingKey::shared(), &replayed_statement)
        .is_err());
}

/// The nullifier a verifier records must not move when the question changes,
/// or a repeated claim on one payment would be unrecognisable.
#[test]
fn one_payment_shows_one_nullifier_however_the_question_is_worded() {
    let (witness, first) = honest(27 * ZEC / 10, ZEC, 1);
    let (_, second) = honest(27 * ZEC / 10, 2 * ZEC, 1);

    assert_eq!(first.nullifier, second.nullifier);
    assert_eq!(first.holder_tag, second.holder_tag);

    // Both are still real, separately provable claims.
    let proof = Proof::create(ProvingKey::shared(), &witness, &second, OsRng).unwrap();
    assert!(proof.verify(VerifyingKey::shared(), &second).is_ok());
}
