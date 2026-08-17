//! The exchange, end to end, with real proofs on both sides.

use rand::rngs::OsRng;
use zclaim_circuits::{
    testing::{address_for, holder, merchant_named, payment_witness, OTHER_CAFE, QUANTUM_CAFE, ZEC},
    Pool,
};
use zclaim_core::{AmountPredicate, Comparison, Predicate, VerifierDomain, Zatoshi};
use zclaim_inference::{Decision, Guard, Policy};
use zclaim_proof::ProvingKey;
use zclaim_zcash::{AnchorAuthenticator, RootWindow};

use crate::{Holder, Presentation, Refusal, Rejection, Response, Verifier};

const TIP: u32 = 3_500_000;
const PAID: u64 = 27 * ZEC / 10;

/// A holder sitting on a 2.7 ZEC payment to Quantum Cafe, and a verifier whose
/// node has seen the anchor that payment is under.
fn scene() -> (Holder, Verifier<RootWindow>) {
    let merchant = address_for(QUANTUM_CAFE);
    let (witness, anchor) = payment_witness(merchant, PAID, 0x09, holder(0x11));

    let mut roots = RootWindow::new();
    roots.observe(anchor, Pool::Ironwood, TIP);

    (
        Holder::new(witness, anchor, Pool::Ironwood, Guard::new(Policy::demo())),
        Verifier::new(
            VerifierDomain::new("quantum-cafe.example"),
            Pool::Ironwood,
            AnchorAuthenticator::new(roots, 100),
        ),
    )
}

fn at_least(zat: u64) -> Predicate {
    Predicate {
        merchant: merchant_named("quantum-cafe", QUANTUM_CAFE),
        amount: AmountPredicate {
            operator: Comparison::Gte,
            value: Zatoshi(zat),
        },
    }
}

fn answer(h: &mut Holder, request: &zclaim_core::ProofRequest) -> Response {
    h.respond(request, TIP, ProvingKey::shared(), OsRng)
        .expect("responding must not error")
}

fn presentation(response: Response) -> Presentation {
    match response {
        Response::Answer { presentation, .. } => *presentation,
        Response::Refused(r) => panic!("expected an answer, got refusal: {}", r.describe()),
    }
}

/// The demo: the verifier learns the predicate holds and nothing else.
#[test]
fn the_headline_exchange_works() {
    let (mut alice, mut cafe) = scene();
    let request = cafe.request(at_least(ZEC), "loyalty-tier", TIP + 100, &mut OsRng);

    let p = presentation(answer(&mut alice, &request));
    let accepted = cafe.accept(&request, &p).expect("an honest claim is believed");

    assert_eq!(accepted.predicate, request.predicate);
    assert_eq!(accepted.anchor.pool, Pool::Ironwood);
}

/// A verifier whose node has never seen the anchor must refuse, even though the
/// proof itself is perfectly valid. This is the check that keeps ZClaim tied to
/// Zcash rather than to a tree the prover invented.
#[test]
fn a_proof_against_an_unknown_root_is_refused() {
    let (mut alice, _) = scene();
    let mut blind = Verifier::new(
        VerifierDomain::new("quantum-cafe.example"),
        Pool::Ironwood,
        AnchorAuthenticator::new(RootWindow::new(), 100),
    );

    let request = blind.request(at_least(ZEC), "loyalty-tier", TIP + 100, &mut OsRng);
    let p = presentation(answer(&mut alice, &request));

    assert!(matches!(
        blind.accept(&request, &p),
        Err(Rejection::Anchor(_))
    ));
}

/// One payment, one claim.
#[test]
fn the_same_payment_cannot_be_claimed_twice() {
    let (mut alice, mut cafe) = scene();

    let first = cafe.request(at_least(ZEC), "loyalty-tier", TIP + 100, &mut OsRng);
    let p1 = presentation(answer(&mut alice, &first));
    cafe.accept(&first, &p1).unwrap();

    let second = cafe.request(at_least(ZEC), "loyalty-tier", TIP + 100, &mut OsRng);
    let p2 = presentation(answer(&mut alice, &second));

    assert!(matches!(
        cafe.accept(&second, &p2),
        Err(Rejection::AlreadyClaimed { .. })
    ));
}

/// An answer produced for one application must not satisfy another, even when
/// both ask exactly the same question.
#[test]
fn an_answer_does_not_travel_between_applications() {
    let (mut alice, cafe) = scene();
    let request = cafe.request(at_least(ZEC), "loyalty-tier", TIP + 100, &mut OsRng);
    let p = presentation(answer(&mut alice, &request));

    let mut elsewhere = Verifier::new(
        VerifierDomain::new("insurance.example"),
        Pool::Ironwood,
        AnchorAuthenticator::new(
            {
                let mut roots = RootWindow::new();
                roots.observe(p.statement.anchor, Pool::Ironwood, TIP);
                roots
            },
            100,
        ),
    );

    let their_request = elsewhere.request(at_least(ZEC), "loyalty-tier", TIP + 100, &mut OsRng);
    assert!(elsewhere.accept(&their_request, &p).is_err());
}

#[test]
fn a_question_about_another_merchant_is_not_answered() {
    let (mut alice, cafe) = scene();
    let predicate = Predicate {
        merchant: merchant_named("other-cafe", OTHER_CAFE),
        amount: AmountPredicate {
            operator: Comparison::Gte,
            value: Zatoshi(ZEC),
        },
    };
    let request = cafe.request(predicate, "loyalty-tier", TIP + 100, &mut OsRng);

    // The holder attempts it — the payment really is >= 1 ZEC — and the proof
    // it produces cannot verify, because the note pays a different merchant.
    match answer(&mut alice, &request) {
        Response::Answer { presentation, .. } => {
            let mut cafe = cafe;
            assert!(cafe.accept(&request, &presentation).is_err());
        }
        Response::Refused(_) => {}
    }
}

