//! Durable-nonce execution tests (ROADMAP phase 6 / DESIGN §6.5): a transaction
//! pre-signed against a durable nonce broadcasts successfully *later*, and the
//! participant can invalidate it (opt out) by advancing their own nonce.

use litesvm::LiteSVM;
use mp_keeper::{build_durable_nonce_tx, Keeper};
use solana_hash::Hash;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_nonce::{state::State, versions::Versions};
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_system_interface::instruction as system_instruction;
use solana_transaction::Transaction;

/// Create and initialize a durable-nonce account authorized by `payer`.
fn create_nonce(svm: &mut LiteSVM, payer: &Keypair) -> Keypair {
    let nonce = Keypair::new();
    let lamports = svm.minimum_balance_for_rent_exemption(State::size());
    let ixs = system_instruction::create_nonce_account(
        &payer.pubkey(),
        &nonce.pubkey(),
        &payer.pubkey(),
        lamports,
    );
    let msg = Message::new(&ixs, Some(&payer.pubkey()));
    let tx = Transaction::new(&[payer, &nonce], msg, svm.latest_blockhash());
    svm.send_transaction(tx).expect("create nonce account");
    nonce
}

/// Read the nonce account's currently stored blockhash.
fn nonce_value(svm: &LiteSVM, nonce: &Pubkey) -> Hash {
    let acct = svm.get_account(nonce).expect("nonce account");
    let versions: Versions = bincode::deserialize(&acct.data).expect("nonce data");
    match versions.state() {
        State::Initialized(data) => data.blockhash(),
        State::Uninitialized => panic!("nonce not initialized"),
    }
}

fn fund(svm: &mut LiteSVM) -> Keypair {
    let kp = Keypair::new();
    svm.airdrop(&kp.pubkey(), 1_000_000_000).unwrap();
    kp
}

#[test]
fn presigned_durable_nonce_tx_broadcasts_later() {
    let mut svm = LiteSVM::new();
    let payer = fund(&mut svm);
    let nonce = create_nonce(&mut svm, &payer);

    // Sign the execution now (at "commit"), against the durable nonce.
    let recipient = Pubkey::new_unique();
    let action = system_instruction::transfer(&payer.pubkey(), &recipient, 2_000_000);
    let tx = build_durable_nonce_tx(
        &payer,
        &nonce.pubkey(),
        nonce_value(&svm, &nonce.pubkey()),
        &[action],
    );

    // The regular blockhash moves on; only the durable nonce keeps the tx valid.
    svm.expire_blockhash();

    // The keeper fires it later — it must still be valid.
    let mut keeper = Keeper::new();
    keeper.enqueue(tx);
    for t in keeper.drain() {
        svm.send_transaction(t)
            .expect("durable-nonce tx valid later");
    }

    let bal = svm.get_account(&recipient).map(|a| a.lamports).unwrap_or(0);
    assert_eq!(bal, 2_000_000, "the pre-signed execution should have run");
}

#[test]
fn advancing_the_nonce_opts_the_participant_out() {
    let mut svm = LiteSVM::new();
    let payer = fund(&mut svm);
    let nonce = create_nonce(&mut svm, &payer);

    let recipient = Pubkey::new_unique();
    let action = system_instruction::transfer(&payer.pubkey(), &recipient, 2_000_000);
    let presigned = build_durable_nonce_tx(
        &payer,
        &nonce.pubkey(),
        nonce_value(&svm, &nonce.pubkey()),
        &[action],
    );

    // Participant opts out by advancing their own nonce (a moved blockhash lets
    // the nonce advance to a fresh value).
    svm.expire_blockhash();
    let advance = system_instruction::advance_nonce_account(&nonce.pubkey(), &payer.pubkey());
    let msg = Message::new(&[advance], Some(&payer.pubkey()));
    let tx = Transaction::new(&[&payer], msg, svm.latest_blockhash());
    svm.send_transaction(tx).expect("advance nonce");

    // The pre-signed transaction now references a stale nonce and is rejected.
    assert!(
        svm.send_transaction(presigned).is_err(),
        "advancing the nonce must invalidate the pre-signed execution"
    );
    let bal = svm.get_account(&recipient).map(|a| a.lamports).unwrap_or(0);
    assert_eq!(bal, 0, "the opted-out execution must not have run");
}
