//! Unit tests for `current_track_selection.rs`, in their own file so the
//! module stays under the 800-line cap — the same split its glide and
//! start-restore tests already use.

use super::super::PreSearch;
use super::*;
use reprise_core::queries::BrowseFilter;

#[test]
fn nav_10b_playback_scroll_policy_distinguishes_user_intent() {
    assert_eq!(
        reveal_policy(CurrentTrackChange::PlaybackStarted, false),
        TrackRevealPolicy::MarkerOnly
    );
    assert_eq!(
        reveal_policy(CurrentTrackChange::ExplicitTransport, true),
        TrackRevealPolicy::Center
    );
    assert_eq!(
        reveal_policy(CurrentTrackChange::AutomaticAdvance, false),
        TrackRevealPolicy::Center
    );
    assert_eq!(
        reveal_policy(CurrentTrackChange::AutomaticAdvance, true),
        TrackRevealPolicy::MarkerOnly
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_16_only_a_user_start_during_the_search_arms_the_centering() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let track_list = TrackList::new(
        Rc::new(crate::test_db::open().unwrap()),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        super::super::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );

    *track_list.shared.filter.borrow_mut() = "needle".to_owned();
    track_list.update_current_track(1, None, CurrentTrackChange::PlaybackStarted);
    assert!(track_list.shared.pre_search.get().playback_started);

    track_list.shared.pre_search.set(PreSearch::default());
    track_list.update_current_track(2, None, CurrentTrackChange::AutomaticAdvance);
    assert!(!track_list.shared.pre_search.get().playback_started);

    track_list.shared.pre_search.set(PreSearch::default());
    track_list.shared.filter.borrow_mut().clear();
    track_list.update_current_track(3, None, CurrentTrackChange::PlaybackStarted);
    assert!(!track_list.shared.pre_search.get().playback_started);

    track_list.shared.pre_search.set(PreSearch {
        anchor: Some((4, 12.0)),
        playback_started: true,
    });
    crate::ui::track_list::track_list_reload::prepare_filter_change(
        &track_list.shared,
        "",
        "new query",
    );
    assert!(!track_list.shared.pre_search.get().playback_started);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_10b_row_activation_marker_does_not_move_selection_or_viewport() {
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=100 {
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
    while gtk4::glib::MainContext::default().iteration(false) {}

    let position = 60;
    let track_id = track_list.shared.model.track_at(position).unwrap().id;
    track_list
        .shared
        .column_view
        .scroll_to(position, None, gtk4::ListScrollFlags::FOCUS, None);
    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    // `scroll_to` settles over later main-loop turns, so pumping once is not
    // enough to establish the precondition. This is test setup, not the
    // behaviour under test: wait until the viewport actually moved.
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        adjustment.value() > 0.0
    });
    let before = adjustment.value();
    assert!(
        before > 0.0,
        "precondition: the list must be scrolled away from the top"
    );
    track_list.shared.selection.select_item(10, true);
    track_list.update_current_track(track_id, None, CurrentTrackChange::PlaybackStarted);
    while gtk4::glib::MainContext::default().iteration(false) {}

    assert!((adjustment.value() - before).abs() < 0.5);
    assert!(track_list.shared.selection.is_selected(10));
    assert!(!track_list.shared.selection.is_selected(position));

    let auto_position = 80;
    let auto_track_id = track_list.shared.model.track_at(auto_position).unwrap().id;
    track_list
        .shared
        .last_scroll_activity
        .set(Some(std::time::Instant::now()));
    track_list.update_current_track(auto_track_id, None, CurrentTrackChange::AutomaticAdvance);
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert!(
        (adjustment.value() - before).abs() < 0.5,
        "automatic advance must not fight an active scroll"
    );

    track_list.shared.last_scroll_activity.set(None);
    track_list.update_current_track(auto_track_id, None, CurrentTrackChange::AutomaticAdvance);
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert!(
        (adjustment.value() - before).abs() >= 0.5,
        "idle automatic advance must center the new track"
    );

    track_list.update_current_track(track_id, None, CurrentTrackChange::SessionRestore);
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert!(track_list.shared.selection.is_selected(10));
    assert!(!track_list.shared.selection.is_selected(position));

    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fil_9_filter_changes_center_the_visible_playing_track() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=100 {
        let title = if (31..=60).contains(&id) {
            format!("Match Track {id:03}")
        } else {
            format!("Other Track {id:03}")
        };
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, genre, added_at) \
             VALUES (?1, ?2, ?3, 'Synthetic Artist', ?4, 0)",
            (
                id,
                format!("/synthetic/{id:03}.flac"),
                title,
                if (31..=60).contains(&id) {
                    "Synthetic"
                } else {
                    "Other"
                },
            ),
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
    while gtk4::glib::MainContext::default().iteration(false) {}

    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    let playing_id = 51;
    let unfiltered_ids = track_list.shared.current_view_ids();
    let unfiltered_position = unfiltered_ids
        .iter()
        .position(|id| *id == playing_id)
        .and_then(|position| u32::try_from(position).ok())
        .unwrap();
    let unfiltered_row_height =
        super::super::display_test_geometry::measured_row_height(&track_list.shared.column_view)
            .expect("the settled unfiltered list must expose measured rows");
    adjustment.set_value(f64::from(unfiltered_position) * unfiltered_row_height);
    while gtk4::glib::MainContext::default().iteration(false) {}
    track_list.update_current_track(playing_id, None, CurrentTrackChange::PlaybackStarted);

    *track_list.shared.browse_filter.borrow_mut() = BrowseFilter {
        genre: Some("Synthetic".to_string()),
        ..BrowseFilter::default()
    };
    crate::ui::track_list::track_list_reload::reload_centering_playing_track(&track_list.shared);
    assert_eq!(track_list.shared.model.n_items(), 30);
    assert_playing_track_centered(&track_list, playing_id, &adjustment);

    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        super::super::display_test_geometry::measured_row_height(&track_list.shared.column_view)
            .is_some()
    });

    let filtered_ids = track_list.shared.current_view_ids();
    let filtered_position = filtered_ids
        .iter()
        .position(|id| *id == playing_id)
        .and_then(|position| u32::try_from(position).ok())
        .unwrap();
    let filtered_row_height =
        super::super::display_test_geometry::measured_row_height(&track_list.shared.column_view)
            .expect("the settled filtered list must expose measured rows");
    adjustment.set_value(f64::from(filtered_position) * filtered_row_height);
    while gtk4::glib::MainContext::default().iteration(false) {}

    *track_list.shared.browse_filter.borrow_mut() = BrowseFilter::default();
    crate::ui::track_list::track_list_reload::reload_centering_playing_track(&track_list.shared);
    assert_eq!(track_list.shared.model.n_items(), 100);
    assert_playing_track_centered(&track_list, playing_id, &adjustment);

    window.close();
}

