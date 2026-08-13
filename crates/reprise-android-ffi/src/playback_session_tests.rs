use super::*;

#[test]
fn a_snapshot_with_an_out_of_range_cursor_has_no_track_identity() {
    let mut state = SessionState::new();
    state.track_ids = vec![41];
    state.uris = vec!["content://provider/only.flac".to_owned()];
    state.snapshot.current_index = Some(2);
    state.snapshot.current_track_id = Some(41);
    state.snapshot.current_track_uri = Some("content://provider/only.flac".to_owned());

    let snapshot = state.presented_snapshot();

    assert_eq!(snapshot.current_track_id, None);
    assert_eq!(snapshot.current_track_uri, None);
}

/// `presented_snapshot` runs on every position tick, so resolving the
/// playing track must not walk the queue.
///
/// A wall-clock deadline would be the obvious test and the wrong one: this
/// suite shares a machine with other builds, so a fixed bound turns red for
/// reasons that have nothing to do with the code, and a test that cries
/// wolf stops being read. Comparing a long queue against a short one
/// measures the *shape* of the cost instead, and load inflates both
/// measurements together, so it cancels out of the ratio.
///
/// A scan is roughly `LARGE / SMALL` times dearer here; the threshold sits
/// far below that and far above the noise.
#[test]
fn presenting_a_long_queue_costs_about_what_a_short_one_costs() {
    const SNAPSHOTS: usize = 50_000;
    const LARGE: usize = 10_000;
    const SMALL: usize = 10;
    const MAX_RATIO: f64 = 8.0;

    fn cost(queue_size: usize, snapshots: usize) -> std::time::Duration {
        let mut state = SessionState::new();
        let ids = (0..queue_size as i64).collect::<Vec<_>>();
        let uris = ids
            .iter()
            .map(|id| format!("content://provider/{id}.flac"))
            .collect();
        // The last track: a scan by id pays the whole queue for it, an
        // index does not care where it sits.
        state.set_tracks(ids, uris, queue_size - 1);

        let started = std::time::Instant::now();
        for _ in 0..snapshots {
            std::hint::black_box(state.presented_snapshot());
        }
        started.elapsed()
    }

    // Warm the allocator before either measurement counts.
    let _ = cost(SMALL, SNAPSHOTS / 10);
    let short = cost(SMALL, SNAPSHOTS);
    let long = cost(LARGE, SNAPSHOTS);

    let ratio = long.as_secs_f64() / short.as_secs_f64().max(f64::EPSILON);
    assert!(
        ratio <= MAX_RATIO,
        "resolving the playing track must not walk the queue: \
         {LARGE} tracks cost {long:?} against {short:?} for {SMALL}, \
         a factor of {ratio:.1} over the {MAX_RATIO:.0}× allowed",
    );
}
