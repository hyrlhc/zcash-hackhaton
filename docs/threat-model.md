# ZClaim — Threat Model

What ZClaim defends against, what it does not, and who has to be honest for each
property to hold.

---

## The parties

| Party | Holds | Wants |
|---|---|---|
| **Holder** | A shielded payment's note material, and a long-term `holder_sk` | To answer one question and reveal nothing else |
| **Verifier** | An application domain, a view of the chain | A truthful answer — and, if adversarial, everything else too |
| **Merchant** | The receiving address; can decrypt payments made to it | Usually a bystander; modelled as capable, not trusted |
| **Observer** | Network traffic, published proofs | To link holders across applications |

---

## Properties, and what enforces them

| # | Property | Enforced by | Kind |
|---|---|---|---|
| 1 | A false predicate cannot be proved | Range check + `NoteCommit` binding | Cryptographic |
| 2 | The amount is not revealed | Zero-knowledge of Halo2 IPA | Cryptographic |
| 3 | The note is really in a Zcash tree | Merkle path **+ anchor authentication** | Cryptographic + operational |
| 4 | The payment really went to the named merchant | Four-coordinate receiver binding | Cryptographic |
| 5 | An answer cannot be reused elsewhere | `context` binding | Cryptographic |
| 6 | One payment cannot be claimed twice at a verifier | `nullifier` + verifier ledger | Cryptographic + operational |
| 7 | Two verifiers cannot correlate a holder | Domain-scoped `nullifier` and `holder_tag` | Cryptographic |
| 8 | A leaked witness is not a credential | `holder_tag` | Cryptographic |
| 9 | Repeated questions do not reveal the amount | Inference Guard | **Policy** |

Rows 1–8 hold against a fully adversarial verifier. Row 9 does not, and the
distinction is the most important thing in this document.

---

## Attacks and outcomes

### Prove a payment that did not clear the bar

Blocked. `diff = direction · (v - threshold)` wraps to a value above 2²⁵³ when
the comparison fails, and the strict 70-bit lookup range check rejects it. `v`
itself cannot be inflated because `NoteCommit` binds it to the commitment the
Merkle path reaches.

Tests: `gte_rejects_amounts_below_the_threshold`,
`inflating_the_amount_breaks_the_commitment`, `a_false_claim_does_not_verify`.

### Invent a tree containing a note of one's choosing

Blocked, **but only by the verifier**. The circuit proves the note is in *some*
tree with root `anchor`; `anchor` is a public input, and a prover may set it to
anything. A verifier that skips `AnchorAuthenticator` accepts fabrications.

This is the single most important operational requirement in the system, which
is why the type that verifies proofs cannot be reached without going through one
(`zclaim_protocol::Verifier`), and why the demo shows a chain-backed verifier
refusing the demo's own root.

Tests: `a_fabricated_anchor_is_refused`,
`a_proof_against_an_unknown_root_is_refused`.

### Present a root from the other shielded pool

Blocked. After NU6.3 the Orchard and Ironwood pools have separate commitment
trees. `Pool` travels with the anchor, and the root window is keyed by it.

Test: `a_root_from_the_other_pool_is_refused`.

### Present a very old root

Blocked by the acceptance window. The trade-off is explicit: too tight and
honest provers whose wallet is a few blocks behind get rejected; too loose and a
reorged root stays acceptable. A root that stays current across empty blocks is
aged from its newest sighting, not its first.

Tests: `a_stale_anchor_is_refused`,
`a_root_seen_repeatedly_is_aged_from_its_newest_sighting`.

### Burn funds to a fake receiver and call it a payment

Blocked. See "Why the merchant binding is four values" in
`architecture-decision.md`. A note carrying the merchant's `pk_d` under a
diversifier the merchant never used is unspendable by them, and is refused.

Tests: `the_diversified_base_point_is_bound_too`,
`a_negated_transmission_key_is_not_the_merchant`.

### Replay an answer at a second application

Blocked. `context = H(domain, nonce, predicate, purpose, expiry)` is a public
input the circuit copy-constrains, so an answer satisfies exactly one request.
The verifier additionally recomputes the whole statement from the request it
published and compares field by field.

Tests: `a_proof_does_not_replay_into_another_application`,
`an_answer_does_not_travel_between_applications`,
`a_proof_does_not_upgrade_to_a_stronger_threshold`.

### Claim the same payment twice for two rewards

Blocked. The nullifier is stable for one payment at one verifier, and the
verifier keeps a ledger of the ones it has honoured.

Test: `the_same_payment_cannot_be_claimed_twice`.

### Collude across applications to build a profile

Blocked. Both published tags are scoped to `H(verifier domain)`, so the same
payment and the same holder look unrelated at two verifiers. Two colluding
verifiers comparing their logs find nothing in common.

