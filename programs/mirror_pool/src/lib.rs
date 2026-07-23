//! mirror-pool on-chain program.
//!
//! ROADMAP phase 2 (this milestone): membership Merkle tree, nullifier set, and
//! the round state machine — with the Groth16 proof check stubbed until phase 4.
//! See `docs/DESIGN.md` §6 and §11.

use anchor_lang::prelude::*;

declare_id!("9D5M9HPLS2VMPXTQyg5V4WniTJpTDF73SVAFnUY3AtKJ");

#[program]
pub mod mirror_pool {
    use super::*;

    /// Placeholder instruction; real instructions land in this phase.
    pub fn initialize(_ctx: Context<Initialize>) -> Result<()> {
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize {}
