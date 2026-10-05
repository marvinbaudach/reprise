use super::*;

#[test]
fn default_is_zero_like_the_counters_it_replaces() {
    assert_eq!(Generation::default(), Generation(0));
}

#[test]
fn next_increments_and_wraps_at_the_maximum() {
    assert_eq!(Generation(0).next(), Generation(1));
    assert_eq!(Generation(41).next(), Generation(42));
    assert_eq!(Generation(u64::MAX).next(), Generation(0));
}

#[test]
fn equality_is_by_value() {
    assert_eq!(Generation(7), Generation(7));
    assert_ne!(Generation(7), Generation(8));
    assert_ne!(Generation(7), Generation(7).next());
}
