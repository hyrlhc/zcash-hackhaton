//! WebAssembly bindings for ZClaim.
//!
//! Two objects cross into JavaScript, and the split between them is the whole
//! point of the design:
//!
//! - [`Wallet`] holds the witness and the holder key, and decides whether to
//!   answer. Nothing it owns is reachable from JavaScript — there is no getter
//!   for the note, the amount, or the holder secret, deliberately.
//! - [`Verifier`] holds a domain, a window of chain roots, and a ledger of
//!   claims it has honoured. It can check a proof and nothing else.
//!
//! Everything crossing the boundary is a JSON string in the wire format defined
//! by `zclaim_circuits::wire`, so the TypeScript layer and the Rust verifier
//! agree on the encoding by construction rather than by convention.

use ff::PrimeField;
use pasta_curves::pallas;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use zclaim_circuits::{
    testing::{address_for, holder as holder_key, merchant_named, payment_witness},
    wire::from_hex,
    Pool,
};
use zclaim_core::{Predicate, ProofRequest, VerifierDomain, Zatoshi};
use zclaim_inference::{Decision, Guard, Knowledge, Policy};
use zclaim_proof::ProvingKey;
use zclaim_protocol::{Holder, Presentation, Refusal, Response};
use zclaim_zcash::{AnchorAuthenticator, RootWindow};

type Verified<T> = Result<T, JsError>;

fn parse<T: for<'a> Deserialize<'a>>(json: &str, what: &str) -> Verified<T> {
    serde_json::from_str(json).map_err(|e| JsError::new(&format!("invalid {what}: {e}")))
}

fn emit<T: Serialize>(value: &T) -> Verified<String> {
    serde_json::to_string(value).map_err(|e| JsError::new(&e.to_string()))
}

fn pool_from(name: &str) -> Verified<Pool> {
    match name {
        "orchard" => Ok(Pool::Orchard),
        "ironwood" => Ok(Pool::Ironwood),
        other => Err(JsError::new(&format!(
            "unknown pool {other:?}; expected \"orchard\" or \"ironwood\""
        ))),
    }
}

/// Builds the proving and verifying keys ahead of time.
///
/// Both are derived deterministically — there is no ceremony and no shared
/// secret — but derivation takes a moment, and a caller usually wants it to
/// happen behind a progress indicator rather than inside the first proof.
#[wasm_bindgen(js_name = warmUp)]
pub fn warm_up() {
    let _ = ProvingKey::shared();
    let _ = zclaim_proof::VerifyingKey::shared();
}

// ---------------------------------------------------------------------------
// Shared value types
// ---------------------------------------------------------------------------

/// What a verifier has been able to work out about the hidden amount.
///
/// Zatoshi bounds are strings: `u64::MAX` is the unconstrained upper bound and
/// a JSON number would round it, which would quietly corrupt a wallet's own
/// display of how much it has leaked.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KnowledgeWire {
    low: String,
    high: String,
    exact: bool,
    describe: String,
}

impl From<Knowledge> for KnowledgeWire {
    fn from(k: Knowledge) -> Self {
        KnowledgeWire {
            low: k.low.to_string(),
            high: k.high.to_string(),
            exact: k.is_exact(),
            describe: k.describe(),
        }
    }
}

/// The guard's verdict, in a shape a UI can switch on.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DecisionWire {
    status: &'static str,
    /// What the verifier would know afterwards. Absent when blocked, because
    /// nothing will be answered.
    #[serde(skip_serializing_if = "Option::is_none")]
    resulting: Option<KnowledgeWire>,
    /// Why, for `WARNING` and `BLOCK`. Safe to show the verifier: it depends
    /// only on the question and on history the verifier already has.
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

