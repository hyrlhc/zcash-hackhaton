//! Guard tests, centred on the attack the guard exists to stop.

use zclaim_core::{
    AmountPredicate, Comparison, Merchant, Predicate, VerifierDomain, Zatoshi,
};

use crate::{Decision, Guard, Knowledge, Policy, SubjectKey};

const ZEC: u64 = 100_000_000;

/// The amount the holder actually paid. Known to the test, never to the guard.
const HIDDEN: u64 = 27 * ZEC / 10;

fn subject() -> SubjectKey {
    SubjectKey {
        domain: VerifierDomain::new("quantum-cafe.example"),
        nullifier: "ab".repeat(32),
    }
}

fn ask(threshold_zat: u64) -> Predicate {
    Predicate {
        merchant: Merchant {
            label: "quantum-cafe".into(),
            address: "aa".repeat(43),
        },
        amount: AmountPredicate {
            operator: Comparison::Gte,
            value: Zatoshi(threshold_zat),
        },
    }
}

/// What an honest prover would answer.
fn truth(p: &Predicate) -> bool {
    p.amount.operator.holds(Zatoshi(HIDDEN), p.amount.value)
}

// --- interval arithmetic ----------------------------------------------------

#[test]
fn answers_narrow_the_interval_the_way_they_should() {
    let k = Knowledge::unconstrained();

    let k = k.after(Comparison::Gte, Zatoshi(2 * ZEC), true);
    assert_eq!(k.low, 2 * ZEC, ">= 2 and true means at least 2");

    let k = k.after(Comparison::Gte, Zatoshi(3 * ZEC), false);
    assert_eq!(k.high, 3 * ZEC - 1, ">= 3 and false means at most 3 - 1");

    let k = k.after(Comparison::Lte, Zatoshi(28 * ZEC / 10), true);
    assert_eq!(k.high, 28 * ZEC / 10);

    assert!(!k.is_contradictory());
    assert!(k.low <= HIDDEN && HIDDEN <= k.high, "truth stays in the interval");
}

#[test]
fn an_exact_interval_is_recognised() {
    let k = Knowledge {
        low: HIDDEN,
        high: HIDDEN,
    };
    assert!(k.is_exact());
    assert_eq!(k.width(), 1);
    assert_eq!(k.describe(), "exactly 2.70000000");
}

// --- the attack -------------------------------------------------------------

/// The scenario from the demo. A verifier walks thresholds toward the hidden
/// amount; the guard must refuse before the amount is pinned down.
#[test]
fn threshold_probing_is_blocked_before_the_amount_is_revealed() {
    let mut guard = Guard::new(Policy::demo());
    let subject = subject();

    let probes = [
        ZEC,
        2 * ZEC,
        25 * ZEC / 10,
        26 * ZEC / 10,
        27 * ZEC / 10,
        28 * ZEC / 10,
    ];

    let mut blocked_at = None;
    for (i, threshold) in probes.iter().enumerate() {
        let p = ask(*threshold);
        let decision = guard.evaluate_and_record(&subject, &p, truth(&p));

        if let Decision::Block { .. } = decision {
            blocked_at = Some(i);
            break;
        }
    }

    let blocked_at = blocked_at.expect("the guard must refuse before the probing completes");
    assert!(
        blocked_at < probes.len(),
        "probing reached the end without being stopped"
    );

    let known = guard.knowledge(&subject);
    assert!(
        !known.is_exact(),
        "the guard let the verifier pin the amount to {}",
        known.describe()
    );
    assert!(
        known.width() >= guard.policy().granularity as u128,
        "surviving interval {} is narrower than the policy floor",
        known.describe()
    );
}

/// The first, legitimate question must be answered. A guard that refuses
/// everything is not a guard, it is an outage.
#[test]
fn the_first_honest_question_is_answered() {
    let guard = Guard::new(Policy::demo());
    let decision = guard.evaluate(&subject(), &ask(ZEC));

    assert!(matches!(decision, Decision::Safe { .. }), "{decision:?}");
}

#[test]
fn the_guard_warns_before_it_blocks() {
    let mut guard = Guard::new(Policy::demo());
    let subject = subject();

    let mut labels = Vec::new();
    for threshold in [ZEC, 2 * ZEC, 25 * ZEC / 10, 26 * ZEC / 10, 27 * ZEC / 10] {
        let p = ask(threshold);
        let d = guard.evaluate_and_record(&subject, &p, truth(&p));
        labels.push(d.label());
        if d.label() == "BLOCK" {
            break;
        }
    }

    assert_eq!(labels.first(), Some(&"SAFE"));
    assert!(
        labels.contains(&"WARNING"),
        "the holder should see it coming: {labels:?}"
    );
    assert_eq!(labels.last(), Some(&"BLOCK"), "{labels:?}");
}

// --- the guard must not itself leak ----------------------------------------

