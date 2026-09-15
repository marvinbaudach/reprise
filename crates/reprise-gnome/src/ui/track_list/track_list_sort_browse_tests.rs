use super::*;
use crate::ui::track_list::TrackList;

fn column(view: &gtk4::ColumnView, field: &str) -> gtk4::ColumnViewColumn {
    let columns = view.columns();
    (0..columns.n_items())
        .find_map(|index| {
            columns
                .item(index)
                .and_downcast::<gtk4::ColumnViewColumn>()
                .filter(|column| column.id().as_deref() == Some(field))
        })
        .unwrap_or_else(|| panic!("missing {field} column"))
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn browse_15_a_smart_place_opens_in_its_own_order() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let db = Rc::new(crate::test_db::open().unwrap());
    let smart_lists = reprise_core::library::playlists::list_smart(&db).unwrap();
    let recently_played = smart_lists
        .iter()
        .find(|smart| smart.sort_field == "last_played_at")
        .unwrap();
    let top_rated = smart_lists
        .iter()
        .find(|smart| smart.sort_field == "rating")
        .unwrap();
    let track_list = TrackList::new(
        db,
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        super::super::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );

    track_list.set_source(ViewSource::Smart(recently_played.id));
    assert_eq!(track_list.shared.sort.borrow().field, "last_played_at");
    assert_eq!(track_list.shared.sort.borrow().dir, "desc");
    assert!(track_list
        .shared
        .column_view
        .sorter()
        .and_downcast::<gtk4::ColumnViewSorter>()
        .unwrap()
        .primary_sort_column()
        .is_none());

    sort_by_column(
        &track_list.shared.column_view,
        &column(&track_list.shared.column_view, "title"),
        gtk4::SortType::Ascending,
    );
    track_list.reload();
    assert_eq!(track_list.shared.sort.borrow().field, "title");

    track_list.set_source(ViewSource::Smart(top_rated.id));
    assert_eq!(track_list.shared.sort.borrow().field, "rating");
    assert_eq!(track_list.shared.sort.borrow().dir, "desc");
    assert_eq!(
        track_list
            .shared
            .column_view
            .sorter()
            .and_downcast::<gtk4::ColumnViewSorter>()
            .unwrap()
            .primary_sort_column()
            .and_then(|column| column.id())
            .as_deref(),
        Some("rating")
    );
}
