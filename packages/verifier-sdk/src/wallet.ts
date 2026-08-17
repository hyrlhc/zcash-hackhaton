import { Wallet as WasmWallet } from "../wasm/zclaim.js";
import { start } from "./index.js";
import type { Decision, HolderResponse, Knowledge, ProofRequest, StagedPaymentConfig } from "./types.js";
import { DEMO_PAID_ZATOSHI, QUANTUM_CAFE_SEED } from "./types.js";

function parse<T>(json: string): T {
  return JSON.parse(json) as T;
}

/**
 * A wallet holding one shielded payment.
 *
 * There is no getter for the amount, the note, or the holder key. You can ask
 * it a question, and you can ask what it has already leaked. That is all.
 */
export class Wallet {
  private inner: WasmWallet;

  private constructor(inner: WasmWallet) {
    this.inner = inner;
  }

  /** Builds a wallet around a locally staged payment. Real crypto, local tree. */
  static staged(config: StagedPaymentConfig = demoPayment()): Wallet {
    return new Wallet(WasmWallet.staged(JSON.stringify(config)));
  }

  /**
   * The root of the staged tree. A real verifier must never take this from the
   * prover; the demo uses it only because there is no on-chain note yet.
   */
  stagedAnchorHex(): string {
    return this.inner.stagedAnchorHex();
  }

  screen(request: ProofRequest): Decision {
    return parse(this.inner.screen(JSON.stringify(request)));
  }

  respond(request: ProofRequest, chainHeight: number): HolderResponse {
    return parse(this.inner.respond(JSON.stringify(request), chainHeight));
  }

  exposure(request: ProofRequest): Knowledge {
    return parse(this.inner.exposure(JSON.stringify(request)));
  }

  free(): void {
    this.inner.free();
  }
}

export function demoPayment(): StagedPaymentConfig {
  return {
    paidZatoshi: DEMO_PAID_ZATOSHI,
    merchantSeed: QUANTUM_CAFE_SEED,
    noteSeed: 0x09,
    holderSeed: 0x11,
  };
}

export { start };
export type { HolderResponse, StagedPaymentConfig };
