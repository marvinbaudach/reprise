//! STYLE-13: a real pointer click on a music-table column header sorts it.
//!
//! The other sort tests call `sort_by_column` directly and so skip the whole
//! input path. This one drives the X server's own pointer through `xdotool`,
//! so the press and release travel GDK -> GTK event controllers -> the header
//! gesture exactly as a user's do, and it wires the table with the main
//! window's own `table_columns::install` rather than a hand-built copy, so a
//! wiring mistake there is caught too.

use std::rc::Rc;

use gtk4::prelude::*;

use crate::ui::test_x11_window::{
    wait_for_window_state, x11_window_id, xdotool, TestWindowManager,
};
use crate::ui::track_list::queue_sections::QueueViewModel;
use crate::ui::track_list::track_list_sort::SortState;
use crate::ui::track_list::TrackList;

/// Enough rows that the list page, not the empty-library page, is showing.
const SEEDED_TRACKS: i64 = 20;

/// The header title widget of the column carrying `field`, found the way the
/// header drag finds it: the header row is the `ColumnView`'s first child and
/// its children follow `view.columns()` one to one.
fn header_title_for(view: &gtk4::ColumnView, field: &str) -> gtk4::Widget {
    let columns = view.columns();
    let index = (0..columns.n_items())
        .position(|index| {
            columns
                .item(index)
                .and_downcast::<gtk4::ColumnViewColumn>()
                .is_some_and(|column| column.id().as_deref() == Some(field))
        })
        .unwrap_or_else(|| panic!("missing column {field}"));
    let mut child = view.first_child().and_then(|header| header.first_child());
    for _ in 0..index {
        child = child.and_then(|widget| widget.next_sibling());
    }
    child.unwrap_or_else(|| panic!("missing header title for {field}"))
}

fn click_header(window: &gtk4::Window, track_list: &TrackList, field: &str) {
    let title = header_title_for(&track_list.shared.column_view, field);
    let bounds = title
        .compute_bounds(window)
        .unwrap_or_else(|| panic!("header title for {field} has no bounds"));
    let x = (bounds.x() + bounds.width() / 2.0).round().to_string();
    let y = (bounds.y() + bounds.height() / 2.0).round().to_string();
    xdotool(&[
        "mousemove",
        "--window",
        &x11_window_id(window),
        &x,
        &y,
        "click",
        "1",
    ]);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn style_13_a_pointer_click_on_a_track_list_header_sorts_by_that_column() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let _window_manager = TestWindowManager::start();
    gtk4::init().unwrap();
    crate::ui::style::install_css_string_for_test(&crate::ui::style::app_css_for_test());
    let conn = crate::test_db::open().unwrap();
    for id in 1..=SEEDED_TRACKS {
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO tracks (id, path, title, artist, added_at) \
                 VALUES (?1, ?2, ?3, ?4, 0)",
                (
                    id,
                    format!("/header-click/{id:02}.flac"),
                    format!("Track {id:02}"),
                    format!("Artist {:02}", SEEDED_TRACKS - id),
                ),
            )
            .unwrap();
    }
    let track_list = Rc::new(TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    ));
    super::table_columns::install(&track_list);
    let window = gtk4::Window::builder()
        .default_width(1100)
        .default_height(500)
        .child(track_list.widget())
        .build();
    window.present();
    crate::ui::source_context_surface::settle_layout();

    // The list opens sorted by Artist, so Title is the click that must move
    // the sort: a header that ignored the pointer would leave it untouched.
    for (field, dir) in [("title", "asc"), ("title", "desc"), ("artist", "asc")] {
        click_header(&window, &track_list, field);
        wait_for_window_state(&format!("a {field} header click sorts {dir}"), || {
            *track_list.shared.sort.borrow()
                == SortState {
                    field: field.into(),
                    dir: dir.into(),
                }
        });
    }
    window.close();
}
