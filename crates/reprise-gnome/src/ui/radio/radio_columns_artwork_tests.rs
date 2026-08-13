//! Artwork-specific Radio column tests, split out for the source-file size gate.

use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::connectivity::Connectivity;
use reprise_core::radio::StationRow;

use super::radio_columns::{self, ConnectivitySource, LiveState};
use super::radio_live_cells::RadioLiveCells;
use super::radio_model::RadioObject;
use super::radio_presentation::RadioLiveState;

fn station() -> StationRow {
    StationRow {
        id: 1,
        uuid: None,
        name: "Station".into(),
        stream_url: "https://example.test/stream".into(),
        homepage: None,
        favicon_url: Some("https://images.test/radio-gate-transition.png".into()),
        genre: Some("Jazz".into()),
        codec: None,
        bitrate_kbps: Some(128),
        country_code: Some("DE".into()),
        votes: None,
        added_at: 1,
        removed_at: None,
    }
}

fn descendants_with_class(widget: &gtk4::Widget, class: &str) -> Vec<gtk4::Widget> {
    let mut found = widget
        .has_css_class(class)
        .then(|| widget.clone())
        .into_iter()
        .collect::<Vec<_>>();
    let mut child = widget.first_child();
    while let Some(current) = child {
        found.extend(descendants_with_class(&current, class));
        child = current.next_sibling();
    }
    found
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn artwork_permission_rebinds_visible_radio_images_without_resetting_the_model() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    reprise_core::online_sources::set_enabled(&conn, true).unwrap();
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::ARTWORK_MODULE, false)
        .unwrap();
    let images_allowed = radio_columns::images_allowed_source(&conn);
    assert!(!images_allowed());
    let store = gtk4::gio::ListStore::new::<RadioObject>();
    store.append(&RadioObject::new(station()));
    let selection = gtk4::SingleSelection::new(Some(store));
    let view = gtk4::ColumnView::new(Some(selection.clone()));
    let live: LiveState = Rc::new(RadioLiveState::default);
    let connectivity: ConnectivitySource = Rc::new(|| Connectivity::Online);
    let live_cells = Rc::new(RadioLiveCells::default());
    let artwork_cells = Rc::new(RadioLiveCells::default());
    let query: crate::ui::search_highlight::QuerySource = Rc::new(String::new);
    radio_columns::append_columns(
        &view,
        &live,
        &connectivity,
        &images_allowed,
        &live_cells,
        &artwork_cells,
        &query,
    );
    let window = gtk4::Window::new();
    window.set_default_size(1200, 300);
    window.set_child(Some(&view));
    window.present();
    crate::ui::source_context_surface::settle_layout();

    let before = descendants_with_class(view.upcast_ref(), "reprise-source-image");
    assert_eq!(before.len(), 1);
    let selected_before = selection.selected();

    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::ARTWORK_MODULE, true)
        .unwrap();
    assert!(images_allowed());
    artwork_cells.reapply();
    crate::ui::source_context_surface::settle_layout();

    let after = descendants_with_class(view.upcast_ref(), "reprise-source-image");
    assert_eq!(after.len(), 1);
    assert_ne!(before[0], after[0]);
    assert_eq!(selection.selected(), selected_before);
}
