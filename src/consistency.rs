//! Model-free consistency study: single-shot ASR versus all-k-succeed on a
//! seeded stub target whose prompts have fixed per-trial success probabilities.

use crate::{asr, asr_bootstrap_ci, verdict, ReplicationVerdict, TrialOutcome};
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;
use serde::Serialize;

/// Stated mixture: (share of prompts, per-trial success probability).
pub const MIXTURE: [(f64, f64); 3] = [(0.3, 0.95), (0.4, 0.5), (0.3, 0.0)];

#[derive(Debug, Serialize, PartialEq)]
pub struct KRow {
    pub single_shot_asr: f64,
    pub all_succeed_rate: f64,
    pub gap: f64,
    pub ci_lo: f64,
    pub ci_hi: f64,
    pub verdict_vs_single_shot: ReplicationVerdict,
}

/// Per-prompt success probabilities, assigned deterministically by share.
pub fn prompt_probs(n: usize) -> Vec<f64> {
    let mut v = Vec::with_capacity(n);
    let mut acc = 0.0;
    for (share, p) in MIXTURE {
        acc += share;
        let upto = ((acc * n as f64).round() as usize).min(n);
        while v.len() < upto {
            v.push(p);
        }
    }
    while v.len() < n {
        v.push(MIXTURE[MIXTURE.len() - 1].1);
    }
    v
}

/// Draw `k_max` trials per prompt; returns `[prompt][trial]` success flags.
pub fn simulate(probs: &[f64], k_max: usize, seed: u64) -> Vec<Vec<bool>> {
    let mut rng = Pcg64Mcg::seed_from_u64(seed);
    probs
        .iter()
        .map(|&p| (0..k_max).map(|_| rng.gen::<f64>() < p).collect())
        .collect()
}

/// Rows for k = 1..=k_max from simulated trials.
pub fn study(trials: &[Vec<bool>], k_max: usize, boot: usize, seed: u64) -> Vec<KRow> {
    let to_outcome = |b: bool| if b { TrialOutcome::Success } else { TrialOutcome::Fail };
    let first: Vec<TrialOutcome> = trials.iter().map(|t| to_outcome(t[0])).collect();
    let single = asr(&first);
    (1..=k_max)
        .map(|k| {
            let all: Vec<TrialOutcome> = trials
                .iter()
                .map(|t| to_outcome(t[..k].iter().all(|&b| b)))
                .collect();
            let rate = asr(&all);
            let (lo, hi) = asr_bootstrap_ci(&all, boot, seed);
            KRow {
                single_shot_asr: single,
                all_succeed_rate: rate,
                gap: single - rate,
                ci_lo: lo,
                ci_hi: hi,
                verdict_vs_single_shot: verdict(single, (lo, hi)),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probs_follow_mixture() {
        let p = prompt_probs(100);
        assert_eq!(p.len(), 100);
        assert_eq!(p.iter().filter(|&&x| x == 0.95).count(), 30);
        assert_eq!(p.iter().filter(|&&x| x == 0.5).count(), 40);
        assert_eq!(p.iter().filter(|&&x| x == 0.0).count(), 30);
    }

    #[test]
    fn all_succeed_is_monotone_and_seeded() {
        let t = simulate(&prompt_probs(100), 10, 7);
        assert_eq!(t, simulate(&prompt_probs(100), 10, 7));
        let rows = study(&t, 10, 200, 7);
        assert_eq!(rows.len(), 10);
        for w in rows.windows(2) {
            assert!(w[1].all_succeed_rate <= w[0].all_succeed_rate);
        }
        assert!((rows[0].all_succeed_rate - rows[0].single_shot_asr).abs() < 1e-12);
        assert_eq!(rows[0].verdict_vs_single_shot, ReplicationVerdict::Reproduced);
    }

    #[test]
    fn zero_probability_prompts_never_all_succeed() {
        let t = simulate(&[0.0; 20], 5, 1);
        assert!(study(&t, 5, 50, 1).iter().all(|r| r.all_succeed_rate == 0.0));
    }
}
