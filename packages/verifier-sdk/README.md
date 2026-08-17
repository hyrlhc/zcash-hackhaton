# `@zclaim/verifier-sdk`

Ask whether a Zcash shielded payment cleared a bar, without seeing the payment.

```ts
import { start, Verifier, stagedMerchant, QUANTUM_CAFE_SEED, ZEC } from "@zclaim/verifier-sdk";
import { Wallet } from "@zclaim/verifier-sdk/wallet";

await start();
const wallet = Wallet.staged();
const verifier = new Verifier("my-app.example", "ironwood");
verifier.observeRoot(wallet.stagedAnchorHex(), 3_500_000); // demo only

const request = verifier.createProofRequest(
  {
    merchant: stagedMerchant("quantum-cafe", QUANTUM_CAFE_SEED),
    amount: { operator: "GTE", value: ZEC },
  },
  "loyalty-tier",
  3_500_100,
);

const answer = wallet.respond(request, 3_500_000);
const accepted = verifier.verifyProof(request, answer.presentation);
```

Rebuild WASM after Rust changes:

```bash
npm run build:wasm
```
