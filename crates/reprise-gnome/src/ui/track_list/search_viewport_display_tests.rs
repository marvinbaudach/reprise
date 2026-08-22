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
    let expected = super::reload_restore::flat_list_centered_track_scroll_target(
        Some(playing_id),
        &current_ids,
        row_height,
        adjustment.page_size(),
    )
    .expect("the expanded list must have a centered target");
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
    let expected = super::reload_restore::flat_list_centered_track_scroll_target(
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

// ---------------------------------------------------------------------------
// SEARCH-16's intermediate state: the value *sequence*, not just the endpoint.
//
// The sibling tests above assert where the viewport ends up. That cannot see
// the bug the user reported — "it hops through the top first" — because a hop
// and a clean move have the same endpoint. These two record every position the
// adjustment actually took while the restore ran, and who asked for it.
// ---------------------------------------------------------------------------

/// One position the viewport actually settled at, and the writer that asked
/// for it. `writer` is `gtk` when no probe claimed the value — that is GTK's
/// own allocation pass writing the old offset back, and it is a different bug
/// from ours.
#[derive(Debug)]
pub(super) struct ViewportStep {
    pub(super) writer: String,
    pub(super) value: f64,
}

/// Reduces a probe trail to the steps a user could see.
///
/// A write that asks for the value already on screen emits no change and is
/// not a step; two writes to the same place are one step. What survives is the
/// count the report turns on: one step is a viewport that moved once, two are
/// the hop.
///
/// "Same place" is a whole pixel, not an exact float. The view floors the
/// scroll offset to an integer after we write it — measured here as a
/// `centered.*.apply` asking for 2923.5 followed by an unclaimed 2923.0 — and
/// counting that as a second step would report a hop nobody can see.
/// `scroll_glide` documents the same flooring from the other side.
const SUB_PIXEL: f64 = 1.0;

pub(super) fn viewport_steps(
    entries: Vec<crate::ui::scroll_probe::trail::Entry>,
) -> Vec<ViewportStep> {
    use crate::ui::scroll_probe::trail::Entry;

    let mut steps: Vec<ViewportStep> = Vec::new();
    let mut asked_by: Option<String> = None;
    for entry in entries {
        match entry {
            Entry::Write { writer, .. } | Entry::ScrollTo { writer, .. } => {
                asked_by = Some(writer);
            }
            Entry::Observed { value } => {
                let writer = asked_by.take().unwrap_or_else(|| "gtk".to_owned());
                if steps
                    .last()
                    .is_some_and(|last| (last.value - value).abs() < SUB_PIXEL)
                {
                    continue;
                }
                steps.push(ViewportStep { writer, value });
            }
        }
    }
    steps
}

/// Records every position the adjustment takes until the handler is dropped.
pub(super) fn record_viewport_steps(adjustment: &gtk4::Adjustment) -> gtk4::glib::SignalHandlerId {
    let handler = adjustment.connect_value_changed(|changed| {
        crate::ui::scroll_probe::trail::note_observed(changed.value());
    });
    crate::ui::scroll_probe::trail::start();
    handler
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_16_clearing_after_a_play_reaches_the_track_in_one_step() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let stage = clear_search_stage();
    let playing_id = 90;
    stage
        .track_list
        .update_current_track(playing_id, None, CurrentTrackChange::PlaybackStarted);

    let handler = record_viewport_steps(&stage.adjustment);
    assert!(stage.search.clear_active_query());
    crate::ui::test_settle::settle_for(Duration::from_millis(500));
    stage.adjustment.disconnect(handler);
    let steps = viewport_steps(crate::ui::scroll_probe::trail::take());

    let current_ids = stage.track_list.shared.current_view_ids();
    let row_height = stage.adjustment.upper() / current_ids.len() as f64;
    let expected = super::reload_restore::flat_list_centered_track_scroll_target(
        Some(playing_id),
        &current_ids,
        row_height,
        stage.adjustment.page_size(),
    )
    .expect("the cleared list must have a centered target");

    // Built as a control arm, and it ran as one. Before the rebuild this read
    // `steps.len() == 2` and passed, recording the defect on purpose:
    // `centered.scroll_to 3026.0` — the edge snap this path fired before it
    // could centre — followed by `centered.changed.apply 2923.5`, the centring
    // that moved the list the rest of the way. The second move is the hop the
    // user reported; the first one was ours.
    //
    // One move now, because the restore writes a value an anchor row explains,
    // so GTK's allocation pass reproduces it instead of correcting it
    // (`centered_scroll_restore::centered_anchor`). Going back to a value no
    // anchor row explains is what this test refuses.
    assert_eq!(
        steps.len(),
        1,
        "clearing the search must place the loaded track in one move: {steps:?}"
    );
    assert!(
        steps[0].writer.starts_with("centered."),
        "the centering path must own the move — an unclaimed value would be \
         GTK's allocation pass writing over it: {steps:?}"
    );
    assert!(
        (steps[0].value - expected).abs() <= row_height,
        "the single move must land on the centered target {expected}: {steps:?}"
    );

    stage.window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn start_3_centering_the_loaded_track_reaches_it_in_one_step() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=200 {
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
            (
                id,
                format!("/synthetic/{id:03}.flac"),
                format!("Track {id:03}"),
            ),
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

    // START-3's own order: the session marks the loaded track, then
    // `center_loaded_track` places the viewport on it.
    let position = 140_u32;
    let track_id = track_list.shared.model.track_at(position).unwrap().id;
    track_list.update_current_track(track_id, None, CurrentTrackChange::SessionRestore);

    let handler = record_viewport_steps(&adjustment);
    track_list.center_loaded_track();
    crate::ui::test_settle::settle_for(Duration::from_millis(500));
    adjustment.disconnect(handler);
    let steps = viewport_steps(crate::ui::scroll_probe::trail::take());

    let current_ids = track_list.shared.current_view_ids();
    let row_height = adjustment.upper() / current_ids.len() as f64;
    let expected = super::reload_restore::flat_list_centered_track_scroll_target(
        Some(track_id),
        &current_ids,
        row_height,
        adjustment.page_size(),
    )
    .expect("a 200-row list in a 320px window must have a centered target");

    assert_eq!(
        steps.len(),
        1,
        "the start must place the loaded track in one move: {steps:?}"
    );
    assert!(
        (steps[0].value - expected).abs() <= row_height,
        "the single move must land on the centered target {expected}: {steps:?}"
    );
    assert!(
        steps[0].writer.starts_with("centered."),
        "the centering path must own the move: {steps:?}"
    );

    window.close();
}
