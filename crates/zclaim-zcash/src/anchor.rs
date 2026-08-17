//! Anchor authentication — the check that makes a ZClaim proof mean something.
//!
//! The circuit's `anchor` is a public input, so the proof establishes only that
//! the note sits in a tree with that root. A prover is perfectly free to build
//! its own tree containing a note it invented, and produce a valid proof about
//! it. Verifying the proof and stopping there proves nothing about Zcash.
//!
//! This module is where a verifier decides that a root is one Zcash actually
//! produced, in the pool it meant to ask about, recently enough to matter.

use std::collections::HashMap;

use ff::PrimeField;
use pasta_curves::pallas;
use zclaim_circuits::Pool;

use crate::Error;

/// A tree root a verifier is willing to accept, and what is known about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthenticatedAnchor {
    /// The root.
    pub anchor: pallas::Base,
    /// Which pool's tree it is a root of.
    pub pool: Pool,
    /// The block height at which this root was the tree state.
    pub height: u32,
}

/// Where a verifier gets its trusted set of roots.
///
/// A verifier must obtain roots from something it trusts to speak for the
/// chain — its own Zebra node, or an indexer it accepts. It must never take a
/// root from the party supplying the proof.
pub trait AnchorSource {
    /// Returns what is known about `anchor`, or `None` if this source has never
    /// seen it as a root of `pool`'s tree.
    fn lookup(&self, anchor: pallas::Base, pool: Pool) -> Option<AuthenticatedAnchor>;

    /// The height of the chain tip, used to age out old anchors.
    fn chain_tip(&self) -> u32;
}

/// A source backed by roots already collected from the chain.
///
/// Populated by whatever the deployment uses to follow the chain — Zebra's
/// `z_gettreestate`, or a Zaino `GetTreeState` stream. Holding a window of
/// recent roots is what real Zcash verifiers do, since a prover's witness is
/// always slightly behind the tip.
#[derive(Debug, Default)]
pub struct RootWindow {
    roots: HashMap<([u8; 32], PoolKey), u32>,
    tip: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum PoolKey {
    Orchard,
    Ironwood,
}

impl From<Pool> for PoolKey {
    fn from(p: Pool) -> Self {
        match p {
            Pool::Orchard => PoolKey::Orchard,
            Pool::Ironwood => PoolKey::Ironwood,
        }
    }
}

impl From<PoolKey> for Pool {
    fn from(p: PoolKey) -> Self {
        match p {
            PoolKey::Orchard => Pool::Orchard,
            PoolKey::Ironwood => Pool::Ironwood,
        }
    }
}

impl RootWindow {
    /// An empty window. Accepts nothing until roots are recorded.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a root observed at `height` in `pool`'s tree.
    ///
    /// The caller is asserting it learned this from the chain. Feeding it a
    /// prover-supplied root defeats the entire mechanism.
    ///
    /// A root stays current across every block that adds no note commitments to
    /// its pool, so the same root arrives at several heights. The newest one is
    /// kept: that is when the root was last the tree state, and ageing it from
    /// the first sighting would retire roots that are still live.
    pub fn observe(&mut self, anchor: pallas::Base, pool: Pool, height: u32) {
        self.roots
            .entry((anchor.to_repr(), pool.into()))
            .and_modify(|seen| *seen = (*seen).max(height))
            .or_insert(height);
        self.tip = self.tip.max(height);
    }

    /// Number of roots currently accepted.
    pub fn len(&self) -> usize {
        self.roots.len()
    }

    /// Whether any roots have been recorded.
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }
}

impl AnchorSource for RootWindow {
    fn lookup(&self, anchor: pallas::Base, pool: Pool) -> Option<AuthenticatedAnchor> {
        self.roots
            .get(&(anchor.to_repr(), pool.into()))
            .map(|&height| AuthenticatedAnchor {
                anchor,
                pool,
                height,
            })
    }

    fn chain_tip(&self) -> u32 {
        self.tip
    }
}

/// Decides whether an anchor in a proof statement may be believed.
pub struct AnchorAuthenticator<S> {
    source: S,
    max_age_blocks: u32,
}

impl<S: AnchorSource> AnchorAuthenticator<S> {
    /// Accepts anchors from `source` that are no more than `max_age_blocks`
    /// behind the tip.
    ///
    /// The window is a trade-off: too tight and honest provers whose wallet is
    /// a few blocks behind get rejected; too loose and a very old, possibly
    /// reorged root stays acceptable.
    pub fn new(source: S, max_age_blocks: u32) -> Self {
        AnchorAuthenticator {
            source,
            max_age_blocks,
        }
    }

