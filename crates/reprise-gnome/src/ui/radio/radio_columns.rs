use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::connectivity::Connectivity;
use reprise_core::radio::StationRow;

use super::radio_context_menu;
use super::radio_live_cells::RadioLiveCells;
use super::radio_model::RadioObject;
use super::radio_presentation::{
    format_bitrate, format_country, format_genre, now_playing, row_is_accented, RadioLiveState,
};
use crate::ui::playing_marker;
use crate::ui::strings;
use crate::ui::table_column_widths as widths;

pub(super) type LiveState = Rc<dyn Fn() -> RadioLiveState>;
/// `NET-3b`: read at right-click/context-menu-key time so the Play entry's
/// label always reflects current connectivity, never a stale snapshot.
pub(super) type ConnectivitySource = Rc<dyn Fn() -> Connectivity>;

#[derive(Clone, Copy)]
struct ColumnTitle<'a> {
    text: &'a str,
    playback_accent: bool,
}

#[derive(Clone, Copy)]
struct TextColumnSpec<'a> {
    title: ColumnTitle<'a>,
    sizing: widths::Sizing,
    query: Option<&'a crate::ui::search_highlight::QuerySource>,
}

struct TextColumnContext<'a> {
    live_state: &'a LiveState,
    connectivity: &'a ConnectivitySource,
    cells: &'a Rc<RadioLiveCells>,
}

fn apply_playing_style(widget: &gtk4::Widget, playing: bool) {
    if playing {
        widget.add_css_class("reprise-radio-playing");
    } else {
        widget.remove_css_class("reprise-radio-playing");
    }
}

/// `sizing` fixes the column's width; see [`widths`] for why every column
/// must carry one (STYLE-9).
fn text_column(
    view: &gtk4::ColumnView,
    spec: TextColumnSpec<'_>,
    render: impl Fn(&StationRow, &RadioLiveState) -> String + 'static,
    context: &TextColumnContext<'_>,
) {
    let TextColumnSpec {
        title,
        sizing,
        query,
    } = spec;
    let live_state = context.live_state.clone();
    let connectivity = context.connectivity.clone();
    let cells = context.cells.clone();
    let is_title = title.playback_accent;
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
    let render = Rc::new(render);
    let query = query.cloned();
    let cells_for_bind = cells.clone();
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
        // The live half of this cell, re-runnable on its own: registering it
        // is what lets a playback change reach an already-bound cell without
        // a model signal (see `radio_live_cells`).
        let apply = {
            let live_state = live_state.clone();
            let render = render.clone();
            let query = query.clone();
            Rc::new(move || {
                let live = live_state();
                let text = render(&row, &live);
                if let Some(query) = query.as_ref() {
                    crate::ui::search_highlight::apply(&label, &text, &query());
                } else {
                    label.set_text(&text);
                }
                let loaded = row_is_accented(row.id, &live);
                apply_playing_style(label.upcast_ref(), loaded);
                if is_title {
                    if loaded {
                        label.add_css_class(playing_marker::PLAYING_TITLE_CLASS);
                    } else {
                        label.remove_css_class(playing_marker::PLAYING_TITLE_CLASS);
                    }
                }
            }) as Rc<dyn Fn()>
        };
        apply();
        cells_for_bind.register(item, apply);
    });
    let cells_for_unbind = cells.clone();
    factory.connect_unbind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        cells_for_unbind.unregister(item);
        let Some(surface) = item.child().and_downcast::<gtk4::Box>() else {
            return;
        };
        let Some(label) = surface.first_child().and_downcast::<gtk4::Label>() else {
            return;
        };
        label.set_text("");
        label.remove_css_class("reprise-radio-playing");
        if is_title {
            label.remove_css_class(playing_marker::PLAYING_TITLE_CLASS);
        }
    });
    let column = gtk4::ColumnViewColumn::builder()
        .title(title.text)
        .factory(&factory)
        .resizable(true)
        .build();
    sizing.apply(&column);
    view.append_column(&column);
}