#[test]
fn an_expired_request_is_refused() {
    let (mut alice, cafe) = scene();
    let request = cafe.request(at_least(ZEC), "loyalty-tier", TIP - 1, &mut OsRng);

    assert!(matches!(
        answer(&mut alice, &request),
        Response::Refused(Refusal::Expired(_))
    ));
}

#[test]
fn a_false_claim_is_refused_rather_than_attempted() {
    let (mut alice, cafe) = scene();
    let request = cafe.request(at_least(100 * ZEC), "loyalty-tier", TIP + 100, &mut OsRng);

    assert!(matches!(
        answer(&mut alice, &request),
        Response::Refused(Refusal::ClaimIsFalse)
    ));
}

/// The attack the project exists to stop: bisecting the hidden amount by asking
/// progressively tighter thresholds. The guard must cut it off while the
/// verifier still knows only a coarse range.
#[test]
fn threshold_probing_is_cut_off_before_the_amount_is_pinned() {
    let (mut alice, cafe) = scene();

    let probes = [ZEC, 2 * ZEC, 25 * ZEC / 10, 26 * ZEC / 10, 27 * ZEC / 10];
    let mut blocked_at = None;

    for (i, threshold) in probes.iter().enumerate() {
        let request = cafe.request(at_least(*threshold), "loyalty-tier", TIP + 100, &mut OsRng);
        if let Response::Refused(Refusal::Guarded(Decision::Block { .. })) =
            answer(&mut alice, &request)
        {
            blocked_at = Some(i);
            break;
        }
    }

    let blocked_at = blocked_at.expect("the guard must eventually refuse");
    assert!(
        blocked_at < probes.len() - 1,
        "the guard must refuse before the final probe pins the amount"
    );

    // And what the verifier ended up knowing is still coarse.
    let request = cafe.request(at_least(ZEC), "loyalty-tier", TIP + 100, &mut OsRng);
    let known = alice.screen(&request);
    assert!(known.is_allowed(), "a question already answered stays answerable");
}

/// A claim the holder cannot prove is still an answer: the verifier learns
/// "no". The guard has to count it, or its picture of what the verifier knows
/// drifts and the probing defence stops working.
#[test]
fn an_unprovable_claim_still_narrows_what_the_guard_believes_is_known() {
    let (mut alice, cafe) = scene();

    let low = cafe.request(at_least(2 * ZEC), "loyalty-tier", TIP + 100, &mut OsRng);
    let _ = answer(&mut alice, &low);

    let high = cafe.request(at_least(4 * ZEC), "loyalty-tier", TIP + 100, &mut OsRng);
    assert!(matches!(
        answer(&mut alice, &high),
        Response::Refused(Refusal::ClaimIsFalse)
    ));

    // The verifier now knows 2 <= amount < 4, and the guard knows that it knows.
    let exposure = alice.exposure(&high);
    assert_eq!(exposure.low, 2 * ZEC);
    assert_eq!(exposure.high, 4 * ZEC - 1);
}

/// The holder and the verifier are separate programs, so everything between
/// them has to survive JSON. This is the format the TypeScript SDK speaks.
#[test]
fn the_whole_exchange_survives_json() {
    let (mut alice, mut cafe) = scene();
    let request = cafe.request(at_least(ZEC), "loyalty-tier", TIP + 100, &mut OsRng);

    let over_the_wire = serde_json::to_string(&request).unwrap();
    let request: zclaim_core::ProofRequest = serde_json::from_str(&over_the_wire).unwrap();

    let p = presentation(answer(&mut alice, &request));
    let over_the_wire = serde_json::to_string(&p).unwrap();
    let p: Presentation = serde_json::from_str(&over_the_wire).unwrap();

    cafe.accept(&request, &p)
        .expect("a claim that went through JSON is still believed");
}

/// A presentation is the one message that crosses the trust boundary, so it
/// must not carry anything the verifier is not entitled to.
#[test]
fn a_serialised_presentation_carries_only_public_data() {
    let (mut alice, cafe) = scene();
    let request = cafe.request(at_least(ZEC), "loyalty-tier", TIP + 100, &mut OsRng);
    let p = presentation(answer(&mut alice, &request));

    let json: serde_json::Value = serde_json::to_value(&p).unwrap();
    let keys: Vec<&String> = json["statement"].as_object().unwrap().keys().collect();

    assert_eq!(
        keys,
        [
            "anchor",
            "context",
            "direction",
            "domainTag",
            "holderTag",
            "merchant",
            "nullifier",
            "pool",
            "threshold",
        ]
        .iter()
        .collect::<Vec<_>>(),
        "the wire statement gained or lost a field; check it is still public"
    );

    let text = serde_json::to_string(&p).unwrap();
    assert!(
        !text.contains(&hex::encode(PAID.to_le_bytes())),
        "the amount must not appear on the wire in any form"
    );
}

/// Tampering with the statement after the proof was made must not be repairable
/// by re-encoding it.
#[test]
fn a_statement_edited_in_transit_is_rejected() {
    let (mut alice, mut cafe) = scene();
    let request = cafe.request(at_least(ZEC), "loyalty-tier", TIP + 100, &mut OsRng);
    let p = presentation(answer(&mut alice, &request));

    let mut json: serde_json::Value = serde_json::to_value(&p).unwrap();
    json["statement"]["threshold"] = serde_json::json!("500000000");
    let tampered: Presentation = serde_json::from_value(json).unwrap();

    assert!(
        matches!(
            cafe.accept(&request, &tampered),
            Err(Rejection::WrongQuestion(_))
        ),
        "a raised threshold is not the question that was asked"
    );
}
