//! TAG-1 display regressions for the Tag Editor's save refresh.
//!
//! `track_list_reload`'s own display test asserts the *end* state: once
//! everything has settled, the viewport sits where it was. That is not what
//! the user reported — they see the table snap to the very top and come back
//! a moment later, which an end-state assertion cannot catch.
//!
//! These tests therefore sample the vertical adjustment on a timer while the
//! save runs, so a top-of-table state that lasts longer than one sample
//! interval fails the assertion. A dip that happens and is undone inside a
//! single main-loop turn is never on screen and is deliberately not failed.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use libadwaita as adw;

use super::super::track_list_reload::capture_reload_anchor;
use super::super::TrackList;
use super::refresh_after_tag_mutation_with_anchor;

const ROWS: i64 = 300;
const ANCHOR_ROW: u32 = 200;
/// A jump smaller than this is row-snapping noise, not the reported
/// "flies to the top of the library" — the same threshold
/// `track_list_builder`'s `REPRISE_DEBUG_SCROLL` diagnostic uses.
const VISIBLE_JUMP_PX: f64 = 80.0;
/// Long enough for the deferred save reload, its scroll restore, the dialog's
/// focus handover, and dozens of viewport samples over them.
const SETTLE: Duration = Duration::from_millis(500);

/// The edited value the test looks for on screen. Artist is realised in the
/// default 900 px layout; Album has a visible header there but GTK does not
/// realise its horizontally clipped row cells, so inspecting descendants
/// cannot use it as rendering evidence.
const EDITED_ARTIST: &str = "Edited Artist";
const RESORTED_TITLE: &str = "Target 01";

struct Fixture {
    track_list: TrackList,
    window: adw::Window,
    adjustment: gtk4::Adjustment,
    /// Stands in for the open Tag Editor dialog: something else in the window
    /// that can hold keyboard focus while the table does not.
    elsewhere: gtk4::Button,
    /// How often the view re-ran its query — one per `run_query`, which is one
    /// `items_changed(0, old, new)` and one full window re-read each.
    queries: Rc<Cell<usize>>,
}

/// A mapped library table of `ROWS` synthetic tracks, scrolled to
/// `ANCHOR_ROW`, with that row selected and holding keyboard focus — the
/// state the user is in when they open the Tag Editor on a row.
fn scrolled_library() -> Fixture {
    adw::init().unwrap();
    // An `AdwDialog` only behaves like the real Tag Editor — sliding in over
    // the window and handing keyboard focus back on close — when animations
    // are on. Xvfb inherits whatever the settings default to, so pin it.
    if let Some(settings) = gtk4::Settings::default() {
        settings.set_gtk_enable_animations(true);
    }
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=ROWS {
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, genre, added_at) \
             VALUES (?1, ?2, ?3, 'Synthetic Artist', 'Post-Hardcore', 0)",
            (
                id,
                format!("/synthetic/{id:03}.flac"),
                format!("Track {id:03}"),
            ),
        )
        .unwrap();
    }
    tx.commit().unwrap();

    let queries = Rc::new(Cell::new(0usize));
    let counted = queries.clone();
    let track_list = TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        move |_, _, _, _| counted.set(counted.get() + 1),
        super::super::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );
    // Pin a title sort whose order the Artist edit cannot change; otherwise
    // the edited row correctly moves within the production default Artist
    // sort and the "written value reached the visible cells" assertion tests
    // sorting instead of refresh/rebinding.
    *track_list.shared.sort.borrow_mut() = crate::ui::track_list_sort::SortState {
        field: "title".into(),
        dir: "asc".into(),
    };
    super::super::track_list_reload::reload(&track_list.shared);
    let elsewhere = gtk4::Button::with_label("Elsewhere");
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.append(&elsewhere);
    let table = track_list.widget();
    table.set_vexpand(true);
    content.append(table);
    let window = adw::Window::builder()
        .default_width(900)
        .default_height(320)
        .content(&content)
        .build();
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    // `FOCUS` mirrors a click on the row: it both scrolls there and makes
    // that row the table's focus row.
    track_list
        .shared
        .column_view
        .scroll_to(ANCHOR_ROW, None, gtk4::ListScrollFlags::FOCUS, None);
    track_list.shared.selection.select_item(ANCHOR_ROW, true);
    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        adjustment.value() > 0.0
    });
    // Keyboard focus has to sit *inside* the table, on the anchored row's
    // widget — that is what the Tag Editor captures when it opens, and the
    // recycling of exactly that widget is what these tests are about.
    track_list.shared.column_view.grab_focus();
    crate::ui::test_settle::settle_for(Duration::from_millis(100));
    Fixture {
        track_list,
        window,
        adjustment,
        elsewhere,
        queries,
    }
}