fn state_column(
    view: &gtk4::ColumnView,
    live_state: &LiveState,
    connectivity: &ConnectivitySource,
    cells: &Rc<RadioLiveCells>,
) {
    let factory = gtk4::SignalListItemFactory::new();
    let live_for_gesture = live_state.clone();
    let connectivity_for_gesture = connectivity.clone();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let cell = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        let marker = playing_marker::build();
        marker.set_visible(false);
        cell.append(&marker);
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
    let cells_for_bind = cells.clone();
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
        let Some(marker) = cell.first_child().and_downcast::<gtk4::Box>() else {
            return;
        };
        let Some(icon) = marker.next_sibling().and_downcast::<gtk4::Image>() else {
            return;
        };
        let Some(object) = item.item().and_downcast::<RadioObject>() else {
            return;
        };
        let station_id = object.row().id;
        let apply = {
            let live_state = live_state.clone();
            Rc::new(move || {
                let live = live_state();
                let loaded = row_is_accented(station_id, &live);
                playing_marker::set_playing(&marker, live.playing);
                marker.set_visible(loaded);
                icon.set_icon_name(Some("network-wireless-symbolic"));
                icon.set_visible(!loaded);
                apply_playing_style(cell.upcast_ref(), loaded);
            }) as Rc<dyn Fn()>
        };
        apply();
        cells_for_bind.register(item, apply);
    });
    let cells_for_unbind = cells.clone();
    factory.connect_unbind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        cells_for_unbind.unregister(item);
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
    widths::pin(&column, widths::ICON_ACTION);
    view.append_column(&column);
}

fn artwork_column(
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
        let cell = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
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
        let Some(object) = item.item().and_downcast::<RadioObject>() else {
            return;
        };
        while let Some(child) = cell.first_child() {
            cell.remove(&child);
        }
        let row = object.row();
        let artwork = crate::ui::podcasts::source_image::SourceImage::new_after_startup(
            row.favicon_url.as_deref(),
            "audio-input-microphone-symbolic",
            36,
            crate::ui::podcasts::source_image::gate_open(),
        );
        cell.append(artwork.widget());
    });
    let column = gtk4::ColumnViewColumn::builder()
        .factory(&factory)
        .resizable(false)
        .build();
    widths::pin(&column, crate::ui::source_row::MEDIA_WIDTH);
    view.append_column(&column);
}

