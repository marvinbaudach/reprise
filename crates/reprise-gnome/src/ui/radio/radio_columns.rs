use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::connectivity::Connectivity;
use reprise_core::radio::StationRow;

use super::radio_context_menu;
use super::radio_model::RadioObject;
use super::radio_presentation::{
    format_bitrate, format_country, format_genre, now_playing, row_is_accented, RadioLiveState,
};
use crate::ui::strings;

pub(super) type LiveState = Rc<dyn Fn() -> RadioLiveState>;
/// `NET-3b`: read at right-click/context-menu-key time so the Play entry's
/// label always reflects current connectivity, never a stale snapshot.
pub(super) type ConnectivitySource = Rc<dyn Fn() -> Connectivity>;

fn apply_playing_style(widget: &gtk4::Widget, playing: bool) {
    if playing {
        widget.add_css_class("reprise-radio-playing");
    } else {
        widget.remove_css_class("reprise-radio-playing");
    }
}

fn text_column(
    view: &gtk4::ColumnView,
    title: &str,
    expand: bool,
    render: impl Fn(&StationRow, &RadioLiveState) -> String + 'static,
    live_state: &LiveState,
    connectivity: &ConnectivitySource,
) {
    let factory = gtk4::SignalListItemFactory::new();
    let live_for_gesture = live_state.clone();
    let connectivity_for_gesture = connectivity.clone();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let label = gtk4::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        let live = live_for_gesture.clone();
        let connectivity = connectivity_for_gesture.clone();
        let surface = crate::ui::source_context_surface::wrap(&label);
        radio_context_menu::wire_gesture(
            &surface,
            item,
            move |id| row_is_accented(id, &live()),
            move || connectivity(),
        );
        item.set_child(Some(&surface));
    });
    let live_state = live_state.clone();
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(surface) = item.child().and_downcast::<gtk4::Box>() else {
            return;
        };
        let Some(label) = surface.first_child().and_downcast::<gtk4::Label>() else {
            return;
        };
        let Some(object) = item.item().and_downcast::<RadioObject>() else {
            return;
        };
        let row = object.row();
        let live = live_state();
        label.set_text(&render(&row, &live));
        apply_playing_style(label.upcast_ref(), row_is_accented(row.id, &live));
    });
    factory.connect_unbind(|_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(surface) = item.child().and_downcast::<gtk4::Box>() else {
            return;
        };
        let Some(label) = surface.first_child().and_downcast::<gtk4::Label>() else {
            return;
        };
        label.set_text("");
        label.remove_css_class("reprise-radio-playing");
    });
    let column = gtk4::ColumnViewColumn::builder()
        .title(title)
        .factory(&factory)
        .resizable(true)
        .expand(expand)
        .build();
    view.append_column(&column);
}

fn state_column(
    view: &gtk4::ColumnView,
    live_state: &LiveState,
    connectivity: &ConnectivitySource,
) {
    let factory = gtk4::SignalListItemFactory::new();
    let live_for_gesture = live_state.clone();
    let connectivity_for_gesture = connectivity.clone();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let cell = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        let icon = gtk4::Image::new();
        icon.set_pixel_size(24);
        cell.append(&icon);
        let live = live_for_gesture.clone();
        let connectivity = connectivity_for_gesture.clone();
        let surface = crate::ui::source_context_surface::wrap(&cell);
        radio_context_menu::wire_gesture(
            &surface,
            item,
            move |id| row_is_accented(id, &live()),
            move || connectivity(),
        );
        item.set_child(Some(&surface));
    });
    let live_state = live_state.clone();
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(surface) = item.child().and_downcast::<gtk4::Box>() else {
            return;
        };
        let Some(cell) = surface.first_child().and_downcast::<gtk4::Box>() else {
            return;
        };
        let Some(icon) = cell.first_child().and_downcast::<gtk4::Image>() else {
            return;
        };
        let Some(object) = item.item().and_downcast::<RadioObject>() else {
            return;
        };
        let row = object.row();
        let live = live_state();
        let playing = row_is_accented(row.id, &live);
        icon.set_icon_name(Some(if playing {
            "audio-volume-high-symbolic"
        } else {
            "network-wireless-symbolic"
        }));
        apply_playing_style(cell.upcast_ref(), playing);
    });
    factory.connect_unbind(|_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(surface) = item.child().and_downcast::<gtk4::Box>() else {
            return;
        };
        let Some(cell) = surface.first_child().and_downcast::<gtk4::Box>() else {
            return;
        };
        cell.remove_css_class("reprise-radio-playing");
    });
    let column = gtk4::ColumnViewColumn::builder()
        .factory(&factory)
        .resizable(false)
        .build();
    view.append_column(&column);
}

