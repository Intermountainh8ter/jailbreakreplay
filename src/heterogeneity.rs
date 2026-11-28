//! Model-free heterogeneity study: mean single-shot ASR fixed at 0.5 while the
//! concentration of Beta(c/2, c/2) per-prompt success probabilities varies.

use crate::{asr, asr_bootstrap_ci, TrialOutcome};
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;
use serde::Serialize;

pub const CONCENTRATIONS: [f64; 5] = [0.5, 1.0, 2.0, 10.0, 100.0];
pub const K: usize = 10;

#[derive(Debug, Serialize, PartialEq)]
pub struct ConcRow {
    pub concentration: f64,
    pub single_shot_asr: f64,
    pub single_shot_ci_lo: f64,
    pub single_shot_ci_hi: f64,
    pub all10_rate: f64,
    pub all10_ci_lo: f64,
    pub all10_ci_hi: f64,
    pub gap: f64,
}

fn std_normal(rng: &mut Pcg64Mcg) -> f64 {
    let u1: f64 = 1.0 - rng.gen::<f64>();
    let u2: f64 = rng.gen();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// Gamma(shape, 1) by Marsaglia–Tsang, with the shape<1 boost.
pub fn gamma(shape: f64, rng: &mut Pcg64Mcg) -> f64 {
    if shape < 1.0 {
        let u: f64 = 1.0 - rng.gen::<f64>();
        return gamma(shape + 1.0, rng) * u.powf(1.0 / shape);
    }
    let d = shape - 1.0 / 3.0;
    let c = 1.0 / (9.0 * d).sqrt();
    loop {
        let x = std_normal(rng);
        let v = (1.0 + c * x).powi(3);
        if v <= 0.0 {
            continue;
        }
        let u: f64 = 1.0 - rng.gen::<f64>();
        if u.ln() < 0.5 * x * x + d - d * v + d * (v.ln()) {
            return d * v;
        }
    }
}

pub fn beta(a: f64, b: f64, rng: &mut Pcg64Mcg) -> f64 {
    let x = gamma(a, rng);
    let y = gamma(b, rng);
    x / (x + y)
}

/// Per-prompt trial flags `[prompt][trial]` for Beta(c/2, c/2) probabilities.
pub fn simulate(c: f64, prompts: usize, k: usize, seed: u64) -> Vec<Vec<bool>> {
    let mut rng = Pcg64Mcg::seed_from_u64(seed);
    (0..prompts)
        .map(|_| {
            let p = beta(c / 2.0, c / 2.0, &mut rng);
            (0..k).map(|_| rng.gen::<f64>() < p).collect()
        })
        .collect()
}

pub fn row(c: f64, trials: &[Vec<bool>], boot: usize, seed: u64) -> ConcRow {
    let o = |b: bool| if b { TrialOutcome::Success } else { TrialOutcome::Fail };
    let first: Vec<_> = trials.iter().map(|t| o(t[0])).collect();
    let all: Vec<_> = trials.iter().map(|t| o(t.iter().all(|&b| b))).collect();
    let (s_lo, s_hi) = asr_bootstrap_ci(&first, boot, seed);
    let (a_lo, a_hi) = asr_bootstrap_ci(&all, boot, seed);
    let (s, a) = (asr(&first), asr(&all));
    ConcRow {
        concentration: c,
        single_shot_asr: s,
        single_shot_ci_lo: s_lo,
        single_shot_ci_hi: s_hi,
        all10_rate: a,
        all10_ci_lo: a_lo,
        all10_ci_hi: a_hi,
        gap: s - a,
    }
}

pub fn study(prompts: usize, boot: usize, seed: u64) -> Vec<ConcRow> {
    CONCENTRATIONS
        .iter()
        .map(|&c| row(c, &simulate(c, prompts, K, seed), boot, seed))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beta_mean_is_half() {
        let mut rng = Pcg64Mcg::seed_from_u64(3);
        for c in CONCENTRATIONS {
            let m: f64 = (0..4000).map(|_| beta(c / 2.0, c / 2.0, &mut rng)).sum::<f64>() / 4000.0;
            assert!((m - 0.5).abs() < 0.03, "c={c} mean={m}");
        }
    }

    #[test]
    fn seeded_and_k10_falls_with_concentration() {
        let a = study(500, 50, 7);
        assert_eq!(a, study(500, 50, 7));
        assert_eq!(a.len(), 5);
        assert!(a[0].all10_rate > a[4].all10_rate);
    }
}
