//! Chain-analysis adversaries that try to name the round's initiator.

use crate::scenario::Round;

/// An attribution strategy: given a round, guess which participant initiated it.
pub trait Attributor {
    fn name(&self) -> &'static str;
    fn guess_initiator(&self, round: &Round) -> usize;
}

/// The canonical timing deanonymizer: the wallet that executed **first** is
/// assumed to be the initiator. This is exactly what a real observer does to a
/// leader/copy-trading pattern — and what mirror-pool's synchronized jitter is
/// designed to defeat.
pub struct EarliestExecutor;

impl Attributor for EarliestExecutor {
    fn name(&self) -> &'static str {
        "earliest-executor"
    }

    fn guess_initiator(&self, round: &Round) -> usize {
        round
            .executions
            .iter()
            .min_by(|a, b| {
                a.time
                    .partial_cmp(&b.time)
                    .expect("execution times are finite")
            })
            .expect("round has at least one execution")
            .participant
    }
}
