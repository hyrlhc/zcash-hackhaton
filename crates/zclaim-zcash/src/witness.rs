//! Building a tree witness for a note.
//!
//! To prove a note is in the commitment tree we need its authentication path.
//! The path is built the way every Zcash wallet builds one: take the tree
//! frontier as of the block *before* the note's commitment was appended, then
//! append commitments in chain order and read the path off the result.

use ff::PrimeField;
use incrementalmerkletree::{Hashable, Level};
use orchard::{
    note::ExtractedNoteCommitment,
    tree::{MerkleHashOrchard, MerklePath},
    Note, NOTE_COMMITMENT_TREE_DEPTH,
};
use pasta_curves::pallas;
use zclaim_circuits::NoteWitness;
use zclaim_core::HolderKey;

use crate::Error;

/// Accumulates note commitments in chain order and produces the authentication
/// path for one of them.
///
/// This is the wallet-side half of the flow: the commitments come from
/// compact blocks (Zaino's `GetBlockRange`, or `lightwalletd`'s equivalent),
/// and the starting frontier from `GetTreeState`.
#[derive(Debug)]
pub struct TreeWitnessBuilder {
    /// Left-hand siblings, indexed by level, for the path down to
    /// `base_position`. Levels where no left sibling exists hold the empty
    /// subtree root for that level.
    frontier: [MerkleHashOrchard; NOTE_COMMITMENT_TREE_DEPTH],
    /// Commitments appended since the frontier, in chain order.
    appended: Vec<MerkleHashOrchard>,
    /// Position of the first appended commitment.
    base_position: u32,
}

impl TreeWitnessBuilder {
    /// Starts from an empty tree. Useful for regtest and for tests; on a real
    /// chain, start from [`TreeWitnessBuilder::from_frontier`].
    pub fn empty() -> Self {
        TreeWitnessBuilder {
            frontier: empty_levels(),
            appended: Vec::new(),
            base_position: 0,
        }
    }

    /// Starts from a tree frontier read from the chain.
    ///
    /// `frontier[level]` is the left-hand sibling at that level on the path
    /// down to `base_position`, i.e. the ommer the indexer reports for a tree
    /// holding exactly `base_position` leaves. Levels with no left sibling —
    /// those where bit `level` of `base_position` is zero — are ignored, so a
    /// caller may pass anything there.
    pub fn from_frontier(
        frontier: [pallas::Base; NOTE_COMMITMENT_TREE_DEPTH],
        base_position: u32,
    ) -> Result<Self, Error> {
        let mut nodes = empty_levels();
        for (slot, value) in nodes.iter_mut().zip(frontier.iter()) {
            *slot = Option::from(MerkleHashOrchard::from_bytes(&value.to_repr()))
                .ok_or_else(|| Error::MalformedTree("frontier node is not a valid hash".into()))?;
        }

        Ok(TreeWitnessBuilder {
            frontier: nodes,
            appended: Vec::new(),
            base_position,
        })
    }

    /// Appends a note commitment observed on-chain, returning its position.
    pub fn append(&mut self, cmx: ExtractedNoteCommitment) -> u32 {
        let position = self.base_position + self.appended.len() as u32;
        self.appended.push(MerkleHashOrchard::from_cmx(&cmx));
        position
    }

    /// Number of commitments appended so far.
    pub fn len(&self) -> usize {
        self.appended.len()
    }

    /// Whether anything has been appended.
    pub fn is_empty(&self) -> bool {
        self.appended.is_empty()
    }

    /// Produces the authentication path and resulting anchor for the
    /// commitment at `position`.
    ///
    /// The path returned here is what the prover feeds the circuit; the anchor
    /// is what the verifier must independently authenticate against the chain.
    pub fn witness_at(
        &self,
        position: u32,
    ) -> Result<([pallas::Base; NOTE_COMMITMENT_TREE_DEPTH], pallas::Base), Error> {
        let index = self.index_of(position)?;
        let cmx = cmx_from_node(self.appended[index])?;
        let auth_path = self.auth_path(position);

        let anchor = MerklePath::from_parts(position, auth_path).root(cmx);

        Ok((
            auth_path.map(|n| n.inner()),
            pallas::Base::from_repr(anchor.to_bytes())
                .expect("an Orchard anchor is always a field element"),
        ))
    }

