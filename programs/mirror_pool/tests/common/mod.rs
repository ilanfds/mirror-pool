//! Shared LiteSVM test helpers: PDAs, instruction builders, and — since
//! `propose` now verifies a real Groth16 proof — a bootstrap that deposits a
//! membership note and produces a valid proposal for it (off-chain proving via
//! `mp-proof`, against the same dev key embedded in the program).

#![allow(dead_code)]

use anchor_lang::{AccountDeserialize, InstructionData};
use ark_std::rand::{rngs::StdRng, SeedableRng};
use litesvm::LiteSVM;
use mp_crypto::{field, IncrementalMerkleTree, Note};
use mp_proof::circuit::ProposeCircuit;
use mp_proof::proving::{prove, setup};
use mp_proof::solana::proof_to_bytes;
use mp_proof::{DEV_SETUP_SEED, TREE_HEIGHT};
use solana_compute_budget_interface::ComputeBudgetInstruction;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_pubkey::Pubkey;
pub use solana_signer::Signer;
use solana_transaction::Transaction;

pub const PROGRAM_SO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/deploy/mirror_pool.so"
);

pub fn program_id() -> Pubkey {
    Pubkey::new_from_array(mirror_pool::ID.to_bytes())
}

pub fn system_program_id() -> Pubkey {
    Pubkey::new_from_array([0u8; 32])
}

pub fn load() -> Option<LiteSVM> {
    if !std::path::Path::new(PROGRAM_SO).exists() {
        eprintln!("skipping: {PROGRAM_SO} not built (run `anchor build`)");
        return None;
    }
    let mut svm = LiteSVM::new();
    svm.add_program_from_file(program_id(), PROGRAM_SO)
        .expect("load program .so");
    Some(svm)
}

pub fn fund(svm: &mut LiteSVM) -> Keypair {
    let kp = Keypair::new();
    svm.airdrop(&kp.pubkey(), 5_000_000_000).unwrap();
    kp
}

pub fn pool_pda() -> Pubkey {
    Pubkey::find_program_address(&[b"pool"], &program_id()).0
}

pub fn round_pda(round_id: u64) -> Pubkey {
    Pubkey::find_program_address(&[b"round", round_id.to_le_bytes().as_ref()], &program_id()).0
}

pub fn nullifier_pda(round_id: u64, nf: &[u8; 32]) -> Pubkey {
    Pubkey::find_program_address(
        &[b"nullifier", round_id.to_le_bytes().as_ref(), nf.as_ref()],
        &program_id(),
    )
    .0
}

pub fn read_round(svm: &LiteSVM, round_id: u64) -> mirror_pool::Round {
    let acct = svm.get_account(&round_pda(round_id)).expect("round exists");
    let mut d = acct.data.as_slice();
    mirror_pool::Round::try_deserialize(&mut d).expect("deserialize Round")
}

/// Send one or more instructions as a single transaction; returns success.
pub fn send(svm: &mut LiteSVM, payer: &Keypair, ixs: &[Instruction]) -> bool {
    let msg = Message::new(ixs, Some(&payer.pubkey()));
    let tx = Transaction::new(&[payer], msg, svm.latest_blockhash());
    svm.send_transaction(tx).is_ok()
}

pub fn init_pool(svm: &mut LiteSVM, payer: &Keypair) -> bool {
    let ix = Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(pool_pda(), false),
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: mirror_pool::instruction::InitializePool {}.data(),
    };
    send(svm, payer, &[ix])
}

pub fn deposit(svm: &mut LiteSVM, payer: &Keypair, commitment: [u8; 32]) -> bool {
    let ix = Instruction {
        program_id: program_id(),
        accounts: vec![AccountMeta::new(pool_pda(), false)],
        data: mirror_pool::instruction::Deposit { commitment }.data(),
    };
    send(svm, payer, &[ix])
}

pub fn open_ix(payer: &Pubkey, round_id: u64, threshold: u64) -> Instruction {
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

pub fn commit_ix(round_id: u64) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![AccountMeta::new(round_pda(round_id), false)],
        data: mirror_pool::instruction::Commit { round_id }.data(),
    }
}

pub fn advance_ix(round_id: u64) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![AccountMeta::new(round_pda(round_id), false)],
        data: mirror_pool::instruction::AdvanceRound { round_id }.data(),
    }
}

/// A valid proposal (public inputs + proof) for a bootstrapped pool.
#[derive(Clone)]
pub struct Proposal {
    pub round_id: u64,
    pub nullifier: [u8; 32],
    pub action: [u8; 32],
    pub root: [u8; 32],
    pub proof_a: [u8; 64],
    pub proof_b: [u8; 128],
    pub proof_c: [u8; 64],
}

/// Deposit one membership note on-chain and build a valid proposal for
/// `round_id`/`action_val` against the resulting single-leaf root. The off-chain
/// tree mirrors the on-chain one, so the proof's root matches `pool.roots`.
pub fn deposit_and_prove(
    svm: &mut LiteSVM,
    payer: &Keypair,
    round_id: u64,
    action_val: u64,
) -> Proposal {
    let note = Note::new(field::from_u64(1), field::from_u64(2));
    let commitment = note.commitment();
    assert!(
        deposit(svm, payer, field::to_bytes_be(&commitment)),
        "deposit failed"
    );

    let mut tree = IncrementalMerkleTree::new(TREE_HEIGHT);
    let idx = tree.insert(commitment).unwrap();
    let root = tree.root();
    let path = tree.opening(idx).unwrap();

    let round_id_fr = field::from_u64(round_id);
    let action_fr = field::from_u64(action_val);
    let circuit = ProposeCircuit::from_witness(&note, &path, root, round_id_fr, action_fr);

    let mut rng = StdRng::seed_from_u64(DEV_SETUP_SEED);
    let (pk, _vk) = setup(TREE_HEIGHT, &mut rng);
    let proof = proof_to_bytes(&prove(&pk, circuit, &mut rng));

    Proposal {
        round_id,
        nullifier: field::to_bytes_be(&note.nullifier(round_id_fr)),
        action: field::to_bytes_be(&action_fr),
        root: field::to_bytes_be(&root),
        proof_a: proof.a,
        proof_b: proof.b,
        proof_c: proof.c,
    }
}

pub fn propose_ix(payer: &Pubkey, p: &Proposal) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(pool_pda(), false),
            AccountMeta::new(round_pda(p.round_id), false),
            AccountMeta::new(nullifier_pda(p.round_id, &p.nullifier), false),
            AccountMeta::new(*payer, true),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: mirror_pool::instruction::Propose {
            round_id: p.round_id,
            nullifier: p.nullifier,
            action: p.action,
            root: p.root,
            proof_a: p.proof_a,
            proof_b: p.proof_b,
            proof_c: p.proof_c,
        }
        .data(),
    }
}

/// Send a proposal with a raised compute-unit limit (Groth16 verification is
/// well above the 200k default).
pub fn send_propose(svm: &mut LiteSVM, payer: &Keypair, p: &Proposal) -> bool {
    send(
        svm,
        payer,
        &[
            ComputeBudgetInstruction::set_compute_unit_limit(1_400_000),
            propose_ix(&payer.pubkey(), p),
        ],
    )
}
