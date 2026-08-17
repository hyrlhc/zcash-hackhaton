//! Proving and verification for ZClaim.
//!
//! The proving system is `halo2_proofs` IPA over the Pasta curves — the same
//! one Zcash uses for the Orchard Action circuit. There is no trusted setup, so
//! keys are derived deterministically by both sides rather than distributed.

use std::sync::OnceLock;

use halo2_proofs::{
    plonk,
    poly::commitment::Params,
    transcript::{Blake2bRead, Blake2bWrite},
};
use pasta_curves::{pallas, vesta};
use rand::RngCore;
use zclaim_circuits::{NoteWitness, Statement, ZClaimCircuit, K};

/// Errors from proving or verification.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The prover could not satisfy the circuit. Usually means the witness does
    /// not actually satisfy the predicate.
    #[error("could not produce a proof: {0}")]
    Prove(plonk::Error),
    /// The proof did not verify against the statement.
    #[error("proof rejected: {0}")]
    Verify(plonk::Error),
}

/// Proving key. Building one takes a few seconds, so callers should reuse it.
pub struct ProvingKey {
    params: Params<vesta::Affine>,
    pk: plonk::ProvingKey<vesta::Affine>,
}

impl ProvingKey {
    /// Derives the proving key. Deterministic — no ceremony, no shared secret.
    pub fn build() -> Self {
        let params = Params::new(K);
        let circuit = ZClaimCircuit::default();
        let vk = plonk::keygen_vk(&params, &circuit).expect("vk generation is infallible here");
        let pk = plonk::keygen_pk(&params, vk, &circuit).expect("pk generation is infallible here");
        ProvingKey { params, pk }
    }

    /// A process-wide proving key, built on first use.
    pub fn shared() -> &'static ProvingKey {
        static KEY: OnceLock<ProvingKey> = OnceLock::new();
        KEY.get_or_init(ProvingKey::build)
    }
}

/// Verifying key.
pub struct VerifyingKey {
    params: Params<vesta::Affine>,
    vk: plonk::VerifyingKey<vesta::Affine>,
}

impl VerifyingKey {
    /// Derives the verifying key.
    pub fn build() -> Self {
        let params = Params::new(K);
        let circuit = ZClaimCircuit::default();
        let vk = plonk::keygen_vk(&params, &circuit).expect("vk generation is infallible here");
        VerifyingKey { params, vk }
    }

    /// A process-wide verifying key, built on first use.
    pub fn shared() -> &'static VerifyingKey {
        static KEY: OnceLock<VerifyingKey> = OnceLock::new();
        KEY.get_or_init(VerifyingKey::build)
    }
}

/// An encoded ZClaim proof.
///
/// Carries no witness material: it is a transcript of commitments and openings,
/// and reveals nothing beyond the fact that the statement holds.
#[derive(Clone, PartialEq, Eq)]
pub struct Proof(Vec<u8>);

impl std::fmt::Debug for Proof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Proof({} bytes)", self.0.len())
    }
}

impl Proof {
    /// The proof bytes, as sent to a verifier.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Hex encoding, for transport over JSON.
    pub fn to_hex(&self) -> String {
        hex::encode(&self.0)
    }

    /// Parses a hex-encoded proof.
    pub fn from_hex(s: &str) -> Result<Self, hex::FromHexError> {
        hex::decode(s).map(Proof)
    }

    /// Wraps raw proof bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Proof(bytes)
    }

    /// Produces a proof that `witness` satisfies `statement`.
    pub fn create(
        pk: &ProvingKey,
        witness: &NoteWitness,
        statement: &Statement,
        mut rng: impl RngCore,
    ) -> Result<Self, Error> {
        let circuit = ZClaimCircuit::new(witness, statement);
        let column = statement.to_instance_column();
        let instances: &[&[pallas::Base]] = &[&column];

        let mut transcript = Blake2bWrite::<_, vesta::Affine, _>::init(vec![]);
        plonk::create_proof(
            &pk.params,
            &pk.pk,
            &[circuit],
            &[instances],
            &mut rng,
            &mut transcript,
        )
        .map_err(Error::Prove)?;

        Ok(Proof(transcript.finalize()))
    }

    /// Checks this proof against a public statement.
    ///
    /// A pass means: some note committed under `statement.anchor` pays
    /// `statement.merchant` and satisfies the comparison. It does **not** mean
    /// the anchor is a real chain root — the caller must establish that
    /// separately. See `zclaim_zcash::AnchorAuthenticator`.
    pub fn verify(&self, vk: &VerifyingKey, statement: &Statement) -> Result<(), Error> {
        let column = statement.to_instance_column();
        let instances: &[&[pallas::Base]] = &[&column];

        let strategy = plonk::SingleVerifier::new(&vk.params);
        let mut transcript = Blake2bRead::init(&self.0[..]);
        plonk::verify_proof(&vk.params, &vk.vk, strategy, &[instances], &mut transcript)
            .map_err(Error::Verify)
    }
}

#[cfg(test)]
mod tests;
