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

## 2026-11-27T10:51:47Z constant-judge proposal:bfac15e5c7726d5b01e50e4a7ce07560096ba5f6662575be00961d30a4618650

Hypothesis: across the 10 stored JailbreakBench replication cells, at most 3
and at least 1 have a paper-claimed ASR inside the bootstrap 95% CI of at
least one of an always-harmful, a never-harmful or a seeded coin-flip
(p=0.5) judge. The never-harmful judge is expected to match the claim of 0.0
for JailbreakChat on llama-2-7b-chat.

Metric: `/summary/cells_reproduced_by_floor_judge` in
`results/646-constant-judge.json`. Expected: 1 to 3 cells of 10.

Falsifier: zero cells reproduced by any floor judge, or more than 3 of 10.

Scope: model-free; judges never read a response; it says nothing about any
real model. Seed 7, 2000 bootstrap iterations over prompts, n_prompts as stored.
