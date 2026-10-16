//! jailbreakreplay CLI — scaffold.
//!
//! `score` reads a trials JSON produced by a future replay driver and
//! prints the ASR, 95% CI, and verdict against the paper's claim.

use clap::{Parser, Subcommand};
use jailbreakreplay::{
    asr, asr_bootstrap_ci, calibrate, grade,
    http::{prompt_sha256_hex, sample_many, HttpTarget},
    refusal_only_grader, verdict, Grader, LabeledResponse, Sampling, TrialOutcome,
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
    /// Replay a prompts fixture against a live chat-completions
    /// endpoint and emit a trials file. Only outcomes + prompt-SHA256
    /// are written out — raw completions are discarded in-process.
    /// Pre-reg §6: the grader must calibrate before this runs; `replay`
    /// loads the calibration fixture and refuses if the grader fails
    /// the threshold.
    Replay {
        /// JSON file describing the HTTP target.
        #[arg(long)]
        target: std::path::PathBuf,
        /// JSON file describing the pinned sampling parameters.
        #[arg(long)]
        sampling: std::path::PathBuf,
        /// Optional path to a `Grader` JSON. Omit for the built-in
        /// refusal-only grader.
        #[arg(long)]
        grader: Option<std::path::PathBuf>,
        /// Hand-labeled fixture (JSON array of `LabeledResponse`) used
        /// to gate the grader at `--threshold` accuracy.
        #[arg(long)]
        calibration: std::path::PathBuf,
        /// Pre-reg §6 calibration threshold.
        #[arg(long, default_value_t = 0.95)]
        threshold: f64,
        /// JSON-Lines file of prompts, one `{"prompt": "..."}` per line.
        #[arg(long)]
        prompts: std::path::PathBuf,
        /// Trials per prompt (seed advances per trial so a single prompt
        /// is sent `n` times).
        #[arg(long, default_value_t = 1)]
        n: usize,
        /// Destination trials JSON; `{"outcomes": [...], "rows":
        /// [{"prompt_sha256_hex": "...", "outcomes": [...]}]}` so a
        /// reviewer can map a row back to a prompt without the dataset
        /// being in-repo.
        #[arg(long)]
        out: std::path::PathBuf,
    },
    /// Print the version banner and exit.
    Version,
}

#[derive(serde::Deserialize)]
struct PromptRow {
    prompt: String,
}

#[derive(serde::Serialize)]
struct ReplayRow {
    prompt_sha256_hex: String,
    outcomes: Vec<TrialOutcome>,
}

#[derive(serde::Serialize)]
struct ReplayOut {
    model: String,
    outcomes: Vec<TrialOutcome>,
    rows: Vec<ReplayRow>,
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
        Cmd::Replay {
            target,
            sampling,
            grader,
            calibration,
            threshold,
            prompts,
            n,
            out,
        } => {
            let target: HttpTarget =
                serde_json::from_str(&std::fs::read_to_string(&target)?)?;
            let sampling: Sampling =
                serde_json::from_str(&std::fs::read_to_string(&sampling)?)?;
            let grader = load_grader(grader)?;
            let labeled: Vec<LabeledResponse> =
                serde_json::from_str(&std::fs::read_to_string(&calibration)?)?;
            let cal = calibrate(&grader, &labeled, threshold);
            if !cal.passed {
                eprintln!(
                    "{}",
                    serde_json::to_string_pretty(&cal)?
                );
                eprintln!(
                    "grader accuracy {:.4} < threshold {:.4}; pre-reg §6 blocks replay",
                    cal.accuracy, cal.threshold
                );
                std::process::exit(2);
            }
            let text = std::fs::read_to_string(&prompts)?;
            let mut rows: Vec<ReplayRow> = Vec::new();
            let mut all_outcomes: Vec<TrialOutcome> = Vec::new();
            for (i, line) in text.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let row: PromptRow = serde_json::from_str(trimmed)
                    .map_err(|e| anyhow::anyhow!("line {}: {e}", i + 1))?;
                let outcomes = sample_many(&target, &sampling, &grader, &row.prompt, n)
                    .map_err(|e| anyhow::anyhow!("prompt {}: {e}", i + 1))?;
                all_outcomes.extend(outcomes.iter().copied());
                rows.push(ReplayRow {
                    prompt_sha256_hex: prompt_sha256_hex(&row.prompt),
                    outcomes,
                });
            }
            let out_doc = ReplayOut {
                model: target.model.clone(),
                outcomes: all_outcomes,
                rows,
            };
            std::fs::write(&out, serde_json::to_string_pretty(&out_doc)?)?;
            eprintln!("wrote {} rows to {}", out_doc.rows.len(), out.display());
        }
        Cmd::Version => println!("jailbreakreplay {}", env!("CARGO_PKG_VERSION")),
    }
    Ok(())
}
