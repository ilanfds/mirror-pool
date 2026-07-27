//! The anonymity ruler CLI: advertised k vs the effective-k a funding-graph
//! adversary is left with.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use mp_trace::{Member, PoolSample, Score};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "mp-trace", about = "mirror-pool anonymity ruler")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Score a pool sampled as JSON: {"members":[{"funding_source":"..."}]}.
    Score {
        #[arg(long)]
        file: PathBuf,
    },
    /// Score a built-in example: an advertised k=30 pool whose funding is
    /// concentrated, so its effective anonymity is far lower.
    Demo,
}

fn report(sample: &PoolSample) {
    let s: Score = sample.score();
    println!("advertised k                       = {}", s.advertised_k);
    println!(
        "distinct funding sources           = {}",
        s.distinct_sources
    );
    println!(
        "effective-k (min-entropy)          = {:.2}",
        s.effective_k_min_entropy
    );
    println!(
        "effective-k (Shannon)              = {:.2}",
        s.effective_k_shannon
    );
    println!("self-fill floor (largest funder ⚔) = {}", s.self_fill_floor);
    println!();
    println!(
        "The advertised k counts members; the effective-k counts them after an\n\
         adversary groups by funding source. The floor is the cover that survives\n\
         if the single largest funder is hostile."
    );
}

/// A concentrated pool: 30 members, but most trace back to a few funders.
fn demo_pool() -> PoolSample {
    let mut members = Vec::new();
    // 14 funded from one exchange withdrawal — one correlated blob.
    for i in 0..14 {
        members.push(Member {
            wallet: format!("a{i}"),
            funding_source: "kraken-hot".into(),
        });
    }
    // 6 from a second.
    for i in 0..6 {
        members.push(Member {
            wallet: format!("b{i}"),
            funding_source: "coinbase-hot".into(),
        });
    }
    // 10 genuinely independent.
    for i in 0..10 {
        members.push(Member {
            wallet: format!("c{i}"),
            funding_source: format!("indep-{i}"),
        });
    }
    PoolSample { members }
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Score { file } => {
            let json = std::fs::read_to_string(&file)
                .with_context(|| format!("reading {}", file.display()))?;
            let sample: PoolSample = serde_json::from_str(&json).context("parsing pool JSON")?;
            report(&sample);
        }
        Cmd::Demo => {
            println!("(built-in example — advertised k = 30, funding concentrated)\n");
            report(&demo_pool());
        }
    }
    Ok(())
}
