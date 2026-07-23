//! Round state-machine tests (ROADMAP phase 2): the
//! `Propose -> Commit -> Execute -> Closed` lifecycle, the threshold GO/ABORT
//! decision, nullifier-based double-propose protection, and phase/window guards.
//!
//! Skips if the program artifact is absent so `cargo test --workspace` stays
//! green without the Solana toolchain.

use anchor_lang::{AccountDeserialize, InstructionData};
use litesvm::LiteSVM;
use mirror_pool::{RoundOutcome, RoundPhase};
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::Transaction;

const PROGRAM_SO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/deploy/mirror_pool.so"
);
const ACTION: [u8; 32] = [7u8; 32];

fn program_id() -> Pubkey {
    Pubkey::new_from_array(mirror_pool::ID.to_bytes())
}

fn system_program_id() -> Pubkey {
    Pubkey::new_from_array([0u8; 32])
}

fn load() -> Option<LiteSVM> {
    if !std::path::Path::new(PROGRAM_SO).exists() {
        eprintln!("skipping: {PROGRAM_SO} not built (run `anchor build`)");
        return None;
    }
    let mut svm = LiteSVM::new();
    svm.add_program_from_file(program_id(), PROGRAM_SO)
        .expect("load program .so");
    Some(svm)
}

/// A fresh, funded keypair — models a distinct participant and keeps each
/// transaction's signature unique.
fn fund(svm: &mut LiteSVM) -> Keypair {
    let kp = Keypair::new();
    svm.airdrop(&kp.pubkey(), 1_000_000_000).unwrap();
    kp
}

fn round_pda(round_id: u64) -> Pubkey {
    Pubkey::find_program_address(&[b"round", round_id.to_le_bytes().as_ref()], &program_id()).0
}

fn nullifier_pda(round_id: u64, nf: &[u8; 32]) -> Pubkey {
    Pubkey::find_program_address(
        &[b"nullifier", round_id.to_le_bytes().as_ref(), nf.as_ref()],
        &program_id(),
    )
    .0
}

fn read_round(svm: &LiteSVM, round_id: u64) -> mirror_pool::Round {
    let acct = svm.get_account(&round_pda(round_id)).expect("round exists");
    let mut d = acct.data.as_slice();
    mirror_pool::Round::try_deserialize(&mut d).expect("deserialize Round")
}

fn tx_ok(svm: &mut LiteSVM, payer: &Keypair, ix: Instruction) -> bool {
    let msg = Message::new(&[ix], Some(&payer.pubkey()));
    let tx = Transaction::new(&[payer], msg, svm.latest_blockhash());
    svm.send_transaction(tx).is_ok()
}

fn open_ix(payer: &Pubkey, round_id: u64, threshold: u64) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(round_pda(round_id), false),
            AccountMeta::new(*payer, true),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: mirror_pool::instruction::OpenRound {
            round_id,
            propose_slots: 5,
            commit_slots: 5,
            execute_slots: 5,
            threshold,
        }
        .data(),
    }
}

fn propose_ix(payer: &Pubkey, round_id: u64, nf: [u8; 32]) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(round_pda(round_id), false),
            AccountMeta::new(nullifier_pda(round_id, &nf), false),
            AccountMeta::new(*payer, true),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: mirror_pool::instruction::Propose {
            round_id,
            nullifier: nf,
            action: ACTION,
        }
        .data(),
    }
}

fn commit_ix(round_id: u64) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![AccountMeta::new(round_pda(round_id), false)],
        data: mirror_pool::instruction::Commit { round_id }.data(),
    }
}

fn advance_ix(round_id: u64) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![AccountMeta::new(round_pda(round_id), false)],
        data: mirror_pool::instruction::AdvanceRound { round_id }.data(),
    }
}

