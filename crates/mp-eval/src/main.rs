//! Run the adversarial evaluation and print a comparison report.

use mp_eval::{
    attribution_accuracy, longitudinal_earliest_accuracy, EarliestExecutor, Mode, Scenario,
};
use rand::{rngs::StdRng, SeedableRng};

fn main() {
    let crowd_size = 50;
    let rounds = 5000;
    let window = 100.0;
    let mut rng = StdRng::seed_from_u64(42);
    let baseline = 1.0 / crowd_size as f64;

    println!("mirror-pool — adversarial evaluation");
    println!("crowd size N = {crowd_size}, rounds = {rounds}, window = {window}");
    println!("random guess = {baseline:.4}  (1/N)\n");

    // Per-round timing adversary: name the earliest executor.
    println!("per-round adversary  = earliest-executor");
    println!("{:<14} {:>22}", "behavior", "attribution accuracy");
    println!("{:-<14} {:->22}", "", "");
    for mode in [Mode::Naive, Mode::MirrorPool] {
        let scenario = Scenario {
            crowd_size,
            window,
            mode,
            frequent_initiator: None,
        };
        let acc = attribution_accuracy(&scenario.simulate(rounds, &mut rng), &EarliestExecutor);
        println!("{:<14} {:>21.4}", format!("{mode:?}"), acc);
    }

    // Cross-round adversary against a persistent power initiator (wallet 0).
    println!("\ncross-round adversary = most-frequently-earliest (power initiator = wallet 0)");
    println!("{:<14} {:>22}", "behavior", "power-initiator hit rate");
    println!("{:-<14} {:->22}", "", "");
    for mode in [Mode::Naive, Mode::MirrorPool] {
        let scenario = Scenario {
            crowd_size,
            window,
            mode,
            frequent_initiator: Some(0),
        };
        let acc = longitudinal_earliest_accuracy(&scenario.simulate(rounds, &mut rng));
        println!("{:<14} {:>21.4}", format!("{mode:?}"), acc);
    }

    println!(
        "\nInterpretation: both a per-round and a cross-round timing adversary succeed against\n\
         naive copy-trading and collapse to ~random against mirror-pool's synchronized jitter."
    );
}
