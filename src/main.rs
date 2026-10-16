//! jailbreakreplay CLI — scaffold.
//!
//! `score` reads a trials JSON produced by a future replay driver and
//! prints the ASR, 95% CI, and verdict against the paper's claim.

use clap::{Parser, Subcommand};
use jailbreakreplay::{
    asr, asr_bootstrap_ci, calibrate, grade, refusal_only_grader, verdict, Grader,
    LabeledResponse, TrialOutcome,
};

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
    /// Grade a JSON-Lines file of attack transcripts (one `{"transcript":
    /// "..."}` per line) with a scripted grader (pre-reg §3). Prints
    /// outcomes as JSONL to stdout.
    Grade {
        transcripts: std::path::PathBuf,
        /// Path to a JSON Grader file. Omit to use the built-in
        /// refusal-only grader (useful as a starting point only).
        #[arg(long)]
        grader: Option<std::path::PathBuf>,
    },
    /// Calibrate a grader against a hand-labeled fixture (pre-reg §6).
    /// Prints the Calibration result. Exits 2 if accuracy falls below
    /// the threshold so a pipeline cannot run a replication with an
    /// uncalibrated grader.
    Calibrate {
        fixture: std::path::PathBuf,
        #[arg(long)]
        grader: Option<std::path::PathBuf>,
        #[arg(long, default_value_t = 0.95)]
        threshold: f64,
    },
    /// Dump the built-in refusal-only grader as JSON (starting point
    /// for a paper-specific grader).
    DefaultGrader,
    /// Print the version banner and exit.
    Version,
}

fn load_grader(p: Option<std::path::PathBuf>) -> anyhow::Result<Grader> {
    match p {
        Some(path) => Ok(serde_json::from_str(&std::fs::read_to_string(&path)?)?),
        None => Ok(refusal_only_grader()),
    }
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
        Cmd::Grade {
            transcripts,
            grader,
        } => {
            let g = load_grader(grader)?;
            let text = std::fs::read_to_string(&transcripts)?;
            #[derive(serde::Deserialize)]
            struct Row {
                transcript: String,
            }
            for (i, line) in text.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let row: Row = serde_json::from_str(trimmed)
                    .map_err(|e| anyhow::anyhow!("line {}: {e}", i + 1))?;
                let outcome = grade(&row.transcript, &g);
                println!("{}", serde_json::to_string(&outcome)?);
            }
        }
        Cmd::Calibrate {
            fixture,
            grader,
            threshold,
        } => {
            let g = load_grader(grader)?;
            let labeled: Vec<LabeledResponse> =
                serde_json::from_str(&std::fs::read_to_string(&fixture)?)?;
            let c = calibrate(&g, &labeled, threshold);
            let passed = c.passed;
            println!("{}", serde_json::to_string_pretty(&c)?);
            if !passed {
                std::process::exit(2);
            }
        }
        Cmd::DefaultGrader => {
            println!(
                "{}",
                serde_json::to_string_pretty(&refusal_only_grader())?
            );
        }
        Cmd::Version => println!("jailbreakreplay {}", env!("CARGO_PKG_VERSION")),
    }
    Ok(())
}
