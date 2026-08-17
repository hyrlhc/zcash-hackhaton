//! The public statement: everything a verifier sees.

use group::Curve;
use orchard::Address;
use pasta_curves::{arithmetic::CurveAffine, pallas};
use zclaim_core::{Comparison, Merchant, ProofRequest};

use crate::Error;

/// Number of public inputs the circuit consumes.
pub const NUM_INSTANCES: usize = 11;

pub(crate) const I_ANCHOR: usize = 0;
pub(crate) const I_MERCHANT_G_D_X: usize = 1;
pub(crate) const I_MERCHANT_G_D_Y: usize = 2;
pub(crate) const I_MERCHANT_PK_D_X: usize = 3;
pub(crate) const I_MERCHANT_PK_D_Y: usize = 4;
pub(crate) const I_THRESHOLD: usize = 5;
pub(crate) const I_DIRECTION: usize = 6;
pub(crate) const I_DOMAIN_TAG: usize = 7;
pub(crate) const I_NULLIFIER: usize = 8;
pub(crate) const I_HOLDER_TAG: usize = 9;
pub(crate) const I_CONTEXT: usize = 10;

/// Which shielded pool's note commitment tree an anchor belongs to.
///
/// After NU6.3 the Orchard and Ironwood pools have separate trees. A verifier
/// that accepts an anchor without knowing its pool can be fed a root from the
/// pool it did not mean to query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pool {
    /// The Orchard pool (ZIP 224). No new value may enter it after NU6.3.
    Orchard,
    /// The Ironwood pool (ZIP 229 / ZIP 2005).
    Ironwood,
}

/// A merchant's Orchard receiver, in the form the circuit compares against.
///
/// Both points are carried in full affine coordinates. Publishing only the
/// x-coordinates would leave the sign of each `y` free, and a note built on the
/// negated points is a note the merchant can neither see nor spend — a payment
/// that never arrived but would still satisfy an x-only check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MerchantBinding {
    /// `x(g_d)`, the diversified base point.
    pub g_d_x: pallas::Base,
    /// `y(g_d)`.
    pub g_d_y: pallas::Base,
    /// `x(pk_d)`, the diversified transmission key.
    pub pk_d_x: pallas::Base,
    /// `y(pk_d)`.
    pub pk_d_y: pallas::Base,
}

impl MerchantBinding {
    /// Extracts the binding from a parsed Orchard address.
    pub fn from_address(address: &Address) -> Self {
        let g_d = address.g_d().to_affine();
        let pk_d = address.pk_d().inner().to_affine();
        let (g_d_x, g_d_y) = coordinates(&g_d);
        let (pk_d_x, pk_d_y) = coordinates(&pk_d);

        MerchantBinding {
            g_d_x,
            g_d_y,
            pk_d_x,
            pk_d_y,
        }
    }

    /// Parses the address a verifier named in its request.
    pub fn from_merchant(merchant: &Merchant) -> Result<Self, Error> {
        let raw = merchant
            .address_bytes()
            .map_err(|e| Error::Statement(e.to_string()))?;
        let address: Address = Option::from(Address::from_raw_address_bytes(&raw))
            .ok_or_else(|| Error::Statement("merchant address is not a valid receiver".into()))?;

        Ok(MerchantBinding::from_address(&address))
    }
}

fn coordinates(point: &pallas::Affine) -> (pallas::Base, pallas::Base) {
    let c = point
        .coordinates()
        .expect("an address point is never the identity");
    (*c.x(), *c.y())
}

/// The public statement a ZClaim proof is checked against.
///
/// # Anchor authentication
///
/// `anchor` is a public input, which means the circuit proves the note is in
/// *some* tree with that root — not that the root is real. A verifier that
/// accepts a prover-supplied anchor is trivially fooled: the prover can build a
/// tree containing any note it likes. The anchor must be independently
/// confirmed against chain state, together with its [`Pool`], before the proof
/// means anything. `zclaim-zcash` does this.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Statement {
    /// Root of the note commitment tree the note is claimed to be in.
    pub anchor: pallas::Base,
    /// Which pool's tree `anchor` must come from.
    pub pool: Pool,
    /// The merchant receiver the note must pay.
    pub merchant: MerchantBinding,
    /// The comparison threshold, in zatoshi.
    pub threshold: u64,
    /// `+1` for `>=`, `-1` for `<=`.
    pub direction: i8,
    /// The verifier's scope, which the nullifier and holder tag are derived
    /// under. Public because a verifier has to know the tags below were scoped
    /// to *its* domain and not one the prover picked.
    pub domain_tag: pallas::Base,
    /// Note-scoped, verifier-scoped tag. Equal for two claims about the same
    /// payment at the same verifier, unrelated across verifiers.
    pub nullifier: pallas::Base,
    /// Holder-scoped, verifier-scoped tag. Equal for two claims by the same
    /// holder at the same verifier, unrelated across verifiers.
    pub holder_tag: pallas::Base,
    /// Binding to this exact request.
    pub context: pallas::Base,
}

impl Statement {
    /// Builds the statement a verifier will check, from the request it issued.
    ///
    /// `nullifier` and `holder_tag` come from the prover — they are outputs of
    /// the proof, not inputs the verifier chooses — but the circuit recomputes
    /// both from witness material, so a prover cannot pick them freely.
    pub fn from_request(
        anchor: pallas::Base,
        pool: Pool,
        request: &ProofRequest,
        nullifier: pallas::Base,
        holder_tag: pallas::Base,
    ) -> Result<Self, Error> {
        Ok(Statement {
            anchor,
            pool,
            merchant: MerchantBinding::from_merchant(&request.predicate.merchant)?,
            threshold: request.predicate.amount.value.0,
            direction: request.predicate.amount.operator.direction(),
            domain_tag: request.domain_tag().inner(),
            nullifier,
            holder_tag,
            context: request.context().inner(),
        })
    }

    /// Flattens into the instance column layout the circuit expects.
    pub fn to_instance_column(&self) -> Vec<pallas::Base> {
        let mut v = vec![pallas::Base::zero(); NUM_INSTANCES];
        v[I_ANCHOR] = self.anchor;
        v[I_MERCHANT_G_D_X] = self.merchant.g_d_x;
        v[I_MERCHANT_G_D_Y] = self.merchant.g_d_y;
        v[I_MERCHANT_PK_D_X] = self.merchant.pk_d_x;
        v[I_MERCHANT_PK_D_Y] = self.merchant.pk_d_y;
        v[I_THRESHOLD] = pallas::Base::from(self.threshold);
        v[I_DIRECTION] = direction_to_field(self.direction);
        v[I_DOMAIN_TAG] = self.domain_tag;
        v[I_NULLIFIER] = self.nullifier;
        v[I_HOLDER_TAG] = self.holder_tag;
        v[I_CONTEXT] = self.context;
        v
    }

    /// The comparison this statement encodes.
    pub fn comparison(&self) -> Comparison {
        if self.direction >= 0 {
            Comparison::Gte
        } else {
            Comparison::Lte
        }
    }
}

/// Maps `+1`/`-1` onto field elements. `-1` is `p - 1`.
pub(crate) fn direction_to_field(direction: i8) -> pallas::Base {
    if direction >= 0 {
        pallas::Base::one()
    } else {
        -pallas::Base::one()
    }
}
