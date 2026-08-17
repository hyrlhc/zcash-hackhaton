//! Deterministic fixtures for tests and demos.
//!
//! Notes and trees built here use real Orchard cryptography — real Sinsemilla
//! commitments, real `MerkleCRH^Orchard`. What is *not* real is their
//! provenance: the tree is constructed locally rather than read from a chain.
//! Anything user-facing that relies on this module must say so.

use ff::PrimeField;
use incrementalmerkletree::{Hashable, Level};
use orchard::{
    keys::{FullViewingKey, Scope, SpendingKey},
    note::{ExtractedNoteCommitment, RandomSeed, Rho},
    tree::{MerkleHashOrchard, MerklePath},
    value::NoteValue,
    Address, Note, NoteVersion, NOTE_COMMITMENT_TREE_DEPTH,
};
use pasta_curves::pallas;
use zclaim_core::{HolderKey, Merchant};

use crate::statement::MerchantBinding;

/// One ZEC, in zatoshi.
pub const ZEC: u64 = 100_000_000;

/// Seed identifying the demo merchant.
pub const QUANTUM_CAFE: u8 = 0x51;
/// Seed identifying a different merchant, for negative tests.
pub const OTHER_CAFE: u8 = 0x77;

/// A deterministic Orchard address for the given seed.
pub fn address_for(seed: u8) -> Address {
    let sk = SpendingKey::from_bytes([seed; 32]).expect("seed is a valid spending key");
    FullViewingKey::from(&sk).address_at(0u32, Scope::External)
}

/// The merchant binding the circuit compares a note's recipient against.
pub fn binding_for(addr: &Address) -> MerchantBinding {
    MerchantBinding::from_address(addr)
}

/// The demo merchant as a verifier would name it in a request.
pub fn merchant_named(label: &str, seed: u8) -> Merchant {
    Merchant::new(label, &address_for(seed).to_raw_address_bytes())
}

/// A deterministic holder key. Distinct seeds give distinct holders.
pub fn holder(seed: u8) -> HolderKey {
    HolderKey::from_seed(&[seed; 32])
}

/// A real Ironwood-format (`V3`) note paying `value_zat` to `recipient`.
pub fn note_paying(recipient: Address, value_zat: u64, seed: u8) -> Note {
    let rho = Rho::from_bytes(&field_bytes(seed)).expect("seed is a valid rho");
    let rseed = RandomSeed::from_bytes(field_bytes(seed ^ 0x5A), &rho).expect("rseed is valid");
    Note::from_parts(
        recipient,
        NoteValue::from_raw(value_zat),
        rho,
        rseed,
        NoteVersion::V3,
    )
    .expect("note parts are consistent")
}

/// Spreads `seed` over 32 bytes that are always below the Pallas modulus.
///
/// Zeroing the top byte keeps the value under 2^248, so every seed a caller
/// picks yields a usable field element instead of failing for the handful of
/// byte patterns that happen to be non-canonical.
fn field_bytes(seed: u8) -> [u8; 32] {
    let mut bytes = [seed; 32];
    bytes[31] = 0;
    bytes
}

/// Places `cmx` at position 0 of an otherwise empty note commitment tree,
/// returning the authentication path and the anchor it hashes to.
pub fn tree_witness(
    cmx: ExtractedNoteCommitment,
) -> ([pallas::Base; NOTE_COMMITMENT_TREE_DEPTH], pallas::Base) {
    let auth_path: [MerkleHashOrchard; NOTE_COMMITMENT_TREE_DEPTH] =
        core::array::from_fn(|i| MerkleHashOrchard::empty_root(Level::from(i as u8)));
    let anchor = MerklePath::from_parts(0, auth_path).root(cmx);
    (
        auth_path.map(|n| n.inner()),
        pallas::Base::from_repr(anchor.to_bytes()).expect("anchor is a field element"),
    )
}

/// Builds the note, tree and witness for a payment in one step.
pub fn payment_witness(
    recipient: Address,
    value_zat: u64,
    seed: u8,
    holder: HolderKey,
) -> (crate::NoteWitness, pallas::Base) {
    let note = note_paying(recipient, value_zat, seed);
    let cmx = ExtractedNoteCommitment::from(note.commitment());
    let (path, anchor) = tree_witness(cmx);
    (crate::NoteWitness::from_note(&note, holder, path, 0), anchor)
}
