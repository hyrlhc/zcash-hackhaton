//! The wire format for a public statement.
//!
//! A verifier and a holder are separate programs, often in separate languages,
//! so the statement needs an encoding. Defining it here rather than in each SDK
//! means there is one canonical answer to "what does a ZClaim statement look
//! like on the wire", and the Rust verifier is the reference implementation of
//! it.
//!
//! Field elements travel as 32-byte little-endian hex, the same `to_repr`
//! encoding Zcash uses. Amounts travel as decimal strings because zatoshi
//! values exceed what JSON numbers represent exactly.

use ff::PrimeField;
use pasta_curves::pallas;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::statement::{MerchantBinding, Pool, Statement};

/// Serialises a field element as 32-byte little-endian hex.
pub fn to_hex(f: pallas::Base) -> String {
    hex::encode(f.to_repr())
}

/// Parses a field element from 32-byte little-endian hex.
///
/// Rejects anything that is not canonical: a non-canonical encoding would let
/// two distinct byte strings name the same element, and a verifier comparing
/// bytes rather than elements could then be desynchronised from one comparing
/// elements.
pub fn from_hex(s: &str) -> Result<pallas::Base, String> {
    let bytes = hex::decode(s).map_err(|e| format!("not hex: {e}"))?;
    let repr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| format!("expected 32 bytes, got {}", s.len() / 2))?;

    Option::from(pallas::Base::from_repr(repr))
        .ok_or_else(|| "not a canonical field element".to_string())
}

mod field_hex {
    use super::*;

    pub fn serialize<S: Serializer>(f: &pallas::Base, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&to_hex(*f))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<pallas::Base, D::Error> {
        let s = String::deserialize(d)?;
        from_hex(&s).map_err(serde::de::Error::custom)
    }
}

mod u64_string {
    use super::*;

    pub fn serialize<S: Serializer>(v: &u64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&v.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// [`Pool`] on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PoolWire {
    /// The Orchard pool.
    Orchard,
    /// The Ironwood pool.
    Ironwood,
}

impl From<Pool> for PoolWire {
    fn from(p: Pool) -> Self {
        match p {
            Pool::Orchard => PoolWire::Orchard,
            Pool::Ironwood => PoolWire::Ironwood,
        }
    }
}

impl From<PoolWire> for Pool {
    fn from(p: PoolWire) -> Self {
        match p {
            PoolWire::Orchard => Pool::Orchard,
            PoolWire::Ironwood => Pool::Ironwood,
        }
    }
}

/// [`MerchantBinding`] on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MerchantBindingWire {
    /// `x(g_d)`.
    #[serde(with = "field_hex")]
    pub g_d_x: pallas::Base,
    /// `y(g_d)`.
    #[serde(with = "field_hex")]
    pub g_d_y: pallas::Base,
    /// `x(pk_d)`.
    #[serde(with = "field_hex")]
    pub pk_d_x: pallas::Base,
    /// `y(pk_d)`.
    #[serde(with = "field_hex")]
    pub pk_d_y: pallas::Base,
}

/// [`Statement`] on the wire.
///
/// Every field here is public by construction — this is exactly what a verifier
/// is allowed to see, and nothing in it narrows the amount beyond the
/// comparison it encodes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementWire {
    /// Note commitment tree root.
    #[serde(with = "field_hex")]
    pub anchor: pallas::Base,
    /// Which pool's tree the anchor belongs to.
    pub pool: PoolWire,
    /// The merchant receiver.
    pub merchant: MerchantBindingWire,
    /// Comparison threshold, in zatoshi, as a decimal string.
    #[serde(with = "u64_string")]
    pub threshold: u64,
    /// `1` for `>=`, `-1` for `<=`.
    pub direction: i8,
    /// `H(verifier domain)`.
    #[serde(with = "field_hex")]
    pub domain_tag: pallas::Base,
    /// The payment's tag at this verifier.
    #[serde(with = "field_hex")]
    pub nullifier: pallas::Base,
    /// The holder's pseudonym at this verifier.
    #[serde(with = "field_hex")]
    pub holder_tag: pallas::Base,
    /// Binding to the request.
    #[serde(with = "field_hex")]
    pub context: pallas::Base,
}

