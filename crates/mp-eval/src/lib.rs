//! mirror-pool adversarial evaluation (`docs/ROADMAP.md` Phase 8).
//!
//! Does the synchronized crowd *actually* defeat chain-analysis, or is it noise
//! that a real observer sees through? This harness answers with numbers: it
//! simulates rounds under two behaviors — mirror-pool (uniform, jittered,
//! synchronized) and naive copy-trading (initiator first, followers react) —
//! and runs the canonical timing deanonymizer against both.
//!
//! - [`scenario`] — synthetic round generation,
//! - [`adversary`] — attribution strategies,
//! - [`metrics`] — attribution accuracy vs the `1/N` random baseline.
//!
//! The result (see `docs/ADVERSARIAL.md`): the same heuristic that names the
//! initiator ~100% of the time under copy-trading drops to ~`1/N` — random —
//! under mirror-pool.

pub mod adversary;
pub mod metrics;
pub mod scenario;

pub use adversary::{Attributor, EarliestExecutor};
pub use metrics::attribution_accuracy;
pub use scenario::{Execution, Mode, Round, Scenario};
