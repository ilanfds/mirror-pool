//! Incremental fixed-height Merkle tree over Poseidon.
//!
//! This is the membership accumulator of `docs/DESIGN.md` §5. It implements the
//! Tornado-style incremental insert (`filled_subtrees` + precomputed zero
//! subtrees) so the **on-chain program can reproduce the exact same root with
//! O(height) work per deposit** (ROADMAP phase 2). It additionally stores the
//! inserted leaves so that off-chain agents can produce Merkle **openings** for
//! their membership proofs (ROADMAP phase 3/5).
//!
//! Empty leaves are the field zero; `zeros[i]` is the root of an all-empty
//! subtree of height `i`.

use crate::field::{from_u64, F};
use crate::poseidon::hash2;

/// Default tree height. Capacity is `2^height` members.
pub const DEFAULT_HEIGHT: usize = 26;

/// Errors from tree operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MerkleError {
    /// The tree is full (`2^height` leaves already inserted).
    Full,
}

/// A Merkle opening: the sibling hashes from a leaf up to the root, plus the
/// leaf's index (whose bits select left/right at each level).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MerklePath {
    /// Index of the leaf this path authenticates.
    pub index: usize,
    /// Sibling nodes, level 0 (leaf level) first.
    pub siblings: Vec<F>,
}

impl MerklePath {
    /// Recompute the root implied by this path and a candidate `leaf`.
    pub fn compute_root(&self, leaf: F) -> F {
        let mut cur = leaf;
        let mut idx = self.index;
        for sib in &self.siblings {
            cur = if idx & 1 == 0 {
                hash2(cur, *sib)
            } else {
                hash2(*sib, cur)
            };
            idx >>= 1;
        }
        cur
    }

    /// Verify this opening authenticates `leaf` under `root`.
    pub fn verify(&self, leaf: F, root: F) -> bool {
        self.compute_root(leaf) == root
    }
}

/// An incremental Merkle tree.
#[derive(Clone, Debug)]
pub struct IncrementalMerkleTree {
    height: usize,
    /// `zeros[i]` = root of an all-empty subtree of height `i`; `len == height + 1`.
    zeros: Vec<F>,
    /// Left-hand nodes needed to fold in the next leaf; `len == height`.
    filled_subtrees: Vec<F>,
    current_root: F,
    next_index: usize,
    leaves: Vec<F>,
}

impl IncrementalMerkleTree {
    /// Create an empty tree of the given height (`1..=32`).
    pub fn new(height: usize) -> Self {
        assert!((1..=32).contains(&height), "height must be in 1..=32");
        let mut zeros = Vec::with_capacity(height + 1);
        zeros.push(from_u64(0)); // empty-leaf value
        for i in 1..=height {
            zeros.push(hash2(zeros[i - 1], zeros[i - 1]));
        }
        let filled_subtrees = zeros[..height].to_vec();
        let current_root = zeros[height];
        Self {
            height,
            zeros,
            filled_subtrees,
            current_root,
            next_index: 0,
            leaves: Vec::new(),
        }
    }

    /// Height of the tree.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Number of leaves inserted so far.
    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    /// Whether no leaves have been inserted.
    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    /// Maximum number of leaves (`2^height`).
    pub fn capacity(&self) -> u64 {
        1u64 << self.height
    }

    /// Current Merkle root (maintained incrementally).
    pub fn root(&self) -> F {
        self.current_root
    }

    /// Insert a leaf, returning its index. Uses the Tornado incremental update.
    pub fn insert(&mut self, leaf: F) -> Result<usize, MerkleError> {
        let index = self.next_index;
        if index as u64 >= self.capacity() {
            return Err(MerkleError::Full);
        }
        let mut cur = leaf;
        let mut idx = index;
        for h in 0..self.height {
            if idx & 1 == 0 {
                // `cur` is a left child; its right sibling is still empty.
                self.filled_subtrees[h] = cur;
                cur = hash2(cur, self.zeros[h]);
            } else {
                // `cur` is a right child; fold in the stored left sibling.
                cur = hash2(self.filled_subtrees[h], cur);
            }
            idx >>= 1;
        }
        self.current_root = cur;
        self.leaves.push(leaf);
        self.next_index += 1;
        Ok(index)
    }

