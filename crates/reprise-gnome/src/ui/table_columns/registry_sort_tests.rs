use std::rc::Rc;

use gtk4::prelude::*;
use reprise_view::columns::{layout, ColumnId, Layout, ReleaseColumn};

use super::super::single_sort_indicator;
use super::*;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn sortable_columns_follow_the_saved_layout_not_the_responsive_fold() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let view = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);
    let sortable = |key: ColumnId| {
        let column = gtk4::ColumnViewColumn::builder()
            .title(key.as_str())
            .id(key.as_str())
            .build();
        column.set_sorter(Some(&gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal)));
        view.append_column(&column);
        column
    };
    let title = sortable(ColumnId::Title);
    let artist = sortable(ColumnId::Artist);
    let registry = ColumnRegistry::new(
        &view,
        Rc::new(crate::test_db::open().unwrap()),
        TableKeys {
            layout: "test.sortable-layout.layout",
            widths: "test.sortable-layout.widths",
        },
        vec![
            (ColumnId::Title, title.clone()),
            (ColumnId::Artist, artist.clone()),
        ],
    );
    registry.configure(
        Rc::new(|key| key.as_str().to_owned()),
        Rc::new(|_| 120),
        ColumnId::Title,
    );

    let hidden_artist = layout::set_visible(&registry.layout(), ColumnId::Artist, false);
    registry.apply(&hidden_artist);
    assert_eq!(
        EditorModel::sortable_columns(registry.as_ref())
            .into_iter()
            .map(|column| column.id)
            .collect::<Vec<_>>(),
        [ColumnId::Title.as_str()]
    );

    registry.apply(&Layout::<ColumnId>::default());
    title.set_visible(false);
    assert_eq!(
        EditorModel::sortable_columns(registry.as_ref())
            .into_iter()
            .map(|column| column.id)
            .collect::<Vec<_>>(),
        [ColumnId::Title.as_str(), ColumnId::Artist.as_str()],
        "a responsive GTK visibility fold must not rewrite the user's saved choices"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn track_list_sorters_and_the_accepted_sort_field_whitelist_coincide() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let track_list = crate::ui::track_list::TrackList::new(
        Rc::new(crate::test_db::open().unwrap()),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        crate::ui::track_list::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let columns = track_list.shared.column_view.columns();
    let sorter_fields = (0..columns.n_items())
        .filter_map(|index| {
            let column = columns
                .item(index)
                .and_downcast::<gtk4::ColumnViewColumn>()?;
            column.sorter()?;
            column.id().map(|field| field.to_string())
        })
        .collect::<Vec<_>>();
    let accepted_fields = (0..columns.n_items())
        .filter_map(|index| {
            let column = columns
                .item(index)
                .and_downcast::<gtk4::ColumnViewColumn>()?;
            let field = column.id()?.to_string();
            ColumnId::from_sort_field(&field).map(|_| field)
        })
        .collect::<Vec<_>>();

    assert_eq!(
        sorter_fields, accepted_fields,
        "every sorter-bearing track column must be accepted by the observer, and every accepted field must carry a sorter"
    );
}

#[test]
fn hiding_primary_sort_chooses_first_visible_sortable_free_column() {
    let layout = layout::set_visible(
        &Layout::<ReleaseColumn>::default(),
        ReleaseColumn::Title,
        false,
    );

    assert_eq!(
        sort_fallback(&layout, Some(ReleaseColumn::Title), |_| true),
        SortFallback::Use(ReleaseColumn::Date)
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn style_13_hiding_the_sorted_column_keeps_a_visible_sort_indicator() {
    fn sortable_column(key: ColumnId) -> gtk4::ColumnViewColumn {
        let column = gtk4::ColumnViewColumn::builder()
            .title(key.as_str())
            .id(key.as_str())
            .build();
        column.set_sorter(Some(&gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal)));
        column
    }

    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let view = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);
    let title = sortable_column(ColumnId::Title);
    let artist = sortable_column(ColumnId::Artist);
    view.append_column(&title);
    view.append_column(&artist);
    let store = gtk4::gio::ListStore::new::<gtk4::glib::Object>();
    let sorted = gtk4::SortListModel::new(Some(store), view.sorter());
    view.set_model(Some(&gtk4::NoSelection::new(Some(sorted))));
    single_sort_indicator::mark(&view);
    let registry = ColumnRegistry::new(
        &view,
        Rc::new(crate::test_db::open().unwrap()),
        TableKeys {
            layout: reprise_core::library::settings::COLUMN_LAYOUT_KEY,
            widths: reprise_core::library::settings::COLUMN_WIDTHS_KEY,
        },
        vec![
            (ColumnId::Title, title.clone()),
            (ColumnId::Artist, artist.clone()),
        ],
    );
    registry.apply(&registry.layout());
    view.sort_by_column(Some(&artist), gtk4::SortType::Descending);
    let window = gtk4::Window::builder()
        .default_width(500)
        .default_height(160)
        .child(&view)
        .build();
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    EditorModel::set_visible(registry.as_ref(), ColumnId::Artist.as_str(), false);
    while gtk4::glib::MainContext::default().iteration(false) {}

    let sorter = view
        .sorter()
        .and_downcast::<gtk4::ColumnViewSorter>()
        .expect("ColumnView owns its aggregate sorter");
    assert_eq!(sorter.primary_sort_column().as_ref(), Some(&title));
    assert_eq!(sorter.primary_sort_order(), gtk4::SortType::Ascending);
    assert_eq!(
        single_sort_indicator::count_primary_indicators(view.upcast_ref()),
        1,
        "the visible fallback sort must retain exactly one header indicator"
    );
    window.close();
}
