//! mirror-pool participant agent (`docs/ROADMAP.md` Phase 5).
//!
//! The agent is the software each member runs. This crate provides its
//! testable, cluster-independent core:
//!
//! - [`keystore`] — persistence for the secret membership note `(k, r)`,
//! - [`policy`] — the action policy that decides which rounds to join.
//!
//! The cluster-facing pieces (durable-nonce pre-signing, round watching, and
//! native-stake execution) build on top of these.

pub mod keystore;
pub mod policy;

pub use keystore::Keystore;
pub use policy::{Action, ActionKind, Policy};
