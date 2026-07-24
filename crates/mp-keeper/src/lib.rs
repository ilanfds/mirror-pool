//! Trust-minimized execution synchronizer (`docs/ROADMAP.md` Phase 6, DESIGN
//! §6.5).
//!
//! Participants pre-sign their execution transaction **once** (at commit) against
//! a **durable nonce**, so it stays valid until *their own* nonce advances. A
//! keeper collects these pre-signed transactions and fires the whole crowd in
//! one tight window at execute time.
//!
//! The keeper is trust-minimized: it holds transactions the participant already
//! signed, so it **cannot alter or steal** — only broadcast or withhold. And it
//! never interacts with protocols itself; each transaction runs on the
//! participant's own wallet, so the crowd is many distinct wallets, not one.

use solana_hash::Hash;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;

/// Pre-sign a durable-nonce execution transaction.
///
/// `participant` is the fee payer and the nonce authority. `nonce_value` is the
/// nonce account's currently stored blockhash. The returned transaction remains
/// valid until the participant advances `nonce_account` — advancing it is the
/// participant's unilateral opt-out.
pub fn build_durable_nonce_tx(
    participant: &Keypair,
    nonce_account: &solana_pubkey::Pubkey,
    nonce_value: Hash,
    action: &[Instruction],
) -> Transaction {
    let message = Message::new_with_nonce(
        action.to_vec(),
        Some(&participant.pubkey()),
        nonce_account,
        &participant.pubkey(),
    );
    let mut tx = Transaction::new_unsigned(message);
    tx.sign(&[participant], nonce_value);
    tx
}

/// Collects pre-signed executions and fires them as one synchronized batch.
#[derive(Default)]
pub struct Keeper {
    queue: Vec<Transaction>,
}

impl Keeper {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a participant's pre-signed execution.
    pub fn enqueue(&mut self, tx: Transaction) {
        self.queue.push(tx);
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Take the queued transactions to broadcast (fire the crowd). Broadcasting
    /// itself — via RPC or into a test SVM — is the caller's job.
    pub fn drain(&mut self) -> Vec<Transaction> {
        std::mem::take(&mut self.queue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeper_batches_and_drains() {
        let mut k = Keeper::new();
        assert!(k.is_empty());
        // A trivial (unsigned) transaction stands in here; the durable-nonce
        // construction is exercised end to end in tests/durable_nonce.rs.
        k.enqueue(Transaction::default());
        k.enqueue(Transaction::default());
        assert_eq!(k.len(), 2);
        assert_eq!(k.drain().len(), 2);
        assert!(k.is_empty());
    }
}