/// Every `GtkLabel` text currently realised under the table — what the user
/// can actually read on screen, as opposed to what the model holds.
fn visible_labels(fixture: &Fixture) -> Vec<String> {
    fn collect(widget: &gtk4::Widget, out: &mut Vec<String>) {
        if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
            out.push(label.text().to_string());
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            collect(&current, out);
            child = current.next_sibling();
        }
    }
    let mut out = Vec::new();
    collect(fixture.track_list.shared.column_view.upcast_ref(), &mut out);
    out
}

fn viewport_labels(fixture: &Fixture) -> Vec<String> {
    fn collect(widget: &gtk4::Widget, viewport: &gtk4::ColumnView, out: &mut Vec<String>) {
        if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
            let viewport_width = viewport.width() as f32;
            let viewport_height = viewport.height() as f32;
            if widget.compute_bounds(viewport).is_some_and(|bounds| {
                bounds.x() < viewport_width
                    && bounds.x() + bounds.width() > 0.0
                    && bounds.y() < viewport_height
                    && bounds.y() + bounds.height() > 0.0
            }) {
                out.push(label.text().to_string());
            }
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            collect(&current, viewport, out);
            child = current.next_sibling();
        }
    }
    let viewport = &fixture.track_list.shared.column_view;
    let mut out = Vec::new();
    collect(viewport.upcast_ref(), viewport, &mut out);
    out
}

/// Writes the new artist the way a successful tag write does: into the
/// database, behind the view's back. Only a re-query (or a cell patch) can
/// bring it on screen.
fn write_artist_to_db(fixture: &Fixture) {
    let conn = crate::test_db::connection(&fixture.track_list.shared.conn);
    let track = fixture
        .track_list
        .shared
        .model
        .track_at(ANCHOR_ROW)
        .unwrap();
    conn.execute(
        "UPDATE tracks SET artist = ?1 WHERE id = ?2",
        (EDITED_ARTIST, track.id),
    )
    .unwrap();
}

/// How often the viewport is sampled. Roughly half a 60 Hz frame: a position
/// the viewport holds for longer than this had a frame drawn on it.
const SAMPLE_INTERVAL: Duration = Duration::from_millis(8);

/// Samples the viewport position every [`SAMPLE_INTERVAL`] from now on.
///
/// Sampling on the frame clock (`add_tick_callback`) would be the more direct
/// question — "was this ever painted?" — but it is not a reliable instrument
/// here: with nothing damaging the widget GTK draws no frames at all, and an
/// empty sample set makes the assertion below vacuously true rather than
/// failing loudly. A timer always fires, so a jump that survives longer than
/// one sample interval is caught whether or not GTK happened to redraw.
fn record_viewport(fixture: &Fixture) -> Rc<RefCell<Vec<f64>>> {
    let samples = Rc::new(RefCell::new(Vec::<f64>::new()));
    let collected = samples.clone();
    let adjustment = fixture.adjustment.clone();
    gtk4::glib::timeout_add_local(SAMPLE_INTERVAL, move || {
        collected.borrow_mut().push(adjustment.value());
        gtk4::glib::ControlFlow::Continue
    });
    samples
}

