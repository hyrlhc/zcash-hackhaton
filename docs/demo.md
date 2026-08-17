# ZClaim — Demo

One command, about sixty seconds.

```bash
cargo run --release -p quantum-cafe
```

Add `--chain` to point the verifier at live Zcash testnet:

```bash
cargo run --release -p quantum-cafe -- --chain
```

The first run compiles the workspace and generates the proving key, which takes
a few minutes. Do that before you present; afterwards each run takes about five
seconds.

---

## The story

Alice paid Quantum Cafe **2.7 ZEC** in a shielded transaction. The cafe's
loyalty program wants to know whether she spent at least 1 ZEC there.

Every other system answers this by looking at the payment. ZClaim answers it
without.

---

## Beat 0 — what is real

The program says this before showing any result, so nobody has to take it on
trust later.

```
  REAL in this run
    - Orchard note, committed with NoteCommit^Orchard
    - Commitment tree hashed with MerkleCRH^Orchard
    - Halo2 proof over Pasta curves, the Orchard proving system
    - Anchor authentication, replay rejection, context binding

  STAGED in this run
    - The note was constructed locally, not read from a chain, so
      the root it sits under is one this program built.
```

**Say:** there is no mock proving mode in this repository. The one thing that is
staged is where the note came from, and section 4 shows the chain refusing it.

---

## Beat 1 — the honest question

The verifier publishes a request. The wallet proves. The verifier checks four
things: that the statement answers the request it published, that the anchor is
a root the chain produced, that the proof verifies, and that this payment has
not been claimed here before.

```
  VALID

      Merchant            Quantum Cafe
      Requirement         >= 1.00000000 ZEC
      Anchor              1c251406…7c7732 (Ironwood, height 3500000)
      Proof               4960 bytes

      Exact amount        HIDDEN
      Wallet              HIDDEN
      Transaction         HIDDEN
      Other payments      HIDDEN
      Identity            HIDDEN

      Payment tag         80118b23…23452a   (this verifier only)
      Holder pseudonym    4b4a72d2…426f24   (this verifier only)
```

**Say:** the verifier now knows one bit. Not a range, not a rounded figure — one
bit. The two tags at the bottom are the only persistent identifiers, and both
are scoped to this verifier's domain.

---

## Beat 2 — the answer does not travel

An insurer asks the same question and gets its own answer, under a completely
different pseudonym. Then it tries to reuse that answer at the cafe.

```
  The insurer asks the same question and gets its own answer.

      Holder pseudonym here    209002db…073a2c
      Holder pseudonym at the cafe   (unrelated)

  The two verifiers cannot tell they are looking at one person,
  or at one payment. Both tags are scoped to the asking domain.

  Replaying it at the cafe fails, as it must:
      the answer does not match the request: the statement does not match
      the published request
```

**Say:** a proof that works anywhere is a bearer token. This one is bound to a
single request from a single application, through the `context` public input.
And if the insurer and the cafe pool their logs, they find nothing in common.

---

## Beat 3 — the verifier turns greedy

This is the part nobody else has.

```
  >= 1.00000000 ZEC   YES   SAFE      verifier now knows: 1.00000000 ZEC or more
  >= 2.00000000 ZEC   YES   SAFE      verifier now knows: 2.00000000 ZEC or more
  >= 2.50000000 ZEC   YES   WARNING   verifier now knows: 2.50000000 ZEC or more
                                      repeated thresholds are narrowing the
                                      hidden amount to 2.00000000 .. 2.49999999 ZEC
  >= 2.60000000 ZEC   —     BLOCKED

      REQUEST BLOCKED
      answering this could narrow the amount to 2.50000000 .. 2.59999999 ZEC,
      below the 0.50000000 ZEC floor

  >= 2.70000000 ZEC   —     BLOCKED
```

**Say, and this is the line that matters:** the wallet is not refusing because
the answer is inconvenient. It decides on the question alone, before it looks at
the amount — so a refusal tells the verifier nothing it could not already have
worked out from the history it already has.

**Expect the question** *"couldn't the verifier just run its own guard and turn
it off?"* — yes, which is exactly why the guard runs on the wallet. It lives with
the party that has something to lose.

---

## Beat 4 — against the real chain (`--chain` only)

```
  Reading note commitment tree roots from https://testnet.zec.rocks:443

      Server              v0.5.3
      Chain               test
      Tip                 4278904
      Consensus branch    37a5165b

      Real anchor         5994ca56…9bce07
      Leaves in the tree  248631

      Real root           ACCEPTED  (height 4278904)
      This demo's root    REFUSED
```

**Say:** the verifier reads note commitment tree roots from a real
Ironwood-era Zcash node — consensus branch `37a5165b` is NU6.3 — and it refuses
our own demo tree. That refusal is the honest boundary of the project: the
proving side is complete, the chain-reading side is live, and what is missing is
a note of ours on testnet, which needs funds and a wallet scan rather than any
further cryptography.

---

## Beat 5 — the tally

```
      About the amount    2.50000000 ZEC or more
      Actual amount       2.70000000 ZEC   (never left the wallet)
```

**Close with:** after five questions the verifier is still 0.2 ZEC away, and it
cannot close the gap by asking.

> Don't ask for the data. Ask for the proof.

---

## Also worth showing

The test suite reads as the list of properties being claimed:

```bash
cargo test --workspace --release -- --list
```

```
a_stolen_witness_cannot_impersonate_the_holder
a_negated_transmission_key_is_not_the_merchant
one_note_yields_unlinkable_nullifiers_across_verifiers
a_proof_does_not_upgrade_to_a_stronger_threshold
a_refusal_does_not_reveal_which_way_the_answer_went
an_unprovable_claim_still_narrows_what_the_guard_believes_is_known
...
```

85 of them, including real IPA proofs rather than only `MockProver` runs.

The chain client on its own:

```bash
cargo run -p zclaim-zcash --features lightwalletd --example chain
```

---

## If something goes wrong on stage

| Symptom | Cause | What to do |
|---|---|---|
| First run hangs for minutes | Proving key generation | Run it once beforehand |
| `--chain` prints "Could not reach the node" | No network, or the endpoint is down | Drop `--chain`; beats 1–3 and 5 are self-contained |
| Different tags every run | Fresh nonces and a fresh note per run | Point at it; the unlinkability is the property |

---

## Questions to expect

**"Is the ZK proof real, or is this a mock?"** Real. `zclaim-proof` builds an
IPA proving key at `k = 11` and the printed 4960 bytes is the proof. The
note-commitment and Merkle constraints are Zcash's own gadgets, unmodified, via
`orchard/unstable-voting-circuits`.

**"What stops me from inventing a tree that contains whatever note I like?"**
Nothing in the circuit — `anchor` is a public input a prover chooses. The
verifier is what stops it, by authenticating the root against its own view of
the chain. That is why `zclaim_protocol::Verifier` cannot be constructed without
an `AnchorAuthenticator`, and why beat 4 exists.

**"Does the proof show that *you* made the payment?"** It shows that a party who
can decrypt the payment, and who holds the secret behind `holder_tag`, attests
to the predicate. Both the payer and the merchant can decrypt an output, so the
proof does not distinguish them. Closing that means proving `ovk` decryption of
the `out_ciphertext` in-circuit — ChaCha20-Poly1305 inside Halo2 — which is out
of scope and written down in `architecture-decision.md` rather than glossed.

**"Is the Inference Guard cryptography?"** No, and it is labelled as policy
everywhere it appears. It bounds what a cooperating wallet answers. See
`threat-model.md` for the full list of what it does not do.
