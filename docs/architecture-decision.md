# ZClaim — Architecture Decision

Status: **decided and implemented**. Date: 2026-08-17.

Evidence: `cargo test --workspace --release` — 85 tests passing, including real
Halo2 IPA proofs (not just `MockProver`) and a live read of Zcash testnet tree
state.

---

## The question

> Can real Zcash shielded transaction data be used as a private witness for a
> predicate proof?

## The answer

**YES.** The proof is over Zcash consensus state, using Zcash's own circuit
code. There is no merchant-signed credential anywhere in the design, and no
fallback architecture.

One gap remains, and it is not a cryptographic one: ZClaim has no note of its
own on a public chain yet, because that needs testnet funds and a wallet scan.
Everything that reads *from* the chain is wired and verified against
`testnet.zec.rocks` (see "Against the real chain", below).

---

## What exact witness do we have?

Private, never leaves the prover:

```
g_d          diversified base point of the recipient address
pk_d         diversified transmission key of the recipient address
v            note value, in zatoshi
rho, psi     note nullifier-derivation randomness
rcm          note commitment trapdoor  (rcm_v2 Orchard pool, rcm_v3 Ironwood pool)
path, pos    authentication path into the note commitment tree
holder_sk    the holder's long-term secret, independent of the note
```

Everything but `holder_sk` is recoverable from a note the prover can decrypt —
as recipient via `ivk`, or as sender via `ovk` (the same primitive Glasspane
exposes).

## What exact public inputs do we need?

```
0   anchor              root of the note commitment tree
1   merchant_g_d_x      \
2   merchant_g_d_y       |  the merchant's complete Orchard receiver
3   merchant_pk_d_x      |
4   merchant_pk_d_y     /
5   threshold           the comparison bound, in zatoshi
6   direction           +1 for >=, -1 for <=
7   domain_tag          H(verifier domain)
8   nullifier           Poseidon(psi, domain_tag)
9   holder_tag          Poseidon(holder_sk, domain_tag)
10  context             H(domain, nonce, predicate, purpose, expiry)
```

## What exact predicate can we prove?

> I know an Orchard-protocol note whose commitment is a leaf of the note
> commitment tree with root `anchor`; that note pays the receiver
> `(g_d, pk_d)`; its value satisfies the comparison against `threshold`; the
> published `nullifier` is derived from that note and this verifier's domain;
> the published `holder_tag` is derived from a secret I hold and this verifier's
> domain; and all of it is bound to the request `context`.

For the demo: *"I paid Quantum Cafe at least 1 ZEC"*, where the real payment was
2.7 ZEC and the verifier learns neither 2.7, nor the address, nor the
transaction, nor any other payment.

## What part is actually Zcash-native?

| Constraint | Implementation | Ours or Zcash's? |
|---|---|---|
| `cm = NoteCommit^Orchard_rcm(g_d, pk_d, v, rho, psi)` | `orchard::circuit::note_commit::gadgets::note_commit` | **Zcash's**, unmodified |
| `cmx` in tree with root `anchor` | `halo2_gadgets::sinsemilla::merkle` + `OrchardHashDomains::MerkleCrh` | **Zcash's**, unmodified |
| Proving system | `halo2_proofs` IPA over Pallas/Vesta, `k = 11` | **Zcash's**, same as the Action circuit |
| Merchant receiver equality | four equalities to public inputs | ours (trivial) |
| `v ⋛ threshold` | one linear gate + strict 10-bit lookup range check | ours (trivial) |
| `nullifier`, `holder_tag` | `halo2_gadgets::poseidon`, `P128Pow5T3` | Zcash's gadget, our composition |

We wrote no cryptography. The constraints we added are a subtraction, a range
check, four equalities and two hashes.

The `orchard/unstable-voting-circuits` feature that exposes the note-commitment
gadget exists precisely so third parties can build circuits over Orchard notes;
the Zcash shielded-voting protocol is its other consumer. Using it is the
sanctioned path, not a hack around visibility.

## Why Zcash, specifically?

