//! On-chain Groth16 verification of proposals (ROADMAP phase 4 — the MVP).
//!
//! A proof generated off-chain by `mp-proof` (against the dev key embedded in
//! the program) is verified inside the `propose` instruction. These tests close
//! the anonymous-proposal loop end to end and check the failure paths.
//!
//! Skips if the program artifact is absent.

mod common;
use common::*;
use mirror_pool::RoundPhase;

#[test]
fn valid_proof_is_accepted() {
    let Some(mut svm) = load() else { return };
    let payer = fund(&mut svm);
    let rid = 100;
    assert!(init_pool(&mut svm, &payer));
    let proposal = deposit_and_prove(&mut svm, &payer, rid, 777);
    assert!(send(&mut svm, &payer, &[open_ix(&payer.pubkey(), rid, 1)]));

    assert!(
        send_propose(&mut svm, &payer, &proposal),
        "a valid proof must be accepted on-chain"
    );

    let r = read_round(&svm, rid);
    assert_eq!(r.phase, RoundPhase::Propose);
    assert!(r.action_set);
    assert_eq!(r.proposal_count, 1);
    assert_eq!(r.action, proposal.action);
}

#[test]
fn tampered_proof_is_rejected() {
    let Some(mut svm) = load() else { return };
    let payer = fund(&mut svm);
    let rid = 101;
    assert!(init_pool(&mut svm, &payer));
    let proposal = deposit_and_prove(&mut svm, &payer, rid, 777);
    assert!(send(&mut svm, &payer, &[open_ix(&payer.pubkey(), rid, 1)]));

    let mut bad = proposal.clone();
    bad.proof_a[0] ^= 0xff;
    assert!(
        !send_propose(&mut svm, &payer, &bad),
        "a corrupted proof must be rejected"
    );
}

#[test]
fn wrong_action_is_rejected() {
    let Some(mut svm) = load() else { return };
    let payer = fund(&mut svm);
    let rid = 102;
    assert!(init_pool(&mut svm, &payer));
    let proposal = deposit_and_prove(&mut svm, &payer, rid, 777);
    assert!(send(&mut svm, &payer, &[open_ix(&payer.pubkey(), rid, 1)]));

    // The proof commits to the original action; submitting a different action
    // changes a public input and the proof no longer verifies (non-malleability).
    let mut bad = proposal.clone();
    bad.action[31] ^= 1;
    assert!(
        !send_propose(&mut svm, &payer, &bad),
        "swapping the action must invalidate the proof"
    );
}

#[test]
fn unknown_root_is_rejected() {
    let Some(mut svm) = load() else { return };
    let payer = fund(&mut svm);
    let rid = 103;
    assert!(init_pool(&mut svm, &payer));
    let proposal = deposit_and_prove(&mut svm, &payer, rid, 777);
    assert!(send(&mut svm, &payer, &[open_ix(&payer.pubkey(), rid, 1)]));

    // A root the pool never produced is rejected before verification.
    let mut bad = proposal.clone();
    bad.root = [7u8; 32];
    assert!(
        !send_propose(&mut svm, &payer, &bad),
        "an unknown membership root must be rejected"
    );
}
