//! The ZClaim demo.
//!
//! Alice paid Quantum Cafe 2.7 ZEC in a shielded transaction. A loyalty program
//! wants to know whether she spent at least 1 ZEC there. It gets an answer, and
//! nothing else. Then it gets greedy, and stops getting answers.
//!
//! Every proof printed below is a real Halo2 proof over a real Orchard note.
//! What is *not* real is where the note came from: see [`provenance`].

use rand::rngs::OsRng;
use zclaim_circuits::{
    testing::{address_for, holder, merchant_named, payment_witness, QUANTUM_CAFE, ZEC},
    Pool,
};
use zclaim_core::{AmountPredicate, Comparison, Predicate, VerifierDomain, Zatoshi};
use zclaim_inference::{Decision, Guard, Policy};
use zclaim_proof::ProvingKey;
use zclaim_protocol::{Holder, Refusal, Response, Verifier};
use zclaim_zcash::{AnchorAuthenticator, LightwalletClient, RootWindow, TESTNET_ENDPOINT};

/// What Alice actually paid. Nothing downstream of the wallet ever sees this.
const PAID: u64 = 27 * ZEC / 10;

/// A stand-in chain height.
const TIP: u32 = 3_500_000;

const DOMAIN: &str = "loyalty.quantum-cafe.example";

fn main() {
    banner();

    // `--chain [endpoint]` replaces the staged root window with real roots read
    // off a live Zcash node, and shows what that changes.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let chain = match args.first().map(String::as_str) {
        Some("--chain") => Some(
            args.get(1)
                .cloned()
                .unwrap_or_else(|| TESTNET_ENDPOINT.to_string()),
        ),
        _ => None,
    };

    provenance(chain.is_some());

    let merchant = address_for(QUANTUM_CAFE);
    let (witness, anchor) = payment_witness(merchant, PAID, 0x09, holder(0x11));

    // The verifier's node has followed the chain and seen this root. This is
    // the step that ties the whole thing to Zcash: without it, a prover could
    // invent a tree and prove anything about it.
    let mut roots = RootWindow::new();
    roots.observe(anchor, Pool::Ironwood, TIP);

    let mut alice = Holder::new(witness, anchor, Pool::Ironwood, Guard::new(Policy::demo()));
    let mut loyalty = Verifier::new(
        VerifierDomain::new(DOMAIN),
        Pool::Ironwood,
        AnchorAuthenticator::new(roots, 100),
    );

    section("1. The honest question");
    println!("  Quantum Cafe's loyalty program asks one thing:\n");
    println!("      Did this customer pay Quantum Cafe at least 1 ZEC?\n");

    let pk = ProvingKey::shared();
    let request = loyalty.request(at_least(ZEC), "loyalty-tier", TIP + 100, &mut OsRng);

    match alice
        .respond(&request, TIP, pk, OsRng)
        .expect("responding must not error")
    {
        Response::Answer { presentation, .. } => {
            let size = presentation.proof.as_bytes().len();
            match loyalty.accept(&request, &presentation) {
                Ok(accepted) => {
                    println!("  VALID");
                    println!();
                    println!("      Merchant            Quantum Cafe");
                    println!(
                        "      Requirement         >= {} ZEC",
                        Zatoshi(ZEC).as_zec_string()
                    );
                    println!(
                        "      Anchor              {} (Ironwood, height {})",
                        short(&hex::encode(to_bytes(presentation.statement.anchor))),
                        accepted.anchor.height
                    );
                    println!("      Proof               {size} bytes");
                    println!();
                    println!("      Exact amount        HIDDEN");
                    println!("      Wallet              HIDDEN");
                    println!("      Transaction         HIDDEN");
                    println!("      Other payments      HIDDEN");
                    println!("      Identity            HIDDEN");
                    println!();
                    println!(
                        "      Payment tag         {}   (this verifier only)",
                        short(&accepted.nullifier)
                    );
                    println!(
                        "      Holder pseudonym    {}   (this verifier only)",
                        short(&accepted.holder_tag)
                    );
                }
                Err(e) => println!("  REJECTED: {e}"),
            }
        }
        Response::Refused(r) => println!("  No answer: {}", r.describe()),
    }

    section("2. The same answer, offered to somebody else");
    replay_elsewhere(&mut alice, anchor);

    section("3. The verifier turns greedy");
    println!("  Each answer above narrows nothing much. A sequence of them");
    println!("  narrows a great deal. So the wallet — not the verifier — keeps");
    println!("  track of what these questions add up to, and stops answering.\n");

    let mut already_explained = false;
    for threshold in [ZEC, 2 * ZEC, 25 * ZEC / 10, 26 * ZEC / 10, 27 * ZEC / 10] {
        probe(&mut alice, &loyalty, threshold, pk, &mut already_explained);
    }

    let mut last = 4;
    if let Some(endpoint) = chain {
        section("4. Against the real chain");
        against_the_chain(&endpoint, anchor);
        last = 5;
    }

    section(&format!("{last}. What the verifier ended up knowing"));
    let final_request = loyalty.request(at_least(ZEC), "loyalty-tier", TIP + 100, &mut OsRng);
    println!(
        "      About the amount    {}",
        alice.exposure(&final_request).describe()
    );
    println!("      Actual amount       {} ZEC   (never left the wallet)", Zatoshi(PAID).as_zec_string());
    println!();
    println!("  It cannot get closer by asking, because the wallet stops answering");
    println!("  before the next question would narrow the range past the floor.");
    println!();
    println!("  Don't ask for the data. Ask for the proof.");
    println!();
}

