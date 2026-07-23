//! The `S_propose` R1CS circuit (`docs/DESIGN.md` §6.4).
//!
//! Statement (default open configuration):
//!
//! ```text
//! I KNOW k, r, leaf_index, merkle_path SUCH THAT
//!     C  = H(k, r)                          (commitment)
//!   ∧ MerkleRoot(C, path, index) = root      (membership)
//!   ∧ nullifier = H(k, round_id)             (per-round nullifier)
//! ```
//!
//! Public inputs (in this exact order — the on-chain verifier must match):
//! `[root, nullifier, round_id, action]`. `action` is bound as a public input
//! so the proof is non-malleable with respect to it (a relayer cannot swap the
//! action without the witness).

use crate::poseidon::PoseidonGadget;
use ark_bn254::Fr;
use ark_r1cs_std::fields::fp::FpVar;
use ark_r1cs_std::prelude::*;
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use mp_crypto::{MerklePath, Note};

/// Witness + public inputs for one proposal proof.
#[derive(Clone)]
pub struct ProposeCircuit {
    /// Merkle tree height (length of the path).
    pub height: usize,

    // Public inputs.
    pub root: Fr,
    pub nullifier: Fr,
    pub round_id: Fr,
    pub action: Fr,

    // Private witness.
    pub k: Fr,
    pub r: Fr,
    pub path_elements: Vec<Fr>,
    pub path_indices: Vec<bool>,
}

impl ProposeCircuit {
    /// Build the circuit from off-chain `mp-crypto` values.
    pub fn from_witness(
        note: &Note,
        path: &MerklePath,
        root: Fr,
        round_id: Fr,
        action: Fr,
    ) -> Self {
        let height = path.siblings.len();
        let path_indices = (0..height).map(|i| (path.index >> i) & 1 == 1).collect();
        Self {
            height,
            root,
            nullifier: note.nullifier(round_id),
            round_id,
            action,
            k: note.k,
            r: note.r,
            path_elements: path.siblings.clone(),
            path_indices,
        }
    }

    /// Public inputs in the canonical order expected by the verifier.
    pub fn public_inputs(&self) -> Vec<Fr> {
        vec![self.root, self.nullifier, self.round_id, self.action]
    }

    /// A structurally-valid all-zero circuit of the given height, used to run
    /// the Groth16 trusted setup (only the constraint shape matters there).
    pub fn dummy(height: usize) -> Self {
        use ark_ff::Zero;
        Self {
            height,
            root: Fr::zero(),
            nullifier: Fr::zero(),
            round_id: Fr::zero(),
            action: Fr::zero(),
            k: Fr::zero(),
            r: Fr::zero(),
            path_elements: vec![Fr::zero(); height],
            path_indices: vec![false; height],
        }
    }
}

impl ConstraintSynthesizer<Fr> for ProposeCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        let poseidon = PoseidonGadget::bn254_t3();

        // Public inputs, in canonical order.
        let root = FpVar::new_input(cs.clone(), || Ok(self.root))?;
        let nullifier = FpVar::new_input(cs.clone(), || Ok(self.nullifier))?;
        let round_id = FpVar::new_input(cs.clone(), || Ok(self.round_id))?;
        let action = FpVar::new_input(cs.clone(), || Ok(self.action))?;

        // Private witness.
        let k = FpVar::new_witness(cs.clone(), || Ok(self.k))?;
        let r = FpVar::new_witness(cs.clone(), || Ok(self.r))?;

        // C = H(k, r).
        let commitment = poseidon.hash2(&k, &r)?;

        // Walk the Merkle path up to the root.
        let mut cur = commitment;
        for level in 0..self.height {
            let sibling = FpVar::new_witness(cs.clone(), || Ok(self.path_elements[level]))?;
            let is_right = Boolean::new_witness(cs.clone(), || Ok(self.path_indices[level]))?;
            // is_right == true  => current node is the right child: H(sibling, cur)
            // is_right == false => current node is the left  child: H(cur, sibling)
            let left = FpVar::conditionally_select(&is_right, &sibling, &cur)?;
            let right = FpVar::conditionally_select(&is_right, &cur, &sibling)?;
            cur = poseidon.hash2(&left, &right)?;
        }
        cur.enforce_equal(&root)?;

        // nullifier = H(k, round_id).
        let nf = poseidon.hash2(&k, &round_id)?;
        nf.enforce_equal(&nullifier)?;

        // Bind `action` into the constraint system so it is a genuine public
        // input (Tornado binds recipient/fee the same way).
        let _action_binding = action.square()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_relations::r1cs::ConstraintSystem;
    use mp_crypto::{field::from_u64, IncrementalMerkleTree};

    fn build(height: usize) -> (ProposeCircuit, ark_bn254::Fr) {
        let mut tree = IncrementalMerkleTree::new(height);
        let note = Note::new(from_u64(11), from_u64(22));
        for i in 0..3 {
            tree.insert(from_u64(100 + i)).unwrap();
        }
        let idx = tree.insert(note.commitment()).unwrap();
        for i in 0..2 {
            tree.insert(from_u64(200 + i)).unwrap();
        }
        let root = tree.root();
        let path = tree.opening(idx).unwrap();
        let circuit = ProposeCircuit::from_witness(&note, &path, root, from_u64(7), from_u64(999));
        (circuit, root)
    }

    #[test]
    fn satisfied_for_valid_witness() {
        let (circuit, _) = build(10);
        let cs = ConstraintSystem::<Fr>::new_ref();
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(cs.is_satisfied().unwrap(), "valid witness must satisfy");
    }

    #[test]
    fn rejects_wrong_root() {
        let (mut circuit, _) = build(10);
        circuit.root = from_u64(0xdead_beef);
        let cs = ConstraintSystem::<Fr>::new_ref();
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(!cs.is_satisfied().unwrap(), "wrong root must not satisfy");
    }

    #[test]
    fn rejects_wrong_nullifier() {
        let (mut circuit, _) = build(10);
        circuit.nullifier = from_u64(0x1234);
        let cs = ConstraintSystem::<Fr>::new_ref();
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(
            !cs.is_satisfied().unwrap(),
            "wrong nullifier must not satisfy"
        );
    }
}
