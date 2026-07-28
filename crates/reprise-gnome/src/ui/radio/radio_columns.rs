use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::radio::StationRow;

use super::radio_context_menu;
use super::radio_model::RadioObject;
use super::radio_presentation::{
    format_bitrate, format_country, format_genre, now_playing, row_is_accented, RadioLiveState,
};
use crate::ui::strings;

pub(super) type OnRemove = Rc<dyn Fn(i64)>;
pub(super) type LiveState = Rc<dyn Fn() -> RadioLiveState>;

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
) {
    let factory = gtk4::SignalListItemFactory::new();
    let live_for_gesture = live_state.clone();
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
        let surface = crate::ui::source_context_surface::wrap(&label);
        radio_context_menu::wire_gesture(&surface, item, move |id| row_is_accented(id, &live()));
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

fn state_column(view: &gtk4::ColumnView, on_remove: &OnRemove, live_state: &LiveState) {
    let factory = gtk4::SignalListItemFactory::new();
    let callback = on_remove.clone();
    let live_for_gesture = live_state.clone();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let cell = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        let icon = gtk4::Image::new();
        icon.set_pixel_size(24);
        let star = gtk4::Button::from_icon_name("starred-symbolic");
        star.add_css_class("flat");
        star.add_css_class("accent");
        star.set_focusable(false);
        star.set_opacity(0.0);
        let item_weak = item.downgrade();
        let callback = callback.clone();
        star.connect_clicked(move |_| {
            let Some(item) = item_weak.upgrade() else {
                return;
            };
            let Some(object) = item.item().and_downcast::<RadioObject>() else {
                return;
            };
            callback(object.row().id);
        });
        let motion = gtk4::EventControllerMotion::new();
        let weak = star.downgrade();
        motion.connect_enter(move |_, _, _| {
            if let Some(star) = weak.upgrade() {
                star.set_opacity(1.0);
            }
        });
        let weak = star.downgrade();
        motion.connect_leave(move |_| {
            if let Some(star) = weak.upgrade() {
                star.set_opacity(0.0);
            }
        });
        cell.add_controller(motion);
        cell.append(&icon);
        cell.append(&star);
        let live = live_for_gesture.clone();
        let surface = crate::ui::source_context_surface::wrap(&cell);
        radio_context_menu::wire_gesture(&surface, item, move |id| row_is_accented(id, &live()));
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
        let Some(star) = icon.next_sibling().and_downcast::<gtk4::Button>() else {
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
        star.set_tooltip_text(Some(&strings::radio_remove_named(&row.name)));
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
    on_remove: &OnRemove,
    live_state: &LiveState,
) {
    state_column(view, on_remove, live_state);
    text_column(
        view,
        &strings::text(strings::RADIO_STATION),
        true,
        |row, _| row.name.clone(),
        live_state,
    );
    text_column(
        view,
        &strings::text(strings::RADIO_GENRE),
        false,
        |row, _| format_genre(row.genre.as_deref()),
        live_state,
    );
    text_column(
        view,
        &strings::text(strings::RADIO_BITRATE),
        false,
        |row, _| format_bitrate(row.bitrate_kbps),
        live_state,
    );
    text_column(
        view,
        &strings::text(strings::RADIO_COUNTRY),
        false,
        |row, _| format_country(row.country_code.as_deref()),
        live_state,
    );
    text_column(
        view,
        &strings::text(strings::RADIO_NOW_PLAYING),
        true,
        |row, live| now_playing(row.id, live),
        live_state,
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
        let on_remove: OnRemove = Rc::new(|_| {});
        let live_state: LiveState = Rc::new(RadioLiveState::default);
        append_columns(&view, &on_remove, &live_state);

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
}