Because the statement is *about Zcash consensus state*. The anchor is a real
Zcash commitment tree root; the commitment is a real Sinsemilla commitment over
a real shielded note. On a transparent chain there is no shielded note to prove
a predicate over — the amount is already public, so the question is vacuous. On
a chain without a note commitment tree there is nothing to Merkle-prove against.
The privacy is not something ZClaim adds; it is Zcash's, and ZClaim is a way to
answer questions about it without giving it up.

---

## Circuit design

Constraints, in synthesis order:

1. **Note commitment integrity** — upstream `note_commit` gadget over the
   witnessed note fields, producing `cm`.
2. **Merkle path validity** — `MerkleCRH^Orchard` from `Extract_P(cm)` up 32
   levels; the resulting root is constrained to the public `anchor`.
3. **Merchant binding** — all four coordinates of `g_d` and `pk_d` constrained
   to public inputs.
4. **Amount predicate** — `diff = direction · (v - threshold)` via a custom
   gate, with `direction` constrained to ±1 and `diff` range-checked to 70 bits.
5. **Scoped tags** — `Poseidon(psi, domain_tag)` and
   `Poseidon(holder_sk, domain_tag)`, each constrained to its public input.
6. **Request binding** — `context` copy-constrained to its public input.

### Why the merchant binding is four values, not one

An earlier version bound only `x(pk_d)`. That is unsound as a *receipt*.

Anyone can construct an on-chain note addressed to `(g_d', pk_d)` where `pk_d`
is the merchant's real transmission key and `g_d'` is a diversified base the
merchant never used. The note commits fine, lands in the tree fine, and passes
an `x(pk_d)`-only check — but `pk_d ≠ [ivk]g_d'`, so the merchant cannot detect
or spend it. The money is burned, not received, and "I paid Quantum Cafe" would
be false while provable.

The y-coordinates matter for the same reason at one remove: `-pk_d` shares an
x-coordinate with `pk_d` and is a valid curve point, but a note to it is
similarly unspendable. Binding both coordinates of both points closes the whole
family. The cost is four instance rows.

### Why the range check is sound

`NoteCommit` already constrains `v` to 64 bits. `threshold` is public and, for
any real predicate, also below 2⁶⁴. If the comparison holds then
`diff < 2⁶⁴ < 2⁷⁰`. If it fails then `diff = p - |v - threshold| > 2²⁵³`, far
outside a 70-bit range. So the 70-bit check is exactly the comparison, and
`direction` cannot be used to rescale a failing difference into range because it
is itself constrained to ±1.

The check must use `strict = true`. With `strict = false`, `copy_check`
decomposes the running sum but leaves the high limb unconstrained, imposing no
bound at all. The spike caught this; `gte_rejects_amounts_below_the_threshold`
exists because of it.

### Why the nullifier is scoped to the domain, not the context

`context` includes the request nonce, so it changes on every request. A
nullifier derived from it would change too, and would therefore be useless for
recognising a second claim on the same payment — which is its only job.

`domain_tag` is `H(verifier domain)` and nothing else. That makes the nullifier
stable for one payment at one verifier, and unrelated at any other. It is a
public input rather than a witness precisely so a prover cannot scope its tags
to a domain of its own invention.

---

## Holder binding

`nullifier = Poseidon(psi, domain_tag)` identifies the *payment*.
`holder_tag = Poseidon(holder_sk, domain_tag)` identifies the *claimant*.

`holder_sk` is a long-term wallet secret with no derivation from the note. Its
job is to stop a leaked witness from being a transferable credential: a thief
who obtains the note material can still construct a statement, but the tag they
produce is not the one this verifier has been seeing, so the claim arrives as an
obviously new party rather than as the holder.

Because the tag is domain-scoped, it doubles as a stable pseudonym within one
application — enough to admit a ticket exactly once — while being unrelated to
what the same holder shows anywhere else.

### What holder binding does *not* do

It does not establish that the claimant is the *payer*. Both the payer (via
`ovk`) and the merchant (via `ivk`) can decrypt an output note, so both know
`psi` and both can satisfy constraint 1. The honest reading of a ZClaim proof is:

> a party that can decrypt this payment, and who holds the secret behind
> `holder_tag`, attests that the payment satisfies the predicate.

