//! Membership notes, commitments, and nullifiers.
//!
//! A [`Note`] is the secret pre-image `(k, r)` a member keeps after joining the
//! pool (`docs/DESIGN.md` §5.1, §6.2). From it we derive:
//!
//! - the **commitment** `C = H(k, r)` — the leaf inserted into the membership
//!   Merkle tree (the "deposit");
//! - the per-round **nullifier** `nf = H(k, round_id)` — revealed when
//!   proposing, to prevent a second proposal by the same member in the same
//!   round, while staying uncorrelated across rounds.

use crate::field::F;
use crate::poseidon::hash2;
use ark_std::rand::Rng;
use ark_std::UniformRand;

/// A member's secret note: the pre-image of a pool commitment.
///
/// Losing a note only costs the ability to propose; it never risks funds, which
/// mirror-pool never custodies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    /// Nullifier secret.
    pub k: F,
    /// Commitment randomness.
    pub r: F,
}

impl Note {
    /// Construct a note from explicit secrets (e.g. when restoring from backup).
    pub fn new(k: F, r: F) -> Self {
        Self { k, r }
    }

    /// Sample a fresh random note.
    pub fn random<R: Rng + ?Sized>(rng: &mut R) -> Self {
        Self {
            k: F::rand(rng),
            r: F::rand(rng),
        }
    }

    /// Pool commitment `C = H(k, r)` — the leaf inserted into the membership tree.
    pub fn commitment(&self) -> F {
        hash2(self.k, self.r)
    }

    /// Per-round proposal nullifier `nf = H(k, round_id)`.
    pub fn nullifier(&self, round_id: F) -> F {
        hash2(self.k, round_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::from_u64;

    #[test]
    fn commitment_is_stable() {
        let note = Note::new(from_u64(42), from_u64(7));
        assert_eq!(note.commitment(), note.commitment());
    }

    #[test]
    fn distinct_notes_have_distinct_commitments() {
        let mut rng = ark_std::test_rng();
        let a = Note::random(&mut rng);
        let b = Note::random(&mut rng);
        assert_ne!(a, b);
        assert_ne!(a.commitment(), b.commitment());
    }

    #[test]
    fn nullifier_is_per_round_and_uncorrelated() {
        let note = Note::new(from_u64(123), from_u64(456));
        let nf0 = note.nullifier(from_u64(0));
        let nf1 = note.nullifier(from_u64(1));
        // Different rounds -> different nullifiers (unlinkable across rounds).
        assert_ne!(nf0, nf1);
        // Same round -> reproducible (so the program can detect a double-propose).
        assert_eq!(nf1, note.nullifier(from_u64(1)));
    }

    #[test]
    fn nullifier_depends_on_k_not_r() {
        // Two notes sharing k but not r produce the same nullifier: the
        // nullifier is a per-member, per-round value keyed on k (DESIGN §6.4).
        let a = Note::new(from_u64(9), from_u64(1));
        let b = Note::new(from_u64(9), from_u64(2));
        assert_eq!(a.nullifier(from_u64(5)), b.nullifier(from_u64(5)));
        // ...but their commitments still differ (r differs).
        assert_ne!(a.commitment(), b.commitment());
    }
}
