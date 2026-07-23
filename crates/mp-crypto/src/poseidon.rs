//! Poseidon hashing over BN254, using circomlib-compatible parameters.
//!
//! We use [`light_poseidon`]'s `new_circom` construction so that the
//! out-of-circuit hash computed here matches (a) the in-circuit Poseidon gadget
//! (ROADMAP phase 3) and (b) an on-chain Poseidon over the same parameters.
//! Keeping a single hash definition across all three domains is what makes the
//! ZK membership proofs verify.

use crate::field::F;
use light_poseidon::{Poseidon, PoseidonHasher};

/// Maximum number of inputs supported by the circom Poseidon construction.
pub const MAX_INPUTS: usize = 12;

/// Poseidon hash of an arbitrary (1..=[`MAX_INPUTS`]) list of field elements.
///
/// # Panics
/// Panics if `inputs` is empty or longer than [`MAX_INPUTS`]; callers hash a
/// fixed, small arity (2), so this is a programming error, not a runtime one.
pub fn hash(inputs: &[F]) -> F {
    assert!(
        (1..=MAX_INPUTS).contains(&inputs.len()),
        "poseidon arity must be in 1..={MAX_INPUTS}, got {}",
        inputs.len()
    );
    let mut hasher = Poseidon::<F>::new_circom(inputs.len())
        .expect("new_circom supports arities up to MAX_INPUTS");
    hasher
        .hash(inputs)
        .expect("poseidon hash never fails for valid arity")
}

/// Two-input Poseidon, the workhorse for commitments, nullifiers, and Merkle
/// inner nodes.
pub fn hash2(a: F, b: F) -> F {
    hash(&[a, b])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::from_u64;

    #[test]
    fn hash_is_deterministic() {
        let a = from_u64(1);
        let b = from_u64(2);
        assert_eq!(hash2(a, b), hash2(a, b));
    }

    #[test]
    fn hash_is_order_sensitive() {
        let a = from_u64(1);
        let b = from_u64(2);
        assert_ne!(hash2(a, b), hash2(b, a));
    }

    #[test]
    fn hash_differs_from_inputs() {
        let a = from_u64(1);
        let b = from_u64(2);
        let h = hash2(a, b);
        assert_ne!(h, a);
        assert_ne!(h, b);
    }
}
