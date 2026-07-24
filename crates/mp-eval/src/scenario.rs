//! Synthetic behavioral scenarios: rounds where a crowd performs the same action
//! and exactly one participant is the (hidden) initiator the adversary hunts.

use rand::Rng;

/// How a round's execution times are generated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// mirror-pool: **every** participant — the initiator included — executes at
    /// an i.i.d. uniform time inside the window. The synchronized, jittered
    /// crowd leaves no ordering signal.
    MirrorPool,
    /// Copy-trading baseline: the initiator acts first and followers react
    /// strictly later, so the initiator is always the earliest executor.
    Naive,
}

/// One wallet's execution within a round.
#[derive(Clone, Copy, Debug)]
pub struct Execution {
    pub participant: usize,
    pub time: f64,
}

/// A single round: the true initiator plus everyone's executions.
#[derive(Clone, Debug)]
pub struct Round {
    pub initiator: usize,
    pub executions: Vec<Execution>,
}

/// Parameters of a scenario.
#[derive(Clone, Copy, Debug)]
pub struct Scenario {
    pub crowd_size: usize,
    pub window: f64,
    pub mode: Mode,
}

impl Scenario {
    /// Simulate one round.
    pub fn simulate_round<R: Rng>(&self, rng: &mut R) -> Round {
        let n = self.crowd_size;
        let initiator = rng.gen_range(0..n);
        let mut executions = Vec::with_capacity(n);

        match self.mode {
            Mode::MirrorPool => {
                for participant in 0..n {
                    executions.push(Execution {
                        participant,
                        time: rng.gen_range(0.0..self.window),
                    });
                }
            }
            Mode::Naive => {
                // The initiator acts in the first tenth of the window; every
                // follower reacts strictly after them.
                let init_time = rng.gen_range(0.0..self.window * 0.1);
                for participant in 0..n {
                    let time = if participant == initiator {
                        init_time
                    } else {
                        init_time + rng.gen_range(0.01..self.window)
                    };
                    executions.push(Execution { participant, time });
                }
            }
        }

        Round {
            initiator,
            executions,
        }
    }

    /// Simulate many rounds.
    pub fn simulate<R: Rng>(&self, rounds: usize, rng: &mut R) -> Vec<Round> {
        (0..rounds).map(|_| self.simulate_round(rng)).collect()
    }

    /// The accuracy a blind random guesser achieves: `1/N`.
    pub fn random_baseline_accuracy(&self) -> f64 {
        1.0 / self.crowd_size as f64
    }
}