impl From<&Decision> for DecisionWire {
    fn from(d: &Decision) -> Self {
        match d {
            Decision::Safe { resulting } => DecisionWire {
                status: "SAFE",
                resulting: Some((*resulting).into()),
                reason: None,
            },
            Decision::Warning { resulting, reason } => DecisionWire {
                status: "WARNING",
                resulting: Some((*resulting).into()),
                reason: Some(reason.clone()),
            },
            Decision::Block { reason } => DecisionWire {
                status: "BLOCK",
                resulting: None,
                reason: Some(reason.clone()),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Wallet
// ---------------------------------------------------------------------------

/// How to stage a payment for the demo.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StagedPayment {
    /// What was paid, in zatoshi, as a decimal string.
    paid_zatoshi: String,
    /// Which fixture merchant received it.
    merchant_seed: u8,
    /// Distinguishes one staged note from another.
    note_seed: u8,
    /// The holder's long-term secret, as a seed. In a real wallet this comes
    /// from wallet storage and is never chosen by a caller.
    holder_seed: u8,
    /// Guard policy. Omit for the demo policy.
    #[serde(default)]
    policy: Option<PolicyWire>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PolicyWire {
    /// Narrowest interval a verifier may reach, in zatoshi.
    granularity: String,
    /// Interval width below which a request is answered but flagged.
    warn_below: String,
    /// Hard cap on questions per subject.
    max_queries: u32,
}

impl PolicyWire {
    fn parse(&self) -> Verified<Policy> {
        let zat = |s: &str, field: &str| -> Verified<u64> {
            s.parse::<u64>()
                .map_err(|e| JsError::new(&format!("policy.{field}: {e}")))
        };
        Ok(Policy {
            granularity: zat(&self.granularity, "granularity")?,
            warn_below: zat(&self.warn_below, "warnBelow")?,
            max_queries: self.max_queries,
        })
    }
}

/// What the wallet returns for a request.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResponseWire {
    /// `"ANSWER"` or `"REFUSED"`.
    status: &'static str,
    /// The guard's verdict, whenever the guard was the one that decided.
    #[serde(skip_serializing_if = "Option::is_none")]
    decision: Option<DecisionWire>,
    /// The presentation to hand the verifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    presentation: Option<serde_json::Value>,
    /// Why there is no answer: `"GUARDED"`, `"EXPIRED"` or `"CLAIM_IS_FALSE"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    refusal: Option<&'static str>,
    /// A line safe to show the verifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

/// A wallet holding one shielded payment.
///
/// Note what is *not* on this type: no way to read the amount, the note, or the
/// holder key. A caller can ask it to answer a question, and can ask what it
/// has already leaked, and that is all.
#[wasm_bindgen]
pub struct Wallet {
    holder: Holder,
    anchor: pallas::Base,
}

#[wasm_bindgen]
impl Wallet {
    /// Builds a wallet around a **locally staged** payment.
    ///
    /// The note and its commitment tree are real Orchard cryptography, but the
    /// tree is constructed here rather than read from a chain. Any interface
    /// built on this constructor has to say so — see `provenance()`.
    #[wasm_bindgen(js_name = staged)]
    pub fn staged(config_json: &str) -> Verified<Wallet> {
        let config: StagedPayment = parse(config_json, "staged payment config")?;
        let paid: u64 = config
            .paid_zatoshi
            .parse()
            .map_err(|e| JsError::new(&format!("paidZatoshi: {e}")))?;

        let policy = match &config.policy {
            Some(p) => p.parse()?,
            None => Policy::demo(),
        };

        let (witness, anchor) = payment_witness(
            address_for(config.merchant_seed),
            paid,
            config.note_seed,
            holder_key(config.holder_seed),
        );

        Ok(Wallet {
            holder: Holder::new(witness, anchor, Pool::Ironwood, Guard::new(policy)),
            anchor,
        })
    }

    /// The root of the staged tree this payment sits under.
    ///
    /// A verifier must never take an anchor from the party presenting a proof.
    /// This exists so the demo can seed a verifier's root window in place of a
    /// chain, and the interface has to label it as the stand-in that it is.
    #[wasm_bindgen(js_name = stagedAnchorHex)]
    pub fn staged_anchor_hex(&self) -> String {
        hex::encode(self.anchor.to_repr())
    }

    /// Runs the guard without answering, for a consent dialog.
    ///
    /// The verdict depends only on the question and on history the verifier
    /// already has, so showing it to the verifier reveals nothing.
    pub fn screen(&self, request_json: &str) -> Verified<String> {
        let request: ProofRequest = parse(request_json, "proof request")?;
        emit(&DecisionWire::from(&self.holder.screen(&request)))
    }

    /// Answers a request, or refuses it.
    #[wasm_bindgen(js_name = respond)]
    pub fn respond(&mut self, request_json: &str, chain_height: u32) -> Verified<String> {
        let request: ProofRequest = parse(request_json, "proof request")?;

        let response = self
            .holder
            .respond(&request, chain_height, ProvingKey::shared(), OsRng)
            .map_err(|e| JsError::new(&e.to_string()))?;

        let wire = match response {
            Response::Answer {
                presentation,
                decision,
            } => ResponseWire {
                status: "ANSWER",
                decision: Some(DecisionWire::from(&decision)),
                presentation: Some(
                    serde_json::to_value(&*presentation)
                        .map_err(|e| JsError::new(&e.to_string()))?,
                ),
                refusal: None,
                message: None,
            },
            Response::Refused(refusal) => ResponseWire {
                status: "REFUSED",
                decision: match &refusal {
                    Refusal::Guarded(d) => Some(DecisionWire::from(d)),
                    _ => None,
                },
                presentation: None,
                refusal: Some(match refusal {
                    Refusal::Guarded(_) => "GUARDED",
                    Refusal::Expired(_) => "EXPIRED",
                    Refusal::ClaimIsFalse => "CLAIM_IS_FALSE",
                }),
                message: Some(refusal.describe()),
            },
        };

        emit(&wire)
    }

    /// What the asking verifier has been able to work out so far.
    ///
    /// This is the wallet's own estimate of what it has given away, and is what
    /// a wallet UI should put in front of its user.
    pub fn exposure(&self, request_json: &str) -> Verified<String> {
        let request: ProofRequest = parse(request_json, "proof request")?;
        emit(&KnowledgeWire::from(self.holder.exposure(&request)))
    }
}

// ---------------------------------------------------------------------------
// Verifier
// ---------------------------------------------------------------------------

/// A believed claim, as the SDK reports it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AcceptanceWire {
    /// The predicate now known to hold.
    predicate: Predicate,
    /// The chain root the payment was proved against.
    anchor: String,
    /// Which pool's tree that root belongs to.
    pool: &'static str,
    /// The height at which the root was the tree state.
    anchor_height: u32,
    /// The payment's tag here. Stable, and meaningless at any other verifier.
    nullifier: String,
    /// The holder's pseudonym here. Same scoping.
    holder_tag: String,
}

/// An application that asks ZClaim questions.
#[wasm_bindgen]
pub struct Verifier {
    inner: zclaim_protocol::Verifier<RootWindow>,
    pool: Pool,
}

#[wasm_bindgen]
impl Verifier {
    /// Builds a verifier for one application domain and one shielded pool.
    ///
    /// Two applications must never share a domain: the domain is what makes a
    /// holder's tags unlinkable between them.
    #[wasm_bindgen(constructor)]
    pub fn new(domain: &str, pool: &str, max_anchor_age_blocks: u32) -> Verified<Verifier> {
        let pool = pool_from(pool)?;
        Ok(Verifier {
            inner: zclaim_protocol::Verifier::new(
                VerifierDomain::new(domain),
                pool,
                AnchorAuthenticator::new(RootWindow::new(), max_anchor_age_blocks),
            ),
            pool,
        })
    }

    /// Records a note commitment tree root observed at `height`.
    ///
    /// The caller is asserting it learned this from something that speaks for
    /// the chain — its own node, or an indexer it accepts. Taking a root from
    /// the party presenting a proof defeats the entire mechanism, because the
    /// circuit proves membership in *some* tree, not in Zcash's.
    #[wasm_bindgen(js_name = observeRoot)]
    pub fn observe_root(&mut self, anchor_hex: &str, height: u32) -> Verified<()> {
        let anchor = from_hex(anchor_hex).map_err(|e| JsError::new(&format!("anchor: {e}")))?;
        self.inner
            .anchors_mut()
            .source_mut()
            .observe(anchor, self.pool, height);
        Ok(())
    }

    /// How many roots this verifier currently accepts.
    #[wasm_bindgen(js_name = knownRootCount)]
    pub fn known_root_count(&self) -> usize {
        self.inner.anchors().source().len()
    }

    /// Publishes a request with a fresh single-use challenge.
    pub fn request(
        &self,
        predicate_json: &str,
        purpose: &str,
        expiry_height: u32,
    ) -> Verified<String> {
        let predicate: Predicate = parse(predicate_json, "predicate")?;
        emit(&self
            .inner
            .request(predicate, purpose, expiry_height, &mut OsRng))
    }

    /// Runs every check, and throws with the reason if any fails.
    ///
    /// "The proof verified" is not the same as "the claim is true". This checks
    /// that the statement answers the request that was published, that the
    /// anchor is a root the chain produced, that the proof verifies, and that
    /// the payment has not already been claimed here.
    pub fn accept(&mut self, request_json: &str, presentation_json: &str) -> Verified<String> {
        let request: ProofRequest = parse(request_json, "proof request")?;
        let presentation: Presentation = parse(presentation_json, "presentation")?;

        let accepted = self
            .inner
            .accept(&request, &presentation)
            .map_err(|e| JsError::new(&e.to_string()))?;

        emit(&AcceptanceWire {
            predicate: accepted.predicate,
            anchor: hex::encode(accepted.anchor.anchor.to_repr()),
            pool: match accepted.anchor.pool {
                Pool::Orchard => "orchard",
                Pool::Ironwood => "ironwood",
            },
            anchor_height: accepted.anchor.height,
            nullifier: accepted.nullifier,
            holder_tag: accepted.holder_tag,
        })
    }

    /// Whether a payment has already been claimed here.
    #[wasm_bindgen(js_name = hasClaimed)]
    pub fn has_claimed(&self, nullifier_hex: &str) -> bool {
        self.inner.has_claimed(nullifier_hex)
    }
}

// ---------------------------------------------------------------------------
// Helpers the demo needs
// ---------------------------------------------------------------------------

/// A fixture merchant, in the form a verifier names one in a request.
#[wasm_bindgen(js_name = stagedMerchant)]
pub fn staged_merchant(label: &str, seed: u8) -> Verified<String> {
    emit(&merchant_named(label, seed))
}

/// Formats a zatoshi amount the way Zcash does, with eight decimal places.
#[wasm_bindgen(js_name = formatZec)]
pub fn format_zec(zatoshi: &str) -> Verified<String> {
    let zat: u64 = zatoshi
        .parse()
        .map_err(|e| JsError::new(&format!("zatoshi: {e}")))?;
    Ok(Zatoshi(zat).as_zec_string())
}

/// What is real in this build and what is staged.
///
/// Exposed as data rather than left to prose so an interface cannot forget to
/// say it.
#[wasm_bindgen]
pub fn provenance() -> String {
    serde_json::json!({
        "real": [
            "Orchard note, committed with NoteCommit^Orchard",
            "Commitment tree hashed with MerkleCRH^Orchard",
            "Halo2 IPA proof over Pasta curves, the Orchard proving system",
            "Anchor authentication, replay rejection, context binding",
        ],
        "staged": [
            "The note is constructed in the browser, not read from a chain, so \
             the root it sits under is one this page built.",
        ],
        "mockProving": false,
    })
    .to_string()
}
