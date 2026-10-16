# jailbreakreplay

**Pre-registered reproducibility harness for published jailbreak attacks.**
Rerun the attack under its paper-specified target, chat template, and
sampling settings; report the replication's ASR with a 95% bootstrap CI
and a verdict (reproduced / narrowed below claim / stronger than claim /
diverged) against the paper's claimed number.

![status: initial scaffold](https://img.shields.io/badge/status-initial%20scaffold-lightgrey)
![language: Rust](https://img.shields.io/badge/language-Rust-blue)
![license: MIT](https://img.shields.io/badge/license-MIT-blue)

## Why

Jailbreak papers routinely claim 80–99% attack-success rates. Practitioners
trying to reproduce them often see 20–40%. The gap is widely known in
private slack channels and never written down. The replication problem
is the field's problem; a replication registry is one way to fix it.

Pairs with `tokenwitness` (verifies chat-template byte-identity between
paper and replication) and `refusalscope` (the ablation + manipulation
check methodology).

## What is in this scaffold

- `TrialOutcome` (`Success` / `Fail` / `Error`) and `asr` which excludes
  errors from the denominator.
- `asr_bootstrap_ci` — percentile bootstrap with a seeded Pcg64Mcg RNG.
- `verdict(claim, ci)` returning one of four pre-registered outcomes.
- `Grader` type (`refusal_patterns`, `success_patterns`, `case_sensitive`)
  and `grade(transcript, &grader) -> TrialOutcome`: pre-reg §3's scripted
  grader. No LLM-as-judge. Any refusal pattern match → `Fail`; all
  success patterns present → `Success`; empty transcript → `Error`.
- `refusal_only_grader()` built-in: 19 canonical safety-tuned refusal
  templates, no success structure. Starting point for paper-specific
  graders.
- `calibrate(&grader, &labeled_fixture, threshold)` → `Calibration`:
  pre-reg §6's self-test. A grader may not be used in a replication
  unless it meets `threshold` (default 0.95) accuracy on hand labels.
- CLI: `jailbreakreplay score`, `grade <transcripts.jsonl>`,
  `calibrate <fixture.json>`, `default-grader`. `calibrate` exits 2 on
  below-threshold accuracy so pipelines block an uncalibrated run.

Not in this commit (openly tracked):

- Replay drivers for PAIR, GCG, PAP.
- Fixtures per paper: exact prompts, template SHA, decoding settings,
  plus the paper-specific `Grader` and its hand-labeled calibration
  responses.
- `verify` subcommand that chains to `tokenwitness` to flag template
  drift.

## Usage (scaffold)

```sh
cargo install --path .
cat > trials.json <<'EOF'
{"outcomes": ["Success","Success","Fail","Success","Fail","Error"]}
EOF
jailbreakreplay score trials.json --claim 0.9 --boot 2000
```

## License

MIT. See `LICENSE`.
