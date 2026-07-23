//! Groth16 proving over BN254 for the `S_propose` circuit (ROADMAP phase 3).
//!
//! Off-chain prover + verifier used to validate the circuit end to end. The
//! keys produced by [`setup`] are **development** keys from a single-party
//! setup; production requires a multi-party ceremony (ROADMAP §5). The on-chain
//! verifier (phase 4) consumes the same proof and public-input encoding.

use crate::circuit::ProposeCircuit;
use ark_bn254::{Bn254, Fr};
use ark_groth16::{Groth16, Proof, ProvingKey, VerifyingKey};
use ark_snark::SNARK;
use ark_std::rand::{CryptoRng, RngCore};

/// Run the (development) trusted setup for a given tree height.
pub fn setup<R: RngCore + CryptoRng>(
    height: usize,
    rng: &mut R,
) -> (ProvingKey<Bn254>, VerifyingKey<Bn254>) {
    Groth16::<Bn254>::circuit_specific_setup(ProposeCircuit::dummy(height), rng)
        .expect("groth16 setup")
}

/// Produce a proof for a fully-populated circuit witness.
pub fn prove<R: RngCore + CryptoRng>(
    pk: &ProvingKey<Bn254>,
    circuit: ProposeCircuit,
    rng: &mut R,
) -> Proof<Bn254> {
    Groth16::<Bn254>::prove(pk, circuit, rng).expect("groth16 prove")
}

/// Verify a proof against public inputs `[root, nullifier, round_id, action]`.
pub fn verify(vk: &VerifyingKey<Bn254>, proof: &Proof<Bn254>, public_inputs: &[Fr]) -> bool {
    Groth16::<Bn254>::verify(vk, public_inputs, proof).expect("groth16 verify")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_std::rand::{rngs::StdRng, SeedableRng};
    use mp_crypto::{field::from_u64, IncrementalMerkleTree, Note};

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
    fn prove_verify_roundtrip() {
        let height = 10;
        let mut rng = StdRng::seed_from_u64(42);
        let (pk, vk) = setup(height, &mut rng);

        let circuit = witness(height);
        let public = circuit.public_inputs();
        let proof = prove(&pk, circuit, &mut rng);

        assert!(verify(&vk, &proof, &public), "valid proof must verify");
    }

    #[test]
    fn tampered_public_input_fails() {
        let height = 10;
        let mut rng = StdRng::seed_from_u64(42);
        let (pk, vk) = setup(height, &mut rng);

        let circuit = witness(height);
        let public = circuit.public_inputs();
        let proof = prove(&pk, circuit, &mut rng);

        // Flip the action (index 3): the proof must no longer verify — this is
        // the non-malleability that binds the action.
        let mut tampered = public.clone();
        tampered[3] = from_u64(1_000_000);
        assert!(!verify(&vk, &proof, &tampered), "tampered action must fail");
    }
}
