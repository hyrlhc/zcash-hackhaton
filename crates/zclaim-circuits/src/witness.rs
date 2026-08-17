//! The private witness, derived from a note the prover can decrypt.

use group::Curve;
use halo2_gadgets::poseidon::primitives::{self as poseidon, ConstantLength};
use orchard::{note::NoteCommitTrapdoor, Note, NoteVersion, NOTE_COMMITMENT_TREE_DEPTH};
use pasta_curves::pallas;
use zclaim_core::{DomainTag, HolderKey};

/// Everything the prover holds and never sends.
#[derive(Clone, Debug)]
pub struct NoteWitness {
    /// The prover's long-term secret. Not derived from the note: this is what
    /// makes answering require being the holder rather than merely having once
    /// seen the note.
    pub holder: HolderKey,
    /// Diversified base point of the recipient address.
    pub g_d: pallas::Affine,
    /// Diversified transmission key of the recipient address.
    pub pk_d: pallas::Affine,
    /// Note value, in zatoshi.
    pub value: u64,
    /// Note `rho`.
    pub rho: pallas::Base,
    /// Note `psi`.
    pub psi: pallas::Base,
    /// Note commitment trapdoor.
    pub rcm: pallas::Scalar,
    /// Authentication path from the note's commitment to the tree root.
    pub merkle_path: [pallas::Base; NOTE_COMMITMENT_TREE_DEPTH],
    /// Leaf position of the note's commitment.
    pub merkle_pos: u32,
}

impl NoteWitness {
    /// Extracts the witness from a decrypted note plus its tree position.
    ///
    /// The note must be one the prover can decrypt: as recipient via `ivk`, or
    /// as sender via `ovk`.
    pub fn from_note(
        note: &Note,
        holder: HolderKey,
        merkle_path: [pallas::Base; NOTE_COMMITMENT_TREE_DEPTH],
        merkle_pos: u32,
    ) -> Self {
        let recipient = note.recipient();
        let rho = note.rho();
        let rseed = note.rseed();

        NoteWitness {
            holder,
            g_d: recipient.g_d().to_affine(),
            pk_d: recipient.pk_d().inner().to_affine(),
            value: note.value().inner(),
            rho: rho.into_inner(),
            psi: rseed.psi(&rho),
            rcm: note_rcm(note).inner(),
            merkle_path,
            merkle_pos,
        }
    }

    /// The nullifier this witness will produce for `domain`.
    pub fn nullifier(&self, domain: DomainTag) -> pallas::Base {
        note_nullifier(self.psi, domain)
    }

    /// The holder tag this witness will produce for `domain`.
    pub fn holder_tag(&self, domain: DomainTag) -> pallas::Base {
        holder_tag(&self.holder, domain)
    }
}

/// Recomputes a note's commitment trapdoor for its plaintext version.
///
/// Ironwood notes (`V3`, ZIP 2005) bind `g_d`, `pk_d`, `value` and `psi` into
/// `rcm` so that the commitment is post-quantum binding; Orchard-pool notes
/// (`V2`) do not.
fn note_rcm(note: &Note) -> NoteCommitTrapdoor {
    let rho = note.rho();
    let rseed = note.rseed();
    match note.version() {
        NoteVersion::V2 => rseed.rcm_v2(&rho),
        NoteVersion::V3 => {
            let recipient = note.recipient();
            rseed.rcm_v3(
                &rho,
                &recipient.g_d(),
                &recipient.pk_d().inner(),
                note.value().inner(),
                &rseed.psi(&rho),
            )
        }
    }
}

/// The ZClaim nullifier: `Poseidon(psi, domain_tag)`.
///
/// `psi` is note-specific and secret — it is derived from the note's `rseed`,
/// which only a party that can decrypt the note plaintext knows. Because the
/// circuit also feeds `psi` into `NoteCommit`, the nullifier is bound to the
/// same note the Merkle path proves, and cannot be chosen freely by the prover.
///
/// The scope is the verifier's domain and nothing else. Scoping to the full
/// request context instead would make the nullifier change on every nonce,
/// which would defeat its purpose:
///
/// - **Double-claim prevention.** One payment yields one nullifier at a given
///   verifier, no matter how the question is phrased, so a second claim on the
///   same payment is recognisable.
/// - **Unlinkability.** Two verifiers see unrelated nullifiers for the same
///   payment, so they cannot correlate a holder by comparing notes.
///
/// What this does *not* do is identify *which party* is proving: both the payer
/// (via `ovk`) and the merchant (via `ivk`) can decrypt the note and so both
/// know `psi`. That is what [`holder_tag`] is for.
pub fn note_nullifier(psi: pallas::Base, domain: DomainTag) -> pallas::Base {
    poseidon::Hash::<_, poseidon::P128Pow5T3, ConstantLength<2>, 3, 2>::init()
        .hash([psi, domain.inner()])
}

/// The holder tag: `Poseidon(holder_secret, domain_tag)`.
///
/// Where the nullifier answers "which payment is this claim about", the holder
/// tag answers "who is claiming". It is what stops a leaked witness from being
/// a transferable credential: whoever proves must also know the holder secret,
/// and the tag they produce will not match the one the legitimate holder has
/// been showing this verifier.
///
/// Because the scope is the verifier's domain, the tag is a stable pseudonym
/// within one application — enough to admit a ticket exactly once — and carries
/// no relation to the tag the same holder shows anywhere else.
pub fn holder_tag(holder: &HolderKey, domain: DomainTag) -> pallas::Base {
    poseidon::Hash::<_, poseidon::P128Pow5T3, ConstantLength<2>, 3, 2>::init()
        .hash([holder.secret(), domain.inner()])
}
