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
- `jailbreakreplay score` CLI.

Not in this commit (openly tracked):

- Replay drivers for PAIR, GCG, PAP.
- Trial grader (deterministic refusal / comply / evasive classifier
  with scripted keyword plus structure checks).
- Fixtures per paper: exact prompts, template SHA, decoding settings.
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
