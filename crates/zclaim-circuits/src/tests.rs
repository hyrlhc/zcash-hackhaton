//! Circuit-level tests.
//!
//! Every test builds a genuine Orchard-protocol note, commits it with the real
//! `NoteCommit^Orchard`, and places it in a tree hashed with the real
//! `MerkleCRH^Orchard`. Only the *provenance* of the tree is local — the
//! cryptography is Zcash's.

use ff::Field;
use halo2_proofs::dev::{MockProver, VerifyFailure};
use orchard::note::ExtractedNoteCommitment;
use pasta_curves::pallas;
use zclaim_core::{DomainTag, VerifierDomain};

use crate::{
    statement::Pool,
    testing::{
        address_for, binding_for, holder, note_paying, payment_witness, tree_witness, OTHER_CAFE,
        QUANTUM_CAFE, ZEC,
    },
    NoteWitness, Statement, ZClaimCircuit, K,
};

const HOLDER: u8 = 0x11;
const OTHER_HOLDER: u8 = 0x22;

fn cafe_domain() -> DomainTag {
    VerifierDomain::new("quantum-cafe.example").tag()
}

struct Case {
    statement: Statement,
    witness: NoteWitness,
}

/// An honest prover: a real note paying `merchant_seed`, proving the comparison
/// encoded by `direction` against `threshold_zat`.
fn case(merchant_seed: u8, value_zat: u64, threshold_zat: u64, direction: i8) -> Case {
    let merchant = address_for(merchant_seed);
    let domain = cafe_domain();
    let (witness, anchor) = payment_witness(merchant, value_zat, 0x09, holder(HOLDER));

    Case {
        statement: Statement {
            anchor,
            pool: Pool::Ironwood,
            merchant: binding_for(&merchant),
            threshold: threshold_zat,
            direction,
            domain_tag: domain.inner(),
            nullifier: witness.nullifier(domain),
            holder_tag: witness.holder_tag(domain),
            context: pallas::Base::from(0xDEAD_BEEF),
        },
        witness,
    }
}

fn gte(merchant_seed: u8, value_zat: u64, threshold_zat: u64) -> Case {
    case(merchant_seed, value_zat, threshold_zat, 1)
}

fn check(c: &Case) -> Result<(), Vec<VerifyFailure>> {
    check_against(c, &c.statement)
}

/// Runs the prover's circuit against a *different* public statement, the way a
/// verifier that was handed someone else's answer would.
fn check_against(c: &Case, published: &Statement) -> Result<(), Vec<VerifyFailure>> {
    let circuit = ZClaimCircuit::new(&c.witness, &c.statement);
    MockProver::run(K, &circuit, vec![published.to_instance_column()])
        .unwrap()
        .verify()
}

// --- the headline claim -----------------------------------------------------

#[test]
fn quantum_cafe_payment_satisfies_the_predicate() {
    let c = gte(QUANTUM_CAFE, 27 * ZEC / 10, ZEC);
    assert!(check(&c).is_ok(), "2.7 ZEC must satisfy >= 1 ZEC");
}

// --- predicate correctness --------------------------------------------------

#[test]
fn gte_rejects_amounts_below_the_threshold() {
    let c = gte(QUANTUM_CAFE, 7 * ZEC / 10, ZEC);
    assert!(check(&c).is_err(), "0.7 ZEC must not satisfy >= 1 ZEC");
}

#[test]
fn gte_is_exact_at_the_boundary() {
    let exact = 27 * ZEC / 10;
    assert!(check(&gte(QUANTUM_CAFE, exact, exact)).is_ok());
    assert!(check(&gte(QUANTUM_CAFE, exact, exact + 1)).is_err());
}

#[test]
fn lte_holds_and_fails_in_the_right_directions() {
    let paid = 27 * ZEC / 10;
    assert!(check(&case(QUANTUM_CAFE, paid, 3 * ZEC, -1)).is_ok(), "2.7 <= 3");
    assert!(
        check(&case(QUANTUM_CAFE, paid, 2 * ZEC, -1)).is_err(),
        "2.7 is not <= 2"
    );
}