impl From<&Statement> for StatementWire {
    fn from(s: &Statement) -> Self {
        StatementWire {
            anchor: s.anchor,
            pool: s.pool.into(),
            merchant: MerchantBindingWire {
                g_d_x: s.merchant.g_d_x,
                g_d_y: s.merchant.g_d_y,
                pk_d_x: s.merchant.pk_d_x,
                pk_d_y: s.merchant.pk_d_y,
            },
            threshold: s.threshold,
            direction: s.direction,
            domain_tag: s.domain_tag,
            nullifier: s.nullifier,
            holder_tag: s.holder_tag,
            context: s.context,
        }
    }
}

impl From<&StatementWire> for Statement {
    fn from(w: &StatementWire) -> Self {
        Statement {
            anchor: w.anchor,
            pool: w.pool.into(),
            merchant: MerchantBinding {
                g_d_x: w.merchant.g_d_x,
                g_d_y: w.merchant.g_d_y,
                pk_d_x: w.merchant.pk_d_x,
                pk_d_y: w.merchant.pk_d_y,
            },
            threshold: w.threshold,
            // Anything not exactly -1 is treated as >=, matching
            // `Statement::comparison`. A hostile value cannot widen what the
            // proof establishes: the circuit constrains `direction` to ±1, so a
            // statement whose direction disagrees with the proof simply fails
            // verification.
            direction: if w.direction < 0 { -1 } else { 1 },
            domain_tag: w.domain_tag,
            nullifier: w.nullifier,
            holder_tag: w.holder_tag,
            context: w.context,
        }
    }
}

impl Serialize for Statement {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        StatementWire::from(self).serialize(s)
    }
}

impl<'de> Deserialize<'de> for Statement {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        StatementWire::deserialize(d).map(|w| Statement::from(&w))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{address_for, binding_for, QUANTUM_CAFE};
    use pasta_curves::group::ff::Field;

    fn statement() -> Statement {
        Statement {
            anchor: pallas::Base::from(7),
            pool: Pool::Ironwood,
            merchant: binding_for(&address_for(QUANTUM_CAFE)),
            threshold: 270_000_000,
            direction: 1,
            domain_tag: pallas::Base::from(11),
            nullifier: pallas::Base::from(13),
            holder_tag: pallas::Base::from(17),
            context: pallas::Base::from(19),
        }
    }

    #[test]
    fn a_statement_survives_json() {
        let s = statement();
        let json = serde_json::to_string(&s).unwrap();
        let back: Statement = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn the_encoding_is_the_one_zcash_uses() {
        let json = serde_json::to_value(statement()).unwrap();
        assert_eq!(
            json["anchor"],
            serde_json::json!("0700000000000000000000000000000000000000000000000000000000000000"),
            "field elements are 32-byte little-endian hex, as in to_repr"
        );
    }

    /// JSON numbers are doubles. A zatoshi amount above 2^53 would be silently
    /// rounded, which for a threshold means proving the wrong thing.
    #[test]
    fn large_amounts_do_not_lose_precision() {
        let mut s = statement();
        s.threshold = 21_000_000 * 100_000_000;
        let back: Statement = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.threshold, 21_000_000 * 100_000_000);

        let json = serde_json::to_value(&s).unwrap();
        assert!(json["threshold"].is_string());
    }

    #[test]
    fn a_non_canonical_field_element_is_refused() {
        // -1 as a repr, plus one: past the modulus, so not a valid element.
        let past_modulus = hex::encode((-pallas::Base::ONE).to_repr().map(|_| 0xffu8));
        let json = format!(r#"{{"anchor":"{past_modulus}"}}"#);
        assert!(serde_json::from_str::<StatementWire>(&json).is_err());
        assert!(from_hex(&past_modulus).is_err());
    }

    #[test]
    fn a_short_field_element_is_refused() {
        assert!(from_hex("00ff").is_err());
        assert!(from_hex("nothex").is_err());
    }

    #[test]
    fn pools_round_trip_by_name() {
        for pool in [Pool::Orchard, Pool::Ironwood] {
            let mut s = statement();
            s.pool = pool;
            let back: Statement = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
            assert_eq!(back.pool, pool);
        }
        assert_eq!(
            serde_json::to_value(PoolWire::Ironwood).unwrap(),
            serde_json::json!("ironwood")
        );
    }

    #[test]
    fn both_comparison_directions_survive() {
        for direction in [1i8, -1] {
            let mut s = statement();
            s.direction = direction;
            let back: Statement = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
            assert_eq!(back.direction, direction);
        }
    }
}
