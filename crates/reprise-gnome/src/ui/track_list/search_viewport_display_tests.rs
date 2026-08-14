//! Visible proof for SEARCH-9's viewport half: a typed query reads from the
//! top, and clearing it returns to where the search began. The rule-named
//! tests live in `track_list_reload.rs` and are display-free by design; these
//! need a real `ColumnView` with a real allocation and are `#[ignore]`d.

use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;

use super::super::current_track_selection::CurrentTrackChange;
use crate::ui::track_list::TrackList;

/// Lets the frame clock run for a moment so GTK can allocate. `iteration(false)`
/// alone never blocks, so the clock never ticks and widgets stay unallocated —
/// mirrors `preferences_window::tests::settle_for`.
fn settle() {
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(60), move || quit.quit());
    main_loop.run();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn typed_search_reads_from_the_top_and_clearing_comes_back() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=200 {
        let title = if (150..=170).contains(&id) {
            format!("Match Track {id:03}")
        } else {
            format!("Other Track {id:03}")
        };
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
            (id, format!("/synthetic/{id:03}.flac"), title),
        )
        .unwrap();
    }
    tx.commit().unwrap();

    let track_list = TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        super::super::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(320)
        .child(track_list.widget())
        .build();
    window.present();
    settle();

    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    // A freshly presented `ColumnView` reports no usable geometry until it has
    // been allocated, and a scroll written before that point is clamped
    // straight back to zero — the precondition below then fails for a reason
    // that has nothing to do with what this test is about.
    //
    // Draining with `iteration(false)` is not enough and looked like it was:
    // it never blocks, so the frame clock that drives allocation never gets a
    // turn, and the adjustment stayed at `upper 0, page 0`. Running a real
    // `MainLoop` briefly is what the preferences display tests already do
    // (`preferences_window::tests::settle_for`), and it is the difference
    // between this test being deterministic and passing whenever the
    // allocation happens to win the race.
    settle();
    for _ in 0..20 {
        if adjustment.upper() > adjustment.page_size() {
            break;
        }
        settle();
    }
    adjustment.set_value(1200.0);
    settle();
    let departed_from = adjustment.value();
    assert!(
        departed_from > 0.0,
        "the test must start away from the top, else it proves nothing \
         (upper {}, page {})",
        adjustment.upper(),
        adjustment.page_size()
    );

    track_list.set_filter("Match");
    settle();
    assert_eq!(
        adjustment.value(),
        0.0,
        "a typed query reads from the top of its results"
    );

    track_list.set_filter("");
    settle();
    assert!(
        (adjustment.value() - departed_from).abs() < 40.0,
        "clearing returns within a row of where the search began: expected \
         about {departed_from}, got {}",
        adjustment.value()
    );

    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_16_a_result_set_that_fits_still_centers_after_clear_all() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=200 {
        let title = if (98..=100).contains(&id) {
            format!("Track {id:03} Needle")
        } else {
            format!("Track {id:03} Other")
        };
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
            (id, format!("/synthetic/{id:03}.flac"), title),
        )
        .unwrap();
    }
    tx.commit().unwrap();

    let track_list = Rc::new(TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        super::super::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    ));
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(320)
        .child(track_list.widget())
        .build();
    window.present();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        track_list
            .shared
            .column_view
            .vadjustment()
            .is_some_and(|adjustment| adjustment.upper() > adjustment.page_size())
    });

    let entry = gtk4::SearchEntry::new();
    entry.set_search_delay(0);
    let toggle = gtk4::ToggleButton::new();
    let popover = crate::ui::window::search_popover::SearchPopover::new(&toggle, &entry);
    let search = crate::ui::window::section_search::SectionSearch::new(&entry, &popover, &toggle);
    crate::ui::window::section_search_wiring::install_tracks(&search, &track_list);
    search.activate_source(&reprise_core::view_source::ViewSource::Library, "Music");

    entry.set_text("Needle");
    track_list.set_filter("Needle");
    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        track_list.shared.model.n_items() == 3 && adjustment.upper() <= adjustment.page_size()
    });
    let playing_id = 99;
    track_list.update_current_track(playing_id, None, CurrentTrackChange::PlaybackStarted);

    search.clear_all();
    crate::ui::test_settle::settle_for(Duration::from_millis(500));

    let current_ids = track_list.shared.current_view_ids();
    assert_eq!(
        current_ids.len(),
        200,
        "Clear all must restore the complete synthetic library"
    );
    assert!(
        adjustment.upper() > adjustment.page_size(),
        "the expanded list must be genuinely scrollable"
    );
    let row_height = adjustment.upper() / current_ids.len() as f64;
    let expected = super::reload_restore::centered_track_scroll_target(
        Some(playing_id),
        &current_ids,
        row_height,
        adjustment.page_size(),
    )
    .expect("the expanded list must have a centered target");
    eprintln!(
        "MUTATION PROBE value={} expected={expected} row_height={row_height} upper={} page={}",
        adjustment.value(),
        adjustment.upper(),
        adjustment.page_size()
    );
    assert!(
        (adjustment.value() - expected).abs() <= row_height,
        "Clear all left the expanded list at {} instead of centering near {expected}",
        adjustment.value()
    );

    window.close();
}

