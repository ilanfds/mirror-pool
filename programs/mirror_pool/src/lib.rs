//! mirror-pool on-chain program.
//!
//! ROADMAP phase 2: membership Merkle tree + deposit (this milestone), then the
//! nullifier set and round state machine. The Groth16 proof check is stubbed
//! until phase 4. See `docs/DESIGN.md` §6 and §11.
//!
//! Field elements are represented on-chain as **32-byte big-endian** limbs and
//! hashed with the Solana Poseidon syscall (`Bn254X5`, big-endian). This matches
//! `mp-crypto` (which hashes the same values off-chain via `light-poseidon`), so
//! the on-chain root reproduces the off-chain root bit-for-bit — the invariant
//! the cross-check test asserts.

use anchor_lang::prelude::*;
#[allow(deprecated)]
use solana_poseidon::{hashv, Endianness, Parameters};

declare_id!("9D5M9HPLS2VMPXTQyg5V4WniTJpTDF73SVAFnUY3AtKJ");

/// Height of the membership tree; capacity is `2^HEIGHT` members.
pub const HEIGHT: usize = 20;

/// Number of recent roots retained so proposals may reference a slightly stale
/// root (as in Tornado's history ring buffer).
pub const ROOT_HISTORY_SIZE: usize = 32;

/// A 32-byte big-endian field element.
pub type Fq = [u8; 32];

#[program]
pub mod mirror_pool {
    use super::*;

    /// Create the singleton pool and initialize its empty membership tree.
    pub fn initialize_pool(ctx: Context<InitializePool>) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        pool.authority = ctx.accounts.authority.key();
        pool.bump = ctx.bumps.pool;
        pool.next_index = 0;
        pool.current_root_index = 0;
        // `init` zero-fills the account, so `roots`, `filled_subtrees`, and
        // `zeros` already start as all-zero; we fill the meaningful ones below.

        // Precompute the zero-subtree hashes. `zeros[h]` is the root of an
        // all-empty subtree of height `h`; the empty leaf is the field zero
        // (all-zero big-endian bytes).
        let mut cur: Fq = [0u8; 32];
        for h in 0..HEIGHT {
            pool.zeros[h] = cur;
            pool.filled_subtrees[h] = cur;
            cur = poseidon2(&cur, &cur)?;
        }
        // `cur` is now the empty-tree root (zeros[HEIGHT]).
        pool.root = cur;
        pool.roots[0] = cur;
        Ok(())
    }

    /// Insert a membership commitment `C = H(k, r)` as a new leaf.
    ///
    /// Uses the Tornado incremental update so the root is recomputed in
    /// O(HEIGHT) hashes. Deposit is open: anyone may add a commitment.
    pub fn deposit(ctx: Context<Deposit>, commitment: Fq) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        let index = pool.next_index;
        require!(
            (index as u128) < (1u128 << HEIGHT),
            MirrorPoolError::TreeFull
        );

        let mut cur = commitment;
        let mut idx = index;
        for h in 0..HEIGHT {
            if idx & 1 == 0 {
                // `cur` is a left child; its right sibling is still empty.
                let zero = pool.zeros[h];
                pool.filled_subtrees[h] = cur;
                cur = poseidon2(&cur, &zero)?;
            } else {
                // `cur` is a right child; fold in the stored left sibling.
                let left = pool.filled_subtrees[h];
                cur = poseidon2(&left, &cur)?;
            }
            idx >>= 1;
        }

        pool.root = cur;
        pool.current_root_index = (pool.current_root_index + 1) % ROOT_HISTORY_SIZE as u32;
        let root_index = pool.current_root_index as usize;
        pool.roots[root_index] = cur;
        pool.next_index += 1;

        emit!(DepositEvent {
            index,
            commitment,
            root: cur
        });
        Ok(())
    }
}

/// Two-input Poseidon over BN254 (`Bn254X5`, big-endian) via the Solana syscall.
#[allow(deprecated)]
fn poseidon2(a: &Fq, b: &Fq) -> Result<Fq> {
    let hash = hashv(Parameters::Bn254X5, Endianness::BigEndian, &[a, b])
        .map_err(|_| error!(MirrorPoolError::PoseidonFailed))?;
    Ok(hash.to_bytes())
}

/// The singleton pool: configuration plus the incremental membership tree.
#[account]
pub struct Pool {
    /// Admin that created the pool.
    pub authority: Pubkey,
    /// PDA bump.
    pub bump: u8,
    /// Number of leaves inserted so far.
    pub next_index: u64,
    /// Current Merkle root.
    pub root: Fq,
    /// Left-hand nodes needed to fold in the next leaf.
    pub filled_subtrees: [Fq; HEIGHT],
    /// Zero-subtree hashes (`zeros[h]` = empty subtree of height `h`).
    pub zeros: [Fq; HEIGHT],
    /// Ring buffer of recent roots.
    pub roots: [Fq; ROOT_HISTORY_SIZE],
    /// Index of the newest entry in `roots`.
    pub current_root_index: u32,
}

impl Pool {
    /// Serialized size (excluding the 8-byte account discriminator):
    /// authority(32) + bump(1) + next_index(8) + root(32)
    /// + filled_subtrees + zeros + roots + current_root_index(4).
    pub const SIZE: usize =
        32 + 1 + 8 + 32 + (HEIGHT * 32) + (HEIGHT * 32) + (ROOT_HISTORY_SIZE * 32) + 4;
}

#[derive(Accounts)]
pub struct InitializePool<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + Pool::SIZE,
        seeds = [b"pool"],
        bump
    )]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut, seeds = [b"pool"], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
}

#[event]
pub struct DepositEvent {
    pub index: u64,
    pub commitment: Fq,
    pub root: Fq,
}

#[error_code]
pub enum MirrorPoolError {
    #[msg("membership tree is full")]
    TreeFull,
    #[msg("poseidon syscall failed")]
    PoseidonFailed,
}
