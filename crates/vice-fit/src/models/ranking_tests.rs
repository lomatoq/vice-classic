use std::cmp::Ordering;

use super::compare_rank_values;

#[test]
fn proposal_integral_is_load_bearing_on_an_exact_code_tie() {
    assert_eq!(
        compare_rank_values(100.0, 2.0, 100.0, 7.0),
        Ordering::Less,
        "removing the proposal leg must make this test red (RT6-A4)"
    );
    assert_eq!(
        compare_rank_values(101.0, 0.0, 100.0, 1.0e9),
        Ordering::Greater,
        "proposal cost must never overrule the physical-bit selector"
    );
}
