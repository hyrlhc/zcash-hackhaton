/* tslint:disable */
/* eslint-disable */

/**
 * An application that asks ZClaim questions.
 */
export class Verifier {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Runs every check, and throws with the reason if any fails.
     *
     * "The proof verified" is not the same as "the claim is true". This checks
     * that the statement answers the request that was published, that the
     * anchor is a root the chain produced, that the proof verifies, and that
     * the payment has not already been claimed here.
     */
    accept(request_json: string, presentation_json: string): string;
    /**
     * Whether a payment has already been claimed here.
     */
    hasClaimed(nullifier_hex: string): boolean;
    /**
     * How many roots this verifier currently accepts.
     */
    knownRootCount(): number;
    /**
     * Builds a verifier for one application domain and one shielded pool.
     *
     * Two applications must never share a domain: the domain is what makes a
     * holder's tags unlinkable between them.
     */
    constructor(domain: string, pool: string, max_anchor_age_blocks: number);
    /**
     * Records a note commitment tree root observed at `height`.
     *
     * The caller is asserting it learned this from something that speaks for
     * the chain — its own node, or an indexer it accepts. Taking a root from
     * the party presenting a proof defeats the entire mechanism, because the
     * circuit proves membership in *some* tree, not in Zcash's.
     */
    observeRoot(anchor_hex: string, height: number): void;
    /**
     * Publishes a request with a fresh single-use challenge.
     */
    request(predicate_json: string, purpose: string, expiry_height: number): string;
}

/**
 * A wallet holding one shielded payment.
 *
 * Note what is *not* on this type: no way to read the amount, the note, or the
 * holder key. A caller can ask it to answer a question, and can ask what it
 * has already leaked, and that is all.
 */
export class Wallet {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * What the asking verifier has been able to work out so far.
     *
     * This is the wallet's own estimate of what it has given away, and is what
     * a wallet UI should put in front of its user.
     */
    exposure(request_json: string): string;
    /**
     * Answers a request, or refuses it.
     */
    respond(request_json: string, chain_height: number): string;
    /**
     * Runs the guard without answering, for a consent dialog.
     *
     * The verdict depends only on the question and on history the verifier
     * already has, so showing it to the verifier reveals nothing.
     */
    screen(request_json: string): string;
    /**
     * Builds a wallet around a **locally staged** payment.
     *
     * The note and its commitment tree are real Orchard cryptography, but the
     * tree is constructed here rather than read from a chain. Any interface
     * built on this constructor has to say so — see `provenance()`.
     */
    static staged(config_json: string): Wallet;
    /**
     * The root of the staged tree this payment sits under.
     *
     * A verifier must never take an anchor from the party presenting a proof.
     * This exists so the demo can seed a verifier's root window in place of a
     * chain, and the interface has to label it as the stand-in that it is.
     */
    stagedAnchorHex(): string;
}

/**
 * Formats a zatoshi amount the way Zcash does, with eight decimal places.
 */
export function formatZec(zatoshi: string): string;

/**
 * What is real in this build and what is staged.
 *
 * Exposed as data rather than left to prose so an interface cannot forget to
 * say it.
 */
export function provenance(): string;

/**
 * A fixture merchant, in the form a verifier names one in a request.
 */
export function stagedMerchant(label: string, seed: number): string;

/**
 * Builds the proving and verifying keys ahead of time.
 *
 * Both are derived deterministically — there is no ceremony and no shared
 * secret — but derivation takes a moment, and a caller usually wants it to
 * happen behind a progress indicator rather than inside the first proof.
 */
export function warmUp(): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_verifier_free: (a: number, b: number) => void;
    readonly __wbg_wallet_free: (a: number, b: number) => void;
    readonly formatZec: (a: number, b: number) => [number, number, number, number];
    readonly provenance: () => [number, number];
    readonly stagedMerchant: (a: number, b: number, c: number) => [number, number, number, number];
    readonly verifier_accept: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly verifier_hasClaimed: (a: number, b: number, c: number) => number;
    readonly verifier_knownRootCount: (a: number) => number;
    readonly verifier_new: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly verifier_observeRoot: (a: number, b: number, c: number, d: number) => [number, number];
    readonly verifier_request: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly wallet_exposure: (a: number, b: number, c: number) => [number, number, number, number];
    readonly wallet_respond: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly wallet_screen: (a: number, b: number, c: number) => [number, number, number, number];
    readonly wallet_staged: (a: number, b: number) => [number, number, number];
    readonly wallet_stagedAnchorHex: (a: number) => [number, number];
    readonly warmUp: () => void;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