/// Runs the same refresh a successful tag write triggers, with the anchor
/// captured before the dialog opened (`finish_apply`'s `save_anchor`).
fn save_refresh(fixture: &Fixture) {
    let written = fixture
        .track_list
        .shared
        .model
        .track_at(ANCHOR_ROW)
        .unwrap();
    let mut anchor = capture_reload_anchor(&fixture.track_list.shared);
    anchor.selected_ids = vec![written.id];
    refresh_after_tag_mutation_with_anchor(
        &fixture.track_list.shared,
        &[written.id],
        &[PathBuf::from(&written.path)],
        anchor,
    );
}

fn year_resorting_library() -> (Fixture, Vec<i64>) {
    adw::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    let mut id = 1_i64;
    for position in 0..150 {
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, album, year, track_no, added_at) \
             VALUES (?1, ?2, ?3, 'Alpha Artist', 'Earlier', 1980, ?4, 0)",
            (
                id,
                format!("/synthetic/alpha-{position:03}.flac"),
                format!("Alpha {position:03}"),
                position + 1,
            ),
        )
        .unwrap();
        id += 1;
    }
    for track_no in 1..=13 {
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, album, year, track_no, added_at) \
             VALUES (?1, ?2, ?3, 'Zulu Artist', 'Anchor Album', NULL, ?4, 0)",
            (
                id,
                format!("/synthetic/anchor-{track_no:02}.flac"),
                format!("Anchor {track_no:02}"),
                track_no,
            ),
        )
        .unwrap();
        id += 1;
    }
    let mut edited_ids = Vec::new();
    for track_no in 1..=13 {
        edited_ids.push(id);
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, album, year, track_no, added_at) \
             VALUES (?1, ?2, ?3, 'Zulu Artist', 'Target Album', NULL, ?4, 0)",
            (
                id,
                format!("/synthetic/target-{track_no:02}.flac"),
                format!("Target {track_no:02}"),
                track_no,
            ),
        )
        .unwrap();
        id += 1;
    }
    for position in 0..40 {
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, album, year, track_no, added_at) \
             VALUES (?1, ?2, ?3, 'Zulu Artist', ?4, ?5, 1, 0)",
            (
                id,
                format!("/synthetic/later-{position:03}.flac"),
                format!("Later {position:03}"),
                format!("Later {position:03}"),
                1990 + position,
            ),
        )
        .unwrap();
        id += 1;
    }
    tx.commit().unwrap();

    let track_list = TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        super::super::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );
    *track_list.shared.sort.borrow_mut() = crate::ui::track_list_sort::SortState {
        field: "artist".into(),
        dir: "asc".into(),
    };
    super::super::track_list_reload::reload(&track_list.shared);
    let elsewhere = gtk4::Button::with_label("Elsewhere");
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.append(&elsewhere);
    let table = track_list.widget();
    table.set_vexpand(true);
    content.append(table);
    let window = adw::Window::builder()
        .default_width(900)
        .default_height(320)
        .content(&content)
        .build();
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        adjustment.upper() > adjustment.page_size()
    });
    let row_height = adjustment.upper() / f64::from(track_list.shared.model.n_items());
    adjustment.set_value(160.0 * row_height);
    let fixture = Fixture {
        track_list,
        window,
        adjustment,
        elsewhere,
        queries: Rc::new(Cell::new(0)),
    };
    crate::ui::test_settle::settle_for(Duration::from_millis(100));
    (fixture, edited_ids)
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn tag_1_year_save_keeps_the_edited_album_inside_the_viewport_after_resort() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let (fixture, edited_ids) = year_resorting_library();
    assert!(viewport_labels(&fixture)
        .iter()
        .any(|label| label == RESORTED_TITLE));

    let old_ids = fixture.track_list.shared.current_view_ids();
    let anchor = capture_reload_anchor(&fixture.track_list.shared);
    let writes = edited_ids
        .iter()
        .map(|edited_id| reprise_core::library::tag_edit::TrackWrite {
            id: *edited_id,
            path: PathBuf::from(format!("/synthetic/target-{edited_id}.flac")),
            patch: reprise_core::library::tag_edit::TrackEditPatch {
                tags: reprise_core::library::tag_edit::TagPatch {
                    year: Some(Some(2099)),
                    ..Default::default()
                },
                ..Default::default()
            },
        })
        .collect::<Vec<_>>();
    let row_height = fixture.adjustment.upper() / old_ids.len() as f64;
    let anchor = crate::ui::tag_edit::tag_reload_anchor::post_save_reload_anchor(
        anchor,
        &edited_ids,
        &writes,
        "artist",
        &old_ids,
        row_height,
    );
    let anchor_id = anchor.anchor.map(|(track_id, _)| track_id).unwrap();
    assert_eq!(anchor_id, edited_ids[0]);

    let conn = crate::test_db::connection(&fixture.track_list.shared.conn);
    for edited_id in &edited_ids {
        conn.execute("UPDATE tracks SET year = 2099 WHERE id = ?1", [edited_id])
            .unwrap();
    }
    refresh_after_tag_mutation_with_anchor(&fixture.track_list.shared, &edited_ids, &[], anchor);
    crate::ui::test_settle::settle_for(SETTLE);

    assert!(
        viewport_labels(&fixture)
            .iter()
            .any(|label| label == RESORTED_TITLE),
        "the edited album moved out of the viewport after its year changed"
    );
    fixture.window.close();
}

