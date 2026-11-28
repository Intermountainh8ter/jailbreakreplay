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
    /// Model-free serving-noise power study: smallest detectable non-refusal difference between two arms per flip rate.
    CacheNoise {
        #[arg(long, default_value_t = 100)]
        prompts: usize,
        #[arg(long, default_value_t = 2000)]
        reps: usize,
        #[arg(long, default_value_t = 7)]
        seed: u64,
        #[arg(long)]
        out: std::path::PathBuf,
    },
    /// Model-free heterogeneity study: all-10-succeed rate versus Beta concentration at fixed mean ASR 0.5.
    Heterogeneity {
        #[arg(long, default_value_t = 1000)]
        prompts: usize,
        #[arg(long, default_value_t = 2000)]
        boot: usize,
        #[arg(long, default_value_t = 7)]
        seed: u64,
        #[arg(long)]
        out: std::path::PathBuf,
    },
    /// Floor study: re-score stored replication cells under judges that never read the response.
    ConstantJudge {
        #[arg(long)]
        verdicts: std::path::PathBuf,
        #[arg(long, default_value_t = 2000)]
        boot: usize,
        #[arg(long, default_value_t = 7)]
        seed: u64,
        #[arg(long)]
        out: std::path::PathBuf,
    },
    /// Grader-agreement study (docs/STUDY-grader-agreement.md): v1, v2 and a Llama Guard classifier on the same stored completions.
    GraderAgreement {
        #[arg(long)]
        prompts: std::path::PathBuf,
        /// Transcript JSONL files, one per cell, in the order to report.
        #[arg(long, num_args = 1..)]
        transcripts: Vec<std::path::PathBuf>,
        #[arg(long)]
        grader_v1: std::path::PathBuf,
        #[arg(long)]
        grader_v2: std::path::PathBuf,
        #[arg(long)]
        classifier_url: String,
        #[arg(long)]
        classifier_model: String,
        #[arg(long)]
        classifier_sha256: Option<String>,
        #[arg(long, default_value_t = 2000)]
        bootstrap: usize,
        #[arg(long, default_value_t = 42)]
        seed: u64,
        #[arg(long)]
        out: std::path::PathBuf,
        #[arg(long)]
        readme: Option<std::path::PathBuf>,
    },
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
    /// Compare two grader result sets on the same prompt lists: for every `<model>-<list>.json`
    /// present in both directories, print non-refusal under each grader and the difference.
    /// Writes a Markdown table to stdout and, with `--readme`, splices it between
    /// `<!-- GRADER-V2:START -->` and `<!-- GRADER-V2:END -->`.
    CompareGraders {
        #[arg(long)]
        a: std::path::PathBuf,
        #[arg(long)]
        b: std::path::PathBuf,
        #[arg(long, default_value = "grader v1")]
        a_label: String,
        #[arg(long, default_value = "grader v2")]
        b_label: String,
        #[arg(long)]
        readme: Option<std::path::PathBuf>,
    },
    /// Model-free consistency study on a seeded stub target: single-shot ASR vs all-k-succeed.
    Consistency {
        #[arg(long, default_value_t = 100)]
        prompts: usize,
        #[arg(long, default_value_t = 10)]
        k_max: usize,
        #[arg(long, default_value_t = 2000)]
        boot: usize,
        #[arg(long, default_value_t = 7)]
        seed: u64,
        #[arg(long)]
        out: std::path::PathBuf,
    },
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

fn non_refusal_of(path: &std::path::Path) -> anyhow::Result<(usize, usize)> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let outcomes = v["outcomes"].as_array().ok_or_else(|| anyhow::anyhow!("{}: no outcomes", path.display()))?;
    let k = outcomes.iter().filter(|o| o.as_str() == Some("Success")).count();
    Ok((k, outcomes.len()))
}

