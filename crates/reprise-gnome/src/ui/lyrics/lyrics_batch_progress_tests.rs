use gtk4::prelude::*;

use super::*;

fn progress(state: LyricsBatchState, checked: usize, total: usize) -> LyricsBatchProgress {
    LyricsBatchProgress {
        state,
        checked,
        total,
        downloaded: 1,
        unavailable: 2,
    }
}

#[test]
fn running_progress_is_determinate_and_names_lyrics_counts() {
    let presentation = presentation(progress(LyricsBatchState::Running, 2, 4));

    assert!(presentation.visible);
    assert_eq!(presentation.title, "Checking missing lyrics…");
    assert!(presentation.detail.contains("2 of 4"));
    assert!(presentation.detail.contains("1 cached"));
    assert!(presentation.detail.contains("2 unavailable"));
    assert_eq!(presentation.fraction, 0.5);
    assert!(!presentation.auto_hide);
}

#[test]
fn terminal_progress_auto_hides_and_idle_stays_hidden() {
    let idle = presentation(progress(LyricsBatchState::Idle, 0, 0));
    assert!(!idle.visible);
    assert!(!idle.auto_hide);

    for (state, title) in [
        (LyricsBatchState::Complete, "Lyrics check complete"),
        (LyricsBatchState::Failed, "Could not check lyrics"),
    ] {
        let presentation = presentation(progress(state, 7, 4));
        assert!(presentation.visible);
        assert_eq!(presentation.title, title);
        assert_eq!(presentation.fraction, 1.0);
        assert!(presentation.auto_hide);
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn lyr_6_scan_controls_show_live_lyrics_batch_progress() {
    gtk4::init().unwrap();
    let button = gtk4::Button::new();
    let view = crate::ui::scan_progress::ScanProgressView::new();
    let controls = ScanControls::new(&button, &view);
    let db = Rc::new(crate::test_db::open().unwrap());
    let batch = LyricsBatch::new(&db);
    install(&controls, &batch);

    batch.set_progress_for_test(LyricsBatchProgress {
        state: LyricsBatchState::Running,
        checked: 2,
        total: 4,
        downloaded: 1,
        unavailable: 0,
    });

    assert!(view.widget().reveals_child());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn lyr_6_the_card_cancel_stops_the_batch_without_sharing_the_scan_flag() {
    gtk4::init().unwrap();
    let button = gtk4::Button::new();
    let view = crate::ui::scan_progress::ScanProgressView::new();
    let controls = ScanControls::new(&button, &view);
    let db = Rc::new(crate::test_db::open().unwrap());
    let batch = LyricsBatch::new(&db);
    install(&controls, &batch);

    // While a real scan owns the card, its cancel belongs to the scan alone.
    button.set_sensitive(false);
    controls.request_cancel();
    assert!(
        !batch.is_cancel_requested(),
        "cancelling a library scan must not abort the lyrics batch"
    );

    // With the card showing the batch, the same gesture stops the batch.
    button.set_sensitive(true);
    controls.request_cancel();
    assert!(
        batch.is_cancel_requested(),
        "the card's cancel gesture must still stop the lyrics batch"
    );

    // A scan starting resets its own cancel flag — never the batch's.
    controls.reset_cancel();
    assert!(
        batch.is_cancel_requested(),
        "starting a scan must not clear a pending lyrics-batch cancel"
    );
}
