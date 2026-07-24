//! mirror-pool on-chain program.
//!
//! Implements the membership Merkle tree + deposit, the round state machine, the
//! nullifier set, and — in `propose` — on-chain Groth16 verification of the
//! anonymous-proposal proof via `groth16-solana`. See `docs/DESIGN.md` §6, §11.
//!
//! Field elements are represented on-chain as **32-byte big-endian** limbs and
//! hashed with the Solana Poseidon syscall (`Bn254X5`, big-endian). This matches
//! `mp-crypto` (which hashes the same values off-chain via `light-poseidon`), so
//! the on-chain root reproduces the off-chain root bit-for-bit — the invariant
//! the cross-check test asserts.

use anchor_lang::prelude::*;
use groth16_solana::groth16::Groth16Verifier;
#[allow(deprecated)]
use solana_poseidon::{hashv, Endianness, Parameters};

mod vk;

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

    /// Open a new round in the `Propose` phase and set its slot windows.
    pub fn open_round(
        ctx: Context<OpenRound>,
        round_id: u64,
        propose_slots: u64,
        commit_slots: u64,
        execute_slots: u64,
        threshold: u64,
    ) -> Result<()> {
        let clock = Clock::get()?;
        let round = &mut ctx.accounts.round;
        round.round_id = round_id;
        round.bump = ctx.bumps.round;
        round.phase = RoundPhase::Propose;
        round.outcome = RoundOutcome::Pending;
        round.action = [0u8; ACTION_LEN];
        round.action_set = false;
        round.proposal_count = 0;
        round.commit_count = 0;
        round.threshold = threshold;
        round.propose_end_slot = clock.slot.saturating_add(propose_slots);
        round.commit_end_slot = round.propose_end_slot.saturating_add(commit_slots);
        round.execute_end_slot = round.commit_end_slot.saturating_add(execute_slots);
        emit!(RoundOpened {
            round_id,
            threshold
        });
        Ok(())
    }

    /// Anonymously summon the round's action (Property A).
    ///
    /// Verifies a Groth16 proof of the `S_propose` statement (DESIGN §6.4): the
    /// prover knows a membership note whose commitment sits under `root` and a
    /// nullifier `= H(k, round_id)`, with `action` bound as a public input. The
    /// proof reveals nothing about *which* member proposed. Replay is prevented
    /// by the nullifier marker PDA (its `init` fails on reuse).
    #[allow(clippy::too_many_arguments)]
    pub fn propose(
        ctx: Context<Propose>,
        round_id: u64,
        nullifier: [u8; 32],
        action: [u8; ACTION_LEN],
        root: [u8; 32],
        proof_a: [u8; 64],
        proof_b: [u8; 128],
        proof_c: [u8; 64],
    ) -> Result<()> {
        let clock = Clock::get()?;
        require!(
            ctx.accounts.round.phase == RoundPhase::Propose,
            MirrorPoolError::WrongPhase
        );
        require!(
            clock.slot < ctx.accounts.round.propose_end_slot,
            MirrorPoolError::WindowClosed
        );

        // The proof must reference a real, recent membership root (not a tree of
        // the prover's own making). Empty history slots are all-zero.
        require!(
            root != [0u8; 32] && ctx.accounts.pool.roots.contains(&root),
            MirrorPoolError::UnknownRoot
        );

        // Public inputs, in the circuit's canonical order.
        let public_inputs = [root, nullifier, round_id_to_input(round_id), action];
        let verifying_key = vk::verifying_key();
        let mut verifier =
            Groth16Verifier::<4>::new(&proof_a, &proof_b, &proof_c, &public_inputs, &verifying_key)
                .map_err(|_| error!(MirrorPoolError::ProofInvalid))?;
        verifier
            .verify()
            .map_err(|_| error!(MirrorPoolError::ProofInvalid))?;

        // Seal the action (first valid proposal sets it; later ones must match).
        let round = &mut ctx.accounts.round;
        if round.action_set {
            require!(round.action == action, MirrorPoolError::ActionMismatch);
        } else {
            round.action = action;
            round.action_set = true;
        }
        round.proposal_count = round.proposal_count.saturating_add(1);
        Ok(())
    }

    /// Sign up to execute the round's action (Property B crowd commit).
    ///
    /// Records the committer once per round — a marker PDA stops a single wallet
    /// from inflating the count, so `commit_count` reflects *distinct*
    /// participants — and grants a cover credit ("provide cover to get cover",
    /// DESIGN §8). Committers act in the open, so crediting them leaks nothing
    /// about the hidden initiator. The binding pre-signed durable-nonce
    /// execution is held off-chain (DESIGN §6.5).
    #[allow(unused_variables)]
    pub fn commit(ctx: Context<Commit>, round_id: u64) -> Result<()> {
        let clock = Clock::get()?;
        require!(
            ctx.accounts.round.phase == RoundPhase::Commit,
            MirrorPoolError::WrongPhase
        );
        require!(
            clock.slot < ctx.accounts.round.commit_end_slot,
            MirrorPoolError::WindowClosed
        );

        let credit = &mut ctx.accounts.credit;
        credit.bump = ctx.bumps.credit;
        credit.credits = credit.credits.saturating_add(1);

        let round = &mut ctx.accounts.round;
        round.commit_count = round.commit_count.saturating_add(1);
        Ok(())
    }

    /// Advance the round to its next phase once the current window has elapsed:
    /// seal (`Propose -> Commit`, aborting if no action was proposed), evaluate
    /// the threshold (`Commit -> Execute` on GO, else abort), and finalize
    /// (`Execute -> Closed`).
    #[allow(unused_variables)]
    pub fn advance_round(ctx: Context<UpdateRound>, round_id: u64) -> Result<()> {
        let clock = Clock::get()?;
        let round = &mut ctx.accounts.round;
        match round.phase {
            RoundPhase::Propose => {
                require!(
                    clock.slot >= round.propose_end_slot,
                    MirrorPoolError::WindowOpen
                );
                if round.action_set {
                    round.phase = RoundPhase::Commit;
                } else {
                    round.phase = RoundPhase::Closed;
                    round.outcome = RoundOutcome::Abort;
                }
            }
            RoundPhase::Commit => {
                require!(
                    clock.slot >= round.commit_end_slot,
                    MirrorPoolError::WindowOpen
                );
                if round.commit_count >= round.threshold {
                    round.phase = RoundPhase::Execute;
                    round.outcome = RoundOutcome::Go;
                } else {
                    round.phase = RoundPhase::Closed;
                    round.outcome = RoundOutcome::Abort;
                }
            }
            RoundPhase::Execute => {
                require!(
                    clock.slot >= round.execute_end_slot,
                    MirrorPoolError::WindowOpen
                );
                round.phase = RoundPhase::Closed;
            }
            RoundPhase::Closed => return err!(MirrorPoolError::RoundClosed),
        }
        emit!(RoundAdvanced {
            round_id: round.round_id,
            phase: round.phase,
            outcome: round.outcome,
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

/// Encode a `u64` round id as a 32-byte big-endian field element, matching the
/// circuit's `round_id` public input (`Fr::from(round_id)`).
fn round_id_to_input(round_id: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[24..].copy_from_slice(&round_id.to_be_bytes());
    out
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

/// Length of the (opaque, for now) sealed action payload.
pub const ACTION_LEN: usize = 32;

/// Persistent phases of a round (Seal and Threshold are transitions, not
/// phases). See DESIGN §6.3.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum RoundPhase {
    Propose,
    Commit,
    Execute,
    Closed,
}

/// Result of the threshold check (and whether the round produced cover).
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum RoundOutcome {
    Pending,
    Go,
    Abort,
}

/// A synchronized round: its phase, sealed action, tallies, and slot windows.
#[account]
pub struct Round {
    pub round_id: u64,
    pub bump: u8,
    pub phase: RoundPhase,
    pub outcome: RoundOutcome,
    /// Sealed action the crowd rallies around (opaque until the vocabulary lands).
    pub action: [u8; ACTION_LEN],
    pub action_set: bool,
    pub proposal_count: u64,
    pub commit_count: u64,
    pub threshold: u64,
    pub propose_end_slot: u64,
    pub commit_end_slot: u64,
    pub execute_end_slot: u64,
}

impl Round {
    /// Serialized size excluding the 8-byte discriminator. Enums are 1-byte
    /// unit variants.
    pub const SIZE: usize = 8 + 1 + 1 + 1 + ACTION_LEN + 1 + 8 + 8 + 8 + 8 + 8 + 8;
}

/// Zero-data marker; the existence of its PDA (seeds include the round id and
/// the nullifier) means "this nullifier has been spent this round".
#[account]
pub struct NullifierMarker {}

/// A participant's cover-credit balance (`seeds = ["credit", owner]`). Earned by
/// providing cover; the reputation half of the incentive layer (DESIGN §8).
#[account]
pub struct CreditAccount {
    pub credits: u64,
    pub bump: u8,
}

impl CreditAccount {
    pub const SIZE: usize = 8 + 1;
}

/// Zero-data marker; its PDA existence (seeds include the round id and the
/// committer) means "this wallet has already committed this round".
#[account]
pub struct CommitMarker {}

#[derive(Accounts)]
#[instruction(round_id: u64)]
pub struct OpenRound<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + Round::SIZE,
        seeds = [b"round", round_id.to_le_bytes().as_ref()],
        bump
    )]
    pub round: Account<'info, Round>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(round_id: u64, nullifier: [u8; 32])]
pub struct Propose<'info> {
    #[account(seeds = [b"pool"], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [b"round", round_id.to_le_bytes().as_ref()], bump = round.bump)]
    pub round: Account<'info, Round>,
    #[account(
        init,
        payer = payer,
        space = 8,
        seeds = [b"nullifier", round_id.to_le_bytes().as_ref(), nullifier.as_ref()],
        bump
    )]
    pub nullifier_marker: Account<'info, NullifierMarker>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(round_id: u64)]