/// `direction` is constrained to +/-1, so a prover cannot pick a scaling factor
/// that maps an out-of-range difference into the range check.
#[test]
fn direction_cannot_be_forged() {
    let mut c = gte(QUANTUM_CAFE, 7 * ZEC / 10, ZEC);
    c.statement.direction = 0;
    assert!(check(&c).is_err(), "direction must be constrained to +/-1");
}

// --- merchant binding -------------------------------------------------------

#[test]
fn a_payment_to_another_merchant_does_not_count() {
    let mut c = gte(OTHER_CAFE, 27 * ZEC / 10, ZEC);
    c.statement.merchant = binding_for(&address_for(QUANTUM_CAFE));
    assert!(check(&c).is_err());
}

/// The whole receiver is bound, not just the transmission key. A note carrying
/// the merchant's `pk_d` under a different diversifier is one the merchant can
/// neither detect nor spend, so it must not pass as a payment to them.
#[test]
fn the_diversified_base_point_is_bound_too() {
    let mut c = gte(QUANTUM_CAFE, 27 * ZEC / 10, ZEC);

    // Keep the transmission key the note really pays; move only `g_d`. Under an
    // `x(pk_d)`-only check this would sail through.
    let foreign = binding_for(&address_for(OTHER_CAFE));
    c.statement.merchant.g_d_x = foreign.g_d_x;
    c.statement.merchant.g_d_y = foreign.g_d_y;

    assert!(check(&c).is_err(), "g_d must be bound as tightly as pk_d");
}

/// Both coordinates of each point are bound, so the negated point — which is on
/// the curve and shares an x-coordinate — is not an accepted substitute.
#[test]
fn a_negated_transmission_key_is_not_the_merchant() {
    let mut c = gte(QUANTUM_CAFE, 27 * ZEC / 10, ZEC);
    c.statement.merchant.pk_d_y = -c.statement.merchant.pk_d_y;
    assert!(check(&c).is_err());
}

// --- witness integrity ------------------------------------------------------

#[test]
fn inflating_the_amount_breaks_the_commitment() {
    let mut c = gte(QUANTUM_CAFE, 27 * ZEC / 10, ZEC);
    c.witness.value = 100 * ZEC;
    assert!(
        check(&c).is_err(),
        "a value that disagrees with the on-chain commitment must not verify"
    );
}

#[test]
fn a_note_outside_the_tree_does_not_verify() {
    let mut c = gte(QUANTUM_CAFE, 27 * ZEC / 10, ZEC);
    c.statement.anchor += pallas::Base::ONE;
    assert!(check(&c).is_err());
}

#[test]
fn a_forged_merkle_path_does_not_reach_the_anchor() {
    let mut c = gte(QUANTUM_CAFE, 27 * ZEC / 10, ZEC);
    c.witness.merkle_path[0] += pallas::Base::ONE;
    assert!(check(&c).is_err());
}

// --- nullifier, holder tag and context binding ------------------------------

#[test]
fn the_nullifier_is_bound_to_the_note() {
    let mut c = gte(QUANTUM_CAFE, 27 * ZEC / 10, ZEC);
    c.statement.nullifier += pallas::Base::ONE;
    assert!(
        check(&c).is_err(),
        "a prover must not be able to choose its nullifier freely"
    );
}

#[test]
fn the_holder_tag_is_bound_to_the_holder_secret() {
    let mut c = gte(QUANTUM_CAFE, 27 * ZEC / 10, ZEC);
    c.statement.holder_tag += pallas::Base::ONE;
    assert!(check(&c).is_err());
}

/// A stolen witness is not a credential. Someone who obtains the note material
/// but not the holder secret cannot reproduce the tag the verifier has been
/// seeing, so the claim arrives under a different, obviously new identity.
#[test]
fn a_stolen_witness_cannot_impersonate_the_holder() {
    let c = gte(QUANTUM_CAFE, 27 * ZEC / 10, ZEC);

    let mut thief = Case {
        witness: c.witness.clone(),
        statement: c.statement.clone(),
    };
    thief.witness.holder = holder(OTHER_HOLDER);

    assert!(
        check(&thief).is_err(),
        "the legitimate holder's tag must be unreachable without their secret"
    );
}

