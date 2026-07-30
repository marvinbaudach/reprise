//! Pointer and keyboard affordances for compact grouped episode rows.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib::variant::ToVariant;
use gtk4::prelude::*;
use reprise_core::podcasts::{EpisodeRow, PodcastKind};

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

pub(super) fn episode_thumbnail(
    row: &EpisodeRow,
    playing: bool,
    images_allowed: bool,
) -> (gtk4::Overlay, gtk4::Image) {
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
    let play = gtk4::Image::from_icon_name(if playing {
        "media-playback-pause-symbolic"
    } else {
        "media-playback-start-symbolic"
    });
    play.add_css_class("reprise-podcast-episode-play-glyph");
    play.set_halign(gtk4::Align::Center);
    play.set_valign(gtk4::Align::Center);
    play.set_opacity(0.0);
    overlay.add_overlay(&play);
    (overlay, play)
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

pub(super) fn install_row_activation(root: &gtk4::Box, episode_id: i64, play_glyph: &gtk4::Image) {
    let hover = gtk4::EventControllerMotion::new();
    let hovered_glyph = play_glyph.downgrade();
    hover.connect_enter(move |_, _, _| {
        if let Some(glyph) = hovered_glyph.upgrade() {
            glyph.set_opacity(1.0);
        }
    });
    let hovered_glyph = play_glyph.downgrade();
    hover.connect_leave(move |_| {
        if let Some(glyph) = hovered_glyph.upgrade() {
            glyph.set_opacity(0.0);
        }
    });
    root.add_controller(hover);

    // input-parity: ACC-8 keyboard=episode-row-enter-space
    let click = gtk4::GestureClick::new();
    let clicked_root = root.downgrade();
    click.connect_released(move |gesture, _, _, _| {
        if gesture.current_button() != 1 {
            return;
        }
        if let Some(root) = clicked_root.upgrade() {
            activate_play(&root, episode_id);
        }
    });
    root.add_controller(click);

    let keys = gtk4::EventControllerKey::new();
    let keyed_root = root.downgrade();
    keys.connect_key_pressed(move |_, key, _, _| {
        if !matches!(
            key,
            gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter | gtk4::gdk::Key::space
        ) {
            return gtk4::glib::Propagation::Proceed;
        }
        if let Some(root) = keyed_root.upgrade() {
            activate_play(&root, episode_id);
        }
        gtk4::glib::Propagation::Stop
    });
    root.add_controller(keys);
}

#[cfg(test)]
#[path = "podcasts_row_interaction_tests.rs"]
mod tests;
