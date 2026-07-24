//! Cover-credit incentive tests (ROADMAP phase 7): committing grants one credit
//! per distinct wallet per round, and a wallet cannot commit twice in a round
//! (so `commit_count` counts distinct participants).
//!
//! Skips if the program artifact is absent.

mod common;
use common::*;
use litesvm::LiteSVM;
use mirror_pool::RoundPhase;
use solana_keypair::Keypair;

/// Drive a fresh round to the `Commit` phase with one valid proposal.
fn reach_commit_phase(svm: &mut LiteSVM, payer: &Keypair, rid: u64) {
    assert!(init_pool(svm, payer));
    let proposal = deposit_and_prove(svm, payer, rid, 999);
    assert!(send(svm, payer, &[open_ix(&payer.pubkey(), rid, 1)]));
    assert!(send_propose(svm, payer, &proposal));

    let r = read_round(svm, rid);
    svm.warp_to_slot(r.propose_end_slot);
    let k = fund(svm);
    assert!(send(svm, &k, &[advance_ix(rid)]));
    assert_eq!(read_round(svm, rid).phase, RoundPhase::Commit);
}

#[test]
fn committing_grants_a_cover_credit() {
    let Some(mut svm) = load() else { return };
    let payer = fund(&mut svm);
    let rid = 200;
    reach_commit_phase(&mut svm, &payer, rid);

    let a = fund(&mut svm);
    let b = fund(&mut svm);
    assert!(send(&mut svm, &a, &[commit_ix(rid, &a.pubkey())]));
    assert!(send(&mut svm, &b, &[commit_ix(rid, &b.pubkey())]));

    assert_eq!(read_credit(&svm, &a.pubkey()), Some(1));
    assert_eq!(read_credit(&svm, &b.pubkey()), Some(1));
    assert_eq!(read_round(&svm, rid).commit_count, 2);

    // A wallet that never committed has no credit account.
    let stranger = fund(&mut svm);
    assert_eq!(read_credit(&svm, &stranger.pubkey()), None);
}

#[test]
fn a_wallet_cannot_commit_twice_in_a_round() {
    let Some(mut svm) = load() else { return };
    let payer = fund(&mut svm);
    let rid = 201;
    reach_commit_phase(&mut svm, &payer, rid);

    let a = fund(&mut svm);
    assert!(send(&mut svm, &a, &[commit_ix(rid, &a.pubkey())]));
    // Second commit by the same wallet: the marker PDA already exists.
    assert!(!send(&mut svm, &a, &[commit_ix(rid, &a.pubkey())]));

    // The count reflects a single distinct committer.
    assert_eq!(read_round(&svm, rid).commit_count, 1);
    assert_eq!(read_credit(&svm, &a.pubkey()), Some(1));
}
