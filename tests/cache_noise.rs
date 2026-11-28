use jailbreakreplay::cache_noise::{power, sign_test_p, study};
use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;

#[test]
fn null_size_is_controlled() {
    let rows = study(100, 300, 3);
    assert!(rows.iter().all(|r| r.size_at_zero < 0.09));
}

#[test]
fn power_grows_with_difference() {
    let mut rng = Pcg64Mcg::seed_from_u64(1);
    let small = power(0.2, 0.02, 100, 300, &mut rng);
    let big = power(0.2, 0.4, 100, 300, &mut rng);
    assert!(big > small);
}

#[test]
fn p_value_is_symmetric() {
    assert_eq!(sign_test_p(3, 12), sign_test_p(9, 12));
}
