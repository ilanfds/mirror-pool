//! Convert arkworks Groth16 artifacts into the byte layout the on-chain
//! `groth16-solana` verifier expects (ROADMAP phase 4).
//!
//! `groth16-solana` (like circom/snarkjs) takes curve points as **big-endian**
//! byte arrays and, in its pairing equation, expects `proof_a` to be **negated**.
//! The G2 coordinate order is also swapped relative to arkworks. We handle all
//! of that here so the same proof verifies both natively (test below) and
//! on-chain (the program links the very same verifier).

use ark_bn254::{Bn254, G1Affine, G2Affine};
use ark_groth16::{Proof, VerifyingKey};
use ark_serialize::CanonicalSerialize;

/// Reverse each `N`-byte block (little-endian arkworks limb -> big-endian, and
/// for `N = 64` this also swaps the two Fq2 coefficients of a G2 coordinate).
fn reverse_blocks<const N: usize>(bytes: &[u8]) -> Vec<u8> {
    bytes
        .chunks(N)
        .flat_map(|c| c.iter().rev().copied())
        .collect()
}

fn g1_bytes(p: &G1Affine) -> [u8; 64] {
    let mut le = Vec::with_capacity(64);
    p.serialize_uncompressed(&mut le).expect("serialize G1");
    reverse_blocks::<32>(&le).try_into().expect("64 bytes")
}

fn g2_bytes(p: &G2Affine) -> [u8; 128] {
    let mut le = Vec::with_capacity(128);
    p.serialize_uncompressed(&mut le).expect("serialize G2");
    reverse_blocks::<64>(&le).try_into().expect("128 bytes")
}

/// A proof in `groth16-solana` byte form: `a` is already negated.
pub struct ProofBytes {
    pub a: [u8; 64],
    pub b: [u8; 128],
    pub c: [u8; 64],
}

/// Convert an arkworks proof, negating `A` as the on-chain verifier requires.
pub fn proof_to_bytes(proof: &Proof<Bn254>) -> ProofBytes {
    let a_neg = -proof.a;
    ProofBytes {
        a: g1_bytes(&a_neg),
        b: g2_bytes(&proof.b),
        c: g1_bytes(&proof.c),
    }
}

/// A verifying key in `groth16-solana` byte form.
pub struct VkBytes {
    pub alpha: [u8; 64],
    pub beta: [u8; 128],
    pub gamma: [u8; 128],
    pub delta: [u8; 128],
    /// One point per public input, plus one (`gamma_abc_g1`).
    pub ic: Vec<[u8; 64]>,
}

/// Convert an arkworks verifying key into the on-chain byte layout.
pub fn vk_to_bytes(vk: &VerifyingKey<Bn254>) -> VkBytes {
    VkBytes {
        alpha: g1_bytes(&vk.alpha_g1),
        beta: g2_bytes(&vk.beta_g2),
        gamma: g2_bytes(&vk.gamma_g2),
        delta: g2_bytes(&vk.delta_g2),
        ic: vk.gamma_abc_g1.iter().map(g1_bytes).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit::ProposeCircuit;
    use crate::proving::{prove, setup};
    use ark_std::rand::{rngs::StdRng, SeedableRng};
    use groth16_solana::groth16::{Groth16Verifier, Groth16Verifyingkey};
    use mp_crypto::{field, field::from_u64, IncrementalMerkleTree, Note};

    fn witness(height: usize) -> ProposeCircuit {
        let mut tree = IncrementalMerkleTree::new(height);
        let note = Note::new(from_u64(11), from_u64(22));
        for i in 0..3 {
            tree.insert(from_u64(100 + i)).unwrap();
        }
        let idx = tree.insert(note.commitment()).unwrap();
        tree.insert(from_u64(500)).unwrap();
        let path = tree.opening(idx).unwrap();
        ProposeCircuit::from_witness(&note, &path, tree.root(), from_u64(7), from_u64(999))
    }

    #[test]
    fn groth16_solana_verifier_accepts_our_proof() {
        let height = 10;
        let mut rng = StdRng::seed_from_u64(1);
        let (pk, vk) = setup(height, &mut rng);

        let circuit = witness(height);
        let public: Vec<[u8; 32]> = circuit
            .public_inputs()
            .iter()
            .map(field::to_bytes_be)
            .collect();
        let public: [[u8; 32]; 4] = public.try_into().unwrap();

        let proof = proof_to_bytes(&prove(&pk, circuit, &mut rng));
        let vkb = vk_to_bytes(&vk);
        let gvk = Groth16Verifyingkey {
            nr_pubinputs: vkb.ic.len(),
            vk_alpha_g1: vkb.alpha,
            vk_beta_g2: vkb.beta,
            vk_gamme_g2: vkb.gamma,
            vk_delta_g2: vkb.delta,
            vk_ic: &vkb.ic,
        };

        // If the byte layout (endianness, G2 order, A negation) is wrong, this
        // fails. The on-chain program links this very verifier.
        let mut verifier =
            Groth16Verifier::<4>::new(&proof.a, &proof.b, &proof.c, &public, &gvk).unwrap();
        assert!(
            verifier.verify().is_ok(),
            "groth16-solana must accept our proof"
        );
    }

    #[test]
    fn tampered_action_is_rejected() {
        let height = 10;
        let mut rng = StdRng::seed_from_u64(2);
        let (pk, vk) = setup(height, &mut rng);

        let circuit = witness(height);
        let mut public: Vec<[u8; 32]> = circuit
            .public_inputs()
            .iter()
            .map(field::to_bytes_be)
            .collect();
        let proof = proof_to_bytes(&prove(&pk, circuit, &mut rng));

        // Corrupt the action public input (index 3).
        public[3] = field::to_bytes_be(&from_u64(424242));
        let public: [[u8; 32]; 4] = public.try_into().unwrap();

        let vkb = vk_to_bytes(&vk);
        let gvk = Groth16Verifyingkey {
            nr_pubinputs: vkb.ic.len(),
            vk_alpha_g1: vkb.alpha,
            vk_beta_g2: vkb.beta,
            vk_gamme_g2: vkb.gamma,
            vk_delta_g2: vkb.delta,
            vk_ic: &vkb.ic,
        };
        let mut verifier =
            Groth16Verifier::<4>::new(&proof.a, &proof.b, &proof.c, &public, &gvk).unwrap();
        assert!(
            verifier.verify().is_err(),
            "tampered action must be rejected"
        );
    }
}
