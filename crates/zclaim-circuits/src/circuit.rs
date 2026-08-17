//! Circuit construction and synthesis.

use halo2_gadgets::{
    ecc::{
        chip::{CircuitVersion, EccChip, EccConfig},
        NonIdentityPoint, ScalarFixed,
    },
    poseidon::{
        primitives as poseidon, Hash as PoseidonHash, Pow5Chip as PoseidonChip,
        Pow5Config as PoseidonConfig,
    },
    sinsemilla::{
        chip::{SinsemillaChip, SinsemillaConfig},
        merkle::{
            chip::{MerkleChip, MerkleConfig},
            MerklePath,
        },
    },
    utilities::lookup_range_check::{LookupRangeCheck, LookupRangeCheckConfig},
};
use halo2_proofs::{
    circuit::{floor_planner, AssignedCell, Layouter, Value},
    plonk::{self, Advice, Column, ConstraintSystem, Constraints, Expression, Instance, Selector},
    poly::Rotation,
};
use orchard::{
    circuit::{
        gadget::assign_free_advice,
        note_commit::{gadgets::note_commit, NoteCommitChip, NoteCommitConfig},
    },
    constants::{OrchardCommitDomains, OrchardFixedBases, OrchardHashDomains},
    value::NoteValue,
    NOTE_COMMITMENT_TREE_DEPTH,
};
use pasta_curves::pallas;
use poseidon::ConstantLength;

use crate::{
    statement::{
        direction_to_field, I_ANCHOR, I_CONTEXT, I_DIRECTION, I_DOMAIN_TAG, I_HOLDER_TAG,
        I_MERCHANT_G_D_X, I_MERCHANT_G_D_Y, I_MERCHANT_PK_D_X, I_MERCHANT_PK_D_Y, I_NULLIFIER,
        I_THRESHOLD,
    },
    NoteWitness, Statement,
};

/// Circuit size.
///
/// The Orchard Action circuit uses `k = 11` while doing strictly more work than
/// this one (two `NoteCommit`s, a `CommitIvk`, a value commitment and a spend
/// authority check), so `k = 11` is comfortable here.
pub const K: u32 = 11;

/// 10-bit lookup words used to range-constrain the signed difference.
///
/// 7 words = 70 bits. `NoteCommit` already constrains `v` to 64 bits, and any
/// meaningful threshold is below 2^64. When the comparison holds the difference
/// is under 2^64; when it fails the subtraction wraps to above 2^253. So a
/// 70-bit check is exactly the comparison.
const DIFF_RANGE_WORDS: usize = 7;

/// Chip configuration.
#[derive(Clone, Debug)]
pub struct Config {
    primary: Column<Instance>,
    q_predicate: Selector,
    advices: [Column<Advice>; 10],
    ecc_config: EccConfig<OrchardFixedBases>,
    poseidon_config: PoseidonConfig<pallas::Base, 3, 2>,
    merkle_config_1: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    merkle_config_2: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    sinsemilla_config_1:
        SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    note_commit_config: NoteCommitConfig,
    range_check: LookupRangeCheckConfig<pallas::Base, 10>,
}

/// The ZClaim predicate circuit.
#[derive(Clone, Debug, Default)]
pub struct ZClaimCircuit {
    g_d: Value<pallas::Affine>,
    pk_d: Value<pallas::Affine>,
    value: Value<NoteValue>,
    rho: Value<pallas::Base>,
    psi: Value<pallas::Base>,
    rcm: Value<pallas::Scalar>,
    merkle_path: Value<[pallas::Base; NOTE_COMMITMENT_TREE_DEPTH]>,
    merkle_pos: Value<u32>,
    holder_secret: Value<pallas::Base>,
    threshold: Value<pallas::Base>,
    direction: Value<pallas::Base>,
    domain_tag: Value<pallas::Base>,
    context: Value<pallas::Base>,
}

