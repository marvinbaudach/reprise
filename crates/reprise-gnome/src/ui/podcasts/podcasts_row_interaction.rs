//! Pointer and keyboard affordances for compact grouped episode rows.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib::variant::ToVariant;
use gtk4::prelude::*;
use reprise_core::podcasts::{EpisodeRow, PodcastKind};

use super::podcasts_selection::SelectMode;

/// One reveal rule for the unsubscribe star, shared by every row in both
/// views: visible while the row is hovered, and visible while the star itself
/// has keyboard focus — a hover-only star is unreachable without a pointer.
/// `host` is whatever widget stands for "the row" at that call site.
pub(super) fn reveal_unsubscribe_on_hover_or_focus(
    host: &impl IsA<gtk4::Widget>,
    unsubscribe: &gtk4::Button,
) {
    let hovered = Rc::new(Cell::new(false));
    let hover = gtk4::EventControllerMotion::new();
    let hovered_state = hovered.clone();
    let hovered_unsubscribe = unsubscribe.downgrade();
    hover.connect_enter(move |_, _, _| {
        hovered_state.set(true);
        if let Some(unsubscribe) = hovered_unsubscribe.upgrade() {
            unsubscribe.set_opacity(1.0);
        }
    });
    let hovered_state = hovered.clone();
    let hovered_unsubscribe = unsubscribe.downgrade();
    hover.connect_leave(move |_| {
        hovered_state.set(false);
        if let Some(unsubscribe) = hovered_unsubscribe.upgrade() {
            unsubscribe.set_opacity(if unsubscribe.has_focus() { 1.0 } else { 0.0 });
        }
    });
    host.as_ref().add_controller(hover);

    unsubscribe.connect_has_focus_notify(move |unsubscribe| {
        unsubscribe.set_opacity(if unsubscribe.has_focus() || hovered.get() {
            1.0
        } else {
            0.0
        });
    });
}

pub(super) fn episode_thumbnail(row: &EpisodeRow, images_allowed: bool) -> gtk4::Overlay {
    let (width, height) = match row.kind {
        PodcastKind::Rss => (32, 32),
        PodcastKind::Youtube => (56, 32),
    };
    let source = super::source_image::SourceImage::new_with_dimensions(
        row.image_url.as_deref().or(row.show_image_url.as_deref()),
        match row.kind {
            PodcastKind::Rss => "audio-input-microphone-symbolic",
            PodcastKind::Youtube => "video-x-generic-symbolic",
        },
        width,
        height,
        images_allowed,
    );
    source
        .widget()
        .add_css_class("reprise-podcast-episode-thumbnail");
    let overlay = gtk4::Overlay::new();
    overlay.set_size_request(width, height);
    overlay.set_child(Some(source.widget()));
    overlay
}

/// Activating a row is the only way to play it now that the per-row play
/// button is gone, so a failed activation is a dead row, not a no-op worth
/// discarding: it happens when the row has lost its action-group ancestor,
/// which leaves no other trace to debug from.
fn activate_play(root: &gtk4::Box, episode_id: i64) {
    if let Err(error) = root.activate_action("podcasts.play", Some(&episode_id.to_variant())) {
        tracing::debug!(%error, episode_id, "podcast row activation did not reach the action");
    }
}

/// The grouped library view's selection action. Registered as `select-row` in
/// the `podcasts` group (`podcasts_view_actions.rs`).
pub(super) const SELECT_ROW_ACTION: &str = "podcasts.select-row";
/// The channel detail view's own, because its selection is per channel.
/// Registered as `select-channel-row` in `youtube_channel_detail.rs`.
pub(super) const SELECT_CHANNEL_ROW_ACTION: &str = "podcasts.select-channel-row";

/// What an input on a row asks for. Deciding this in a pure function keeps the
/// mapping testable without synthesising GTK events, and leaves the handlers
/// with nothing but dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowIntent {
    Play,
    Select(SelectMode),
}

/// `SRC-14`: a press on a row. The first press of a double click still
/// selects — that is what `ColumnView` does, and it keeps the selection honest
/// when the second press never arrives.
pub(super) fn pointer_intent(n_press: i32, state: gtk4::gdk::ModifierType) -> RowIntent {
    if n_press >= 2 {
        return RowIntent::Play;
    }
    if state.contains(gtk4::gdk::ModifierType::CONTROL_MASK) {
        RowIntent::Select(SelectMode::Toggle)
    } else if state.contains(gtk4::gdk::ModifierType::SHIFT_MASK) {
        RowIntent::Select(SelectMode::Range)
    } else {
        RowIntent::Select(SelectMode::Only)
    }
}

/// `SRC-14`: a key on a focused row. `None` means "not ours" and the press
/// keeps propagating.
pub(super) fn key_intent(key: gtk4::gdk::Key, state: gtk4::gdk::ModifierType) -> Option<RowIntent> {
    match key {
        gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter => Some(RowIntent::Play),
        // Playing moved to Enter so Space can do what it does in the track
        // list: build a selection from the keyboard.
        gtk4::gdk::Key::space | gtk4::gdk::Key::KP_Space => Some(RowIntent::Select(
            if state.contains(gtk4::gdk::ModifierType::SHIFT_MASK) {
                SelectMode::Range
            } else {
                SelectMode::Toggle
            },
        )),
        _ => None,
    }
}

fn dispatch(root: &gtk4::Box, episode_id: i64, intent: RowIntent, select_action: &str) {
    match intent {
        RowIntent::Play => activate_play(root, episode_id),
        RowIntent::Select(mode) => {
            let target = (episode_id, mode.as_u8()).to_variant();
            if let Err(error) = root.activate_action(select_action, Some(&target)) {
                tracing::debug!(%error, episode_id, "podcast row selection did not reach the action");
            }
        }
    }
}

/// `select_action` differs per surface: the grouped library view owns one flat
/// selection, the channel detail view one per channel, so each routes to its
/// own action. Everything else about a row's input behaviour is identical, and
/// deliberately lives here once.
pub(super) fn install_row_interaction(
    root: &gtk4::Box,
    episode_id: i64,
    select_action: &'static str,
) {
    // input-parity: ACC-8 keyboard=episode-row-enter-space
    let click = gtk4::GestureClick::new();
    let clicked_root = root.downgrade();
    click.connect_released(move |gesture, n_press, _, _| {
        if gesture.current_button() != 1 {
            return;
        }
        if let Some(root) = clicked_root.upgrade() {
            dispatch(
                &root,
                episode_id,
                pointer_intent(n_press, gesture.current_event_state()),
                select_action,
            );
        }
    });
    root.add_controller(click);

    let keys = gtk4::EventControllerKey::new();
    let keyed_root = root.downgrade();
    keys.connect_key_pressed(move |_, key, _, state| {
        let Some(intent) = key_intent(key, state) else {
            return gtk4::glib::Propagation::Proceed;
        };
        if let Some(root) = keyed_root.upgrade() {
            dispatch(&root, episode_id, intent, select_action);
        }
        gtk4::glib::Propagation::Stop
    });
    root.add_controller(keys);
}

#[cfg(test)]
#[path = "podcasts_row_interaction_tests.rs"]
mod tests;
