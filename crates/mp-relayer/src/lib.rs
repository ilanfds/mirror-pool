//! Trust-minimized proposal submitter (`docs/ROADMAP.md` Phase 6).
//!
//! The relayer submits a proposer's `propose` transaction so the proposer's own
//! wallet is never the on-chain fee payer — the same role Tornado's relayer
//! plays (`docs/DESIGN.md` §4, §6.4). It is trust-minimized: the action is bound
//! inside the Groth16 proof, so the relayer **cannot alter it**; it can only
//! submit or withhold.
//!
//! This crate builds the exact `propose` instruction and a ready-to-broadcast
//! transaction. RPC submission is a thin wrapper over the built transaction and
//! is left to the operator's client.

use mp_agent::ProposalArgs;
use sha2::{Digest, Sha256};
use solana_compute_budget_interface::ComputeBudgetInstruction;
use solana_hash::Hash;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::Transaction;

/// Groth16 verification needs well above the 200k default compute budget.
pub const DEFAULT_COMPUTE_UNIT_LIMIT: u32 = 1_400_000;

fn system_program_id() -> Pubkey {
    Pubkey::new_from_array([0u8; 32])
}

/// Anchor's global instruction discriminator: `sha256("global:<name>")[..8]`.
pub fn propose_discriminator() -> [u8; 8] {
    let digest = Sha256::digest(b"global:propose");
    digest[..8].try_into().expect("8 bytes")
}

/// The pool PDA (`seeds = ["pool"]`).
pub fn pool_pda(program_id: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"pool"], program_id).0
}

/// A round PDA (`seeds = ["round", round_id_le]`).
pub fn round_pda(program_id: &Pubkey, round_id: u64) -> Pubkey {
    Pubkey::find_program_address(&[b"round", round_id.to_le_bytes().as_ref()], program_id).0
}

/// A nullifier marker PDA (`seeds = ["nullifier", round_id_le, nullifier]`).
pub fn nullifier_pda(program_id: &Pubkey, round_id: u64, nullifier: &[u8; 32]) -> Pubkey {
    Pubkey::find_program_address(
        &[
            b"nullifier",
            round_id.to_le_bytes().as_ref(),
            nullifier.as_ref(),
        ],
        program_id,
    )
    .0
}

/// Build the `propose` instruction with `payer` (the relayer) as fee payer.
///
/// The serialized data is byte-identical to Anchor's client encoding:
/// `discriminator(8) || round_id(8, LE) || nullifier(32) || action(32) ||
/// root(32) || proof_a(64) || proof_b(128) || proof_c(64)`.
pub fn propose_instruction(program_id: &Pubkey, payer: &Pubkey, p: &ProposalArgs) -> Instruction {
    let mut data = Vec::with_capacity(8 + 360);
    data.extend_from_slice(&propose_discriminator());
    data.extend_from_slice(&p.round_id.to_le_bytes());
    data.extend_from_slice(&p.nullifier);
    data.extend_from_slice(&p.action);
    data.extend_from_slice(&p.root);
    data.extend_from_slice(&p.proof_a);
    data.extend_from_slice(&p.proof_b);
    data.extend_from_slice(&p.proof_c);

    Instruction {
        program_id: *program_id,
        accounts: vec![
            AccountMeta::new_readonly(pool_pda(program_id), false),
            AccountMeta::new(round_pda(program_id, p.round_id), false),
            AccountMeta::new(nullifier_pda(program_id, p.round_id, &p.nullifier), false),
            AccountMeta::new(*payer, true),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data,
    }
}

/// Build a signed, ready-to-broadcast `propose` transaction, with a raised
/// compute-unit limit, paid and signed by `relayer`.
pub fn build_propose_transaction(
    program_id: &Pubkey,
    relayer: &Keypair,
    recent_blockhash: Hash,
    p: &ProposalArgs,
) -> Transaction {
    let ixs = [
        ComputeBudgetInstruction::set_compute_unit_limit(DEFAULT_COMPUTE_UNIT_LIMIT),
        propose_instruction(program_id, &relayer.pubkey(), p),
    ];
    let msg = Message::new(&ixs, Some(&relayer.pubkey()));
    Transaction::new(&[relayer], msg, recent_blockhash)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_args() -> ProposalArgs {
        ProposalArgs {
            round_id: 7,
            root: [1u8; 32],
            nullifier: [2u8; 32],
            action: [3u8; 32],
            proof_a: [4u8; 64],
            proof_b: [5u8; 128],
            proof_c: [6u8; 64],
        }
    }

    #[test]
    fn discriminator_matches_the_program() {
        use anchor_lang::Discriminator;
        assert_eq!(
            propose_discriminator().as_slice(),
            mirror_pool::instruction::Propose::DISCRIMINATOR
        );
    }

    #[test]
    fn instruction_data_matches_anchor_encoding() {
        use anchor_lang::InstructionData;
        let p = sample_args();
        let anchor_data = mirror_pool::instruction::Propose {
            round_id: p.round_id,
            nullifier: p.nullifier,
            action: p.action,
            root: p.root,
            proof_a: p.proof_a,
            proof_b: p.proof_b,
            proof_c: p.proof_c,
        }
        .data();

        let ix = propose_instruction(&Pubkey::new_unique(), &Pubkey::new_unique(), &p);
        assert_eq!(
            ix.data, anchor_data,
            "relayer must match Anchor byte-for-byte"
        );
    }

    #[test]
    fn accounts_are_the_expected_pdas() {
        let program_id = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let p = sample_args();
        let ix = propose_instruction(&program_id, &payer, &p);

        assert_eq!(ix.accounts[0].pubkey, pool_pda(&program_id));
        assert_eq!(ix.accounts[1].pubkey, round_pda(&program_id, p.round_id));
        assert_eq!(
            ix.accounts[2].pubkey,
            nullifier_pda(&program_id, p.round_id, &p.nullifier)
        );
        assert_eq!(ix.accounts[3].pubkey, payer);
        assert!(ix.accounts[3].is_signer);
    }

    #[test]
    fn transaction_has_budget_and_propose_paid_by_relayer() {
        let relayer = Keypair::new();
        let tx = build_propose_transaction(
            &Pubkey::new_unique(),
            &relayer,
            Hash::default(),
            &sample_args(),
        );
        // Compute-budget instruction + propose instruction.
        assert_eq!(tx.message.instructions.len(), 2);
        assert_eq!(tx.message.account_keys[0], relayer.pubkey());
    }
}
