//! Core primitives for jailbreak reproducibility.
//!
//! One attack-success rate (ASR) with a 95% bootstrap CI; one "reproduces
//! the paper" decision rule that is a function of the paper's claim and
//! the replication's CI. See `docs/PREREGISTRATION.md` for the full plan.

use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;
use serde::{Deserialize, Serialize};

/// Outcome of one trial of one attack against one target.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TrialOutcome {
    Success,
    Fail,
    /// Target returned an unparseable response or the attack errored
    /// out. Not counted toward ASR; tracked separately for the audit.
    Error,
}

/// Attack-success rate over an iterable of trial outcomes. Errors are
/// excluded from both numerator and denominator; the caller is expected
/// to report the error rate separately so a high-error replication is
/// not confused with a low-ASR one.
pub fn asr(trials: &[TrialOutcome]) -> f64 {
    let (hits, counted) = trials.iter().fold((0usize, 0usize), |(h, c), t| match t {
        TrialOutcome::Success => (h + 1, c + 1),
        TrialOutcome::Fail => (h, c + 1),
        TrialOutcome::Error => (h, c),
    });
    if counted == 0 {
        return 0.0;
    }
    hits as f64 / counted as f64
}

/// Percentile bootstrap 95% CI for the ASR by resampling trials with
/// replacement. Returns `(lo, hi)`.
pub fn asr_bootstrap_ci(trials: &[TrialOutcome], iters: usize, seed: u64) -> (f64, f64) {
    assert!(!trials.is_empty());
    let mut rng = Pcg64Mcg::seed_from_u64(seed);
    let mut rates = Vec::with_capacity(iters);
    let n = trials.len();
    let mut sample = Vec::with_capacity(n);
    for _ in 0..iters {
        sample.clear();
        for _ in 0..n {
            let k = (rand::Rng::gen::<u32>(&mut rng) as usize) % n;
            sample.push(trials[k]);
        }
        rates.push(asr(&sample));
    }
    rates.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let lo = rates[(iters as f64 * 0.025).floor() as usize];
    let hi = rates[((iters as f64 * 0.975).ceil() as usize).min(iters - 1)];
    (lo, hi)
}

/// Pre-registered reproduction verdict given the paper's claimed ASR
/// and the replication's 95% CI.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReplicationVerdict {
    /// Paper's claimed ASR falls inside the replication's 95% CI.
    Reproduced,
    /// Replication CI is strictly below the paper's claim.
    NarrowedBelowClaim,
    /// Replication CI is strictly above the paper's claim.
    StrongerThanClaim,
    /// The CI brackets the claim's complement — rare, treated as a
    /// failure case.
    Diverged,
}

pub fn verdict(claim: f64, ci: (f64, f64)) -> ReplicationVerdict {
    let (lo, hi) = ci;
    if lo <= claim && claim <= hi {
        ReplicationVerdict::Reproduced
    } else if hi < claim {
        ReplicationVerdict::NarrowedBelowClaim
    } else if lo > claim {
        ReplicationVerdict::StrongerThanClaim
    } else {
        ReplicationVerdict::Diverged
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TrialOutcome::*;

    #[test]
    fn asr_excludes_errors_from_denominator() {
        let t = vec![Success, Success, Fail, Error, Error];
        // 2 of 3 non-errors succeed.
        assert!((asr(&t) - 2.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn asr_bootstrap_ci_contains_point_estimate_for_balanced_input() {
        let mut t = vec![Success; 50];
        t.extend(vec![Fail; 50]);
        let (lo, hi) = asr_bootstrap_ci(&t, 500, 42);
        assert!(lo <= 0.5 && 0.5 <= hi, "lo={lo} hi={hi}");
    }

    #[test]
    fn verdict_reproduced_when_claim_in_ci() {
        assert_eq!(verdict(0.7, (0.6, 0.8)), ReplicationVerdict::Reproduced);
    }

    #[test]
    fn verdict_narrowed_when_ci_below() {
        assert_eq!(
            verdict(0.9, (0.6, 0.8)),
            ReplicationVerdict::NarrowedBelowClaim
        );
    }

    #[test]
    fn verdict_stronger_when_ci_above() {
        assert_eq!(
            verdict(0.5, (0.6, 0.8)),
            ReplicationVerdict::StrongerThanClaim
        );
    }
}
