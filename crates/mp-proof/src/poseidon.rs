//! In-circuit Poseidon matching `light-poseidon` (circom `Bn254X5`).
//!
//! The membership proof only verifies against the on-chain Merkle root if the
//! Poseidon computed **inside the circuit** is identical to the one used
//! off-chain (`mp-crypto` / `light-poseidon`) and on-chain (the Solana syscall).
//! We guarantee that by reusing `light-poseidon`'s exact round constants and MDS
//! matrix (`get_poseidon_parameters`) and replicating its permutation as R1CS
//! constraints.
//!
//! Construction (circom, width `t = 3`, 2 inputs): state `= [0, in0, in1]`,
//! 8 full rounds (4 + 4) around 57 partial rounds, S-box `x^5`, output
//! `state[0]`.

use ark_bn254::Fr;
use ark_ff::Zero;
use ark_r1cs_std::fields::fp::FpVar;
use ark_r1cs_std::fields::FieldVar;
use ark_relations::r1cs::SynthesisError;

/// Poseidon parameters (constants) for `Bn254X5`, width 3, sourced from
/// `light-poseidon` so the in-circuit hash matches byte-for-byte.
pub struct PoseidonGadget {
    ark: Vec<Fr>,
    mds: Vec<Vec<Fr>>,
    full_rounds: usize,
    partial_rounds: usize,
    width: usize,
}

impl PoseidonGadget {
    /// Load the circom `Bn254X5`, `t = 3` parameters (for hashing two inputs).
    pub fn bn254_t3() -> Self {
        let p = light_poseidon::parameters::bn254_x5::get_poseidon_parameters::<Fr>(3)
            .expect("bn254_x5 t=3 parameters");
        Self {
            ark: p.ark,
            mds: p.mds,
            full_rounds: p.full_rounds,
            partial_rounds: p.partial_rounds,
            width: p.width,
        }
    }

    /// Two-input Poseidon in-circuit: `H(a, b)`.
    pub fn hash2(&self, a: &FpVar<Fr>, b: &FpVar<Fr>) -> Result<FpVar<Fr>, SynthesisError> {
        debug_assert_eq!(self.width, 3);
        // Domain tag is zero in circom's construction.
        let mut state = vec![FpVar::constant(Fr::zero()), a.clone(), b.clone()];
        self.permute(&mut state);
        Ok(state.swap_remove(0))
    }

    fn permute(&self, state: &mut [FpVar<Fr>]) {
        let half = self.full_rounds / 2;
        let total = self.full_rounds + self.partial_rounds;
        for round in 0..half {
            self.add_round_constants(state, round);
            self.sbox_full(state);
            self.apply_mds(state);
        }
        for round in half..half + self.partial_rounds {
            self.add_round_constants(state, round);
            state[0] = pow5(&state[0]);
            self.apply_mds(state);
        }
        for round in half + self.partial_rounds..total {
            self.add_round_constants(state, round);
            self.sbox_full(state);
            self.apply_mds(state);
        }
    }

    fn add_round_constants(&self, state: &mut [FpVar<Fr>], round: usize) {
        for (i, s) in state.iter_mut().enumerate() {
            let c = FpVar::constant(self.ark[round * self.width + i]);
            *s = &*s + &c;
        }
    }

    fn sbox_full(&self, state: &mut [FpVar<Fr>]) {
        for s in state.iter_mut() {
            *s = pow5(s);
        }
    }

    fn apply_mds(&self, state: &mut [FpVar<Fr>]) {
        let mut next = Vec::with_capacity(self.width);
        for i in 0..self.width {
            let mut acc = FpVar::constant(Fr::zero());
            for (j, s) in state.iter().enumerate() {
                let m = FpVar::constant(self.mds[i][j]);
                acc = &acc + &(&m * s);
            }
            next.push(acc);
        }
        state.clone_from_slice(&next);
    }
}

/// `x^5` via three multiplications: `x^2`, `x^4`, `x^4 * x`.
fn pow5(x: &FpVar<Fr>) -> FpVar<Fr> {
    let x2 = x * x;
    let x4 = &x2 * &x2;
    &x4 * x
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_r1cs_std::alloc::AllocVar;
    use ark_r1cs_std::R1CSVar;
    use ark_relations::r1cs::ConstraintSystem;
    use ark_std::UniformRand;

    #[test]
    fn gadget_matches_light_poseidon() {
        let gadget = PoseidonGadget::bn254_t3();
        let mut rng = ark_std::test_rng();

        for _ in 0..20 {
            let a = Fr::rand(&mut rng);
            let b = Fr::rand(&mut rng);
            let expected = mp_crypto::hash2(a, b); // off-chain reference

            let cs = ConstraintSystem::<Fr>::new_ref();
            let a_var = FpVar::new_witness(cs.clone(), || Ok(a)).unwrap();
            let b_var = FpVar::new_witness(cs.clone(), || Ok(b)).unwrap();
            let out = gadget.hash2(&a_var, &b_var).unwrap();

            assert_eq!(out.value().unwrap(), expected, "in-circuit hash mismatch");
            assert!(cs.is_satisfied().unwrap(), "constraints unsatisfied");
        }
    }
}
