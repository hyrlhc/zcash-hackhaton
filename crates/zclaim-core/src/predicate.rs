//! The predicate a verifier may ask ZClaim to prove.
//!
//! Deliberately small. Every operator here maps onto a constraint the circuit
//! actually enforces; there is no operator that is accepted at this layer and
//! silently ignored below.

use serde::{Deserialize, Serialize};

use crate::Error;

/// An amount in zatoshi. 1 ZEC = 100,000,000 zatoshi.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Zatoshi(pub u64);

impl Zatoshi {
    /// One ZEC.
    pub const ZEC: Zatoshi = Zatoshi(100_000_000);

    /// Builds an amount from whole ZEC plus a zatoshi remainder.
    pub const fn from_zec_parts(zec: u64, zatoshi: u64) -> Self {
        Zatoshi(zec * 100_000_000 + zatoshi)
    }

    /// Renders as ZEC for display. Never use this on a hidden amount.
    pub fn as_zec_string(&self) -> String {
        format!("{}.{:08}", self.0 / 100_000_000, self.0 % 100_000_000)
    }
}

/// The comparison a verifier may request against a hidden amount.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Comparison {
    /// `amount >= value`
    Gte,
    /// `amount <= value`
    Lte,
}

impl Comparison {
    /// The sign the circuit applies to `amount - value` before range-checking.
    ///
    /// `Gte` range-checks `amount - value`; `Lte` range-checks `value - amount`.
    /// Either way the check succeeds exactly when the comparison holds.
    pub fn direction(&self) -> i8 {
        match self {
            Comparison::Gte => 1,
            Comparison::Lte => -1,
        }
    }

    /// Evaluates the comparison in the clear. Used by tests and by the prover to
    /// avoid attempting an impossible proof; never by the verifier.
    pub fn holds(&self, amount: Zatoshi, value: Zatoshi) -> bool {
        match self {
            Comparison::Gte => amount >= value,
            Comparison::Lte => amount <= value,
        }
    }
}

/// A condition on the hidden payment amount.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmountPredicate {
    /// Which way the comparison runs.
    pub operator: Comparison,
    /// The threshold, in zatoshi.
    pub value: Zatoshi,
}

/// Raw length of an Orchard shielded address: an 11-byte diversifier followed
/// by a 32-byte diversified transmission key.
pub const RAW_ADDRESS_LEN: usize = 43;

/// The merchant a payment must have gone to.
///
/// `label` is human-readable metadata for the UI and carries no authority. The
/// binding the circuit enforces is `address`: the merchant's complete Orchard
/// receiver, both the diversified base point and the transmission key.
///
/// Binding the whole address rather than just the transmission key matters.
/// A note carrying the merchant's `pk_d` under some *other* diversifier is a
/// well-formed note that lands in the commitment tree, but the merchant cannot
/// detect or spend it — the payment never actually arrives. Checking only
/// `pk_d` would accept exactly that as a receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Merchant {
    /// Display name, e.g. `"quantum-cafe"`.
    pub label: String,
    /// Hex-encoded raw Orchard address, 43 bytes.
    pub address: String,
}

impl Merchant {
    /// Names a merchant by its raw Orchard address.
    pub fn new(label: impl Into<String>, raw_address: &[u8; RAW_ADDRESS_LEN]) -> Self {
        Merchant {
            label: label.into(),
            address: hex::encode(raw_address),
        }
    }

    /// Decodes `address` into the raw form `orchard::Address` parses.
    pub fn address_bytes(&self) -> Result<[u8; RAW_ADDRESS_LEN], Error> {
        let raw = hex::decode(&self.address)
            .map_err(|e| Error::InvalidMerchantAddress(e.to_string()))?;
        raw.try_into().map_err(|v: Vec<u8>| {
            Error::InvalidMerchantAddress(format!(
                "expected {RAW_ADDRESS_LEN} bytes, got {}",
                v.len()
            ))
        })
    }
}

/// The complete statement a verifier asks ZClaim to prove.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Predicate {
    /// Who the payment must have gone to.
    pub merchant: Merchant,
    /// What must be true of the amount.
    pub amount: AmountPredicate,
}

impl Predicate {
    /// Serializes to the canonical form that gets hashed into the context.
    ///
    /// Canonical means: fixed field order, no whitespace, integers as integers.
    /// `serde_json` preserves struct field declaration order, so this is stable
    /// as long as the struct definitions above do not get reordered.
    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).expect("Predicate is always serializable")
    }

    /// Parses a predicate from its canonical JSON form.
    pub fn from_json(s: &str) -> Result<Self, Error> {
        serde_json::from_str(s).map_err(|e| Error::InvalidPredicate(e.to_string()))
    }

    /// A one-line description for logs and UI. Contains only public data.
    pub fn describe(&self) -> String {
        let op = match self.amount.operator {
            Comparison::Gte => ">=",
            Comparison::Lte => "<=",
        };
        format!(
            "{} paid {} {} ZEC",
            self.merchant.label,
            op,
            self.amount.value.as_zec_string()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zec_conversion_is_exact() {
        assert_eq!(Zatoshi::from_zec_parts(2, 70_000_000).0, 270_000_000);
        assert_eq!(
            Zatoshi(270_000_000).as_zec_string(),
            "2.70000000",
            "display must not round a hidden amount into ambiguity"
        );
    }

    #[test]
    fn comparisons_match_the_demo_scenario() {
        let paid = Zatoshi(270_000_000);
        assert!(Comparison::Gte.holds(paid, Zatoshi::ZEC));
        assert!(!Comparison::Gte.holds(Zatoshi(70_000_000), Zatoshi::ZEC));
        assert!(Comparison::Lte.holds(paid, Zatoshi(300_000_000)));
        assert!(!Comparison::Lte.holds(paid, Zatoshi(200_000_000)));
    }

    #[test]
    fn canonical_json_round_trips() {
        let p = Predicate {
            merchant: Merchant {
                label: "quantum-cafe".into(),
                address: "aa".repeat(43),
            },
            amount: AmountPredicate {
                operator: Comparison::Gte,
                value: Zatoshi::ZEC,
            },
        };

        let json = p.canonical_json();
        assert_eq!(Predicate::from_json(&json).unwrap(), p);
        assert!(!json.contains(' '), "canonical form must not contain whitespace");
    }

    /// Two predicates that differ only in threshold must not share an encoding,
    /// or the context hash would collide and the guard could be sidestepped.
    #[test]
    fn different_thresholds_encode_differently() {
        let base = Predicate {
            merchant: Merchant {
                label: "quantum-cafe".into(),
                address: "aa".repeat(43),
            },
            amount: AmountPredicate {
                operator: Comparison::Gte,
                value: Zatoshi::ZEC,
            },
        };
        let mut other = base.clone();
        other.amount.value = Zatoshi(Zatoshi::ZEC.0 + 1);

        assert_ne!(base.canonical_json(), other.canonical_json());
    }
}
