//! mirror-pool proposal relayer.
//!
//! The relayer's core — building the trust-minimized `propose` transaction — is
//! the [`mp_relayer`] library. RPC submission against a live cluster (the thin
//! wrapper that broadcasts the built transaction) is added with the cluster
//! tooling; see `docs/ROADMAP.md` Phase 6.

fn main() {
    println!("mp-relayer: use the `mp_relayer` library to build propose transactions.");
    println!("RPC submission against a cluster is not yet wired (ROADMAP phase 6).");
}
