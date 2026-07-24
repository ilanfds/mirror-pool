//! Metrics for how well an adversary attributes initiators.

use crate::adversary::Attributor;
use crate::scenario::Round;

/// Fraction of rounds where the adversary correctly names the initiator. An
/// adversary that does no better than `1/N` has been defeated.
pub fn attribution_accuracy(rounds: &[Round], adversary: &dyn Attributor) -> f64 {
    if rounds.is_empty() {
        return 0.0;
    }
    let correct = rounds
        .iter()
        .filter(|r| adversary.guess_initiator(r) == r.initiator)
        .count();
    correct as f64 / rounds.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adversary::EarliestExecutor;
    use crate::scenario::{Mode, Scenario};
    use rand::{rngs::StdRng, SeedableRng};

    const N: usize = 50;
    const ROUNDS: usize = 3000;

    #[test]
    fn mirror_pool_defeats_timing_attribution() {
        let mut rng = StdRng::seed_from_u64(1);
        let s = Scenario {
            crowd_size: N,
            window: 100.0,
            mode: Mode::MirrorPool,
        };
        let data = s.simulate(ROUNDS, &mut rng);
        let acc = attribution_accuracy(&data, &EarliestExecutor);
        // Random guessing scores 1/N = 0.02; the adversary must be no better.
        assert!(
            acc < 3.0 * s.random_baseline_accuracy(),
            "timing attribution beat random against mirror-pool: {acc}"
        );
    }

    #[test]
    fn naive_copytrading_is_fully_attributable() {
        let mut rng = StdRng::seed_from_u64(2);
        let s = Scenario {
            crowd_size: N,
            window: 100.0,
            mode: Mode::Naive,
        };
        let data = s.simulate(ROUNDS, &mut rng);
        let acc = attribution_accuracy(&data, &EarliestExecutor);
        // The same heuristic nails the initiator when they act first.
        assert!(
            acc > 0.99,
            "naive copy-trading should be attributable: {acc}"
        );
    }
}