fn assert_playing_track_centered(
    track_list: &TrackList,
    playing_id: i64,
    adjustment: &gtk4::Adjustment,
) {
    let current_ids = track_list.shared.current_view_ids();
    let playing_position = current_ids
        .iter()
        .position(|id| *id == playing_id)
        .and_then(|position| u32::try_from(position).ok())
        .unwrap();
    // Half a row, and why: see `centering_tolerance` in `start_restore_tests`.
    // The restore lands on the row edge nearest the centre, because that is
    // the only value GTK's own anchor reproduces.
    // Range-derived height only bounds permissible centering error; it is not the target oracle.
    let tolerance = adjustment.upper() / f64::from(track_list.shared.model.n_items()) / 2.0;
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        scroll_center::centered_scroll_value(
            playing_position,
            track_list.shared.model.n_items(),
            adjustment.upper(),
            adjustment.page_size(),
        )
        .is_some_and(|target| (adjustment.value() - target).abs() <= tolerance)
    });
    let expected = scroll_center::centered_scroll_value(
        playing_position,
        track_list.shared.model.n_items(),
        adjustment.upper(),
        adjustment.page_size(),
    )
    .expect("expanded list must have centering geometry");
    assert!(
        (adjustment.value() - expected).abs() <= tolerance,
        "filter change must center playing track {playing_id}: \
         actual {}, expected {expected} (within {tolerance})",
        adjustment.value()
    );
}

