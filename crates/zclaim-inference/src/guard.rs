//! The guard itself: decide whether to answer, before answering.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use zclaim_core::{Comparison, Predicate, VerifierDomain, Zatoshi};

use crate::interval::Knowledge;

/// Identifies whose privacy is at stake for a given verifier.
///
/// The nullifier is already scoped to the verifier domain by the circuit, so a
/// key never lets two verifiers' histories be joined — the guard cannot
/// correlate holders even though it is the component tracking them.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubjectKey {
    /// The application doing the asking.
    pub domain: VerifierDomain,
    /// The scoped nullifier, hex-encoded. Identifies the note within the domain.
    pub nullifier: String,
}

/// How aggressively to defend a hidden amount.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Policy {
    /// The narrowest interval a verifier may be allowed to reach, in zatoshi.
    ///
    /// A request whose worst case would leave the amount pinned to fewer than
    /// this many values is refused. The default of 1 ZEC means a verifier can
    /// learn "between 2 and 3 ZEC" but never "exactly 2.7".
    pub granularity: u64,
    /// Interval width below which a request is answered but flagged.
    pub warn_below: u64,
    /// Maximum number of questions per subject, regardless of what they reveal.
    ///
    /// Bounds probing that spreads across merchants or comparison directions in
    /// ways interval tracking alone would not catch.
    pub max_queries: u32,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            granularity: Zatoshi::ZEC.0,
            warn_below: Zatoshi::ZEC.0 * 2,
            max_queries: 8,
        }
    }
}

impl Policy {
    /// A policy tuned for the demo, where amounts are small and the probing
    /// attack has to become visible within a handful of questions.
    ///
    /// `warn_below` must sit above `granularity` but below the interval a
    /// single honest question produces, or every first question is flagged and
    /// the warning stops carrying information.
    pub fn demo() -> Self {
        Policy {
            granularity: Zatoshi::ZEC.0 / 2,
            warn_below: Zatoshi::ZEC.0,
            max_queries: 12,
        }
    }
}

/// What the guard decided about a request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "UPPERCASE")]
pub enum Decision {
    /// Answer it; the verifier still knows little.
    Safe {
        /// What the verifier would know afterwards, in the worst case.
        resulting: Knowledge,
    },
    /// Answer it, but the verifier is closing in.
    Warning {
        /// What the verifier would know afterwards, in the worst case.
        resulting: Knowledge,
        /// Why this was flagged.
        reason: String,
    },
    /// Refuse. Answering would narrow the amount past the policy floor.
    Block {
        /// Why the request was refused.
        reason: String,
    },
}

impl Decision {
    /// Whether a proof may be produced for this request.
    pub fn is_allowed(&self) -> bool {
        !matches!(self, Decision::Block { .. })
    }

    /// A short label for UI and logs.
    pub fn label(&self) -> &'static str {
        match self {
            Decision::Safe { .. } => "SAFE",
            Decision::Warning { .. } => "WARNING",
            Decision::Block { .. } => "BLOCK",
        }
    }
}

/// A question that was answered, recorded so its effect can be replayed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    /// The comparison that was asked.
    pub comparison: Comparison,
    /// The threshold it was asked against.
    pub threshold: Zatoshi,
    /// The answer that was returned.
    pub answer: bool,
}

/// Per-subject history.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct History {
    knowledge: Knowledge,
    observations: Vec<Observation>,
}

/// Tracks what each verifier has learned and refuses requests that would let
/// them learn too much.
///
/// State is in memory. A real deployment persists it, because a guard that
/// forgets on restart is a guard a verifier can reset by waiting.
#[derive(Debug, Default)]
pub struct Guard {
    policy: Policy,
    subjects: HashMap<SubjectKey, History>,
}

impl Guard {
    /// Builds a guard with the given policy.
    pub fn new(policy: Policy) -> Self {
        Guard {
            policy,
            subjects: HashMap::new(),
        }
    }

