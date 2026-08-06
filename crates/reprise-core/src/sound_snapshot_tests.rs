use crate::sound_snapshot::{
    ready_for_matches, sound_work_allowed, ProgressWatch, MIN_READY_FEATURES, PROGRESS_STALL_LIMIT,
};

#[test]
fn sim_4_panel_requires_fifty_profiles_and_the_current_track() {
    assert!(!ready_for_matches(MIN_READY_FEATURES - 1, true));
    assert!(!ready_for_matches(MIN_READY_FEATURES, false));
    assert!(ready_for_matches(MIN_READY_FEATURES, true));
}

#[test]
fn sim_4_progress_rechecks_stop_once_the_inventory_stops_advancing() {
    let advancing = ProgressWatch::default()
        .observe((10, 100))
        .and_then(|watch| watch.observe((11, 100)))
        .and_then(|watch| watch.observe((12, 100)));
    assert!(
        advancing.is_some(),
        "a catching-up library keeps re-checking"
    );

    let mut watch = Some(ProgressWatch::default());
    let mut readings = 0;
    while let Some(current) = watch {
        watch = current.observe((12, 100));
        readings += 1;
        assert!(readings <= PROGRESS_STALL_LIMIT + 2, "the poll never ends");
    }
    assert_eq!(readings, PROGRESS_STALL_LIMIT + 2);

    // A fresh request re-enters the loop with an unused budget, so a backfill
    // that finishes later can still make the panel ready.
    assert!(ProgressWatch::default().observe((12, 100)).is_some());
}

#[test]
fn sim_6_a_disabled_module_does_no_sound_work() {
    assert!(!sound_work_allowed(false, true));
    assert!(!sound_work_allowed(false, false));
    assert!(!sound_work_allowed(true, false));
    assert!(sound_work_allowed(true, true));
}