impl ZClaimCircuit {
    /// Pairs a private witness with the public statement it must satisfy.
    ///
    /// Only the statement's *witness-facing* fields are copied in; the anchor,
    /// merchant key and nullifier are enforced by equality to public inputs
    /// rather than witnessed, so a prover cannot quietly substitute them.
    pub fn new(witness: &NoteWitness, statement: &Statement) -> Self {
        ZClaimCircuit {
            g_d: Value::known(witness.g_d),
            pk_d: Value::known(witness.pk_d),
            value: Value::known(NoteValue::from_raw(witness.value)),
            rho: Value::known(witness.rho),
            psi: Value::known(witness.psi),
            rcm: Value::known(witness.rcm),
            merkle_path: Value::known(witness.merkle_path),
            merkle_pos: Value::known(witness.merkle_pos),
            holder_secret: Value::known(witness.holder.secret()),
            threshold: Value::known(pallas::Base::from(statement.threshold)),
            direction: Value::known(direction_to_field(statement.direction)),
            domain_tag: Value::known(statement.domain_tag),
            context: Value::known(statement.context),
        }
    }
}

impl plonk::Circuit<pallas::Base> for ZClaimCircuit {
    type Config = Config;
    type FloorPlanner = floor_planner::V1;

    fn without_witnesses(&self) -> Self {
        Self::default()
    }

    fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
        let advices = [
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
        ];

        // The comparison gate. `direction` is +1 for `>=` and -1 for `<=`, and
        // is itself constrained to those two values, so a prover cannot pick a
        // scaling factor that makes an out-of-range difference look small.
        let q_predicate = meta.selector();
        meta.create_gate("ZClaim predicate", |meta| {
            let q_predicate = meta.query_selector(q_predicate);
            let v = meta.query_advice(advices[0], Rotation::cur());
            let threshold = meta.query_advice(advices[1], Rotation::cur());
            let diff = meta.query_advice(advices[2], Rotation::cur());
            let direction = meta.query_advice(advices[3], Rotation::cur());

            let one = Expression::Constant(pallas::Base::one());

            Constraints::with_selector(
                q_predicate,
                [
                    (
                        "direction is +1 or -1",
                        (direction.clone() - one.clone()) * (direction.clone() + one),
                    ),
                    (
                        "diff = direction * (v - threshold)",
                        diff - direction * (v - threshold),
                    ),
                ],
            )
        });

        let table_idx = meta.lookup_table_column();
        let lookup = (
            table_idx,
            meta.lookup_table_column(),
            meta.lookup_table_column(),
        );

        let lagrange_coeffs = [
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
        ];
        let rc_a = lagrange_coeffs[2..5].try_into().unwrap();
        let rc_b = lagrange_coeffs[5..8].try_into().unwrap();
        meta.enable_constant(lagrange_coeffs[0]);

        let primary = meta.instance_column();
        meta.enable_equality(primary);
        for advice in advices.iter() {
            meta.enable_equality(*advice);
        }

        let range_check = LookupRangeCheckConfig::configure(meta, advices[9], table_idx);

        let ecc_config =
            EccChip::<OrchardFixedBases>::configure(meta, advices, lagrange_coeffs, range_check);

        let poseidon_config = PoseidonChip::configure::<poseidon::P128Pow5T3>(
            meta,
            advices[6..9].try_into().unwrap(),
            advices[5],
            rc_a,
            rc_b,
        );

        let sinsemilla_config_1 = SinsemillaChip::configure(
            meta,
            advices[..5].try_into().unwrap(),
            advices[6],
            lagrange_coeffs[0],
            lookup,
            range_check,
            false,
        );
        let merkle_config_1 = MerkleChip::configure(meta, sinsemilla_config_1.clone());

        let sinsemilla_config_2 = SinsemillaChip::configure(
            meta,
            advices[5..].try_into().unwrap(),
            advices[7],
            lagrange_coeffs[1],
            lookup,
            range_check,
            false,
        );
        let merkle_config_2 = MerkleChip::configure(meta, sinsemilla_config_2);

