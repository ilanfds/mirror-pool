//! mirror-pool execution keeper.
//!
//! The keeper's core — durable-nonce pre-signing and batched firing — is the
//! [`mp_keeper`] library. Watching rounds over RPC and broadcasting the batch at
//! execute time against a live cluster is added with the cluster tooling; see
//! `docs/ROADMAP.md` Phase 6.

fn main() {
    println!("mp-keeper: use the `mp_keeper` library to pre-sign and batch executions.");
    println!("Live RPC round-watching and broadcast is not yet wired (ROADMAP phase 6).");
}