pub struct Commit<'info> {
    #[account(mut, seeds = [b"round", round_id.to_le_bytes().as_ref()], bump = round.bump)]
    pub round: Account<'info, Round>,
    #[account(
        init_if_needed,
        payer = committer,
        space = 8 + CreditAccount::SIZE,
        seeds = [b"credit", committer.key().as_ref()],
        bump
    )]
    pub credit: Account<'info, CreditAccount>,
    #[account(
        init,
        payer = committer,
        space = 8,
        seeds = [b"commit", round_id.to_le_bytes().as_ref(), committer.key().as_ref()],
        bump
    )]
    pub commit_marker: Account<'info, CommitMarker>,
    #[account(mut)]
    pub committer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(round_id: u64)]
pub struct UpdateRound<'info> {
    #[account(mut, seeds = [b"round", round_id.to_le_bytes().as_ref()], bump = round.bump)]
    pub round: Account<'info, Round>,
}

#[event]
pub struct RoundOpened {
    pub round_id: u64,
    pub threshold: u64,
}

#[event]
pub struct RoundAdvanced {
    pub round_id: u64,
    pub phase: RoundPhase,
    pub outcome: RoundOutcome,
}

#[error_code]
pub enum MirrorPoolError {
    #[msg("membership tree is full")]
    TreeFull,
    #[msg("poseidon syscall failed")]
    PoseidonFailed,
    #[msg("instruction not valid in the round's current phase")]
    WrongPhase,
    #[msg("the phase window has already closed")]
    WindowClosed,
    #[msg("the phase window has not elapsed yet")]
    WindowOpen,
    #[msg("proposed action does not match the round's sealed action")]
    ActionMismatch,
    #[msg("round is already closed")]
    RoundClosed,
    #[msg("proof references an unknown membership root")]
    UnknownRoot,
    #[msg("groth16 proof verification failed")]
    ProofInvalid,
}
