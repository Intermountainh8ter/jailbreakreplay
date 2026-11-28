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

## 2026-11-28T02:34:05Z heterogeneity proposal:abdf90dbdcab7930444fa08350e9a1ab4d334057461479b5691acc5417194a44

Hypothesis: with 1000 synthetic prompts whose per-trial success probabilities
are Beta(c/2, c/2) (mean 0.5), single-shot ASR stays within 0.05 of 0.5 for
every concentration c in {0.5, 1, 2, 10, 100}, while the all-10-succeed rate
ranges over at least 0.20 between c=0.5 and c=100, so the k=1 to k=10 gap is
set by per-prompt heterogeneity, not by mean ASR.

Metric: `/k10_spread/max_minus_min` in `results/784-heterogeneity.json`.
Expected: increases to about 0.27, range 0.20 to 0.35.

Falsifier: a spread in all-10-succeed rate below 0.20 across the
concentrations, or single-shot ASR deviating more than 0.05 from 0.5 at any
concentration.

Scope: synthetic, model-free; it says nothing about any real model. Seed 7,
2000 bootstrap iterations over prompts, 10 trials per prompt.

## 2026-11-28T03:57:29Z cache-noise proposal:a8401b41e8702f7185bf528b668b89780ed979e42b9535dabe0afea0f531584d

Hypothesis: with 100 prompts at baseline non-refusal rate 0.5 and a per-prompt
symmetric flip probability of 0.388 between two serving arms (a published
cache-enabled divergence rate, used here only as a parameter), a paired exact
sign (McNemar-style) test at alpha 0.05 needs a true non-refusal difference of
at least 0.13 to reach 80% power, and at flip probability 0.05 the requirement
is at most 0.08.

Metric: `/summary/min_detectable_refusal_difference` in
`results/804-cache-noise.json` (flip 0.388, 80% power, 100 prompts).
Expected: about 0.17, range 0.13 to 0.22.

Falsifier: a minimum detectable difference below 0.13 at flip probability
0.388, or above 0.08 at flip probability 0.05.

Scope: synthetic, model-free; it says nothing about any real model or about
llama.cpp. Seed 7, 100 prompts, 2000 replications per grid point, difference
grid 0.00 to 0.50 in steps of 0.01 (first grid point reaching 80% power).
