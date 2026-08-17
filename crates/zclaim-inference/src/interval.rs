//! What a sequence of answers has already revealed.
//!
//! Answers to threshold comparisons are exactly interval constraints, so the
//! accumulated knowledge of a verifier is always a single closed interval
//! `[low, high]` containing the hidden amount. That makes the leak measurable
//! rather than a matter of judgement: the width of the interval *is* how much
//! the verifier has learned.

use serde::{Deserialize, Serialize};
use zclaim_core::{Comparison, Zatoshi};

/// The interval a verifier's answers have narrowed the hidden amount to.
///
/// Starts as the full range of representable values and shrinks with each
/// answer. `low` and `high` are inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Knowledge {
    /// Smallest value still consistent with the answers given.
    pub low: u64,
    /// Largest value still consistent with the answers given.
    pub high: u64,
}

impl Default for Knowledge {
    fn default() -> Self {
        Knowledge::unconstrained()
    }
}

impl Knowledge {
    /// Nothing has been revealed yet.
    pub fn unconstrained() -> Self {
        Knowledge {
            low: 0,
            high: u64::MAX,
        }
    }

    /// Number of values still consistent with what has been answered.
    ///
    /// Saturates rather than overflowing on the initial full range.
    pub fn width(&self) -> u128 {
        if self.high < self.low {
            0
        } else {
            (self.high as u128) - (self.low as u128) + 1
        }
    }

    /// True once the answers pin the amount to a single value.
    pub fn is_exact(&self) -> bool {
        self.low == self.high
    }

    /// True if the answers so far are mutually contradictory. Should never
    /// happen with honest proofs over one note; if it does, the caller is
    /// mixing subjects.
    pub fn is_contradictory(&self) -> bool {
        self.low > self.high
    }

    /// Applies the constraint implied by answering `comparison threshold` with
    /// `answer`, returning the knowledge that would result.
    ///
    /// This is total: it computes the outcome without committing to it, so the
    /// guard can decide *before* a proof is produced.
    pub fn after(&self, comparison: Comparison, threshold: Zatoshi, answer: bool) -> Knowledge {
        let t = threshold.0;
        let mut next = *self;

        match (comparison, answer) {
            // amount >= t
            (Comparison::Gte, true) => next.low = next.low.max(t),
            // !(amount >= t)  =>  amount <= t - 1
            (Comparison::Gte, false) => next.high = next.high.min(t.saturating_sub(1)),
            // amount <= t
            (Comparison::Lte, true) => next.high = next.high.min(t),
            // !(amount <= t)  =>  amount >= t + 1
            (Comparison::Lte, false) => next.low = next.low.max(t.saturating_add(1)),
        }

        next
    }

    /// The wider of the two intervals answering this question could produce.
    pub fn widest_after(&self, comparison: Comparison, threshold: Zatoshi) -> Knowledge {
        let (a, b) = self.both_outcomes(comparison, threshold);
        if a.width() >= b.width() {
            a
        } else {
            b
        }
    }

    /// The narrower of the two intervals answering this question could produce.
    ///
    /// This is what the guard decides on. Deciding on the *actual* answer would
    /// itself leak — a verifier that sees a refusal where it expected an answer
    /// learns which way the comparison went. Judging the narrower branch means
    /// the decision depends only on the question and the history, both of which
    /// the verifier already has.
    pub fn narrowest_after(&self, comparison: Comparison, threshold: Zatoshi) -> Knowledge {
        let (a, b) = self.both_outcomes(comparison, threshold);
        if a.width() <= b.width() {
            a
        } else {
            b
        }
    }

    fn both_outcomes(&self, comparison: Comparison, threshold: Zatoshi) -> (Knowledge, Knowledge) {
        (
            self.after(comparison, threshold, true),
            self.after(comparison, threshold, false),
        )
    }

    /// Renders the interval for an audit log. Contains only what the verifier
    /// already knows, so it is safe to show.
    pub fn describe(&self) -> String {
        if self.is_contradictory() {
            return "contradictory".to_string();
        }
        if self.is_exact() {
            return format!("exactly {}", Zatoshi(self.low).as_zec_string());
        }
        match (self.low, self.high) {
            (0, u64::MAX) => "anything".to_string(),
            (low, u64::MAX) => format!("{} ZEC or more", Zatoshi(low).as_zec_string()),
            (0, high) => format!("up to {} ZEC", Zatoshi(high).as_zec_string()),
            (low, high) => format!(
                "{} .. {} ZEC",
                Zatoshi(low).as_zec_string(),
                Zatoshi(high).as_zec_string()
            ),
        }
    }
}
