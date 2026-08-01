//! TAG-1 display regressions for the Tag Editor's save refresh.
//!
//! `track_list_reload`'s own display test asserts the *end* state: once
//! everything has settled, the viewport sits where it was. That is not what
//! the user reported — they see the table snap to the very top and come back
//! a moment later, which an end-state assertion cannot catch.
//!
//! These tests therefore sample the vertical adjustment once per rendered
//! frame (a tick callback runs right before each frame is drawn), so a
//! transient top-of-table state that survives even one frame is visible to
//! the assertion. A dip that happens and is undone within a single main-loop
//! turn is never painted and is deliberately not failed here.

use std::cell::RefCell;
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
/// Long enough for both save reloads, their scroll restores, and ~30 drawn
/// frames over them.
const SETTLE: Duration = Duration::from_millis(500);

struct Fixture {
    track_list: TrackList,
    window: adw::Window,
    adjustment: gtk4::Adjustment,
    /// Stands in for the open Tag Editor dialog: something else in the window
    /// that can hold keyboard focus while the table does not.
    elsewhere: gtk4::Button,
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

    let track_list = TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        super::super::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );
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
    }
}

/// Records the adjustment value of every frame drawn from now on. The tick
/// callback also keeps the frame clock running, so the samples stay dense
/// even while nothing else asks for a redraw.
fn record_painted_frames(fixture: &Fixture) -> Rc<RefCell<Vec<f64>>> {
    let frames = Rc::new(RefCell::new(Vec::<f64>::new()));
    let collected = frames.clone();
    let adjustment = fixture.adjustment.clone();
    fixture
        .track_list
        .shared
        .column_view
        .add_tick_callback(move |_, _| {
            collected.borrow_mut().push(adjustment.value());
            gtk4::glib::ControlFlow::Continue
        });
    frames
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

fn assert_no_painted_jump(frames: &Rc<RefCell<Vec<f64>>>, reference: f64, what: &str) {
    let painted = frames.borrow().clone();
    assert!(
        !painted.is_empty(),
        "precondition: the frame clock must have drawn during {what}"
    );
    let lowest = painted.iter().copied().fold(f64::MAX, f64::min);
    assert!(
        lowest > reference - VISIBLE_JUMP_PX,
        "a painted frame showed the table jumped to the top during {what}: \
         reference={reference}, lowest painted={lowest}"
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

    let frames = record_painted_frames(&fixture);
    crate::ui::test_settle::settle_for(Duration::from_millis(120));
    frames.borrow_mut().clear();

    save_refresh(&fixture);
    crate::ui::test_settle::settle_for(SETTLE);

    assert_no_painted_jump(&frames, before, "the save refresh");
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
    let frames = record_painted_frames(&fixture);

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

    assert_no_painted_jump(&frames, before, "the dialog's focus restore");
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

    let frames = record_painted_frames(&fixture);
    // The dialog closes and focus returns to the library table.
    fixture.track_list.shared.column_view.grab_focus();
    crate::ui::test_settle::settle_for(SETTLE);

    assert_no_painted_jump(
        &frames,
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
