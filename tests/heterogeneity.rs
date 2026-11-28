use jailbreakreplay::heterogeneity::{simulate, study, K};

#[test]
fn trial_shape_and_determinism() {
    let t = simulate(2.0, 50, K, 1);
    assert_eq!(t.len(), 50);
    assert!(t.iter().all(|r| r.len() == K));
    assert_eq!(t, simulate(2.0, 50, K, 1));
}

#[test]
fn gap_is_nonnegative_and_rows_cover_concentrations() {
    let rows = study(200, 50, 7);
    assert_eq!(rows.len(), 5);
    assert!(rows.iter().all(|r| r.gap >= 0.0 && r.all10_rate <= r.single_shot_asr));
}
