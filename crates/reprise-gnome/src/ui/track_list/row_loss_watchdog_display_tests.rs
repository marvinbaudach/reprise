use gtk4::prelude::*;

use super::realized_row_count;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn populated_column_view_has_a_realized_row() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let model = gtk4::gio::ListStore::new::<gtk4::glib::BoxedAnyObject>();
    for index in 0..20 {
        model.append(&gtk4::glib::BoxedAnyObject::new(format!("Track {index}")));
    }
    let selection = gtk4::NoSelection::new(Some(model));
    let view = gtk4::ColumnView::new(Some(selection));
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        item.downcast_ref::<gtk4::ListItem>()
            .unwrap()
            .set_child(Some(&gtk4::Label::new(None)));
    });
    view.append_column(&gtk4::ColumnViewColumn::new(Some("Title"), Some(factory)));
    let window = gtk4::Window::builder()
        .default_width(600)
        .default_height(400)
        .child(&view)
        .build();
    window.present();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        realized_row_count(&view) > 0
    });

    assert!(realized_row_count(&view) > 0);
    let started = std::time::Instant::now();
    for _ in 0..1_000 {
        assert_eq!(realized_row_count(&view), 1);
    }
    let per_tick = started.elapsed() / 1_000;
    eprintln!("healthy realised-row probe: {per_tick:?}");
    assert!(per_tick < std::time::Duration::from_millis(1));
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn model_less_column_view_has_no_realized_rows() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let view = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);
    let window = gtk4::Window::builder()
        .default_width(600)
        .default_height(400)
        .child(&view)
        .build();
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    assert_eq!(realized_row_count(&view), 0);
    window.close();
}
