import init, {
  formatZec as wasmFormatZec,
  provenance as wasmProvenance,
  stagedMerchant as wasmStagedMerchant,
  Verifier as WasmVerifier,
  warmUp,
} from "../wasm/zclaim.js";
import type {
  Acceptance,
  Merchant,
  Pool,
  Predicate,
  Presentation,
  ProofRequest,
  Provenance,
} from "./types.js";

let started: Promise<void> | null = null;

/**
 * Loads the WASM module. Safe to call more than once.
 * Call this once at app start, before constructing a verifier.
 */
export function start(wasmUrl?: string): Promise<void> {
  if (!started) {
    started = (wasmUrl ? init({ module_or_path: wasmUrl }) : init()).then(() => {
      warmUp();
    });
  }
  return started;
}

function parse<T>(json: string): T {
  return JSON.parse(json) as T;
}

/** A third-party verifier: asks a question, then decides whether to believe the answer. */
export class Verifier {
  private inner: WasmVerifier;

  constructor(domain: string, pool: Pool = "ironwood", maxAnchorAgeBlocks = 100) {
    this.inner = new WasmVerifier(domain, pool, maxAnchorAgeBlocks);
  }

  /**
   * Records a note-commitment-tree root. Must come from the chain, never from
   * the party presenting a proof.
   */
  observeRoot(anchorHex: string, height: number): void {
    this.inner.observeRoot(anchorHex, height);
  }

  knownRootCount(): number {
    return this.inner.knownRootCount();
  }

  /** Publishes a request with a fresh single-use challenge. */
  createProofRequest(
    predicate: Predicate,
    purpose: string,
    expiryHeight: number,
  ): ProofRequest {
    return parse(this.inner.request(JSON.stringify(predicate), purpose, expiryHeight));
  }

  /**
   * Runs every check. Throws if the statement does not match, the anchor is
   * unknown, the proof fails, or the payment was already claimed.
   */
  verifyProof(request: ProofRequest, presentation: Presentation): Acceptance {
    return parse(
      this.inner.accept(JSON.stringify(request), JSON.stringify(presentation)),
    );
  }

  verifyNullifier(nullifierHex: string): boolean {
    return this.inner.hasClaimed(nullifierHex);
  }

  free(): void {
    this.inner.free();
  }
}

export function formatZec(zatoshi: string | number): string {
  return wasmFormatZec(String(zatoshi));
}

export function stagedMerchant(label: string, seed: number): Merchant {
  return parse(wasmStagedMerchant(label, seed));
}

export function provenance(): Provenance {
  return parse(wasmProvenance());
}

export type {
  Acceptance,
  Comparison,
  Decision,
  HolderResponse,
  Knowledge,
  Merchant,
  Pool,
  Predicate,
  Presentation,
  ProofRequest,
  Provenance,
  Statement,
} from "./types.js";
export {
  DEMO_PAID_ZATOSHI,
  OTHER_CAFE_SEED,
  QUANTUM_CAFE_SEED,
  ZEC,
} from "./types.js";
