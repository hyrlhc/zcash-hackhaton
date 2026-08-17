/** A comparison against a hidden amount. */
export type Comparison = "GTE" | "LTE";

/** Which shielded pool an anchor belongs to. */
export type Pool = "orchard" | "ironwood";

/** The merchant named in a request. `label` is display-only. */
export interface Merchant {
  label: string;
  /** Hex-encoded raw Orchard address, 43 bytes. */
  address: string;
}

/** The question a verifier may ask. */
export interface Predicate {
  merchant: Merchant;
  amount: {
    operator: Comparison;
    /** Zatoshi. JSON numbers are unsafe above 2^53; keep this as a number only for demo-scale values. */
    value: number;
  };
}

/** Everything a verifier publishes when it asks. */
export interface ProofRequest {
  domain: string;
  nonce: number[];
  predicate: Predicate;
  purpose: string;
  expiry_height: number;
}

export type DecisionStatus = "SAFE" | "WARNING" | "BLOCK";

export interface Knowledge {
  low: string;
  high: string;
  exact: boolean;
  describe: string;
}

export interface Decision {
  status: DecisionStatus;
  resulting?: Knowledge;
  reason?: string;
}

export type RefusalKind = "GUARDED" | "EXPIRED" | "CLAIM_IS_FALSE";

export interface HolderResponse {
  status: "ANSWER" | "REFUSED";
  decision?: Decision;
  presentation?: Presentation;
  refusal?: RefusalKind;
  message?: string;
}

export interface Presentation {
  proof: string;
  statement: Statement;
}

export interface Statement {
  anchor: string;
  pool: Pool;
  merchant: {
    gDX: string;
    gDY: string;
    pkDX: string;
    pkDY: string;
  };
  threshold: string;
  direction: number;
  domainTag: string;
  nullifier: string;
  holderTag: string;
  context: string;
}

export interface Acceptance {
  predicate: Predicate;
  anchor: string;
  pool: Pool;
  anchorHeight: number;
  nullifier: string;
  holderTag: string;
}

export interface Provenance {
  real: string[];
  staged: string[];
  mockProving: boolean;
}

export interface StagedPaymentConfig {
  paidZatoshi: string;
  merchantSeed: number;
  noteSeed: number;
  holderSeed: number;
}

/** Demo merchant seed used by the Quantum Cafe fixtures. */
export const QUANTUM_CAFE_SEED = 0x51;
/** A different merchant, for negative tests. */
export const OTHER_CAFE_SEED = 0x77;
/** One ZEC in zatoshi. */
export const ZEC = 100_000_000;
/** What Alice paid in the demo: 2.7 ZEC. */
export const DEMO_PAID_ZATOSHI = String((27 * ZEC) / 10);