pub(super) fn append_columns(
    view: &gtk4::ColumnView,
    live_state: &LiveState,
    connectivity: &ConnectivitySource,
    cells: &Rc<RadioLiveCells>,
    query: &crate::ui::search_highlight::QuerySource,
) {
    artwork_column(view, live_state, connectivity);
    state_column(view, live_state, connectivity, cells);
    let context = TextColumnContext {
        live_state,
        connectivity,
        cells,
    };
    // Station is the filler: it owns whatever width the pinned columns leave.
    text_column(
        view,
        TextColumnSpec {
            title: ColumnTitle {
                text: &strings::text(strings::RADIO_STATION),
                playback_accent: true,
            },
            sizing: widths::Sizing::filler(widths::TITLE_MIN),
            query: Some(query),
        },
        |row, _| row.name.clone(),
        &context,
    );
    text_column(
        view,
        TextColumnSpec {
            title: ColumnTitle {
                text: &strings::text(strings::RADIO_GENRE),
                playback_accent: false,
            },
            sizing: widths::Sizing::pinned(widths::LABEL),
            query: None,
        },
        |row, _| format_genre(row.genre.as_deref()),
        &context,
    );
    text_column(
        view,
        TextColumnSpec {
            title: ColumnTitle {
                text: &strings::text(strings::RADIO_BITRATE),
                playback_accent: false,
            },
            sizing: widths::Sizing::pinned(widths::NUMERIC),
            query: None,
        },
        |row, _| format_bitrate(row.bitrate_kbps),
        &context,
    );
    text_column(
        view,
        TextColumnSpec {
            title: ColumnTitle {
                text: &strings::text(strings::RADIO_COUNTRY),
                playback_accent: false,
            },
            sizing: widths::Sizing::pinned(widths::SHORT_LABEL),
            query: None,
        },
        |row, _| format_country(row.country_code.as_deref()),
        &context,
    );
    // Now Playing carries live stream metadata — the most volatile text in
    // the table, and the reason this column must never size itself.
    text_column(
        view,
        TextColumnSpec {
            title: ColumnTitle {
                text: &strings::text(strings::RADIO_NOW_PLAYING),
                playback_accent: false,
            },
            sizing: widths::Sizing::pinned(widths::NAME),
            query: None,
        },
        |row, live| now_playing(row.id, live),
        &context,
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

    fn descendant_labels(widget: &gtk4::Widget) -> Vec<gtk4::Label> {
        let mut labels = widget
            .clone()
            .downcast::<gtk4::Label>()
            .ok()
            .into_iter()
            .collect::<Vec<_>>();
        let mut child = widget.first_child();
        while let Some(current) = child {
            labels.extend(descendant_labels(&current));
            child = current.next_sibling();
        }
        labels
    }

    /// UX FIL-5a: Radio highlights only the station-name field its query
    /// searches, not metadata that happens to contain the same text.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_5a_radio_marks_station_name_but_not_metadata() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();

        let mut row = station();
        row.name = "Falling FM".into();
        row.genre = Some("Fall Jazz".into());
        let store = gtk4::gio::ListStore::new::<RadioObject>();
        store.append(&RadioObject::new(row));
        let view = gtk4::ColumnView::new(Some(gtk4::SingleSelection::new(Some(store))));
        let live: LiveState = Rc::new(RadioLiveState::default);
        let connectivity: ConnectivitySource = Rc::new(|| Connectivity::Online);
        let cells = Rc::new(RadioLiveCells::default());
        let query_text = Rc::new(std::cell::RefCell::new("fall".to_owned()));
        let query: crate::ui::search_highlight::QuerySource = {
            let query_text = query_text.clone();
            Rc::new(move || query_text.borrow().clone())
        };
        append_columns(&view, &live, &connectivity, &cells, &query);

        let window = gtk4::Window::new();
        window.set_default_size(1200, 300);
        window.set_child(Some(&view));
        window.present();
        crate::ui::source_context_surface::settle_layout();

        let labels = descendant_labels(view.upcast_ref());
        let station = labels
            .iter()
            .find(|label| label.text() == "Falling FM")
            .expect("station-name label");
        assert!(
            station.uses_markup(),
            "the searched station name was not highlighted"
        );
        assert!(
            labels
                .iter()
                .any(|label| label.text() == "Fall Jazz" && !label.uses_markup()),
            "the unsearched genre claimed the hit"
        );

        query_text.borrow_mut().clear();
        cells.reapply();
        assert!(
            !station.uses_markup(),
            "clearing a query that keeps the same row set left stale markup"
        );
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
        let query: crate::ui::search_highlight::QuerySource = Rc::new(String::new);
        append_columns(
            &view,
            &live_state,
            &connectivity,
            &Rc::new(RadioLiveCells::default()),
            &query,
        );

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

    /// STYLE-9: the radio table must not re-measure itself from the rows
    /// currently on screen, or every scroll shifts the columns.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn style_9_radio_columns_keep_their_width_when_the_rows_change() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();

        let store = gtk4::gio::ListStore::new::<RadioObject>();
        store.append(&RadioObject::new(station()));
        let view = gtk4::ColumnView::new(Some(gtk4::SingleSelection::new(Some(store.clone()))));
        let live_state: LiveState = Rc::new(RadioLiveState::default);
        let connectivity: ConnectivitySource = Rc::new(|| Connectivity::Online);
        let query: crate::ui::search_highlight::QuerySource = Rc::new(String::new);
        append_columns(
            &view,
            &live_state,
            &connectivity,
            &Rc::new(RadioLiveCells::default()),
            &query,
        );

        crate::ui::table_column_widths::assert_stable_across_row_change(&view, || {
            let mut long = station();
            long.name = "Radio Nacional Clásica Buenos Aires Extended".into();
            long.genre = Some("Progressive Electronic Ambient".into());
            long.bitrate_kbps = Some(320);
            long.country_code = Some("AR".into());
            store.splice(0, 1, &[RadioObject::new(long)]);
        });
    }

    /// `SRC-4a`: the radio star was hover-only and not even focusable, so the
    /// context menu was already the only reachable path for keyboard users.
    #[test]
    fn src_4a_the_state_cell_offers_no_hover_star() {
        let source = include_str!("radio_columns.rs");
        let removed_icon = ["starred", "-symbolic"].concat();

        assert!(
            !source.contains(&removed_icon),
            "the hover star is gone from the radio state cell"
        );
    }

    #[test]
    fn nav_10b_the_radio_marker_reapplies_without_rebuilding_the_model() {
        let source = include_str!("radio_columns.rs");

        assert!(source.contains("playing_marker::build"));
        assert!(source.contains("playing_marker::set_playing"));
        assert!(source.contains("cells_for_bind.register"));
        assert!(source.contains("playing_marker::PLAYING_TITLE_CLASS"));
        let model_signal = ["items", "_changed"].concat();
        assert!(!source.contains(&model_signal));
        let retired_glyph = ["audio-volume-high", "-symbolic"].concat();
        assert!(!source.contains(&retired_glyph));

        let append = source
            .split_once("pub(super) fn append_columns")
            .expect("column composition")
            .1;
        let artwork = append.find("artwork_column").expect("artwork column");
        let marker = append.find("state_column").expect("marker column");
        let title = append.find("RADIO_STATION").expect("station title column");
        assert!(artwork < marker && marker < title);
    }
}