struct ClearSearchStage {
    track_list: Rc<TrackList>,
    search: Rc<crate::ui::window::section_search::SectionSearch>,
    _entry: gtk4::SearchEntry,
    _toggle: gtk4::ToggleButton,
    _popover: crate::ui::window::search_popover::SearchPopover,
    window: gtk4::Window,
    adjustment: gtk4::Adjustment,
    departed_from: f64,
}

fn clear_search_stage() -> ClearSearchStage {
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=200 {
        let title = if (80..=100).contains(&id) {
            format!("Track {id:03} Needle")
        } else {
            format!("Track {id:03} Other")
        };
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
            (id, format!("/synthetic/{id:03}.flac"), title),
        )
        .unwrap();
    }
    tx.commit().unwrap();

    let track_list = Rc::new(TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        super::super::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    ));
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(320)
        .child(track_list.widget())
        .build();
    window.present();
    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        adjustment.upper() > adjustment.page_size()
    });
    adjustment.set_value(1_200.0);
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        adjustment.value() > 0.0
    });
    let departed_from = adjustment.value();

    let entry = gtk4::SearchEntry::new();
    entry.set_search_delay(0);
    let toggle = gtk4::ToggleButton::new();
    let popover = crate::ui::window::search_popover::SearchPopover::new(&toggle, &entry);
    let search = crate::ui::window::section_search::SectionSearch::new(&entry, &popover, &toggle);
    crate::ui::window::section_search_wiring::install_tracks(&search, &track_list);
    search.activate_source(&reprise_core::view_source::ViewSource::Library, "Music");
    entry.set_text("Needle");
    track_list.set_filter("Needle");
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        track_list.shared.model.n_items() == 21
            && adjustment.upper() > adjustment.page_size()
            && adjustment.value() < 1.0
    });
    // The query's bounded SEARCH-9 top restore has two idle rounds. Seeing
    // its first zero is not enough: clearing in that setup window would let
    // the second setup write race the behavior this fixture means to test.
    crate::ui::test_settle::settle_for(Duration::from_millis(100));

    ClearSearchStage {
        track_list,
        search,
        _entry: entry,
        _toggle: toggle,
        _popover: popover,
        window,
        adjustment,
        departed_from,
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_16_clearing_after_a_play_centers_the_loaded_track() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let stage = clear_search_stage();
    let playing_id = 90;
    stage
        .track_list
        .update_current_track(playing_id, None, CurrentTrackChange::PlaybackStarted);

    assert!(stage.search.clear_active_query());
    crate::ui::test_settle::settle_for(Duration::from_millis(500));

    let current_ids = stage.track_list.shared.current_view_ids();
    let row_height = stage.adjustment.upper() / current_ids.len() as f64;
    let expected = super::reload_restore::centered_track_scroll_target(
        Some(playing_id),
        &current_ids,
        row_height,
        stage.adjustment.page_size(),
    )
    .expect("the cleared list must have a centered target");
    assert!(
        (stage.adjustment.value() - expected).abs() <= row_height,
        "clearing after playback landed at {} instead of {expected}",
        stage.adjustment.value()
    );

    stage.window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_16_clearing_without_a_play_returns_to_the_pre_search_place() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let stage = clear_search_stage();
    assert!(!stage.track_list.shared.pre_search.get().playback_started);

    assert!(stage.search.clear_active_query());
    crate::ui::test_settle::settle_for(Duration::from_millis(500));

    let row_height = stage.adjustment.upper() / 200.0;
    assert!(
        (stage.adjustment.value() - stage.departed_from).abs() <= row_height,
        "clearing without playback landed at {} instead of the pre-search {}",
        stage.adjustment.value(),
        stage.departed_from
    );

    stage.window.close();
}