pub(super) fn append_columns(
    view: &gtk4::ColumnView,
    live_state: &LiveState,
    connectivity: &ConnectivitySource,
) {
    state_column(view, live_state, connectivity);
    text_column(
        view,
        &strings::text(strings::RADIO_STATION),
        true,
        |row, _| row.name.clone(),
        live_state,
        connectivity,
    );
    text_column(
        view,
        &strings::text(strings::RADIO_GENRE),
        false,
        |row, _| format_genre(row.genre.as_deref()),
        live_state,
        connectivity,
    );
    text_column(
        view,
        &strings::text(strings::RADIO_BITRATE),
        false,
        |row, _| format_bitrate(row.bitrate_kbps),
        live_state,
        connectivity,
    );
    text_column(
        view,
        &strings::text(strings::RADIO_COUNTRY),
        false,
        |row, _| format_country(row.country_code.as_deref()),
        live_state,
        connectivity,
    );
    text_column(
        view,
        &strings::text(strings::RADIO_NOW_PLAYING),
        true,
        |row, live| now_playing(row.id, live),
        live_state,
        connectivity,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ui::source_context_surface;

    fn station() -> StationRow {
        StationRow {
            id: 1,
            uuid: None,
            name: "Station".into(),
            stream_url: "https://example.test/stream".into(),
            homepage: None,
            favicon_url: None,
            genre: Some("Jazz".into()),
            codec: None,
            bitrate_kbps: Some(128),
            country_code: Some("DE".into()),
            votes: None,
            added_at: 1,
            removed_at: None,
        }
    }

    /// The radio table carries the same ACC-1 contract as the podcast table;
    /// see `podcasts_columns`' sibling test.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn acc_1_every_point_of_a_radio_row_reaches_the_context_menu() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        crate::ui::style::install_css_string_for_test(&source_context_surface::css());

        let store = gtk4::gio::ListStore::new::<RadioObject>();
        store.append(&RadioObject::new(station()));
        let view = gtk4::ColumnView::new(Some(gtk4::SingleSelection::new(Some(store))));
        view.add_css_class(source_context_surface::TABLE_CSS_CLASS);
        let live_state: LiveState = Rc::new(RadioLiveState::default);
        // NET-3b made connectivity an explicit input; this coverage test only
        // cares that every row is wrapped in a context surface, so it reports
        // the default Online.
        let connectivity: Rc<dyn Fn() -> reprise_core::connectivity::Connectivity> =
            Rc::new(|| reprise_core::connectivity::Connectivity::Online);
        append_columns(&view, &live_state, &connectivity);

        let window = gtk4::Window::new();
        window.set_default_size(1200, 400);
        window.set_child(Some(&view));
        window.present();
        source_context_surface::settle_layout();

        let uncovered = source_context_surface::row_points_without_a_surface(&view);
        assert!(
            uncovered.is_empty(),
            "radio row points without a context surface: {uncovered:?}"
        );
    }

    /// `SRC-4`: the radio star was hover-only and not even focusable, so the
    /// context menu was already the only reachable path for keyboard users.
    #[test]
    fn src_4_the_state_cell_offers_no_hover_star() {
        let source = include_str!("radio_columns.rs");
        let removed_icon = ["starred", "-symbolic"].concat();

        assert!(
            !source.contains(&removed_icon),
            "the hover star is gone from the radio state cell"
        );
    }
}
