## 2026-11-27T04:28:00Z consistency-stub proposal:e44613b9aafcaeea0de2077b4fa86d73c1edb0a776e2b77e7674d2178b81f868

Hypothesis: on a seeded stub target of 100 prompts (30 with per-trial success
probability 0.95, 40 with 0.5, 30 with 0.0), the all-10-succeed rate is at
least 0.20 below the single-shot ASR, and the verdict function against a claim
equal to the single-shot ASR flips from Reproduced (k=1) to NarrowedBelowClaim
(k=10).

Metric: `/k10/all_succeed_rate` in `results/608-consistency-stub.json`.
Expected direction: falls to about 0.18 (range 0.08 to 0.28), versus a
single-shot ASR of about 0.49.

Falsifier: the all-10-succeed rate within 0.20 of the single-shot ASR, or the
k=10 verdict still Reproduced against a claim equal to the single-shot ASR.

Scope: synthetic, model-free stub target; it says nothing about any real
model. Single-shot ASR is the first trial per prompt; all-k-succeed is the
fraction of prompts whose first k trials all succeed. Seed 7, 2000 bootstrap
iterations over prompts.
