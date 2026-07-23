//! Round state-machine tests (ROADMAP phase 2), now driving `propose` with real
//! Groth16 proofs (phase 4): the `Propose -> Commit -> Execute -> Closed`
//! lifecycle, the threshold GO/ABORT decision, nullifier double-propose
//! protection, and phase/window guards.
//!
//! Skips if the program artifact is absent so `cargo test --workspace` stays
//! green without the Solana toolchain.

mod common;
use common::*;
use mirror_pool::{RoundOutcome, RoundPhase};

#[test]
fn lifecycle_reaches_go_and_closes() {
    let Some(mut svm) = load() else { return };
    let payer = fund(&mut svm);
    let rid = 1;
    assert!(init_pool(&mut svm, &payer));
    let proposal = deposit_and_prove(&mut svm, &payer, rid, 999);

    assert!(send(&mut svm, &payer, &[open_ix(&payer.pubkey(), rid, 3)]));
    assert!(send_propose(&mut svm, &payer, &proposal));

    let r = read_round(&svm, rid);
    assert_eq!(r.phase, RoundPhase::Propose);
    assert!(r.action_set);
    assert_eq!(r.proposal_count, 1);

    // Seal: Propose -> Commit.
    svm.warp_to_slot(r.propose_end_slot);
    let k = fund(&mut svm);
    assert!(send(&mut svm, &k, &[advance_ix(rid)]));
    assert_eq!(read_round(&svm, rid).phase, RoundPhase::Commit);

    // Three commits meet the threshold of 3.
    for _ in 0..3 {
        let c = fund(&mut svm);
        assert!(send(&mut svm, &c, &[commit_ix(rid)]));
    }

    // Threshold: Commit -> Execute (GO).
    let r = read_round(&svm, rid);
    assert_eq!(r.commit_count, 3);
    svm.warp_to_slot(r.commit_end_slot);
    let k = fund(&mut svm);
    assert!(send(&mut svm, &k, &[advance_ix(rid)]));
    let r = read_round(&svm, rid);
    assert_eq!(r.phase, RoundPhase::Execute);
    assert_eq!(r.outcome, RoundOutcome::Go);

    // Finalize: Execute -> Closed.
    svm.warp_to_slot(r.execute_end_slot);
    let k = fund(&mut svm);
    assert!(send(&mut svm, &k, &[advance_ix(rid)]));
    assert_eq!(read_round(&svm, rid).phase, RoundPhase::Closed);
}

#[test]
fn thin_crowd_aborts() {
    let Some(mut svm) = load() else { return };
    let payer = fund(&mut svm);
    let rid = 2;
    assert!(init_pool(&mut svm, &payer));
    let proposal = deposit_and_prove(&mut svm, &payer, rid, 999);

    assert!(send(&mut svm, &payer, &[open_ix(&payer.pubkey(), rid, 5)])); // threshold 5
    assert!(send_propose(&mut svm, &payer, &proposal));

    let r = read_round(&svm, rid);
    svm.warp_to_slot(r.propose_end_slot);
    let k = fund(&mut svm);
    assert!(send(&mut svm, &k, &[advance_ix(rid)]));

    // Only two commits — below the threshold of 5.
    for _ in 0..2 {
        let c = fund(&mut svm);
        assert!(send(&mut svm, &c, &[commit_ix(rid)]));
    }

    let r = read_round(&svm, rid);
    svm.warp_to_slot(r.commit_end_slot);
    let k = fund(&mut svm);
    assert!(send(&mut svm, &k, &[advance_ix(rid)]));

    let r = read_round(&svm, rid);
    assert_eq!(r.phase, RoundPhase::Closed);
    assert_eq!(r.outcome, RoundOutcome::Abort);
}

#[test]
fn no_proposal_aborts_at_seal() {
    let Some(mut svm) = load() else { return };
    let payer = fund(&mut svm);
    let rid = 3;
    assert!(init_pool(&mut svm, &payer));

    // No proposal at all.
    assert!(send(&mut svm, &payer, &[open_ix(&payer.pubkey(), rid, 1)]));
    let r = read_round(&svm, rid);
    svm.warp_to_slot(r.propose_end_slot);
    let k = fund(&mut svm);
    assert!(send(&mut svm, &k, &[advance_ix(rid)]));

    let r = read_round(&svm, rid);
    assert_eq!(r.phase, RoundPhase::Closed);
    assert_eq!(r.outcome, RoundOutcome::Abort);
}

#[test]
fn double_propose_same_nullifier_is_rejected() {
    let Some(mut svm) = load() else { return };
    let payer = fund(&mut svm);
    let rid = 4;
    assert!(init_pool(&mut svm, &payer));
    let proposal = deposit_and_prove(&mut svm, &payer, rid, 999);

    assert!(send(&mut svm, &payer, &[open_ix(&payer.pubkey(), rid, 1)]));
    assert!(send_propose(&mut svm, &payer, &proposal));

    // Same nullifier again: the marker PDA already exists, so `init` fails.
    let p2 = fund(&mut svm);
    assert!(!send_propose(&mut svm, &p2, &proposal));
}

#[test]
fn commit_in_propose_phase_is_rejected() {
    let Some(mut svm) = load() else { return };
    let payer = fund(&mut svm);
    let rid = 5;
    assert!(init_pool(&mut svm, &payer));

    assert!(send(&mut svm, &payer, &[open_ix(&payer.pubkey(), rid, 1)]));
    // Still in Propose: committing is a wrong-phase error.
    let c = fund(&mut svm);
    assert!(!send(&mut svm, &c, &[commit_ix(rid)]));
}

#[test]
fn advance_before_window_is_rejected() {
    let Some(mut svm) = load() else { return };
    let payer = fund(&mut svm);
    let rid = 6;
    assert!(init_pool(&mut svm, &payer));

    assert!(send(&mut svm, &payer, &[open_ix(&payer.pubkey(), rid, 1)]));
    // No warp: the propose window has not elapsed, so advancing must fail.
    let k = fund(&mut svm);
    assert!(!send(&mut svm, &k, &[advance_ix(rid)]));
}
