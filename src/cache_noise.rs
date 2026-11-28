//! Model-free serving-noise power study: two arms (cache on / cache off) over a
//! fixed prompt set, each prompt's outcome flipping between arms with a given
//! probability, and the smallest true non-refusal difference a paired exact
//! sign test detects at 80% power.

use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;
use serde::Serialize;

pub const FLIP_RATES: [f64; 5] = [0.0, 0.05, 0.1, 0.2, 0.388];
pub const ALPHA: f64 = 0.05;
pub const POWER_TARGET: f64 = 0.8;
pub const BASE_RATE: f64 = 0.5;

#[derive(Debug, Serialize, PartialEq)]
pub struct FlipRow {
    pub flip_probability: f64,
    /// First grid difference reaching the power target, if any.
    pub min_detectable_difference: Option<f64>,
    pub power_at_min: Option<f64>,
    /// Type-I rate at true difference 0.
    pub size_at_zero: f64,
}

/// Two-sided exact binomial (p = 0.5) p-value for `b` of `n` discordant pairs.
pub fn sign_test_p(b: usize, n: usize) -> f64 {
    if n == 0 {
        return 1.0;
    }
    let lo = b.min(n - b);
    // P(X <= lo) for Binomial(n, 0.5), via running pmf.
    let mut pmf = 0.5f64.powi(n as i32);
    let mut cdf = pmf;
    for k in 0..lo {
        pmf *= (n - k) as f64 / (k + 1) as f64;
        cdf += pmf;
    }
    (2.0 * cdf).min(1.0)
}

/// Transition probabilities (A=1 -> B=0, A=0 -> B=1) giving marginals
/// P(A)=0.5, P(B)=0.5-d with symmetric flip f at d=0 (clipped when f < d).
fn transitions(f: f64, d: f64) -> (f64, f64) {
    // P(B) = 0.5 (1 - q10) + 0.5 q01 = 0.5 - d  => q10 - q01 = 2d.
    if f >= d {
        (f + d, f - d)
    } else {
        (2.0 * d, 0.0)
    }
}

/// Fraction of `reps` replications whose paired test rejects at `ALPHA`.
pub fn power(f: f64, d: f64, prompts: usize, reps: usize, rng: &mut Pcg64Mcg) -> f64 {
    let (q10, q01) = transitions(f, d);
    let mut rejects = 0usize;
    for _ in 0..reps {
        let (mut b, mut c) = (0usize, 0usize);
        for _ in 0..prompts {
            let a = rng.gen::<f64>() < BASE_RATE;
            let u = rng.gen::<f64>();
            if a && u < q10 {
                b += 1;
            } else if !a && u < q01 {
                c += 1;
            }
        }
        if sign_test_p(b, b + c) < ALPHA {
            rejects += 1;
        }
    }
    rejects as f64 / reps as f64
}

pub fn study(prompts: usize, reps: usize, seed: u64) -> Vec<FlipRow> {
    let mut rng = Pcg64Mcg::seed_from_u64(seed);
    FLIP_RATES
        .iter()
        .map(|&f| {
            let size = power(f, 0.0, prompts, reps, &mut rng);
            let mut found = (None, None);
            for step in 1..=50 {
                let d = step as f64 / 100.0;
                // q10 = f + d must stay a probability.
                if transitions(f, d).0 > 1.0 {
                    break;
                }
                let p = power(f, d, prompts, reps, &mut rng);
                if p >= POWER_TARGET {
                    found = (Some(d), Some(p));
                    break;
                }
            }
            FlipRow {
                flip_probability: f,
                min_detectable_difference: found.0,
                power_at_min: found.1,
                size_at_zero: size,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_test_known_values() {
        assert_eq!(sign_test_p(0, 0), 1.0);
        assert!((sign_test_p(5, 10) - 1.0).abs() < 1e-12);
        assert!((sign_test_p(0, 10) - 2.0 / 1024.0).abs() < 1e-12);
    }

    #[test]
    fn seeded_and_noise_raises_threshold() {
        let a = study(100, 200, 7);
        assert_eq!(a, study(100, 200, 7));
        let lo = a[0].min_detectable_difference.unwrap();
        let hi = a[4].min_detectable_difference.unwrap();
        assert!(hi > lo);
    }
}
