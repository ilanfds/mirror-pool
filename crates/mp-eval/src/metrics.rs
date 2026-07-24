//! Metrics for how well an adversary attributes initiators.

use crate::adversary::{Attributor, EarliestExecutor};
use crate::scenario::Round;
use std::collections::HashMap;

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

/// A **cross-round** adversary: find the wallet that is *most often* the earliest
/// executor and blame it for every round. Under naive copy-trading this extracts
/// a persistent power-initiator; under mirror-pool, earliness is uniform, so it
/// singles out no one. Returns the fraction of rounds it attributes correctly.
pub fn longitudinal_earliest_accuracy(rounds: &[Round]) -> f64 {
    if rounds.is_empty() {
        return 0.0;
    }
    let mut earliest_counts: HashMap<usize, usize> = HashMap::new();
    for r in rounds {
        *earliest_counts
            .entry(EarliestExecutor.guess_initiator(r))
            .or_default() += 1;
    }
    let suspect = earliest_counts
        .iter()
        .max_by_key(|(_, count)| **count)
        .map(|(wallet, _)| *wallet)
        .expect("non-empty rounds yield a suspect");
    let correct = rounds.iter().filter(|r| r.initiator == suspect).count();
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
            frequent_initiator: None,
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
            frequent_initiator: None,
        };
        let data = s.simulate(ROUNDS, &mut rng);
        let acc = attribution_accuracy(&data, &EarliestExecutor);
        // The same heuristic nails the initiator when they act first.
        assert!(
            acc > 0.99,
            "naive copy-trading should be attributable: {acc}"
        );
    }

    #[test]
    fn longitudinal_adversary_extracts_a_power_initiator_only_under_naive() {
        let whale = 3;

        // Naive: the power initiator is earliest in its rounds, so the cross-
        // round adversary singles it out and attributes well above random.
        let mut rng = StdRng::seed_from_u64(10);
        let naive = Scenario {
            crowd_size: 20,
            window: 100.0,
            mode: Mode::Naive,
            frequent_initiator: Some(whale),
        };
        let naive_acc = longitudinal_earliest_accuracy(&naive.simulate(4000, &mut rng));
        assert!(
            naive_acc > 0.4,
            "the power initiator should be extractable under naive: {naive_acc}"
        );

        // mirror-pool: uniform earliness hides the power initiator entirely.
        let mut rng = StdRng::seed_from_u64(11);
        let mp = Scenario {
            crowd_size: 20,
            window: 100.0,
            mode: Mode::MirrorPool,
            frequent_initiator: Some(whale),
        };
        let mp_acc = longitudinal_earliest_accuracy(&mp.simulate(4000, &mut rng));
        assert!(
            mp_acc < 0.15,
            "mirror-pool should hide the power initiator: {mp_acc}"
        );
    }
}
