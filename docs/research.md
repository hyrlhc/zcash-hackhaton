# ZClaim — Research

Research conducted 2026-08-17. Every claim below was checked against the crate
source vendored into `~/.cargo/registry` or the linked primary source, not from
memory.

---

## 1. State of the Zcash stack (August 2026)

| Component | Status | Notes |
|---|---|---|
| **Ironwood (NU6.3)** | Activated mainnet 2026-07-28, block 3,428,143 | New shielded pool + v6 transaction format |
| **zcashd** | Dead | End-of-support halt at block 3,417,100, before Ironwood |
| **Zebra** | 6.0.0 | The only consensus node implementation |
| **Zaino** | 0.3.x | Rust indexer replacing `lightwalletd`; `direct` (Zebra ReadStateService) or `rpc` backends |
| **Zallet** | Current | Wallet daemon, syncs via Zaino |
| **Z3** | 2.0.0 | ZF Docker stack; default topology is Zebra + Zallet, Zaino behind an `indexer` profile |

Any tutorial built on `zcashd` RPCs (including the `z_getpaymentdisclosure` /
`z_validatepaymentdisclosure` pair that ZIP 311 describes) is dead code today.

Sources: [ZF Engineering Update](https://forum.zcashcommunity.com/t/zf-engineering-update-29th-june-to-12th-july-2026/56667),
[crypto.news on the Ironwood schedule](https://crypto.news/zcash-sets-ironwood-upgrade-for-july-28-after-orchard-bug/),
[z3 PR #51](https://github.com/ZcashFoundation/z3/pull/51),
[Zainod releases](https://forum.zcashcommunity.com/t/zainod-release-announcements/55845).

### Relevant crate versions (resolved and compiled locally)

```
orchard          0.15.5
halo2_proofs     0.3.5
halo2_gadgets    0.5.0
sinsemilla       0.1.0     (split out of halo2_gadgets)
halo2_poseidon   0.1.0     (split out of halo2_gadgets)
pasta_curves     0.5.2
incrementalmerkletree 0.8.1
```

---

## 2. Orchard vs. Ironwood — what actually changes for us

This mattered more than expected, and the answer is favourable.

Ironwood is **not** a new shielded protocol. Per
[ZIP 229](https://zips.z.cash/zip-0229) and the
[Ironwood Book](https://zcash.github.io/ironwood/concepts.html), it is a second
*value pool* that reuses the entire Orchard *protocol*: the same Action shape,
the same Pallas/Vesta curves, the same Sinsemilla note commitment, the same
Halo2 Action circuit, the same key machinery.

What differs:

1. **Separate note commitment tree, anchor, and nullifier set.** An Ironwood
   note commitment goes into the Ironwood tree, not the Orchard tree.
2. **Note plaintext lead byte `0x03`** (vs. `0x02`), the quantum-recoverable
   format from [ZIP 2005](https://zips.z.cash/zip-2005). This changes how `rcm`
   is derived (`rcm_v3` binds `g_d`, `pk_d`, `value` and `psi`, so a
   discrete-log-breaking adversary cannot vary note fields).
3. **No new value may enter the Orchard pool** after NU6.3.

**Consequence for ZClaim:** the in-circuit commitment and Merkle machinery is
identical across both pools. `NoteCommit^Orchard` and `MerkleCRH^Orchard` are
the same gadgets. The pool distinction is entirely a question of *which anchor
the verifier authenticates*, which is data-sourcing, not circuit design. One
circuit covers both pools.

In the `orchard` crate this shows up as `NoteVersion::{V2, V3}`, and
`Note::from_parts` gained a `version` parameter.

---

## 3. Question A — what witness is obtainable client-side?

For any Orchard-protocol note the holder can recover (as recipient via `ivk`, or
as sender via `ovk`), the following are available in-process from the `orchard`
crate:

| Witness | Accessor |
|---|---|
| `g_d` (diversified base) | `Address::g_d()` |
| `pk_d` (transmission key) | `Address::pk_d().inner()` |
| `v` (value, zatoshi) | `Note::value()` |
| `rho` | `Note::rho()` → `Rho::into_inner()` |
| `psi` | `RandomSeed::psi(&rho)` |
| `rcm` | `RandomSeed::rcm_v2(&rho)` / `rcm_v3(...)` → `.inner()` |
| Merkle path + position | `orchard::tree::MerklePath` (from the indexer's tree state) |

The corresponding **public** value is `cmx = Extract_P(NoteCommit(...))`, which
appears on-chain in the action, and the tree `anchor`.

Several of these accessors are `pub(crate)` by default. They become public under
the `orchard/unstable-voting-circuits` feature — see §5.

---

## 4. Question B — how does this enter a Halo2 circuit?

Directly, and using Zcash's own circuit code. Under
`unstable-voting-circuits`, `orchard` exposes:

- `orchard::circuit::note_commit::gadgets::note_commit(...)` — the real
  `NoteCommit^Orchard` in-circuit implementation, including every field
  decomposition and canonicity check (~2000 lines we do not have to write or
  audit).
- `orchard::circuit::note_commit::{NoteCommitChip, NoteCommitConfig}`
- `orchard::circuit::gadget::{assign_free_advice, derive_nullifier, AddChip}`
- `orchard::circuit::commit_ivk::gadgets::commit_ivk`
- `orchard::constants::{OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases}`
- `orchard::spec::NonIdentityPallasPoint`

`MerkleCRH^Orchard` needs no feature flag: it is
`halo2_gadgets::sinsemilla::merkle::MerklePath` parameterised with
`OrchardHashDomains::MerkleCrh`.

This is the decisive finding. Without it, a sound circuit would require
reimplementing `NoteCommit`, which is not hackathon-scoped.

---

## 5. Question C — which properties can become predicates?

Verified by building the circuit and running it (see `crates/zclaim-spike`):

| Property | Provable in ZK today | How |
|---|---|---|
| Note exists in the commitment tree | **Yes** | Sinsemilla Merkle path to a public anchor |
| Note value ≥ / ≤ threshold | **Yes** | `v - threshold` with a strict lookup range check |
| Recipient is a specific merchant | **Yes** | Constrain `x(pk_d)` to a public input |
| Note commitment well-formed | **Yes** | Upstream `note_commit` gadget |
| Scoped, unlinkable nullifier | **Yes** | Poseidon over `(holder_secret, context)` |
| Note is **unspent** | **No** (not in MVP) | Needs nullifier non-membership; see §6 |
| Note is part of transaction `txid` | **No** | The circuit binds to the tree, not to a txid |
| Sender identity / spend authority | **Partial** | Requires `ak`/`nk` binding; see the architecture decision |

---

## 6. Question E/F — prior art, and where ZClaim differs

### Orchard Balance Proof + Shielded Voting (Valar Group) — the closest work

- [zips PR #976](https://github.com/zcash/zips/pull/976) — Orchard Balance Proof:
  "the foundational primitive for proving note ownership without revealing
  standard nullifiers".
- [zips PR #1200](https://github.com/zcash/zips/pull/1200) — Shielded Voting Protocol.
- [zips PR #978](https://github.com/zcash/zips/pull/978) — Nullifier PIR.
- [`voting-circuits` 0.10.0](https://github.com/valargroup/voting-circuits) —
  the implementation, built on `orchard 0.15` with exactly the
  `unstable-voting-circuits` feature we use.

This is the same *mechanism* family: prove things about Orchard notes without
spending them. Their ZKP1 (Delegation) proves ownership of up to 5 unspent notes
at a snapshot, derives a governance nullifier `Poseidon(nk, dom, real_nf)`, and
proves nullifier non-membership via an indexed Merkle tree + PIR.

**We must not claim to have invented this primitive.** ZClaim differs in what it
proves and to whom:

| | Shielded Voting | ZClaim |
|---|---|---|
| Prover's role | Note **owner** | Payment **counterparty** |
| Statement | Aggregate balance → voting weight | Predicate on one payment to a named merchant |
| Recipient binding | Not required | **Central** (`pk_d` = merchant) |
| Unspent-ness | Required (PIR + IMT) | **Not required** — a payment that later moved is still a payment |
| Verifier | Governance protocol | Arbitrary third-party app, via SDK |
| Repeated-query defence | Out of scope | **Inference Guard** — the novel part |

### Glasspane — selective disclosure, but not zero-knowledge

[dolepee/glasspane](https://github.com/dolepee/glasspane) shares a per-output
**Outgoing Cipher Key (OCK)**. A verifier runs
`try_output_recovery_with_ock(...)` and recovers the note plaintext: recipient,
**exact value**, and memo.

This is the sharpest contrast available for the demo. Glasspane answers
"which payout was this, exactly?" by revealing it. ZClaim answers "does this
payment satisfy my condition?" while revealing nothing else. Glasspane is a
*disclosure* tool; ZClaim is a *predicate* tool. They are complementary, and
Glasspane's OCK is in fact a good way for a prover to obtain the note plaintext
it then proves over.

### ZAP1 — application-layer attestation, not payment ZK

[Frontier-Compute/zap1](https://github.com/frontier-compute/zap1),
[ZIP draft PR #1243](https://github.com/zcash/zips/pull/1243).
BLAKE2b Merkle commitments over typed lifecycle events, roots anchored in
Orchard shielded memos. Verification is "recompute hash, walk Merkle path, check
anchor" — no zero-knowledge proof, and no statement about payment amounts. Their
"ZAP1 Proof Profile" (optional ZK attachment) is explicitly **reserved and
unimplemented**. Different layer entirely; ZClaim proves over consensus state,
ZAP1 attests to application events.

### ZIP 311 — Payment Disclosures

[ZIP 311](https://zips.z.cash/zip-0311). Lets a *sender* prove they paid a
shielded address, optionally bound to a challenge. Two problems for us: it
reveals the payment details rather than a predicate over them, and its only
implementation was the `zcashd` RPC pair, which no longer exists.

### Others checked

- **ZShield / ZecPass / ZecAuth / ZcashMe** — no substantive public
  implementation of predicate proofs over shielded transaction state was found;
  searches surface unrelated or marketing material. Not treated as prior art.
- **`zync-core`, `zidecar`** — light-client verification primitives, useful
  later for authenticating anchors, not competing designs.
- **`pir-client` / `imt-tree`** — the nullifier non-membership infrastructure
  behind the voting protocol. Relevant only if ZClaim later needs unspent-ness.

---

## 7. Open questions carried into implementation

1. **Anchor authentication.** The circuit takes `anchor` as a public input. A
   verifier that accepts a prover-supplied anchor is trivially fooled. It must
   independently confirm the anchor is a real historical root — and, post-NU6.3,
   confirm *which pool's* root it is. The `voting-circuits` README raises the
   same requirement.
2. **Holder binding.** Knowledge of a note's fields is available to both the
   payer and the merchant. Binding the proof to a specific party requires
   incorporating `ak`/`nk` (recipient) or `ovk`-derived material (sender).
3. **Merkle path sourcing from mainnet.** Zaino exposes subtree roots and tree
   state; wiring this is engineering, not research, but it is not yet done.