#[test]
fn lifecycle_reaches_go_and_closes() {
    let Some(mut svm) = load() else { return };
    let rid = 1;

    let admin = fund(&mut svm);
    assert!(tx_ok(&mut svm, &admin, open_ix(&admin.pubkey(), rid, 3)));

    // Propose an action.
    let proposer = fund(&mut svm);
    assert!(tx_ok(
        &mut svm,
        &proposer,
        propose_ix(&proposer.pubkey(), rid, [1u8; 32])
    ));
    let r = read_round(&svm, rid);
    assert_eq!(r.phase, RoundPhase::Propose);
    assert!(r.action_set);
    assert_eq!(r.proposal_count, 1);
    assert_eq!(r.action, ACTION);

    // Seal: Propose -> Commit.
    svm.warp_to_slot(r.propose_end_slot);
    let k = fund(&mut svm);
    assert!(tx_ok(&mut svm, &k, advance_ix(rid)));
    assert_eq!(read_round(&svm, rid).phase, RoundPhase::Commit);

    // Three commits meet the threshold of 3.
    for _ in 0..3 {
        let c = fund(&mut svm);
        assert!(tx_ok(&mut svm, &c, commit_ix(rid)));
    }
    let r = read_round(&svm, rid);
    assert_eq!(r.commit_count, 3);

    // Threshold: Commit -> Execute (GO).
    svm.warp_to_slot(r.commit_end_slot);
    let k = fund(&mut svm);
    assert!(tx_ok(&mut svm, &k, advance_ix(rid)));
    let r = read_round(&svm, rid);
    assert_eq!(r.phase, RoundPhase::Execute);
    assert_eq!(r.outcome, RoundOutcome::Go);

    // Finalize: Execute -> Closed.
    svm.warp_to_slot(r.execute_end_slot);
    let k = fund(&mut svm);
    assert!(tx_ok(&mut svm, &k, advance_ix(rid)));
    assert_eq!(read_round(&svm, rid).phase, RoundPhase::Closed);
}

#[test]
fn thin_crowd_aborts() {
    let Some(mut svm) = load() else { return };
    let rid = 2;

    let admin = fund(&mut svm);
    assert!(tx_ok(&mut svm, &admin, open_ix(&admin.pubkey(), rid, 5))); // threshold 5

    let proposer = fund(&mut svm);
    assert!(tx_ok(
        &mut svm,
        &proposer,
        propose_ix(&proposer.pubkey(), rid, [1u8; 32])
    ));

    let r = read_round(&svm, rid);
    svm.warp_to_slot(r.propose_end_slot);
    let k = fund(&mut svm);
    assert!(tx_ok(&mut svm, &k, advance_ix(rid)));

    // Only two commits — below the threshold of 5.
    for _ in 0..2 {
        let c = fund(&mut svm);
        assert!(tx_ok(&mut svm, &c, commit_ix(rid)));
    }

    let r = read_round(&svm, rid);
    svm.warp_to_slot(r.commit_end_slot);
    let k = fund(&mut svm);
    assert!(tx_ok(&mut svm, &k, advance_ix(rid)));

    let r = read_round(&svm, rid);
    assert_eq!(r.phase, RoundPhase::Closed);
    assert_eq!(r.outcome, RoundOutcome::Abort);
}

#[test]
fn no_proposal_aborts_at_seal() {
    let Some(mut svm) = load() else { return };
    let rid = 3;

    let admin = fund(&mut svm);
    assert!(tx_ok(&mut svm, &admin, open_ix(&admin.pubkey(), rid, 1)));

    // No proposal at all.
    let r = read_round(&svm, rid);
    svm.warp_to_slot(r.propose_end_slot);
    let k = fund(&mut svm);
    assert!(tx_ok(&mut svm, &k, advance_ix(rid)));

    let r = read_round(&svm, rid);
    assert_eq!(r.phase, RoundPhase::Closed);
    assert_eq!(r.outcome, RoundOutcome::Abort);
}

#[test]
fn double_propose_same_nullifier_is_rejected() {
    let Some(mut svm) = load() else { return };
    let rid = 4;

    let admin = fund(&mut svm);
    assert!(tx_ok(&mut svm, &admin, open_ix(&admin.pubkey(), rid, 1)));

    let p1 = fund(&mut svm);
    assert!(tx_ok(
        &mut svm,
        &p1,
        propose_ix(&p1.pubkey(), rid, [9u8; 32])
    ));

    // Same nullifier, different payer: the nullifier marker PDA already exists,
    // so `init` fails and the transaction reverts.
    let p2 = fund(&mut svm);
    assert!(!tx_ok(
        &mut svm,
        &p2,
        propose_ix(&p2.pubkey(), rid, [9u8; 32])
    ));
}

#[test]
fn commit_in_propose_phase_is_rejected() {
    let Some(mut svm) = load() else { return };
    let rid = 5;

    let admin = fund(&mut svm);
    assert!(tx_ok(&mut svm, &admin, open_ix(&admin.pubkey(), rid, 1)));

    // Still in Propose: committing is a wrong-phase error.
    let c = fund(&mut svm);
    assert!(!tx_ok(&mut svm, &c, commit_ix(rid)));
}

#[test]
fn advance_before_window_is_rejected() {
    let Some(mut svm) = load() else { return };
    let rid = 6;

    let admin = fund(&mut svm);
    assert!(tx_ok(&mut svm, &admin, open_ix(&admin.pubkey(), rid, 1)));

    // No warp: the propose window has not elapsed, so advancing must fail.
    let k = fund(&mut svm);
    assert!(!tx_ok(&mut svm, &k, advance_ix(rid)));
}