/// Counts the widgets in `widget`'s subtree carrying the `.now-playing`
/// marker class — the visible footprint of the now-playing row's cells.
fn count_now_playing(widget: &gtk4::Widget) -> usize {
    let mut count = usize::from(widget.has_css_class("now-playing"));
    let mut child = widget.first_child();
    while let Some(current) = child {
        count += count_now_playing(&current);
        child = current.next_sibling();
    }
    count
}

/// The now-playing marker must be applied to (and cleared from) the already-
/// realised cell widgets IN PLACE — the mechanism that replaced the former
/// `items_changed(pos, 1, 1)` refresh (whose fake remove+insert snapped the
/// viewport to the top). Proves the registered re-appliers actually toggle
/// real widgets, and that the reapply path never panics (RefCell re-entry).
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn now_playing_marker_toggles_visible_cells_in_place() {
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=100 {
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
    while gtk4::glib::MainContext::default().iteration(false) {}

    let column_view: gtk4::Widget = track_list.shared.column_view.clone().upcast();

    // No track playing yet: no cell carries the marker.
    assert_eq!(count_now_playing(&column_view), 0);

    // Start playback on a row visible at the top (no scroll involved): the
    // marker appears on that row's realised cells with no model signal.
    let first_id = track_list.shared.model.track_at(0).unwrap().id;
    track_list.update_current_track(first_id, None, CurrentTrackChange::PlaybackStarted);
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert!(
        count_now_playing(&column_view) > 0,
        "playing row's cells must gain the marker in place"
    );

    // Advancing to another visible row moves the marker; the footprint
    // stays that of a single row (no stale marker left behind).
    let footprint = count_now_playing(&column_view);
    let second_id = track_list.shared.model.track_at(1).unwrap().id;
    track_list.update_current_track(second_id, None, CurrentTrackChange::PlaybackStarted);
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert_eq!(
        count_now_playing(&column_view),
        footprint,
        "marker must move, not accumulate on the previous row"
    );

    // Stopping clears the marker from every cell.
    track_list.on_playback_state(PlaybackState::Stopped);
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert_eq!(count_now_playing(&column_view), 0);

    window.close();
}

#[test]
fn visible_position_finds_the_current_track_in_view_order() {
    assert_eq!(
        visible_position_for_track_in_source(&[41, 42, 43], 42, None, false),
        Some(1)
    );
}

#[test]
fn visible_position_uses_queue_occurrence_then_falls_back_to_first_match() {
    assert_eq!(
        visible_position_for_track_in_source(&[7, 8, 7], 7, Some(2), false),
        Some(2)
    );
    assert_eq!(
        visible_position_for_track_in_source(&[7, 8, 7], 7, Some(1), false),
        Some(0)
    );
    assert_eq!(
        visible_position_for_track_in_source(&[7, 8, 7], 9, None, false),
        None
    );
}

#[test]
fn queue_does_not_highlight_a_pending_duplicate_of_the_current_track() {
    assert_eq!(
        visible_position_for_track_in_source(&[7, 8, 7], 7, None, true),
        None
    );
    assert_eq!(
        visible_position_for_track_in_source(&[7, 8, 7], 7, None, false),
        Some(0)
    );
}

/// START-4: the startup item is loaded, not running. It gets the marker,
/// but the viewport belongs to the startup centering — never to this
/// callback, which fires before the target view even exists.
#[test]
fn start_4_session_restore_marks_without_moving_the_viewport() {
    assert_eq!(
        reveal_policy(CurrentTrackChange::SessionRestore, false),
        TrackRevealPolicy::MarkerOnly
    );
    assert_eq!(
        reveal_policy(CurrentTrackChange::SessionRestore, true),
        TrackRevealPolicy::MarkerOnly
    );
}
