//! Does closing the row context menu move the library's viewport?
//!
//! The report: right-click a row well down the Music library, pick "Play
//! next", and the table twitches up and back. Every queue action the menu
//! offers shares one tail — the popover closes, and
//! `TransientFocusGuard::restore` hands keyboard focus back to the table —
//! so this samples that tail on its own, with no player wired up at all.
//!
//! Sampling is on a timer, not the frame clock: with nothing damaging the
//! widget GTK draws no frames, and an empty sample set would make the
//! assertion vacuously true (see `tag_mutation_refresh_display_tests`).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use libadwaita as adw;

use super::super::TrackList;

const ROWS: i64 = 300;
const ANCHOR_ROW: u32 = 200;
/// A restore may not move the viewport at all. The reported twitch was one
/// row height (32 px) — far under `REPRISE_DEBUG_SCROLL`'s 80 px jump-to-top
/// threshold and still plainly visible — so this tolerates only rounding.
const VISIBLE_JUMP_PX: f64 = 2.0;
/// How far down the table has to sit for a jump to have room to happen.
const SCROLLED_PX: f64 = 80.0;
const SAMPLE_INTERVAL: Duration = Duration::from_millis(8);
const SETTLE: Duration = Duration::from_millis(600);

struct Fixture {
    track_list: TrackList,
    window: adw::Window,
    adjustment: gtk4::Adjustment,
}

fn scrolled_library() -> Fixture {
    adw::init().unwrap();
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
    *track_list.shared.sort.borrow_mut() = crate::ui::track_list_sort::SortState {
        field: "title".into(),
        dir: "asc".into(),
    };
    super::super::track_list_reload::reload(&track_list.shared);
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
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

    track_list
        .shared
        .column_view
        .scroll_to(ANCHOR_ROW, None, gtk4::ListScrollFlags::FOCUS, None);
    track_list.shared.selection.select_item(ANCHOR_ROW, true);
    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        adjustment.value() > 0.0
    });
    track_list.shared.column_view.grab_focus();
    crate::ui::test_settle::settle_for(Duration::from_millis(150));
    Fixture {
        track_list,
        window,
        adjustment,
    }
}

fn record_viewport(adjustment: &gtk4::Adjustment) -> Rc<RefCell<Vec<f64>>> {
    let samples = Rc::new(RefCell::new(Vec::<f64>::new()));
    let collected = samples.clone();
    let adjustment = adjustment.clone();
    gtk4::glib::timeout_add_local(SAMPLE_INTERVAL, move || {
        collected.borrow_mut().push(adjustment.value());
        gtk4::glib::ControlFlow::Continue
    });
    samples
}

/// The popover `show_context_menu` parented onto the table.
fn open_popover(fixture: &Fixture) -> gtk4::Popover {
    let column_view = &fixture.track_list.shared.column_view;
    super::show_context_menu(
        &fixture.track_list.shared,
        column_view,
        ANCHOR_ROW,
        column_view.upcast_ref(),
        10.0,
        100.0,
    );
    crate::ui::test_settle::settle_for(Duration::from_millis(200));
    let mut child = column_view.first_child();
    while let Some(current) = child {
        if let Some(popover) = current.downcast_ref::<gtk4::Popover>() {
            return popover.clone();
        }
        child = current.next_sibling();
    }
    panic!("the context menu did not parent a popover onto the table");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn closing_the_row_context_menu_leaves_the_library_viewport_where_it_was() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let fixture = scrolled_library();
    let popover = open_popover(&fixture);
    let reference = fixture.adjustment.value();
    assert!(
        reference > SCROLLED_PX,
        "precondition: the table must be scrolled far enough for a jump to be visible"
    );

    let samples = record_viewport(&fixture.adjustment);
    popover.popdown();
    crate::ui::test_settle::settle_for(SETTLE);

    assert_still(&samples, reference, "closing the context menu");
    fixture.window.close();
}

/// The reported gesture end to end: right-click a row, pick "Play next".
/// The action fires while the popover is still parented (that is what
/// `popover_lifecycle` guarantees), then the popover closes and focus comes
/// back — all inside one gesture, which is what the user sees twitch.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn choosing_play_next_from_the_row_context_menu_leaves_the_viewport_where_it_was() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let fixture = scrolled_library();
    let popover = open_popover(&fixture);
    let reference = fixture.adjustment.value();
    assert!(reference > SCROLLED_PX, "precondition: scrolled far down");

    let samples = record_viewport(&fixture.adjustment);
    fixture
        .track_list
        .shared
        .column_view
        .activate_action("tracklist.play-next", None)
        .expect("the tracklist action group must carry play-next");
    popover.popdown();
    crate::ui::test_settle::settle_for(SETTLE);

    assert_still(&samples, reference, "choosing Play next");
    fixture.window.close();
}

fn assert_still(samples: &Rc<RefCell<Vec<f64>>>, reference: f64, what: &str) {
    let seen = samples.borrow().clone();
    assert!(
        !seen.is_empty(),
        "precondition: the viewport must have been sampled while {what}"
    );
    let lowest = seen.iter().copied().fold(f64::INFINITY, f64::min);
    println!(
        "[{what}] reference {reference:.1}, lowest {lowest:.1}, \
         drop {:.1} px over {} samples: {seen:?}",
        reference - lowest,
        seen.len()
    );
    assert!(
        reference - lowest < VISIBLE_JUMP_PX,
        "{what} jerked the table up by {:.0} px (from {reference:.0} to {lowest:.0})",
        reference - lowest
    );
}