fn assert_no_visible_jump(samples: &Rc<RefCell<Vec<f64>>>, reference: f64, what: &str) {
    let seen = samples.borrow().clone();
    assert!(
        !seen.is_empty(),
        "precondition: the viewport must have been sampled during {what}"
    );
    let lowest = seen.iter().copied().fold(f64::MAX, f64::min);
    assert!(
        lowest > reference - VISIBLE_JUMP_PX,
        "the table stood jumped to the top during {what}: \
         reference={reference}, lowest sampled={lowest} over {} samples",
        seen.len()
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn tag_1_tag_save_refresh_paints_no_frame_at_the_table_top() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let fixture = scrolled_library();
    let before = fixture.adjustment.value();
    assert!(
        before > VISIBLE_JUMP_PX,
        "precondition: the list must be scrolled well away from the top, got {before}"
    );

    let samples = record_viewport(&fixture);
    crate::ui::test_settle::settle_for(Duration::from_millis(120));
    samples.borrow_mut().clear();

    // A save that changes nothing gives GTK nothing to redraw, and the
    // assertion below would then pass on an empty sample set. Write the tag
    // the real save writes.
    write_artist_to_db(&fixture);
    save_refresh(&fixture);
    crate::ui::test_settle::settle_for(SETTLE);

    assert_no_visible_jump(&samples, before, "the save refresh");
    fixture.window.close();
}

/// The whole point of the save refresh: the edited value has to become
/// visible. `TrackListModel` windows its rows from SQL and caches them, so a
/// row already on screen keeps showing the pre-edit tags until something
/// re-reads it.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn tag_1_save_refresh_shows_the_written_tag_on_screen() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let fixture = scrolled_library();
    assert!(
        !visible_labels(&fixture).iter().any(|t| t == EDITED_ARTIST),
        "precondition: the new artist must not be on screen before the save"
    );

    write_artist_to_db(&fixture);
    save_refresh(&fixture);
    crate::ui::test_settle::settle_for(SETTLE);

    let labels = visible_labels(&fixture);
    assert!(
        labels.iter().any(|t| t == EDITED_ARTIST),
        "the saved artist never reached the visible cells; on screen: {:?}",
        labels
            .iter()
            .filter(|t| !t.is_empty())
            .take(40)
            .collect::<Vec<_>>()
    );
    fixture.window.close();
}

