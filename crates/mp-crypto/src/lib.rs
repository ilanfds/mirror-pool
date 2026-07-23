//! mirror-pool cryptographic primitives.
//!
//! Implements the building blocks from `docs/DESIGN.md` §5 and is the target of
//! `docs/ROADMAP.md` Phase 1:
//!
//! - [`field`] — BN254 `Fr` element type and byte-encoding conventions,
//! - [`poseidon`] — circomlib-compatible Poseidon hashing,
//! - [`note`] — the note type `(k, r)`, commitments, and nullifiers,
//! - [`merkle`] — an incremental fixed-height Merkle tree with openings.
//!
//! Everything here is pure Rust with no Solana dependency, so it can be tested
//! in isolation. The on-chain program and the in-circuit gadgets must reproduce
//! these results bit-for-bit.

pub mod field;
pub mod merkle;
pub mod note;
pub mod poseidon;

pub use field::{F, FIELD_BYTES};
pub use merkle::{IncrementalMerkleTree, MerkleError, MerklePath, DEFAULT_HEIGHT};
pub use note::Note;
pub use poseidon::{hash, hash2};