Tests: `one_note_yields_unlinkable_nullifiers_across_verifiers`,
`a_holder_tag_is_stable_per_verifier_and_unlinkable_across_them`.

### Scope the tags to a domain of one's own invention

Blocked. If `domain_tag` were a witness, a prover could produce a fresh
nullifier for every claim and defeat both the double-claim ledger and the
guard's history. It is a public input, recomputed by the verifier from its own
domain.

Test: `tags_cannot_be_scoped_to_a_domain_of_the_provers_choosing`.

### Steal a witness and claim as the holder

Blocked as impersonation, not as use. A thief can build a statement from stolen
note material, but cannot reproduce `holder_tag` without `holder_sk`, so the
claim arrives under a different pseudonym. A verifier that cares about identity
continuity — a ticket gate, a loyalty tier — sees a stranger.

Tests: `a_stolen_witness_cannot_impersonate_the_holder`,
`a_leaked_witness_does_not_let_a_thief_prove_as_the_holder`.

### Bisect the amount with a sequence of thresholds

**Mitigated, not prevented.** This is the Inference Guard, and it is a policy
layer. See below.

---

## The Inference Guard, honestly

### What it is

Answers to threshold comparisons are interval constraints, so everything a
verifier has learned about a hidden amount is exactly one interval `[low, high]`.
The width of that interval *is* the leak, which makes the guard's decision
measurable rather than a matter of taste.

Before answering, the guard computes the interval each of the two possible
answers would produce and judges the **narrower** one against a floor. If either
branch would narrow past the floor, the question is refused.

### Why it judges the narrower branch

Deciding on the branch that would actually be taken would itself leak. A
verifier that sees a refusal where it expected an answer learns which way the
comparison went. Judging the narrower branch means the verdict is a function of
the question and the history — both of which the verifier already has — so a
refusal carries no information.

Test: `the_decision_is_a_function_of_history_and_question_only`,
`a_refusal_does_not_reveal_which_way_the_answer_went`.

### Why it runs on the holder

A guard on the verifier is the adversary policing itself. It lives with the
party that loses something when it fails.

### Why "cannot prove" counts as an answer

If a holder declines because the claim is false, the verifier learns "no" just
as surely as from a signed denial. The guard records that outcome too, or its
picture of what the verifier knows would drift and the defence would silently
weaken.

Test: `an_unprovable_claim_still_narrows_what_the_guard_believes_is_known`.

### What it does not do

- **It is not information-theoretic.** It tracks intervals, not entropy, and it
  has no model of a verifier's prior. A verifier that already knows the holder
  buys coffee learns more from "≥ 2 ZEC" than the interval width suggests.
- **It does not survive a cooperative holder.** Nothing stops someone from
  running a wallet with the guard disabled and answering everything.
- **It does not span verifiers.** Histories are keyed by domain-scoped
  nullifiers, deliberately: joining them would require exactly the correlation
  the rest of the design prevents. Two colluding verifiers each get their own
  budget.
- **It does not span payments.** A holder with several payments to one merchant
  can be asked about each separately.
- **In-memory state resets.** A guard that forgets on restart is a guard a
  verifier can reset by waiting. Persistence is a deployment requirement.

### The policy knobs

| Knob | Demo value | Meaning |
|---|---|---|
| `granularity` | 0.5 ZEC | The narrowest interval a verifier may reach |
| `warn_below` | 1 ZEC | Answer, but flag it |
| `max_queries` | 12 | Hard cap per subject, whatever the intervals say |

`warn_below` must sit above `granularity` but below the interval a single honest
question produces — otherwise every first question is flagged and the warning
stops carrying information.

The budget exists on top of interval tracking because probing can spread across
merchants and comparison directions in ways interval width alone would not
catch. Repeating an identical question is free: it reveals nothing new, so it is
answered and not charged.

Test: `repeating_a_question_costs_nothing`, `the_query_budget_is_enforced`,
`the_guard_warns_before_it_blocks`.

---

## Operational requirements

A deployment that skips any of these loses the corresponding property, and the
proof will still verify.

1. **Authenticate anchors against your own view of the chain.** Never from the
   party supplying the proof.
2. **Persist the nullifier ledger.** Otherwise double claims work after a
   restart.
3. **Persist guard state.** Otherwise probing works after a restart.
4. **Use a fresh nonce per request** and a real expiry height.
5. **Give each application its own domain.** Sharing one merges the pseudonyms
   the design works to keep apart.
6. **Keep `holder_sk` in wallet storage and never transmit it.** It is not
   recoverable from the note, and losing it means losing continuity of identity
   at every verifier.

---

## Non-goals

- Proving a note is unspent.
- Binding a claim to a specific `txid`.
- Hiding *that* a proof was requested, or its timing.
- Protecting against a compromised prover device.
- Anything about transparent (`t`-address) payments — there is no shielded note
  to prove over, and the amount is public anyway.
