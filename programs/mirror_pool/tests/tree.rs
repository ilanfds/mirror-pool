//! Cross-check the on-chain membership tree against the off-chain `mp-crypto`
//! implementation (ROADMAP phase 2 DoD).
//!
//! The core assertion: after the same commitments are inserted, the root the
//! program computes on-chain (Poseidon syscall, big-endian) equals the root
//! `mp-crypto` computes off-chain (`light-poseidon`), byte-for-byte. This
//! validates both the incremental algorithm and the endianness convention.
//!
//! The program must be built first (`anchor build`). If the artifact is absent,
//! the tests skip so `cargo test --workspace` stays green.

use anchor_lang::{AccountDeserialize, InstructionData};
use litesvm::LiteSVM;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::Transaction;

use mp_crypto::{field, IncrementalMerkleTree, Note};

const PROGRAM_SO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/deploy/mirror_pool.so"
);

fn program_id() -> Pubkey {
    Pubkey::new_from_array(mirror_pool::ID.to_bytes())
}

fn system_program_id() -> Pubkey {
    Pubkey::new_from_array([0u8; 32])
}

/// Boots LiteSVM with the program loaded, a funded payer, and the pool PDA.
/// Returns `None` (test skips) if the program artifact is not built.
fn setup() -> Option<(LiteSVM, Keypair, Pubkey)> {
    if !std::path::Path::new(PROGRAM_SO).exists() {
        eprintln!("skipping: {PROGRAM_SO} not built (run `anchor build`)");
        return None;
    }
    let mut svm = LiteSVM::new();
    svm.add_program_from_file(program_id(), PROGRAM_SO)
        .expect("load program .so");
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();
    let (pool, _) = Pubkey::find_program_address(&[b"pool"], &program_id());
    Some((svm, payer, pool))
}

fn send_ok(svm: &mut LiteSVM, payer: &Keypair, ix: Instruction) {
    let msg = Message::new(&[ix], Some(&payer.pubkey()));
    let tx = Transaction::new(&[payer], msg, svm.latest_blockhash());
    let res = svm.send_transaction(tx);
    assert!(res.is_ok(), "transaction failed: {res:?}");
}

fn initialize_ix(payer: &Pubkey, pool: &Pubkey) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(*pool, false),
            AccountMeta::new(*payer, true),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: mirror_pool::instruction::InitializePool {}.data(),
    }
}

fn deposit_ix(pool: &Pubkey, commitment: [u8; 32]) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![AccountMeta::new(*pool, false)],
        data: mirror_pool::instruction::Deposit { commitment }.data(),
    }
}

fn read_pool(svm: &LiteSVM, pool: &Pubkey) -> mirror_pool::Pool {
    let acct = svm.get_account(pool).expect("pool account exists");
    let mut data = acct.data.as_slice();
    mirror_pool::Pool::try_deserialize(&mut data).expect("deserialize Pool")
}

/// Deterministic note for leaf `i` (no RNG needed).
fn note(i: u64) -> Note {
    Note::new(field::from_u64(1000 + i), field::from_u64(5_000_000 + i))
}

#[test]
fn empty_root_matches_mp_crypto() {
    let Some((mut svm, payer, pool)) = setup() else {
        return;
    };
    send_ok(&mut svm, &payer, initialize_ix(&payer.pubkey(), &pool));

    let onchain = read_pool(&svm, &pool);
    let offchain = IncrementalMerkleTree::new(mirror_pool::HEIGHT);

    assert_eq!(onchain.next_index, 0);
    assert_eq!(
        onchain.root,
        field::to_bytes_be(&offchain.root()),
        "empty-tree root mismatch (check zeros computation / endianness)"
    );
}

#[test]
fn deposits_match_mp_crypto_root() {
    let Some((mut svm, payer, pool)) = setup() else {
        return;
    };
    send_ok(&mut svm, &payer, initialize_ix(&payer.pubkey(), &pool));

    let mut offchain = IncrementalMerkleTree::new(mirror_pool::HEIGHT);

    for i in 0..17u64 {
        let c = note(i).commitment();
        send_ok(&mut svm, &payer, deposit_ix(&pool, field::to_bytes_be(&c)));
        offchain.insert(c).unwrap();

        let onchain = read_pool(&svm, &pool);
        assert_eq!(
            onchain.next_index,
            i + 1,
            "next_index after {} deposits",
            i + 1
        );
        assert_eq!(
            onchain.root,
            field::to_bytes_be(&offchain.root()),
            "root mismatch after {} deposits",
            i + 1
        );
    }
}

#[test]
fn deposit_records_root_in_history() {
    let Some((mut svm, payer, pool)) = setup() else {
        return;
    };
    send_ok(&mut svm, &payer, initialize_ix(&payer.pubkey(), &pool));

    let c = note(0).commitment();
    send_ok(&mut svm, &payer, deposit_ix(&pool, field::to_bytes_be(&c)));

    let onchain = read_pool(&svm, &pool);
    // The newest history entry is the current root.
    assert_eq!(onchain.current_root_index, 1);
    assert_eq!(onchain.roots[1], onchain.root);
}
