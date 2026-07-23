//! Toolchain smoke test: load the BPF program into LiteSVM and invoke it. This
//! proves the whole test harness the ROADMAP phase 2 DoD relies on (build .so
//! -> load -> send transaction) before any real logic is added.
//!
//! The program must be built first (`anchor build`), producing
//! `target/deploy/mirror_pool.so`. If that artifact is absent (e.g. a CI job
//! without the Solana toolchain), the test skips instead of failing to compile,
//! so `cargo test --workspace` stays green everywhere.

use anchor_lang::Discriminator;
use litesvm::LiteSVM;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::Transaction;

const PROGRAM_SO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/deploy/mirror_pool.so"
);

fn program_id() -> Pubkey {
    Pubkey::new_from_array(mirror_pool::ID.to_bytes())
}

#[test]
fn initialize_invokes_successfully() {
    if !std::path::Path::new(PROGRAM_SO).exists() {
        eprintln!("skipping: {PROGRAM_SO} not built (run `anchor build`)");
        return;
    }

    let mut svm = LiteSVM::new();
    let prog = program_id();
    svm.add_program_from_file(prog, PROGRAM_SO)
        .expect("load program .so");

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();

    // `initialize` takes no accounts and no args: instruction data is just the
    // 8-byte Anchor discriminator.
    let data = mirror_pool::instruction::Initialize::DISCRIMINATOR.to_vec();
    let ix = Instruction {
        program_id: prog,
        accounts: vec![],
        data,
    };

    let msg = Message::new(&[ix], Some(&payer.pubkey()));
    let tx = Transaction::new(&[&payer], msg, svm.latest_blockhash());

    let result = svm.send_transaction(tx);
    assert!(result.is_ok(), "initialize should succeed: {result:?}");
}