    /// The source being consulted.
    pub fn source(&self) -> &S {
        &self.source
    }

    /// The source, mutably, so a verifier can keep following the chain.
    ///
    /// Roots go stale, so a long-running verifier has to add new ones as blocks
    /// arrive. What it must never do is take one from the party presenting a
    /// proof.
    pub fn source_mut(&mut self) -> &mut S {
        &mut self.source
    }

    /// Confirms `anchor` is a real root of `pool`'s tree, recent enough to use.
    ///
    /// Call this **before** trusting a verified proof. A proof that verifies
    /// against an unauthenticated anchor says nothing about Zcash.
    pub fn authenticate(
        &self,
        anchor: pallas::Base,
        pool: Pool,
    ) -> Result<AuthenticatedAnchor, Error> {
        let found = self
            .source
            .lookup(anchor, pool)
            .ok_or_else(|| Error::UnknownAnchor {
                anchor: hex::encode(anchor.to_repr()),
                pool,
            })?;

        if found.pool != pool {
            return Err(Error::WrongPool {
                actual: found.pool,
                claimed: pool,
            });
        }

        let tip = self.source.chain_tip();
        if tip.saturating_sub(found.height) > self.max_age_blocks {
            return Err(Error::AnchorTooOld {
                height: found.height,
                limit: self.max_age_blocks,
            });
        }

        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(n: u64) -> pallas::Base {
        pallas::Base::from(n)
    }

    fn window() -> RootWindow {
        let mut w = RootWindow::new();
        w.observe(anchor(1), Pool::Ironwood, 3_500_000);
        w.observe(anchor(2), Pool::Orchard, 3_500_000);
        w
    }

    #[test]
    fn a_known_recent_anchor_is_accepted() {
        let auth = AnchorAuthenticator::new(window(), 100);
        let got = auth.authenticate(anchor(1), Pool::Ironwood).unwrap();
        assert_eq!(got.height, 3_500_000);
        assert_eq!(got.pool, Pool::Ironwood);
    }

    /// The attack this module exists to stop: a prover invents a tree, proves
    /// against its root, and the verifier must refuse.
    #[test]
    fn a_fabricated_anchor_is_refused() {
        let auth = AnchorAuthenticator::new(window(), 100);
        let fabricated = anchor(0xDEAD_BEEF);

        assert!(matches!(
            auth.authenticate(fabricated, Pool::Ironwood),
            Err(Error::UnknownAnchor { .. })
        ));
    }

    /// After NU6.3 the pools have separate trees. A real Orchard root must not
    /// pass as an Ironwood one.
    #[test]
    fn a_root_from_the_other_pool_is_refused() {
        let auth = AnchorAuthenticator::new(window(), 100);
        assert!(matches!(
            auth.authenticate(anchor(2), Pool::Ironwood),
            Err(Error::UnknownAnchor { .. })
        ));
    }

    #[test]
    fn a_stale_anchor_is_refused() {
        let mut w = window();
        w.observe(anchor(3), Pool::Ironwood, 3_400_000);
        w.observe(anchor(4), Pool::Ironwood, 3_500_050);

        let auth = AnchorAuthenticator::new(w, 100);
        assert!(auth.authenticate(anchor(4), Pool::Ironwood).is_ok());
        assert!(matches!(
            auth.authenticate(anchor(3), Pool::Ironwood),
            Err(Error::AnchorTooOld { .. })
        ));
    }

    /// A root that stays current over several blocks is aged from the last
    /// height it was seen at, not the first.
    #[test]
    fn a_root_seen_repeatedly_is_aged_from_its_newest_sighting() {
        let mut w = RootWindow::new();
        w.observe(anchor(1), Pool::Ironwood, 3_500_000);
        w.observe(anchor(1), Pool::Ironwood, 3_500_009);
        w.observe(anchor(1), Pool::Ironwood, 3_500_004);
        w.observe(anchor(9), Pool::Ironwood, 3_500_100);

        assert_eq!(w.len(), 2);

        let auth = AnchorAuthenticator::new(w, 100);
        assert_eq!(
            auth.authenticate(anchor(1), Pool::Ironwood).unwrap().height,
            3_500_009
        );
    }

    #[test]
    fn an_empty_window_accepts_nothing() {
        let auth = AnchorAuthenticator::new(RootWindow::new(), 100);
        assert!(auth.authenticate(anchor(1), Pool::Ironwood).is_err());
    }
}