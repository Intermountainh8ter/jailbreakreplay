//! jailbreakreplay CLI — scaffold.
//!
//! `score` reads a trials JSON produced by a future replay driver and
//! prints the ASR, 95% CI, and verdict against the paper's claim.

use clap::{Parser, Subcommand};
use jailbreakreplay::{asr, asr_bootstrap_ci, verdict, TrialOutcome};

#[derive(Parser)]
#[command(
    name = "jailbreakreplay",
    version,
    about = "Pre-registered jailbreak reproducibility"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Score a trials JSON file against a paper's claimed ASR.
    Score {
        trials: std::path::PathBuf,
        /// Paper's claimed attack-success rate (in [0, 1]).
        #[arg(long)]
        claim: f64,
        /// Bootstrap iterations.
        #[arg(long, default_value_t = 2000)]
        boot: usize,
        /// Bootstrap seed.
        #[arg(long, default_value_t = 42)]
        seed: u64,
    },
    /// Print the version banner and exit.
    Version,
}

#[derive(serde::Deserialize)]
struct Trials {
    outcomes: Vec<TrialOutcome>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Score {
            trials,
            claim,
            boot,
            seed,
        } => {
            let t: Trials = serde_json::from_str(&std::fs::read_to_string(&trials)?)?;
            let a = asr(&t.outcomes);
            let (lo, hi) = asr_bootstrap_ci(&t.outcomes, boot, seed);
            let v = verdict(claim, (lo, hi));
            println!("asr        = {a:.4}");
            println!("95%CI      = [{lo:.4}, {hi:.4}]  (boot={boot}, seed={seed})");
            println!("paper claim = {claim:.4}");
            println!("verdict    = {v:?}");
        }
        Cmd::Version => println!("jailbreakreplay {}", env!("CARGO_PKG_VERSION")),
    }
    Ok(())
}