/// The tags must be scoped to the verifier that asked, not one the prover
/// prefers. `domain_tag` is public precisely so this cannot be swapped.
#[test]
fn tags_cannot_be_scoped_to_a_domain_of_the_provers_choosing() {
    let mut c = gte(QUANTUM_CAFE, 27 * ZEC / 10, ZEC);
    c.statement.domain_tag = VerifierDomain::new("elsewhere.example").tag().inner();
    assert!(check(&c).is_err());
}

/// An answer given to one request must not satisfy another. The prover here
/// builds its circuit for the request it was asked, and the answer is then held
/// up against a different one.
#[test]
fn an_answer_does_not_satisfy_a_different_request() {
    let c = gte(QUANTUM_CAFE, 27 * ZEC / 10, ZEC);
    assert!(check(&c).is_ok());

    let mut different_request = c.statement.clone();
    different_request.context = pallas::Base::from(0x00B0_B0B0);

    assert!(
        check_against(&c, &different_request).is_err(),
        "context binding must make an answer single-use"
    );
}

#[test]
fn two_notes_yield_two_nullifiers() {
    let merchant = address_for(QUANTUM_CAFE);
    let domain = cafe_domain();

    let (a, _) = payment_witness(merchant, 27 * ZEC / 10, 0x09, holder(HOLDER));
    let (b, _) = payment_witness(merchant, 27 * ZEC / 10, 0x0A, holder(HOLDER));

    assert_ne!(
        a.nullifier(domain),
        b.nullifier(domain),
        "two distinct payments must not collapse to one claim"
    );
}

#[test]
fn one_note_yields_unlinkable_nullifiers_across_verifiers() {
    let (w, _) = payment_witness(
        address_for(QUANTUM_CAFE),
        27 * ZEC / 10,
        0x09,
        holder(HOLDER),
    );

    assert_ne!(
        w.nullifier(VerifierDomain::new("app-a.example").tag()),
        w.nullifier(VerifierDomain::new("app-b.example").tag()),
        "two applications must not be able to correlate a payment"
    );
}

/// The same holder shows one verifier a stable pseudonym, and every other
/// verifier an unrelated one.
#[test]
fn a_holder_tag_is_stable_per_verifier_and_unlinkable_across_them() {
    let merchant = address_for(QUANTUM_CAFE);
    let cafe = cafe_domain();
    let elsewhere = VerifierDomain::new("elsewhere.example").tag();

    let (first, _) = payment_witness(merchant, 27 * ZEC / 10, 0x09, holder(HOLDER));
    let (second, _) = payment_witness(merchant, 5 * ZEC, 0x0A, holder(HOLDER));

    assert_eq!(
        first.holder_tag(cafe),
        second.holder_tag(cafe),
        "one holder is one pseudonym within a verifier, across payments"
    );
    assert_ne!(
        first.holder_tag(cafe),
        first.holder_tag(elsewhere),
        "the same holder must look unrelated to a second verifier"
    );
}

#[test]
fn two_holders_are_two_pseudonyms() {
    let merchant = address_for(QUANTUM_CAFE);
    let domain = cafe_domain();

    let (a, _) = payment_witness(merchant, 27 * ZEC / 10, 0x09, holder(HOLDER));
    let (b, _) = payment_witness(merchant, 27 * ZEC / 10, 0x09, holder(OTHER_HOLDER));

    assert_ne!(a.holder_tag(domain), b.holder_tag(domain));
}

/// The same note at the same verifier always yields the same nullifier, which
/// is what lets a verifier reject a repeated claim — regardless of how the
/// question was worded or which nonce it carried.
#[test]
fn the_same_claim_is_recognisable() {
    let note = note_paying(address_for(QUANTUM_CAFE), 27 * ZEC / 10, 0x09);
    let (path, _) = tree_witness(ExtractedNoteCommitment::from(note.commitment()));
    let domain = cafe_domain();

    assert_eq!(
        NoteWitness::from_note(&note, holder(HOLDER), path, 0).nullifier(domain),
        NoteWitness::from_note(&note, holder(HOLDER), path, 0).nullifier(domain)
    );
}
