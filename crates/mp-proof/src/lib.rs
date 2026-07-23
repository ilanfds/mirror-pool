//! mirror-pool Groth16 proving (arkworks).
//!
//! `docs/ROADMAP.md` Phase 3: the `S_propose` statement (`docs/DESIGN.md` §6.4)
//! as an R1CS circuit, off-chain proving, and the public-input encoding shared
//! with the on-chain verifier.
//!
//! - [`poseidon`] — the in-circuit Poseidon gadget, matching `light-poseidon`.
//! - [`circuit`] — the `S_propose` membership/nullifier circuit.
//! - [`proving`] — Groth16 setup, prove, and verify over BN254.

pub mod circuit;
pub mod poseidon;
pub mod proving;

pub use circuit::ProposeCircuit;
pub use poseidon::PoseidonGadget;