/// One probe in the bisection attack.
fn probe(
    alice: &mut Holder,
    loyalty: &Verifier<RootWindow>,
    threshold: u64,
    pk: &ProvingKey,
    already_explained: &mut bool,
) {
    let request = loyalty.request(at_least(threshold), "loyalty-tier", TIP + 100, &mut OsRng);
    let question = format!(">= {} ZEC", Zatoshi(threshold).as_zec_string());

    match alice
        .respond(&request, TIP, pk, OsRng)
        .expect("responding must not error")
    {
        Response::Answer { decision, .. } => match decision {
            Decision::Safe { resulting } => {
                println!(
                    "  {question:<18}  YES   SAFE      verifier now knows: {}",
                    resulting.describe()
                );
            }
            Decision::Warning { resulting, reason } => {
                println!(
                    "  {question:<18}  YES   WARNING   verifier now knows: {}",
                    resulting.describe()
                );
                println!("  {:<36}{reason}", "");
            }
            Decision::Block { .. } => unreachable!("a blocked request yields no answer"),
        },
        Response::Refused(Refusal::Guarded(Decision::Block { reason })) => {
            println!("  {question:<18}  —     BLOCKED");
            if !*already_explained {
                println!();
                println!("      REQUEST BLOCKED");
                println!("      {reason}");
                println!();
                println!("      The wallet is not refusing because the answer is");
                println!("      inconvenient. It refuses on the question alone, so a");
                println!("      refusal reveals nothing a further question could not.");
                println!();
                *already_explained = true;
            }
        }
        Response::Refused(other) => {
            println!("  {question:<18}  NO    {}", other.describe());
        }
    }
}

/// The same proof, presented to a different application.
fn replay_elsewhere(alice: &mut Holder, anchor: pasta_curves::pallas::Base) {
    let mut insurer = Verifier::new(
        VerifierDomain::new("insurer.example"),
        Pool::Ironwood,
        AnchorAuthenticator::new(
            {
                let mut roots = RootWindow::new();
                roots.observe(anchor, Pool::Ironwood, TIP);
                roots
            },
            100,
        ),
    );

    // Alice answers the insurer's own question honestly...
    let theirs = insurer.request(at_least(ZEC), "underwriting", TIP + 100, &mut OsRng);
    let response = alice
        .respond(&theirs, TIP, ProvingKey::shared(), OsRng)
        .expect("responding must not error");

    let Response::Answer { presentation, .. } = response else {
        println!("  (the wallet declined this one)");
        return;
    };

    match insurer.accept(&theirs, &presentation) {
        Ok(accepted) => {
            println!("  The insurer asks the same question and gets its own answer.\n");
            println!(
                "      Holder pseudonym here    {}",
                short(&accepted.holder_tag)
            );
            println!("      Holder pseudonym at the cafe   (unrelated)");
            println!();
            println!("  The two verifiers cannot tell they are looking at one person,");
            println!("  or at one payment. Both tags are scoped to the asking domain.");
        }
        Err(e) => println!("  REJECTED: {e}"),
    }

    // ...and now the insurer tries to reuse that answer at the cafe.
    let mut cafe = Verifier::new(
        VerifierDomain::new(DOMAIN),
        Pool::Ironwood,
        AnchorAuthenticator::new(
            {
                let mut roots = RootWindow::new();
                roots.observe(anchor, Pool::Ironwood, TIP);
                roots
            },
            100,
        ),
    );
    let cafe_request = cafe.request(at_least(ZEC), "loyalty-tier", TIP + 100, &mut OsRng);

    println!();
    match cafe.accept(&cafe_request, &presentation) {
        Ok(_) => println!("  PROBLEM: an answer meant for the insurer was accepted at the cafe"),
        Err(e) => println!("  Replaying it at the cafe fails, as it must:\n      {e}"),
    }
}

