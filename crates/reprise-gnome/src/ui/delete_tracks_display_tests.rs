//! Timer-based BROWSE-11 regressions for the post-delete model delta.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::gio::prelude::*;
use gtk4::prelude::*;

use super::{capture_catalog_delete_reload, reload_after_catalog_delete};
use crate::ui::track_list::track_list_context_menu::current_selection_positions;
use crate::ui::track_list::TrackList;

const ROWS: i64 = 140;
const FIRST_DELETED: u32 = 90;
const SAMPLE_INTERVAL: Duration = Duration::from_millis(8);
const SETTLE: Duration = Duration::from_millis(450);
const VISIBLE_JUMP_PX: f64 = 80.0;

fn mapped_track_list() -> (Rc<TrackList>, gtk4::Window) {
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=ROWS {
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
        crate::ui::track_list::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    ));
    *track_list.shared.sort.borrow_mut() = crate::ui::track_list_sort::SortState {
        field: "title".into(),
        dir: "asc".into(),
    };
    crate::ui::track_list::reload(&track_list.shared);
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
    (track_list, window)
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn browse_11_catalog_delete_is_a_remove_only_delta_with_stable_focus_and_viewport() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window) = mapped_track_list();
    let shared = &track_list.shared;

    shared
        .column_view
        .scroll_to(FIRST_DELETED, None, gtk4::ListScrollFlags::FOCUS, None);
    shared.selection.select_range(FIRST_DELETED, 2, true);
    shared.column_view.grab_focus();
    let adjustment = shared.column_view.vadjustment().unwrap();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        adjustment.value() > VISIBLE_JUMP_PX
    });
    let before_scroll = adjustment.value();
    let reload_state = capture_catalog_delete_reload(shared);
    let before_ids = shared.current_view_ids();
    let removed_ids = before_ids[FIRST_DELETED as usize..FIRST_DELETED as usize + 2].to_vec();

    let changes = Rc::new(RefCell::new(Vec::new()));
    let recorded_changes = changes.clone();
    shared
        .model
        .connect_items_changed(move |_, position, removed, added| {
            recorded_changes
                .borrow_mut()
                .push((position, removed, added));
        });
    let samples = Rc::new(RefCell::new(Vec::new()));
    let recorded_samples = samples.clone();
    let sampled_adjustment = adjustment.clone();
    let timer = gtk4::glib::timeout_add_local(SAMPLE_INTERVAL, move || {
        recorded_samples
            .borrow_mut()
            .push(sampled_adjustment.value());
        gtk4::glib::ControlFlow::Continue
    });

    let conn = crate::test_db::connection(&shared.conn);
    conn.execute(
        "DELETE FROM tracks WHERE id IN (?1, ?2)",
        (&removed_ids[0], &removed_ids[1]),
    )
    .unwrap();
    reload_after_catalog_delete(shared, &removed_ids, reload_state);
    crate::ui::test_settle::settle_for(SETTLE);
    timer.remove();

    assert_eq!(*changes.borrow(), vec![(FIRST_DELETED, 2, 0)]);
    assert_eq!(current_selection_positions(shared), vec![FIRST_DELETED]);
    let focus = gtk4::prelude::RootExt::focus(&window)
        .expect("the delete handover must leave keyboard focus in the table");
    assert!(
        focus == shared.column_view.clone().upcast::<gtk4::Widget>()
            || focus.is_ancestor(&shared.column_view),
        "focus escaped the track table to {}",
        focus.type_().name()
    );
    let seen = samples.borrow();
    assert!(
        !seen.is_empty(),
        "the timer must sample the delete handover"
    );
    let lowest = seen.iter().copied().fold(f64::MAX, f64::min);
    assert!(
        lowest > before_scroll - VISIBLE_JUMP_PX,
        "delete exposed a jump toward the top: before={before_scroll}, lowest={lowest}"
    );
    assert_eq!(shared.model.n_items(), ROWS as u32 - 2);

    window.close();
}
