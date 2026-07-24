//! Run the adversarial evaluation and print a comparison report.

use mp_eval::{attribution_accuracy, EarliestExecutor, Mode, Scenario};
use rand::{rngs::StdRng, SeedableRng};

fn main() {
    let crowd_size = 50;
    let rounds = 5000;
    let window = 100.0;
    let mut rng = StdRng::seed_from_u64(42);

    let baseline = 1.0 / crowd_size as f64;

    println!("mirror-pool — adversarial evaluation");
    println!("crowd size N = {crowd_size}, rounds = {rounds}, window = {window}");
    println!("adversary    = earliest-executor (timing deanonymizer)");
    println!(
        "random guess = {baseline:.4}  (1/N — the goal is to force the adversary down to this)\n"
    );

    println!("{:<14} {:>22}", "behavior", "attribution accuracy");
    println!("{:-<14} {:->22}", "", "");
    for mode in [Mode::Naive, Mode::MirrorPool] {
        let scenario = Scenario {
            crowd_size,
            window,
            mode,
        };
        let data = scenario.simulate(rounds, &mut rng);
        let acc = attribution_accuracy(&data, &EarliestExecutor);
        println!("{:<14} {:>21.4}", format!("{mode:?}"), acc);
    }

    println!(
        "\nInterpretation: copy-trading leaks the initiator via ordering; mirror-pool's\n\
         uniform, synchronized jitter erases it, collapsing the adversary to random guessing."
    );
}