    /// Produce a Merkle opening for a previously inserted leaf.
    pub fn opening(&self, index: usize) -> Option<MerklePath> {
        if index >= self.leaves.len() {
            return None;
        }
        let mut siblings = Vec::with_capacity(self.height);
        let mut level: Vec<F> = self.leaves.clone();
        let mut idx = index;
        for h in 0..self.height {
            let sibling = level.get(idx ^ 1).copied().unwrap_or(self.zeros[h]);
            siblings.push(sibling);
            level = Self::parent_level(&level, self.zeros[h]);
            idx >>= 1;
        }
        Some(MerklePath { index, siblings })
    }

    /// Full recomputation of the root from stored leaves (cross-check for the
    /// incremental update; also the empty-tree root when there are no leaves).
    pub fn recompute_root(&self) -> F {
        if self.leaves.is_empty() {
            return self.zeros[self.height];
        }
        let mut level = self.leaves.clone();
        for h in 0..self.height {
            level = Self::parent_level(&level, self.zeros[h]);
        }
        level[0]
    }

    /// Compute the parent level, zero-padding a missing right sibling with the
    /// zero-subtree hash for this level.
    fn parent_level(level: &[F], zero: F) -> Vec<F> {
        let mut parents = Vec::with_capacity(level.len().div_ceil(2));
        let mut i = 0;
        while i < level.len() {
            let left = level[i];
            let right = if i + 1 < level.len() {
                level[i + 1]
            } else {
                zero
            };
            parents.push(hash2(left, right));
            i += 2;
        }
        parents
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::from_u64;

    fn leaf(n: u64) -> F {
        from_u64(1000 + n)
    }

    #[test]
    fn empty_tree_root_matches_zeros() {
        let t = IncrementalMerkleTree::new(10);
        assert_eq!(t.root(), t.recompute_root());
        assert!(t.is_empty());
    }

    #[test]
    fn incremental_root_matches_full_recompute() {
        let mut t = IncrementalMerkleTree::new(12);
        for n in 0..20 {
            t.insert(leaf(n)).unwrap();
            // The cheap incremental root must equal the full recomputation.
            assert_eq!(t.root(), t.recompute_root(), "mismatch after {n} inserts");
        }
    }

    #[test]
    fn openings_verify_for_every_leaf() {
        let mut t = IncrementalMerkleTree::new(16);
        let count = 25;
        for n in 0..count {
            t.insert(leaf(n)).unwrap();
        }
        let root = t.root();
        for n in 0..count {
            let path = t.opening(n as usize).expect("leaf exists");
            assert!(path.verify(leaf(n), root), "opening {n} failed to verify");
        }
    }

    #[test]
    fn opening_rejects_wrong_leaf() {
        let mut t = IncrementalMerkleTree::new(8);
        for n in 0..5 {
            t.insert(leaf(n)).unwrap();
        }
        let path = t.opening(2).unwrap();
        assert!(path.verify(leaf(2), t.root()));
        assert!(!path.verify(leaf(99), t.root()));
    }

    #[test]
    fn opening_out_of_range_is_none() {
        let mut t = IncrementalMerkleTree::new(8);
        t.insert(leaf(0)).unwrap();
        assert!(t.opening(1).is_none());
    }

    #[test]
    fn works_at_production_height() {
        let mut t = IncrementalMerkleTree::new(DEFAULT_HEIGHT);
        for n in 0..4 {
            t.insert(leaf(n)).unwrap();
        }
        let root = t.root();
        let path = t.opening(3).unwrap();
        assert_eq!(path.siblings.len(), DEFAULT_HEIGHT);
        assert!(path.verify(leaf(3), root));
        assert_eq!(t.root(), t.recompute_root());
    }

    #[test]
    fn tiny_tree_fills_and_rejects_overflow() {
        let mut t = IncrementalMerkleTree::new(2); // capacity 4
        for n in 0..4 {
            t.insert(leaf(n)).unwrap();
        }
        assert_eq!(t.insert(leaf(4)), Err(MerkleError::Full));
    }
}