/// Points the verifier's anchor authentication at a live node.
///
/// The point of this section is not that it succeeds — it is that the staged
/// anchor now fails. Everything in sections 1 to 3 stays valid against a tree
/// the demo built; here the verifier only accepts roots Zcash actually
/// published, and the demo's tree is not one of them.
fn against_the_chain(endpoint: &str, staged_anchor: pasta_curves::pallas::Base) {
    println!("  Reading note commitment tree roots from {endpoint}\n");

    let client = match LightwalletClient::connect(endpoint) {
        Ok(c) => c,
        Err(e) => {
            println!("  Could not reach the node: {e}");
            return;
        }
    };

    let info = match client.chain_info() {
        Ok(i) => i,
        Err(e) => {
            println!("  Node did not answer: {e}");
            return;
        }
    };

    println!("      Server              {}", info.server_version);
    println!("      Chain               {}", info.chain);
    println!("      Tip                 {}", info.tip_height);
    println!("      Consensus branch    {}", info.consensus_branch);

    let mut window = RootWindow::new();
    let tip = match client.fill_root_window(&mut window, Pool::Orchard, 8) {
        Ok(tip) => tip,
        Err(e) => {
            println!("\n  Could not read tree state: {e}");
            return;
        }
    };

    let real = match client.tree_state(tip) {
        Ok(state) => state,
        Err(e) => {
            println!("\n  Could not read tree state: {e}");
            return;
        }
    };

    println!();
    println!(
        "      Real anchor         {}",
        short(&hex::encode(to_bytes(real.anchor())))
    );
    println!("      Leaves in the tree  {}", real.size());
    println!();

    let auth = AnchorAuthenticator::new(window, 100);
    match auth.authenticate(real.anchor(), Pool::Orchard) {
        Ok(a) => println!("      Real root           ACCEPTED  (height {})", a.height),
        Err(e) => println!("      Real root           REFUSED — {e}"),
    }
    match auth.authenticate(staged_anchor, Pool::Orchard) {
        Ok(_) => println!("      PROBLEM: the demo's own root was accepted as a chain root"),
        Err(_) => println!("      This demo's root    REFUSED"),
    }

    println!();
    println!("  That refusal is the honest state of the project. The proving side");
    println!("  is complete and the verifying side is wired to the chain; what is");
    println!("  missing is a note of our own on testnet, which needs funds and a");
    println!("  wallet scan rather than any further cryptography.");
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

fn banner() {
    println!();
    println!("  ZCLAIM — Quantum Cafe");
    println!("  Don't ask for the data. Ask for the proof.");
    println!();
}

/// Says plainly which parts of this demo are real and which are staged.
///
/// The distinction the project cares about is not "mock crypto vs real crypto"
/// — the cryptography here is entirely real — but where the note came from.
fn provenance(chain_mode: bool) {
    println!("  REAL in this run");
    println!("    - Orchard note, committed with NoteCommit^Orchard");
    println!("    - Commitment tree hashed with MerkleCRH^Orchard");
    println!("    - Halo2 proof over Pasta curves, the Orchard proving system");
    println!("    - Anchor authentication, replay rejection, context binding");
    println!();
    println!("  STAGED in this run");
    println!("    - The note was constructed locally, not read from a chain, so");
    println!("      the root it sits under is one this program built. Sections 1");
    println!("      to 3 are otherwise exactly what a real payment would produce.");
    if !chain_mode {
        println!();
        println!("      Run with --chain to point the verifier at live testnet");
        println!("      roots and watch it refuse this one.");
    }
    println!();
}

fn section(title: &str) {
    println!();
    println!("  ────────────────────────────────────────────────────────────");
    println!("  {title}");
    println!("  ────────────────────────────────────────────────────────────");
    println!();
}

fn short(hex: &str) -> String {
    format!("{}…{}", &hex[..8], &hex[hex.len() - 6..])
}

fn to_bytes(f: pasta_curves::pallas::Base) -> [u8; 32] {
    <pasta_curves::pallas::Base as ff::PrimeField>::to_repr(&f)
}