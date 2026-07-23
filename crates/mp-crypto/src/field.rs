//! BN254 scalar field (`Fr`) element type and byte-encoding conventions.
//!
//! The canonical in-memory element is [`F`]. The **canonical byte encoding**
//! used throughout mirror-pool is **little-endian**, matching arkworks'
//! `CanonicalSerialize`. Big-endian helpers are provided for the circom /
//! on-chain-verifier boundary (ROADMAP phase 4), where 32-byte big-endian limbs
//! are expected by `groth16-solana` / the `alt_bn128` syscalls.
//!
//! Pinning this convention now is deliberate: endianness / field-encoding
//! mismatches between the off-chain hasher, the in-circuit gadget, and the
//! on-chain verifier are the hardest bugs to diagnose later.

use ark_bn254::Fr;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};

/// The mirror-pool field element: the BN254 scalar field `Fr`.
pub type F = Fr;

/// Byte width of a canonical field-element encoding.
pub const FIELD_BYTES: usize = 32;

/// Encode a field element as 32 little-endian bytes (the canonical encoding).
pub fn to_bytes_le(x: &F) -> [u8; FIELD_BYTES] {
    let mut out = [0u8; FIELD_BYTES];
    x.serialize_compressed(&mut out[..])
        .expect("Fr always serializes into 32 bytes");
    out
}

/// Decode a field element from 32 little-endian bytes.
///
/// Returns `None` if the encoding is non-canonical (i.e. `>=` field modulus).
pub fn from_bytes_le(bytes: &[u8; FIELD_BYTES]) -> Option<F> {
    F::deserialize_compressed(&bytes[..]).ok()
}

/// Encode a field element as 32 big-endian bytes (circom / on-chain boundary).
pub fn to_bytes_be(x: &F) -> [u8; FIELD_BYTES] {
    let mut b = to_bytes_le(x);
    b.reverse();
    b
}

/// Decode a field element from 32 big-endian bytes.
///
/// Returns `None` if the encoding is non-canonical.
pub fn from_bytes_be(bytes: &[u8; FIELD_BYTES]) -> Option<F> {
    let mut le = *bytes;
    le.reverse();
    from_bytes_le(&le)
}

/// Convenience constructor from a `u64` (round ids, indices, test vectors).
pub fn from_u64(n: u64) -> F {
    F::from(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_std::UniformRand;

    #[test]
    fn le_roundtrips() {
        let mut rng = ark_std::test_rng();
        for _ in 0..1000 {
            let x = F::rand(&mut rng);
            assert_eq!(from_bytes_le(&to_bytes_le(&x)), Some(x));
        }
    }

    #[test]
    fn be_roundtrips() {
        let mut rng = ark_std::test_rng();
        for _ in 0..1000 {
            let x = F::rand(&mut rng);
            assert_eq!(from_bytes_be(&to_bytes_be(&x)), Some(x));
        }
    }

    #[test]
    fn le_and_be_are_byte_reverses() {
        let x = from_u64(0x0102_0304_0506_0708);
        let mut le = to_bytes_le(&x);
        let be = to_bytes_be(&x);
        le.reverse();
        assert_eq!(le, be);
    }

    #[test]
    fn all_ones_is_non_canonical() {
        // 0xFF..FF exceeds the BN254 scalar modulus, so it must be rejected.
        assert_eq!(from_bytes_le(&[0xFF; FIELD_BYTES]), None);
    }
}
