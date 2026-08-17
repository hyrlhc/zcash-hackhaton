# ZClaim

**Don't ask for the data. Ask for the proof.**

Zcash already hides the payment. We are trying to let an application ask *one
question* about that payment — and learn nothing else.

---

## What we are trying to do

A shielded ZEC payment is invisible on an explorer. That is the point of Zcash.
It is also a problem the moment a real service needs a decision:

> Did this person pay the event desk at least the ticket price?

Today, vanilla Zcash gives you two bad answers:

1. **See nothing.** The door is blind. There is no “did they pay?” API.
2. **Share a viewing key.** The door sees the exact amount, the memo, more than
   it asked for. Privacy is spent to get a boolean.

We want a third answer: **yes or no.** Not the amount. Not the wallet. Not the
transaction. Not a list of other payments.

That is the whole product.

```
Hidden payment          One question              One bit
2.7 ZEC to the cafe  →  “at least 1 ZEC?”     →  TRUE
                         exact amount              still hidden
```

If the cafe then asks `≥ 2`, `≥ 2.5`, `≥ 2.6` to hunt the 2.7, the wallet
stops. A sequence of honest questions is still an attack. We call that the
**Inference Guard**.

---

## What vanilla Zcash already does — and what it does not

| | Vanilla Zcash | With ZClaim |
|---|---|---|
| Explorer | Payment is hidden | Still hidden |
| Can an app ask “paid enough?” | No. Blind, or open the note | Yes. One bit |
| Viewing key | Opens the receipt | Not used |
| Same answer at another app | n/a | Fails. Bound to one request |
| Asking again with a higher bar | n/a | Wallet can refuse |

We are not replacing Zcash privacy. We are **not spending it** to make an app
work. The chain stays silent at the gate. The phone hands over a proof, not a
statement dump.

On a transparent chain this question is empty: the amount is already public.
ZClaim only makes sense *because* Zcash hid the payment first.

---

## Run it

```bash
cargo run --release -p quantum-cafe
```

Browser:

```bash
cargo run -p chain-api --release          # reads live testnet roots
cd apps/verifier-demo && npm install && npm run dev
```

Open [http://localhost:5173](http://localhost:5173). Leave ZClaim **off** to see
what the chain publishes (block, root — never who or how much). Turn it **on**
to ask the question.

Live roots: `https://testnet.zec.rocks:443` (public testnet, read-only). No
ZEC is sent by this demo. The 2.7 ZEC note in the proof is built locally; the
demo says so. `--chain` on the CLI shows a real verifier **refusing** that
local root.

Turkish walkthrough: [`docs/kullanim.md`](docs/kullanim.md).

---

## Honest limit

The cryptography is real. A note of ours on public testnet is not wired into
the prover yet — that needs a funded wallet scan, not a new circuit. We do not
pretend otherwise.

---

## For people who want the how

Halo2 IPA over Pasta (`k = 11`), Zcash’s own `NoteCommit^Orchard` and
`MerkleCRH^Orchard` (`orchard/unstable-voting-circuits`). No trusted setup, no
merchant-signed credential.

The wallet proves: *this Orchard note sits under this tree root, pays this
receiver, and the amount clears this bar* — bound to this app and this
request. The verifier checks the proof **and** that the root is a real chain
root. Tags are scoped per app so two doors cannot join logs. The Guard tracks
interval width and refuses a question that would pin the amount; it decides
from the question, not from the secret answer.

Tests: `cargo test --workspace --release`. Design notes:
[`docs/architecture-decision.md`](docs/architecture-decision.md),
[`docs/threat-model.md`](docs/threat-model.md).

Don't ask for the data. Ask for the proof.
