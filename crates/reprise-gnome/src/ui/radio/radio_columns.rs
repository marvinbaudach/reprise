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

pub(super) type OnRemove = Rc<dyn Fn(i64)>;
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
        radio_context_menu::wire_gesture(
            &label,
            item,
            move |id| row_is_accented(id, &live()),
            move || connectivity(),
        );
        item.set_child(Some(&label));
    });
    let live_state = live_state.clone();
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(label) = item.child().and_downcast::<gtk4::Label>() else {
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
        let Some(label) = item.child().and_downcast::<gtk4::Label>() else {
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
    on_remove: &OnRemove,
    live_state: &LiveState,
    connectivity: &ConnectivitySource,
) {
    let factory = gtk4::SignalListItemFactory::new();
    let callback = on_remove.clone();
    let live_for_gesture = live_state.clone();
    let connectivity_for_gesture = connectivity.clone();
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
        let connectivity = connectivity_for_gesture.clone();
        radio_context_menu::wire_gesture(
            &cell,
            item,
            move |id| row_is_accented(id, &live()),
            move || connectivity(),
        );
        item.set_child(Some(&cell));
    });
    let live_state = live_state.clone();
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(cell) = item.child().and_downcast::<gtk4::Box>() else {
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
        let Some(cell) = item.child().and_downcast::<gtk4::Box>() else {
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
    connectivity: &ConnectivitySource,
) {
    state_column(view, on_remove, live_state, connectivity);
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