    /// Builds the full witness for `note` at `position`, under `holder`.
    pub fn note_witness(
        &self,
        note: &Note,
        holder: HolderKey,
        position: u32,
    ) -> Result<NoteWitness, Error> {
        let (path, _anchor) = self.witness_at(position)?;
        Ok(NoteWitness::from_note(note, holder, path, position))
    }

    /// The anchor the tree currently hashes to, given everything appended.
    pub fn anchor_for(&self, position: u32) -> Result<pallas::Base, Error> {
        self.witness_at(position).map(|(_, anchor)| anchor)
    }

    /// Walks the tree one level at a time, collecting the sibling of
    /// `position` at each level.
    ///
    /// Only the span of nodes covering the appended commitments is ever
    /// materialised. Everything to its left is summarised by the frontier and
    /// everything to its right is an empty subtree, which is why this costs
    /// `O(appended × depth)` rather than `O(2^depth)`.
    fn auth_path(&self, position: u32) -> [MerkleHashOrchard; NOTE_COMMITMENT_TREE_DEPTH] {
        let mut path = empty_levels();

        // `layer` holds the nodes covering indices `[start, start + layer.len())`
        // at the current level.
        let mut layer = self.appended.clone();
        let mut start = self.base_position as u64;

        for (level, slot) in path.iter_mut().enumerate() {
            let level_u8 = level as u8;

            // Pad the layer out to even alignment on both ends, so that every
            // node in it has its sibling present.
            if start % 2 == 1 {
                layer.insert(0, self.frontier[level]);
                start -= 1;
            }
            if layer.len() % 2 == 1 {
                layer.push(MerkleHashOrchard::empty_root(Level::from(level_u8)));
            }

            // The sibling of the target is now guaranteed to be inside the
            // padded layer, because a sibling is only ever one index away.
            let target = (position as u64) >> level;
            let sibling = target ^ 1;
            *slot = layer[(sibling - start) as usize];

            layer = layer
                .chunks(2)
                .map(|pair| MerkleHashOrchard::combine(Level::from(level_u8), &pair[0], &pair[1]))
                .collect();
            start /= 2;
        }

        path
    }

    fn index_of(&self, position: u32) -> Result<usize, Error> {
        position
            .checked_sub(self.base_position)
            .map(|i| i as usize)
            .filter(|i| *i < self.appended.len())
            .ok_or_else(|| {
                Error::MalformedTree(format!("position {position} is outside this witness range"))
            })
    }
}

fn empty_levels() -> [MerkleHashOrchard; NOTE_COMMITMENT_TREE_DEPTH] {
    core::array::from_fn(|i| MerkleHashOrchard::empty_root(Level::from(i as u8)))
}

