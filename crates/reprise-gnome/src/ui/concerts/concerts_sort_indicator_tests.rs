use super::*;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn two_concert_sorts_leave_one_indicator() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let runtime = ConcertsRuntime::setup(&conn);
    let view = build_view(conn, &runtime);
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(500)
        .child(view.root())
        .build();
    window.present();
    crate::ui::source_context_surface::settle_layout();
    let artist = column_by_id(&view.shared.column_view, "artist");
    let city = column_by_id(&view.shared.column_view, "city");

    view.shared
        .column_view
        .sort_by_column(Some(&artist), gtk4::SortType::Ascending);
    view.shared
        .column_view
        .sort_by_column(Some(&city), gtk4::SortType::Ascending);
    crate::ui::source_context_surface::settle_layout();

    assert_eq!(
        crate::ui::table_columns::single_sort_indicator::count_primary_indicators(
            view.shared.column_view.upcast_ref(),
        ),
        1
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn losing_the_location_still_falls_back_to_the_date_sort() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let runtime = ConcertsRuntime::setup(&conn);
    let view = build_view(conn, &runtime);
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(500)
        .child(view.root())
        .build();
    window.present();
    crate::ui::source_context_surface::settle_layout();
    view.shared.location_columns.apply(true);

    view.shared
        .location_columns
        .sort_by_distance(gtk4::SortType::Descending);
    view.shared.location_columns.apply(false);
    crate::ui::source_context_surface::settle_layout();

    assert_eq!(
        view.shared.location_columns.primary_sort(),
        (Some("date".to_owned()), gtk4::SortType::Ascending)
    );
    assert_eq!(
        crate::ui::table_columns::single_sort_indicator::count_primary_indicators(
            view.shared.column_view.upcast_ref(),
        ),
        1
    );
    window.close();
}
