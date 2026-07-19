use super::counts_as_play;

#[test]
fn counts_as_play_matches_the_scrobble_threshold() {
    assert!(!counts_as_play(89_999, 180_000));
    assert!(counts_as_play(90_000, 180_000));
    assert!(!counts_as_play(239_999, 600_000));
    assert!(counts_as_play(240_000, 600_000));
    assert!(!counts_as_play(1_000, 0));
    assert!(!counts_as_play(1_000, -1));
}
