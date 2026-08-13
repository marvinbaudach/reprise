use gtk4::prelude::*;

use super::{row_widget_counts, RowWidgetCounts};
use crate::ui::track_list::row_loss_watchdog_state::{TickInput, WatchdogState};

fn collect_row_widgets(widget: &gtk4::Widget, rows: &mut Vec<gtk4::Widget>) {
    if widget.css_name() == "row" {
        rows.push(widget.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        collect_row_widgets(&current, rows);
        child = current.next_sibling();
    }
}

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
        row_widget_counts(&view).allocated > 0
    });

    assert!(row_widget_counts(&view).allocated > 0);
    let mut state = WatchdogState::default();
    for tick_index in 0..5 {
        let rows = row_widget_counts(&view);
        let decision = state.tick(
            TickInput {
                suspicious: rows.allocated == 0,
                row_widgets_present: rows.present,
                row_widgets_allocated: rows.allocated,
                now_ms: tick_index * 2_000,
            },
            true,
        );
        assert!(!decision.confirmed);
    }
    let started = std::time::Instant::now();
    for _ in 0..1_000 {
        assert_eq!(
            row_widget_counts(&view),
            RowWidgetCounts {
                present: 1,
                allocated: 1,
            }
        );
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

    assert_eq!(row_widget_counts(&view), RowWidgetCounts::default());
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn populated_but_unallocated_rows_are_reported_separately() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let model = gtk4::gio::ListStore::new::<gtk4::glib::BoxedAnyObject>();
    for index in 0..206 {
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
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        let _ = view.measure(gtk4::Orientation::Vertical, 600);
        let mut rows = Vec::new();
        collect_row_widgets(view.upcast_ref(), &mut rows);
        rows.len() >= 200
    });

    let mut rows = Vec::new();
    collect_row_widgets(view.upcast_ref(), &mut rows);
    assert!(
        rows.len() >= 200,
        "the fixture created too few row widgets: {}",
        rows.len()
    );
    assert!(rows.iter().all(|row| row.css_name() == "row"));
    assert!(rows
        .iter()
        .all(|row| row.type_().name().contains("ColumnViewRow")));
    assert!(
        rows.iter().all(|row| row.height() == 0),
        "detached row heights: {:?}",
        rows.iter().map(gtk4::Widget::height).collect::<Vec<_>>()
    );
    let expected = RowWidgetCounts {
        present: rows.len(),
        allocated: 0,
    };
    assert_eq!(row_widget_counts(&view), expected);
    let started = std::time::Instant::now();
    for _ in 0..1_000 {
        assert_eq!(row_widget_counts(&view), expected);
    }
    let per_tick = started.elapsed() / 1_000;
    eprintln!("unallocated-row probe: {per_tick:?}");
    assert!(per_tick < std::time::Duration::from_millis(1));
    window.close();
}