fn cmx_from_node(node: MerkleHashOrchard) -> Result<ExtractedNoteCommitment, Error> {
    Option::from(ExtractedNoteCommitment::from_bytes(&node.to_bytes()))
        .ok_or_else(|| Error::MalformedTree("leaf is not a valid note commitment".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zclaim_circuits::testing::{address_for, note_paying, tree_witness, QUANTUM_CAFE, ZEC};

    fn cmx_of(note: &Note) -> ExtractedNoteCommitment {
        ExtractedNoteCommitment::from(note.commitment())
    }

    /// A single note in an empty tree must produce the same anchor as the
    /// reference path construction.
    #[test]
    fn a_lone_note_matches_the_reference_anchor() {
        let note = note_paying(address_for(QUANTUM_CAFE), 27 * ZEC / 10, 0x09);

        let mut builder = TreeWitnessBuilder::empty();
        let position = builder.append(cmx_of(&note));
        assert_eq!(position, 0);

        let (path, anchor) = builder.witness_at(0).unwrap();
        let (expected_path, expected_anchor) = tree_witness(cmx_of(&note));

        assert_eq!(path, expected_path);
        assert_eq!(anchor, expected_anchor);
    }

    /// With several commitments in the tree, the witness for each one must
    /// reproduce the same anchor.
    #[test]
    fn every_note_in_a_filled_tree_reaches_the_same_anchor() {
        let merchant = address_for(QUANTUM_CAFE);
        let notes: Vec<_> = (0..5)
            .map(|i| note_paying(merchant, (i as u64 + 1) * ZEC, 0x20 + i))
            .collect();

        let mut builder = TreeWitnessBuilder::empty();
        let positions: Vec<_> = notes.iter().map(|n| builder.append(cmx_of(n))).collect();

        let anchors: Vec<_> = positions
            .iter()
            .map(|p| builder.anchor_for(*p).unwrap())
            .collect();

        assert!(
            anchors.windows(2).all(|w| w[0] == w[1]),
            "all notes in one tree must share an anchor"
        );
    }

    /// Appending more commitments after a note moves the anchor. This is the
    /// reason a verifier must accept a window of recent roots rather than only
    /// the tip.
    #[test]
    fn appending_more_commitments_changes_the_anchor() {
        let merchant = address_for(QUANTUM_CAFE);
        let note = note_paying(merchant, ZEC, 0x31);

        let mut builder = TreeWitnessBuilder::empty();
        let position = builder.append(cmx_of(&note));
        let before = builder.anchor_for(position).unwrap();

        builder.append(cmx_of(&note_paying(merchant, 2 * ZEC, 0x32)));
        let after = builder.anchor_for(position).unwrap();

        assert_ne!(before, after);
    }

    /// A witness built from a mid-chain frontier must agree with one built by
    /// replaying the whole tree from empty.
    #[test]
    fn a_mid_chain_frontier_agrees_with_a_full_replay() {
        let merchant = address_for(QUANTUM_CAFE);
        let notes: Vec<_> = (0..7)
            .map(|i| note_paying(merchant, (i as u64 + 1) * ZEC, 0x40 + i))
            .collect();

        let mut full = TreeWitnessBuilder::empty();
        for note in &notes {
            full.append(cmx_of(note));
        }

        // Rebuild from a frontier covering the first three commitments.
        let split = 3u32;
        let frontier = frontier_after(&notes[..split as usize]);
        let mut partial = TreeWitnessBuilder::from_frontier(frontier, split).unwrap();
        for note in &notes[split as usize..] {
            partial.append(cmx_of(note));
        }

        for position in split..notes.len() as u32 {
            assert_eq!(
                full.witness_at(position).unwrap(),
                partial.witness_at(position).unwrap(),
                "position {position} disagrees between replay and frontier"
            );
        }
    }

    #[test]
    fn a_position_outside_the_range_is_rejected() {
        let mut builder = TreeWitnessBuilder::empty();
        builder.append(cmx_of(&note_paying(address_for(QUANTUM_CAFE), ZEC, 0x09)));

        assert!(builder.witness_at(7).is_err());
    }

    /// The left-hand siblings of the path down to position `notes.len()`,
    /// which is what an indexer's tree state reports.
    fn frontier_after(notes: &[Note]) -> [pallas::Base; NOTE_COMMITMENT_TREE_DEPTH] {
        let mut layer: Vec<_> = notes
            .iter()
            .map(|n| MerkleHashOrchard::from_cmx(&cmx_of(n)))
            .collect();
        let mut ommers = empty_levels();
        let next = notes.len() as u64;

        for (level, slot) in ommers.iter_mut().enumerate() {
            if (next >> level) & 1 == 1 {
                *slot = layer[((next >> level) - 1) as usize];
            }
            if layer.len() % 2 == 1 {
                layer.push(MerkleHashOrchard::empty_root(Level::from(level as u8)));
            }
            layer = layer
                .chunks(2)
                .map(|p| MerkleHashOrchard::combine(Level::from(level as u8), &p[0], &p[1]))
                .collect();
        }

        ommers.map(|n| n.inner())
    }
}