/// One save is one re-query. A second one costs a full sorted window read
/// plus another `items_changed(0, old, new)` — the signal every scroll and
/// selection restore in this module then has to undo — for a result the first
/// one already produced.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn tag_1_save_refresh_requeries_the_view_once() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let fixture = scrolled_library();

    write_artist_to_db(&fixture);
    fixture.queries.set(0);
    save_refresh(&fixture);
    crate::ui::test_settle::settle_for(SETTLE);

    assert_eq!(
        fixture.queries.get(),
        1,
        "the save re-ran the view's query {} times",
        fixture.queries.get()
    );
    fixture.window.close();
}

/// The Tag Editor restores keyboard focus through
/// `TransientFocusGuard::capture`, which remembers *the widget* that had
/// focus when the dialog opened. Opened from the track table, that widget is
/// a `GtkColumnView` row — and those are recycled: after the save's
/// `items_changed(0, old, new)` the very same widget is bound to a different
/// row. Restoring focus onto it therefore scrolls wherever it now lives.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn tag_1_restoring_dialog_focus_after_a_save_keeps_the_viewport() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let fixture = scrolled_library();
    let before = fixture.adjustment.value();
    assert!(
        before > VISIBLE_JUMP_PX,
        "precondition: the list must be scrolled well away from the top, got {before}"
    );

    // Opening the editor captures whatever the table had focused — the row
    // the user clicked. Without that, this test would capture the window
    // itself and prove nothing.
    let focused = gtk4::prelude::GtkWindowExt::focus(&fixture.window)
        .expect("precondition: something in the window must hold focus");
    assert!(
        focused.is_ancestor(&fixture.track_list.shared.column_view),
        "precondition: focus must sit on a row inside the table, not on {}",
        focused.type_()
    );
    let guard = crate::ui::transient_focus::TransientFocusGuard::capture(&fixture.window);
    let samples = record_viewport(&fixture);

    save_refresh(&fixture);
    crate::ui::test_settle::settle_for(SETTLE);
    let restored = fixture.adjustment.value();
    assert!(
        (restored - before).abs() < VISIBLE_JUMP_PX,
        "precondition: the save refresh itself must have put the viewport back, \
         before={before}, restored={restored}"
    );

    // The dialog finished closing: focus goes back to the captured widget.
    guard.restore();
    crate::ui::test_settle::settle_for(SETTLE);

    assert_no_visible_jump(&samples, before, "the dialog's focus restore");
    assert!(
        (fixture.adjustment.value() - before).abs() < VISIBLE_JUMP_PX,
        "restoring the dialog's focus moved the viewport: before={before}, after={}",
        fixture.adjustment.value()
    );
    fixture.window.close();
}

/// The reported sequence: the dialog closes, GTK hands keyboard focus back to
/// the table, and the table scrolls to whichever row it now considers
/// focused. The save's `items_changed(0, old, new)` has meanwhile reset that
/// focus row to the top, because the scroll restore deliberately scrolls
/// without `ListScrollFlags::FOCUS`.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn tag_1_focus_returning_to_the_table_after_a_save_keeps_the_viewport() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let fixture = scrolled_library();
    let before = fixture.adjustment.value();
    assert!(
        before > VISIBLE_JUMP_PX,
        "precondition: the list must be scrolled well away from the top, got {before}"
    );

    // The dialog is open: it, not the table, owns keyboard focus.
    fixture.elsewhere.grab_focus();
    save_refresh(&fixture);
    crate::ui::test_settle::settle_for(SETTLE);
    let restored = fixture.adjustment.value();
    assert!(
        (restored - before).abs() < VISIBLE_JUMP_PX,
        "precondition: the save refresh itself must have put the viewport back, \
         before={before}, restored={restored}"
    );

    let samples = record_viewport(&fixture);
    // The dialog closes and focus returns to the library table.
    fixture.track_list.shared.column_view.grab_focus();
    crate::ui::test_settle::settle_for(SETTLE);

    assert_no_visible_jump(
        &samples,
        restored,
        "the focus handover after the dialog closed",
    );
    assert!(
        (fixture.adjustment.value() - restored).abs() < VISIBLE_JUMP_PX,
        "focus returning to the table moved the viewport: restored={restored}, after={}",
        fixture.adjustment.value()
    );
    fixture.window.close();
}
