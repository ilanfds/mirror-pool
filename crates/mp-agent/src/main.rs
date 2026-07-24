//! mirror-pool participant agent CLI.

use anyhow::Result;
use ark_std::rand::SeedableRng;
use clap::{Parser, Subcommand};
use mp_agent::{Keystore, Policy};
use mp_crypto::{field, Note};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "mp-agent", about = "mirror-pool participant agent")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate a new membership note keystore.
    Keygen {
        #[arg(long, default_value = "agent-note.json")]
        out: PathBuf,
    },
    /// Print the pool commitment (the deposit leaf) for a keystore.
    Commitment {
        #[arg(long)]
        keystore: PathBuf,
    },
    /// Print the default action policy as JSON.
    Policy,
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Keygen { out } => {
            let mut seed = [0u8; 32];
            getrandom::getrandom(&mut seed).expect("os entropy");
            let mut rng = ark_std::rand::rngs::StdRng::from_seed(seed);
            let note = Note::random(&mut rng);
            Keystore::from_note(&note).save(&out)?;
            println!("wrote keystore to {}", out.display());
            println!(
                "commitment: {}",
                hex::encode(field::to_bytes_be(&note.commitment()))
            );
        }
        Cmd::Commitment { keystore } => {
            let note = Keystore::load(&keystore)?.to_note()?;
            println!("{}", hex::encode(field::to_bytes_be(&note.commitment())));
        }
        Cmd::Policy => {
            println!("{}", serde_json::to_string_pretty(&Policy::default())?);
        }
    }
    Ok(())
}