Closing this properly means proving in-circuit that the prover can decrypt the
transaction's `out_ciphertext` under an `ovk` — which is ChaCha20-Poly1305 and a
BLAKE2b KDF inside Halo2, well outside hackathon scope. It is written down here
rather than papered over.

For the demo scenario the gap does not bite: the merchant is the party asking
questions or the party being named, not an adversary trying to impersonate its
own customer.

---

## Against the real chain

`zclaim-zcash` speaks the light wallet gRPC protocol (`lightwalletd` and Zaino
both serve it). Verified against `testnet.zec.rocks:443`:

```
server              v0.5.3 (ECC LightWalletD, fronting Zebra 6.3.0)
chain               test
consensus branch    37a5165b       (NU6.3 / Ironwood)
tip                 4,278,899
orchard tree        248,631 leaves
anchor              5994ca56…9bce07
```

Run it: `cargo run -p zclaim-zcash --features lightwalletd --example chain`, or
`cargo run -p quantum-cafe -- --chain`.

The protobuf messages are declared by hand, so the build needs no `protoc`.
Tree state arrives as the classic hex `CommitmentTree` encoding, which
`lightwalletd.rs` parses into `incrementalmerkletree`'s type; from there both
the anchor and the frontier a prover needs come out directly.

With a chain-backed root window, the demo's own locally-built anchor is
**refused**. That refusal is the honest boundary of the project today.

---

## What is not yet true

Stated plainly, because a proof that looks sound but is not is worse than no
proof.

### 1. No ZClaim note exists on a public chain

The tests and demo build a real Orchard commitment tree, but locally. The
cryptography is real; the note's provenance is not. Producing one needs testnet
funds, a shielded payment, and a wallet scan to recover the note — engineering
and logistics, not cryptography. Everything downstream of "here is a note and
its path" already works, and `TreeWitnessBuilder::from_frontier` accepts exactly
what `TreeState::frontier` returns.

### 2. Holder binding does not identify the payer

See "What holder binding does not do", above.

### 3. No unspent-ness proof — and we do not need one

The shielded-voting protocol needs nullifier non-membership (indexed Merkle tree
+ PIR) because a spent note must not carry voting weight. ZClaim does not: a
payment that was later moved is still a payment that happened. A deliberate
scope decision, not an oversight.

### 4. Not bound to a specific `txid`

The circuit binds to the commitment tree, not to a transaction. It proves a note
exists and pays the merchant, not that it came from a nominated transaction.
Sufficient for the predicate; worth knowing before anyone claims otherwise.

### 5. The Inference Guard is a policy layer, not a cryptographic guarantee

It bounds what a *cooperating* wallet will answer. It cannot bound what a
verifier learns from a holder who answers everything, and it is not a
differential-privacy mechanism. See `threat-model.md`.

---

## Rejected alternatives

**Merchant-signed credential** (the original sketch: merchant signs "this person
paid", user proves over the credential). Rejected: it makes the merchant a
trusted issuer and reduces Zcash to a payment rail — the proof would say nothing
about chain state, and the same design would work on any chain. The feasibility
result makes it unnecessary.

**Memo-anchored Poseidon commitment** (user embeds `Poseidon(v, merchant, …)` in
the payment memo, later proves over it). Rejected: the memo is in the encrypted
ciphertext and is not committed by `cmx`, so verifying it really is on chain
requires either in-circuit ChaCha20-Poly1305 or disclosing the OCK — which
reveals the exact value and defeats the purpose.

**Reimplementing `NoteCommit`** in our own gadget. Rejected once
`unstable-voting-circuits` was found: ~2000 lines of decomposition and canonicity
checks that we would have to write, audit, and get exactly right.

**Running the Inference Guard on the verifier.** Rejected: it is the adversary
policing itself. The guard lives with the party that has something to lose.

---

## Honesty policy for the demo

- Everything cryptographic is real: real Sinsemilla commitments, real
  `MerkleCRH^Orchard`, real Halo2 IPA proofs, real verification.
- The note's *provenance* is local, and the demo says so in its first screen
  rather than in a footnote.
- `--chain` shows the real chain refusing the local anchor, so nobody can leave
  the room believing more than is true.