        let note_commit_config =
            NoteCommitChip::configure(meta, advices, sinsemilla_config_1.clone());

        Config {
            primary,
            q_predicate,
            advices,
            ecc_config,
            poseidon_config,
            merkle_config_1,
            merkle_config_2,
            sinsemilla_config_1,
            note_commit_config,
            range_check,
        }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), plonk::Error> {
        SinsemillaChip::<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>::load(
            config.sinsemilla_config_1.clone(),
            &mut layouter,
        )?;

        let ecc_chip = EccChip::construct(config.ecc_config.clone(), CircuitVersion::AnchoredBase);

        // 1. Note commitment integrity, using Zcash's own NoteCommit^Orchard.
        let g_d = NonIdentityPoint::new(
            ecc_chip.clone(),
            layouter.namespace(|| "witness g_d"),
            self.g_d,
        )?;
        let pk_d = NonIdentityPoint::new(
            ecc_chip.clone(),
            layouter.namespace(|| "witness pk_d"),
            self.pk_d,
        )?;
        let value = assign_free_advice(
            layouter.namespace(|| "witness value"),
            config.advices[0],
            self.value,
        )?;
        let rho = assign_free_advice(
            layouter.namespace(|| "witness rho"),
            config.advices[0],
            self.rho,
        )?;
        let psi = assign_free_advice(
            layouter.namespace(|| "witness psi"),
            config.advices[0],
            self.psi,
        )?;
        let rcm = ScalarFixed::new(
            ecc_chip.clone(),
            layouter.namespace(|| "witness rcm"),
            self.rcm,
        )?;

        let cm = note_commit(
            layouter.namespace(|| "cm = NoteCommit^Orchard_rcm(g_d, pk_d, v, rho, psi)"),
            SinsemillaChip::construct(config.sinsemilla_config_1.clone()),
            ecc_chip.clone(),
            NoteCommitChip::construct(config.note_commit_config.clone()),
            g_d.inner(),
            pk_d.inner(),
            value.clone(),
            rho,
            psi.clone(),
            rcm,
        )?;

        // 2. Merkle path validity, using Zcash's own MerkleCRH^Orchard.
        let root = {
            let merkle_inputs = MerklePath::construct(
                [
                    MerkleChip::construct(config.merkle_config_1.clone()),
                    MerkleChip::construct(config.merkle_config_2.clone()),
                ],
                OrchardHashDomains::MerkleCrh,
                self.merkle_pos,
                self.merkle_path,
            );
            let leaf = cm.extract_p().inner().clone();
            merkle_inputs.calculate_root(layouter.namespace(|| "Merkle path"), leaf)?
        };
        layouter.constrain_instance(root.cell(), config.primary, I_ANCHOR)?;

        // 3. The note pays the merchant named in the statement — the whole
        //    receiver, not just the transmission key. See `MerchantBinding`.
        layouter.constrain_instance(g_d.inner().x().cell(), config.primary, I_MERCHANT_G_D_X)?;
        layouter.constrain_instance(g_d.inner().y().cell(), config.primary, I_MERCHANT_G_D_Y)?;
        layouter.constrain_instance(pk_d.inner().x().cell(), config.primary, I_MERCHANT_PK_D_X)?;
        layouter.constrain_instance(pk_d.inner().y().cell(), config.primary, I_MERCHANT_PK_D_Y)?;

        // 4. The amount predicate.
        let threshold = assign_free_advice(
            layouter.namespace(|| "witness threshold"),
            config.advices[1],
            self.threshold,
        )?;
        layouter.constrain_instance(threshold.cell(), config.primary, I_THRESHOLD)?;

        let direction = assign_free_advice(
            layouter.namespace(|| "witness direction"),
            config.advices[3],
            self.direction,
        )?;
        layouter.constrain_instance(direction.cell(), config.primary, I_DIRECTION)?;

        let diff_value = self
            .value
            .map(|v| pallas::Base::from(v.inner()))
            .zip(self.threshold)
            .zip(self.direction)
            .map(|((v, t), d)| d * (v - t));

        let diff = layouter.assign_region(
            || "diff = direction * (value - threshold)",
            |mut region| {
                config.q_predicate.enable(&mut region, 0)?;

                let v_cell = region.assign_advice(
                    || "value",
                    config.advices[0],
                    0,
                    || self.value.map(|v| pallas::Base::from(v.inner())),
                )?;
                region.constrain_equal(value.cell(), v_cell.cell())?;

                threshold.copy_advice(|| "threshold", &mut region, config.advices[1], 0)?;
                direction.copy_advice(|| "direction", &mut region, config.advices[3], 0)?;

                region.assign_advice(|| "diff", config.advices[2], 0, || diff_value)
            },
        )?;

        // `strict` is required. Without it `copy_check` decomposes the running
        // sum but leaves the high limb unconstrained, which bounds nothing.
        config.range_check.copy_check(
            layouter.namespace(|| "diff is in range"),
            diff,
            DIFF_RANGE_WORDS,
            true,
        )?;

        // 5. The two scoped tags. Both hang off `domain_tag`, which is public,
        //    so a prover cannot scope them to a domain of its own choosing.
        let domain_tag = assign_free_advice(
            layouter.namespace(|| "witness domain tag"),
            config.advices[0],
            self.domain_tag,
        )?;
        layouter.constrain_instance(domain_tag.cell(), config.primary, I_DOMAIN_TAG)?;

        // The nullifier uses the same psi that NoteCommit consumed, so it is
        // bound to the note the Merkle path proved.
        let nullifier = poseidon_hash(
            &config,
            layouter.namespace(|| "nullifier = Poseidon(psi, domain_tag)"),
            [psi, domain_tag.clone()],
        )?;
        layouter.constrain_instance(nullifier.cell(), config.primary, I_NULLIFIER)?;

        // 6. Holder binding. The secret is never derived from the note, so
        //    reproducing this tag requires being the holder, not merely having
        //    obtained a copy of the witness.
        let holder_secret = assign_free_advice(
            layouter.namespace(|| "witness holder secret"),
            config.advices[0],
            self.holder_secret,
        )?;
        let holder_tag = poseidon_hash(
            &config,
            layouter.namespace(|| "holder_tag = Poseidon(holder_secret, domain_tag)"),
            [holder_secret, domain_tag],
        )?;
        layouter.constrain_instance(holder_tag.cell(), config.primary, I_HOLDER_TAG)?;

        // 7. Request binding. `context` takes part in no arithmetic — the copy
        //    constraint against the instance column is the whole point. It
        //    forces the proof to name exactly one request, so an answer given
        //    to one verifier's challenge satisfies no other.
        let context = assign_free_advice(
            layouter.namespace(|| "witness context"),
            config.advices[0],
            self.context,
        )?;
        layouter.constrain_instance(context.cell(), config.primary, I_CONTEXT)?;

        Ok(())
    }
}

/// One `Poseidon` over two field elements, using the same parameter set Zcash
/// uses for `PRF^nfOrchard`.
fn poseidon_hash(
    config: &Config,
    layouter: impl Layouter<pallas::Base>,
    message: [AssignedCell<pallas::Base, pallas::Base>; 2],
) -> Result<AssignedCell<pallas::Base, pallas::Base>, plonk::Error> {
    let mut layouter = layouter;
    PoseidonHash::<_, _, poseidon::P128Pow5T3, ConstantLength<2>, 3, 2>::init(
        PoseidonChip::construct(config.poseidon_config.clone()),
        layouter.namespace(|| "Poseidon init"),
    )?
    .hash(layouter.namespace(|| "Poseidon hash"), message)
}