fn compare_graders(a: &std::path::Path, b: &std::path::Path, a_label: &str, b_label: &str, readme: Option<&std::path::Path>) -> anyhow::Result<()> {
    let mut names: Vec<String> = std::fs::read_dir(b)?.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".json") && a.join(n).exists()).collect();
    names.sort();
    let mut t = format!("| Model | List | {a_label} non-refusal | {b_label} non-refusal | Difference (pp) |\n|---|---|---|---|---|\n");
    for n in &names {
        let (ka, na) = non_refusal_of(&a.join(n))?;
        let (kb, nb) = non_refusal_of(&b.join(n))?;
        let stem = n.trim_end_matches(".json");
        let (model, list) = ["-jailbreakbench", "-authored-100", "-harmbench", "-advbench"].iter().find_map(|suf| stem.strip_suffix(suf).map(|m| (m.to_string(), suf.trim_start_matches('-').to_string()))).unwrap_or((stem.to_string(), String::new()));
        let (ra, rb) = (ka as f64 / na.max(1) as f64, kb as f64 / nb.max(1) as f64);
        t.push_str(&format!("| {model} | {list} | {ka}/{na} ({:.1}%) | {kb}/{nb} ({:.1}%) | {:+.1} |\n", ra * 100.0, rb * 100.0, (rb - ra) * 100.0));
    }
    print!("{t}");
    if let Some(r) = readme {
        const START: &str = "<!-- GRADER-V2:START -->"; const END: &str = "<!-- GRADER-V2:END -->";
        let text = std::fs::read_to_string(r)?;
        let (s, e) = (text.find(START).ok_or_else(|| anyhow::anyhow!("README missing {START}"))?, text.find(END).ok_or_else(|| anyhow::anyhow!("README missing {END}"))?);
        std::fs::write(r, format!("{}{}\n{}{}", &text[..s], START, t, &text[e..]))?;
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::ConstantJudge { verdicts, boot, seed, out } => {
            let started = std::time::Instant::now();
            let started_at = chrono_free_utc();
            let stored: jailbreakreplay::constant_judge::Stored = serde_json::from_str(&std::fs::read_to_string(&verdicts)?)?;
            let (rows, summary) = jailbreakreplay::constant_judge::run_cells(&stored.cells, boot, seed);
            let doc = serde_json::json!({
                "summary": summary,
                "cells": rows,
                "provenance": {
                    "started_at": started_at,
                    "seeds": [seed],
                    "command": format!("jailbreakreplay constant-judge --verdicts {} --boot {boot} --seed {seed} --out {}", verdicts.display(), out.display()),
                    "hardware": "cpu only, model-free",
                    "wall_s": started.elapsed().as_secs_f64(),
                    "data_sources": [
                        {"url": "https://github.com/JailbreakBench/artifacts", "license": "MIT"},
                        {"url": "results/local-qwen3/jbb-replication-verdicts.json", "license": "MIT (in-crate)"}
                    ]
                }
            });
            if let Some(d) = out.parent() { std::fs::create_dir_all(d)?; }
            std::fs::write(&out, serde_json::to_string_pretty(&doc)?)?;
            println!("{}", serde_json::to_string(&doc["summary"])?);
        }
        Cmd::GraderAgreement { prompts, transcripts, grader_v1, grader_v2, classifier_url, classifier_model, classifier_sha256, bootstrap, seed, out, readme } => {
            let study = jailbreakreplay::agreement::run(&jailbreakreplay::agreement::Inputs { prompts: &prompts, transcripts: &transcripts, grader_v1: &grader_v1, grader_v2: &grader_v2, classifier_url: &classifier_url, classifier_model: &classifier_model, classifier_sha256, boot_iters: bootstrap, seed })?;
            if let Some(d) = out.parent() { std::fs::create_dir_all(d)?; }
            std::fs::write(&out, serde_json::to_string_pretty(&study)?)?;
            let block = jailbreakreplay::agreement::render(&study);
            if let Some(r) = readme {
                let text = std::fs::read_to_string(&r)?;
                let (s, e) = ("<!-- GRADER-AGREEMENT:START -->", "<!-- GRADER-AGREEMENT:END -->");
                let (a, b) = (text.find(s).ok_or_else(|| anyhow::anyhow!("README lacks {s}"))?, text.find(e).ok_or_else(|| anyhow::anyhow!("README lacks {e}"))?);
                std::fs::write(&r, format!("{}{}\n{}\n{}", &text[..a], s, block, &text[b..]))?;
            }
            print!("{block}");
        }
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
        Cmd::CompareGraders { a, b, a_label, b_label, readme } => compare_graders(&a, &b, &a_label, &b_label, readme.as_deref())?,
        Cmd::Consistency { prompts, k_max, boot, seed, out } => {
            use jailbreakreplay::consistency::{prompt_probs, simulate, study, MIXTURE};
            anyhow::ensure!(prompts > 0 && k_max > 0, "prompts and k-max must be positive");
            let started = std::time::Instant::now();
            let started_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();
            let rows = study(&simulate(&prompt_probs(prompts), k_max, seed), k_max, boot, seed);
            let mut doc = serde_json::Map::new();
            for (i, r) in rows.iter().enumerate() {
                doc.insert(format!("k{}", i + 1), serde_json::to_value(r)?);
            }
            doc.insert("provenance".into(), serde_json::json!({
                "started_at": started_iso(started_at),
                "seeds": [seed],
                "command": format!("jailbreakreplay consistency --prompts {prompts} --k-max {k_max} --boot {boot} --seed {seed}"),
                "hardware": "cpu",
                "wall_s": started.elapsed().as_secs_f64(),
                "mixture": MIXTURE.iter().map(|(s, p)| serde_json::json!({"share": s, "p_success": p})).collect::<Vec<_>>(),
                "data_sources": [{"url": "synthetic: seeded in-crate stub target, no external data", "license": "MIT (in-crate)"}],
            }));
            std::fs::write(&out, serde_json::to_string_pretty(&doc)?)?;
            eprintln!("wrote {}", out.display());
        }
        Cmd::Heterogeneity { prompts, boot, seed, out } => {
            use jailbreakreplay::heterogeneity::{study, CONCENTRATIONS, K};
            anyhow::ensure!(prompts > 0, "prompts must be positive");
            let started = std::time::Instant::now();
            let started_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();
            let rows = study(prompts, boot, seed);
            let k10: Vec<f64> = rows.iter().map(|r| r.all10_rate).collect();
            let max = k10.iter().cloned().fold(f64::MIN, f64::max);
            let min = k10.iter().cloned().fold(f64::MAX, f64::min);
            let dev = rows.iter().map(|r| (r.single_shot_asr - 0.5).abs()).fold(0.0, f64::max);
            let mut doc = serde_json::Map::new();
            doc.insert("rows".into(), serde_json::to_value(&rows)?);
            doc.insert("k10_spread".into(), serde_json::json!({
                "max": max, "min": min, "max_minus_min": max - min,
                "max_single_shot_deviation_from_half": dev,
            }));
            doc.insert("provenance".into(), serde_json::json!({
                "started_at": started_iso(started_at),
                "seeds": [seed],
                "command": format!("jailbreakreplay heterogeneity --seed {seed} --prompts {prompts} --boot {boot}"),
                "hardware": "cpu",
                "wall_s": started.elapsed().as_secs_f64(),
                "concentrations": CONCENTRATIONS, "trials_per_prompt": K,
                "data_sources": [{"url": "synthetic: seeded in-crate Beta(c/2,c/2) stub target, no external data", "license": "MIT (in-crate)"}],
            }));
            std::fs::write(&out, serde_json::to_string_pretty(&doc)?)?;
            eprintln!("wrote {}", out.display());
        }
        Cmd::CacheNoise { prompts, reps, seed, out } => {
            use jailbreakreplay::cache_noise::{study, ALPHA, BASE_RATE, FLIP_RATES, POWER_TARGET};
            anyhow::ensure!(prompts > 0 && reps > 0, "prompts and reps must be positive");
            let started = std::time::Instant::now();
            let started_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();
            let rows = study(prompts, reps, seed);
            let at = |f: f64| rows.iter().find(|r| r.flip_probability == f).and_then(|r| r.min_detectable_difference);
            let mut doc = serde_json::Map::new();
            doc.insert("rows".into(), serde_json::to_value(&rows)?);
            doc.insert("summary".into(), serde_json::json!({
                "min_detectable_refusal_difference": at(0.388),
                "min_detectable_difference_at_flip_0_05": at(0.05),
                "min_detectable_difference_at_flip_0": at(0.0),
            }));
            doc.insert("provenance".into(), serde_json::json!({
                "started_at": started_iso(started_at),
                "seeds": [seed],
                "command": format!("jailbreakreplay cache-noise --seed {seed} --prompts {prompts} --reps {reps}"),
                "hardware": "cpu",
                "wall_s": started.elapsed().as_secs_f64(),
                "flip_rates": FLIP_RATES, "alpha": ALPHA, "power_target": POWER_TARGET, "base_rate": BASE_RATE,
                "data_sources": [{"url": "synthetic: seeded in-crate two-arm flip simulation, no external data", "license": "MIT (in-crate)"}],
            }));
            std::fs::write(&out, serde_json::to_string_pretty(&doc)?)?;
            eprintln!("wrote {}", out.display());
        }
        Cmd::Version => println!("jailbreakreplay {}", env!("CARGO_PKG_VERSION")),
    }
    Ok(())
}

/// UTC ISO-8601 from unix seconds (civil-from-days).
fn started_iso(secs: u64) -> String {
    let (days, rem) = ((secs / 86400) as i64, secs % 86400);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// UTC ISO timestamp from the system clock without a new dependency.
fn chrono_free_utc() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}
