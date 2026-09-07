use std::rc::Rc;

use gtk4::prelude::*;

use super::super::track_list_model_change::{ModelChange, ModelChangeKind};
use super::super::TrackList;
use reprise_core::queries::BrowseFilter;
use reprise_core::view_source::ViewSource;

fn label_with_text(widget: &gtk4::Widget, expected: &str) -> Option<gtk4::Label> {
    if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
        if label.text() == expected {
            return Some(label.clone());
        }
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(label) = label_with_text(&current, expected) {
            return Some(label);
        }
        child = current.next_sibling();
    }
    None
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn narrowed_removal_then_marker_reapply_keeps_surviving_cell_text() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1_i64..=30 {
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, ?4, 0)",
            (
                id,
                format!("/synthetic/{id:03}.flac"),
                format!("Track {id:03}"),
                format!("Artist {id:03}"),
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
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(500)
        .child(track_list.widget())
        .build();
    window.present();
    crate::ui::test_settle::settle_for(std::time::Duration::from_millis(100));

    let expected = "Artist 006";
    let column_view: gtk4::Widget = track_list.shared.column_view.clone().upcast();
    let surviving_label = label_with_text(&column_view, expected)
        .expect("precondition: the surviving title cell must be realized");
    fixture_conn
        .execute("DELETE FROM tracks WHERE id = 1", [])
        .unwrap();
    let generation = track_list.shared.model.generation();
    track_list.shared.model.set_query_browsed_ai_changed(
        &ViewSource::Library,
        "title",
        "asc",
        "",
        &BrowseFilter::default(),
        &[],
        false,
        ModelChange {
            kind: ModelChangeKind::Span,
            position: 0,
            removed: 1,
            added: 0,
            before_total: 30,
            after_total: 29,
            generation,
        },
    );
    assert_eq!(
        surviving_label.text(),
        expected,
        "the narrowed removal itself must preserve the surviving cell"
    );
    assert!(
        surviving_label.is_mapped(),
        "surviving cell must remain bound"
    );

    track_list.shared.playing_track_id.set(Some(6));
    track_list.shared.reapply_now_playing_markers();

    assert_eq!(
        surviving_label.text(),
        expected,
        "marker reapply must resolve the ListItem's current position"
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ordinary_marker_reapply_does_not_rerender_text_cells() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    crate::test_db::connection(&conn)
        .execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (1, '/synthetic/one.flac', 'Track One', 'Artist One', 0)",
            [],
        )
        .unwrap();
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
    crate::ui::test_settle::settle_for(std::time::Duration::from_millis(100));

    let column_view: gtk4::Widget = track_list.shared.column_view.clone().upcast();
    let artist_label = label_with_text(&column_view, "Artist One")
        .expect("precondition: the artist cell must be realized");
    artist_label.set_text("render sentinel");
    track_list.shared.playing_track_id.set(Some(1));
    track_list.shared.reapply_now_playing_markers();

    assert!(artist_label.has_css_class("now-playing"));
    assert_eq!(
        artist_label.text(),
        "render sentinel",
        "an ordinary playback change must only toggle the marker class"
    );
    window.close();
}