/// A refusal must not itself be an oracle.
///
/// The guard judges the *narrower* of the two intervals the pending question
/// could produce, so it refuses whenever either answer would leak too much —
/// not only when the real answer would. A verifier therefore cannot read the
/// answer off the refusal.
#[test]
fn a_refusal_does_not_reveal_which_way_the_answer_went() {
    let subject = subject();

    // A history that leaves the amount at `>= 2.5 ZEC`, then a question whose
    // "yes" branch stays wide open and whose "no" branch would pin it tightly.
    let mut guard = Guard::new(Policy::demo());
    for threshold in [ZEC, 2 * ZEC, 25 * ZEC / 10] {
        let p = ask(threshold);
        guard.evaluate_and_record(&subject, &p, truth(&p));
    }

    let probe = ask(26 * ZEC / 10);
    let knowledge = guard.knowledge(&subject);

    let if_yes = knowledge.after(Comparison::Gte, probe.amount.value, true);
    let if_no = knowledge.after(Comparison::Gte, probe.amount.value, false);
    assert!(
        if_yes.width() > if_no.width(),
        "this test needs an asymmetric question to be meaningful"
    );

    assert!(
        matches!(guard.evaluate(&subject, &probe), Decision::Block { .. }),
        "the narrow branch must trigger a refusal even though the real answer \
         would have taken the wide branch"
    );
}

/// `evaluate` never sees the hidden amount, so identical histories must produce
/// identical decisions no matter what was actually paid.
#[test]
fn the_decision_is_a_function_of_history_and_question_only() {
    let subject = subject();

    let labels_for = |hidden: u64| {
        let mut guard = Guard::new(Policy::demo());
        let mut labels = Vec::new();
        for threshold in [ZEC, 2 * ZEC, 25 * ZEC / 10, 26 * ZEC / 10] {
            let p = ask(threshold);
            labels.push(guard.evaluate(&subject, &p).label());
            let answer = p.amount.operator.holds(Zatoshi(hidden), p.amount.value);
            guard.record(&subject, &p, answer);
        }
        labels
    };

    assert_eq!(labels_for(27 * ZEC / 10), labels_for(400 * ZEC));
}

// --- bookkeeping ------------------------------------------------------------

#[test]
fn repeating_a_question_costs_nothing() {
    let mut guard = Guard::new(Policy::demo());
    let subject = subject();
    let p = ask(ZEC);

    guard.evaluate_and_record(&subject, &p, true);
    let before = guard.knowledge(&subject);

    for _ in 0..20 {
        assert!(
            matches!(guard.evaluate(&subject, &p), Decision::Safe { .. }),
            "asking the same question again reveals nothing new"
        );
    }
    assert_eq!(guard.knowledge(&subject), before);
}

#[test]
fn the_query_budget_is_enforced() {
    let policy = Policy {
        granularity: 1,
        warn_below: 1,
        max_queries: 3,
    };
    let mut guard = Guard::new(policy);
    let subject = subject();

    for i in 0..3 {
        let p = ask(ZEC + i);
        assert!(guard.evaluate_and_record(&subject, &p, truth(&p)).is_allowed());
    }

    let decision = guard.evaluate(&subject, &ask(ZEC + 99));
    assert!(
        matches!(decision, Decision::Block { .. }),
        "the budget must bind even when the interval is still wide: {decision:?}"
    );
}

#[test]
fn verifiers_have_separate_histories() {
    let mut guard = Guard::new(Policy::demo());

    let a = SubjectKey {
        domain: VerifierDomain::new("app-a.example"),
        nullifier: "aa".repeat(32),
    };
    let b = SubjectKey {
        domain: VerifierDomain::new("app-b.example"),
        nullifier: "bb".repeat(32),
    };

    for threshold in [ZEC, 2 * ZEC, 25 * ZEC / 10] {
        let p = ask(threshold);
        guard.evaluate_and_record(&a, &p, truth(&p));
    }

    assert_eq!(
        guard.knowledge(&b),
        Knowledge::unconstrained(),
        "one verifier's probing must not consume another's budget"
    );
    assert!(matches!(
        guard.evaluate(&b, &ask(ZEC)),
        Decision::Safe { .. }
    ));
}

#[test]
fn a_blocked_request_is_not_recorded() {
    let policy = Policy {
        granularity: 1,
        warn_below: 1,
        max_queries: 1,
    };
    let mut guard = Guard::new(policy);
    let subject = subject();

    guard.evaluate_and_record(&subject, &ask(ZEC), true);
    let before = guard.knowledge(&subject);

    guard.evaluate_and_record(&subject, &ask(2 * ZEC), true);
    assert_eq!(
        guard.knowledge(&subject),
        before,
        "a refused question must not move the interval"
    );
    assert_eq!(guard.history(&subject).len(), 1);
}

#[test]
fn decisions_serialise_for_the_verifier_ui() {
    let d = Decision::Block {
        reason: "narrowing past the floor".into(),
    };
    let json = serde_json::to_string(&d).unwrap();
    assert!(json.contains("\"status\":\"BLOCK\""), "{json}");
}
