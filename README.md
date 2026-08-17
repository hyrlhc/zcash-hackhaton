# ZClaim

**Don't ask for the data. Ask for the proof.**

ZClaim proves that a Zcash shielded payment satisfies a condition, without
revealing the payment.

A user pays Quantum Cafe 2.7 ZEC. A verifier asks *"did you pay Quantum Cafe at
least 1 ZEC?"* ZClaim answers `TRUE`. The verifier does not learn the amount,
the address, the transaction, or any other payment — and when it tries to
extract the amount by asking again with a higher bar, the wallet stops
answering.

```
cargo run --release -p quantum-cafe
```

Tarayıcı demosu:

```
cd apps/verifier-demo && npm install && npm run dev
# başka terminal:
cargo run -p chain-api --release
```

Tarayıcı: [http://localhost:5173](http://localhost:5173)

Üstteki anahtar **ZClaim kapalı** ile açılır. Sayfa yüklenince gerçek testnet kökü istenir.

Türkçe anlatım ve adım adım kullanım: [`docs/kullanim.md`](docs/kullanim.md).

---

## The two claims

**1. One question, one bit.** The proof is a real Halo2 proof over a real
Orchard note, using Zcash's own note-commitment and Merkle gadgets. No
merchant-signed credential, no trusted issuer, no oracle.

**2. A sequence of questions is also an attack.** Each of `>= 1`, `>= 2`,
`>= 2.5`, `>= 2.6` is individually sound; together they bisect the amount out of
a system built to hide it. The **Inference Guard** tracks what the answers add
up to and refuses before the interval narrows past a floor. It runs on the
wallet, because the verifier is the adversary.

The second is what makes this more than a circuit.

---

## Status

`cargo test --workspace --release` — **85 tests passing**, including real IPA
proofs (not only `MockProver`) and a live read of Zcash testnet tree state.

| | |
|---|---|
| Note commitment | Zcash's own `NoteCommit^Orchard` gadget, unmodified |
| Tree membership | Zcash's own `MerkleCRH^Orchard`, 32 levels |
| Proving system | `halo2_proofs` IPA over Pallas/Vesta, no trusted setup, `k = 11` |
| Pools | Orchard and Ironwood (NU6.3) — one circuit covers both |
| Chain reads | Live, via light wallet gRPC against `testnet.zec.rocks:443` |
| Chain writes | **None.** No ZClaim note exists on a public chain yet |

The last row is the honest caveat, and the demo says it on its own first screen.
The tests and demo build a genuine Orchard commitment tree, but locally.
Producing an on-chain one needs testnet funds and a wallet scan — logistics, not
cryptography. Run `--chain` and watch a chain-backed verifier refuse the demo's
own root.

There is no mock proving mode anywhere in the repository.

---

## The statement

Private witness, never leaves the prover:

```
g_d, pk_d       recipient address points
v               note value, in zatoshi
rho, psi, rcm   note randomness
path, pos       authentication path into the note commitment tree
holder_sk       long-term wallet secret, independent of the note
```

Public inputs, all the verifier sees:

```
anchor                          note commitment tree root
merchant g_d.x, g_d.y,
         pk_d.x, pk_d.y         the merchant's complete Orchard receiver
threshold, direction            the bar, and which way it points
domain_tag                      H(verifier domain)
nullifier    = Poseidon(psi, domain_tag)
holder_tag   = Poseidon(holder_sk, domain_tag)
context      = H(domain, nonce, predicate, purpose, expiry)
```

> I know an Orchard note committed under `anchor`, paying the receiver
> `(g_d, pk_d)`, whose value satisfies the comparison against `threshold`; the
> published tags are derived from that note and from a secret I hold, both
> scoped to this verifier; and all of it is bound to `context`.

Everything but `holder_sk` is recoverable from a note the prover can decrypt —
as recipient via `ivk`, or as sender via `ovk`.

## Why Zcash

The statement is *about Zcash consensus state*. The anchor is a Zcash commitment
tree root; the commitment is a Sinsemilla commitment over a shielded note. On a
transparent chain the amount is already public, so the question is vacuous; on a
chain without a note commitment tree there is nothing to Merkle-prove against.
The privacy is not something ZClaim adds — it is Zcash's, and ZClaim is a way to
answer questions about it without giving it up.

The `orchard/unstable-voting-circuits` feature that exposes the note-commitment
gadget exists so third parties can build circuits over Orchard notes. Using it
is the sanctioned path, not a workaround.

## Three design decisions worth a second look

**The merchant binding is four field elements, not one.** Binding only
`x(pk_d)` is unsound as a receipt: anyone can address a note to the merchant's
real `pk_d` under a diversified base the merchant never used. It commits fine
and lands in the tree fine, but the merchant can neither detect nor spend it —
the money is burned, not received. Binding both coordinates of both points
closes the family. Costs four instance rows.

**The tags are scoped to `H(domain)`, not to the request context.** `context`
carries a fresh nonce, so a nullifier derived from it would change every request
and be useless for spotting a second claim on the same payment. Scoping to the
domain makes the nullifier stable at one verifier and unrelated at any other.

**The guard decides on the question, never on the answer.** It evaluates the
*narrower* of the two intervals the two possible answers would produce. If it
decided on the branch actually taken, a refusal where an answer was expected
would itself leak which way the comparison went.

## How this differs from existing work

- **[Glasspane](https://github.com/dolepee/glasspane)** shares an Outgoing
  Cipher Key so a verifier can decrypt one output — recovering the exact value.
  That is disclosure. ZClaim proves a predicate and reveals nothing.
- **[ZAP1](https://github.com/frontier-compute/zap1)** anchors BLAKE2b Merkle
  roots of application events in shielded memos. No ZK, no statement about
  amounts, different layer.
- **[Shielded Voting](https://github.com/valargroup/voting-circuits)** is the
  closest work and shares the upstream mechanism. It proves aggregate ownership
  for governance weight. ZClaim binds to a *recipient* and a payment predicate
  for third-party verifiers, needs no unspent-ness proof, and adds the Inference
  Guard.

Full comparison in [`docs/research.md`](docs/research.md).

## Layout

```
crates/zclaim-core/        predicates, verifier domains, request context
crates/zclaim-circuits/    the Halo2 circuit, witness, public statement
crates/zclaim-proof/       proving and verifying keys, proof bytes
crates/zclaim-inference/   the Inference Guard
crates/zclaim-zcash/       anchor authentication, tree witnesses, gRPC chain client
crates/zclaim-protocol/    the Holder and Verifier roles, wired together
apps/quantum-cafe/         terminal demo
apps/verifier-demo/        browser demo
packages/verifier-sdk/     TypeScript SDK (WASM)
```

```
docs/research.md              stack survey, prior art, feasibility questions
docs/architecture-decision.md the design, and what is not yet true
docs/threat-model.md          what holds, what does not, and against whom
docs/demo.md                  how to run and narrate the demo
docs/kullanim.md              Turkish explainer + usage guide
```

## Development

Rust is pinned by `rust-toolchain.toml` (1.97.1). No `protoc` needed — the
protobuf messages are declared by hand.

```bash
cargo test --workspace --release
cargo test --workspace --release --features zclaim-zcash/lightwalletd
cargo clippy --workspace --all-targets -- -D warnings

cargo run --release -p quantum-cafe            # the demo
cargo run --release -p quantum-cafe -- --chain # ...against live testnet
cargo run -p zclaim-zcash --features lightwalletd --example chain

cd apps/verifier-demo && npm install && npm run dev   # browser demo
```

The first release build generates the proving key and takes a few minutes. Do
that before demoing.