    /// The policy in force.
    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    /// What this verifier currently knows about this subject.
    pub fn knowledge(&self, subject: &SubjectKey) -> Knowledge {
        self.subjects
            .get(subject)
            .map(|h| h.knowledge)
            .unwrap_or_default()
    }

    /// Questions answered for this subject so far.
    pub fn history(&self, subject: &SubjectKey) -> &[Observation] {
        self.subjects
            .get(subject)
            .map(|h| h.observations.as_slice())
            .unwrap_or(&[])
    }

    /// Decides whether to answer `predicate` for `subject`, without answering it.
    ///
    /// The decision is made on the **worst case over both possible answers**,
    /// never on the answer that would actually be given. Deciding on the real
    /// answer would leak it: a verifier that sees "blocked" where it expected
    /// "allowed" learns which way the comparison went.
    ///
    /// It follows that the decision depends only on the question and the
    /// history, both of which the verifier already knows. The guard therefore
    /// reveals nothing by refusing.
    pub fn evaluate(&self, subject: &SubjectKey, predicate: &Predicate) -> Decision {
        let history = self.subjects.get(subject);
        let asked = history.map(|h| h.observations.len()).unwrap_or(0) as u32;
        let current = history.map(|h| h.knowledge).unwrap_or_default();

        if asked >= self.policy.max_queries {
            return Decision::Block {
                reason: format!(
                    "query budget exhausted: {} of {} questions already answered for this claim",
                    asked, self.policy.max_queries
                ),
            };
        }

        let comparison = predicate.amount.operator;
        let threshold = predicate.amount.value;

        // Repeating an identical question reveals nothing new, so allow it and
        // do not charge it against the interval.
        if let Some(h) = history {
            if h.observations
                .iter()
                .any(|o| o.comparison == comparison && o.threshold == threshold)
            {
                return Decision::Safe {
                    resulting: current,
                };
            }
        }

        let narrowest = current.narrowest_after(comparison, threshold);
        let widest = current.widest_after(comparison, threshold);

        // If *either* answer would narrow past the floor, the question is
        // unsafe to answer at all — refusing only on the inconvenient branch
        // would tell the verifier which branch it was.
        if narrowest.width() < self.policy.granularity as u128 {
            return Decision::Block {
                reason: format!(
                    "answering this could narrow the amount to {}, below the {} ZEC floor",
                    narrowest.describe(),
                    Zatoshi(self.policy.granularity).as_zec_string()
                ),
            };
        }

        if narrowest.width() < self.policy.warn_below as u128 {
            return Decision::Warning {
                resulting: widest,
                reason: format!(
                    "repeated thresholds are narrowing the hidden amount to {}",
                    narrowest.describe()
                ),
            };
        }

        Decision::Safe { resulting: widest }
    }

    /// Records that a question was answered, updating what the verifier knows.
    ///
    /// Call this only after a proof was actually produced and handed over.
    pub fn record(&mut self, subject: &SubjectKey, predicate: &Predicate, answer: bool) {
        let entry = self.subjects.entry(subject.clone()).or_default();
        let comparison = predicate.amount.operator;
        let threshold = predicate.amount.value;

        entry.knowledge = entry.knowledge.after(comparison, threshold, answer);
        entry.observations.push(Observation {
            comparison,
            threshold,
            answer,
        });
    }

    /// Evaluates and, if allowed, records in one step.
    ///
    /// Returns the decision. When it is [`Decision::Block`] nothing is recorded,
    /// because nothing was answered.
    pub fn evaluate_and_record(
        &mut self,
        subject: &SubjectKey,
        predicate: &Predicate,
        answer: bool,
    ) -> Decision {
        let decision = self.evaluate(subject, predicate);
        if decision.is_allowed() {
            self.record(subject, predicate, answer);
        }
        decision
    }

    /// Discards a subject's history. Exposed for tests and for honouring a
    /// holder's request to forget.
    pub fn forget(&mut self, subject: &SubjectKey) {
        self.subjects.remove(subject);
    }
}
