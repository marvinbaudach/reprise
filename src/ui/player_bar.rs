//! The bottom player bar: track title/artist, transport controls, a seek
//! scale, and volume. A `gtk::ActionBar` embedded via
//! `adw::ToolbarView::add_bottom_bar` in `window.rs`.
//!
//! `PlayerBar` owns every widget it displays; callers (see
//! `player_controller.rs`) only interact with it through the `set_*`/
//! `connect_*` methods below, never by reaching into its widgets directly.
//! That keeps the seek-scale feedback-loop guard (`updating_scale`) a private
//! implementation detail instead of something every caller has to remember
//! to respect: `set_position` (programmatic, from position ticks/track
//! changes) and the user dragging the scale both end up calling
//! `gtk::Range::set_value`/firing `value-changed`, and only the latter should
//! trigger a seek.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::pango;
use gtk4::prelude::*;

use crate::format::format_duration;
use crate::player::PlaybackState;
use crate::ui::strings;

const ICON_PLAY: &str = "media-playback-start-symbolic";
const ICON_PAUSE: &str = "media-playback-pause-symbolic";

/// Icons `gtk::ScaleButton` cycles through by value, lowest first — mirrors
/// the stock GNOME volume-button icon set (mute/low/medium/high).
const VOLUME_ICONS: [&str; 4] = [
    "audio-volume-muted-symbolic",
    "audio-volume-low-symbolic",
    "audio-volume-medium-symbolic",
    "audio-volume-high-symbolic",
];
const VOLUME_MIN: f64 = 0.0;
const VOLUME_MAX: f64 = 1.0;
const VOLUME_STEP: f64 = 0.05;
const VOLUME_DEFAULT: f64 = 1.0;

/// Fixed width for the title/artist column so the transport controls stay
/// centered regardless of how long a track's metadata is (labels ellipsize
/// instead of pushing the layout around).
const TRACK_INFO_WIDTH: i32 = 220;

const ZERO_TIME_LABEL: &str = "0:00";
const CENTER_BOX_SPACING: i32 = 8;

pub struct PlayerBar {
    bar: gtk4::ActionBar,
    title_label: gtk4::Label,
    artist_label: gtk4::Label,
    play_pause_button: gtk4::Button,
    position_label: gtk4::Label,
    duration_label: gtk4::Label,
    scale: gtk4::Scale,
    volume_button: gtk4::ScaleButton,
    /// True while `set_position`/`set_state` are the ones setting `scale`'s
    /// value, so `connect_seek`'s handler can tell a programmatic update
    /// apart from the user dragging the scale. See the module doc comment.
    updating_scale: Rc<Cell<bool>>,
}

impl PlayerBar {
    pub fn new() -> Self {
        let title_label = build_track_label();
        // Bold via a Pango attribute (set once, applies to every future
        // `set_text`) rather than per-call `set_markup`, which would require
        // escaping angle brackets/ampersands in track titles on every update.
        let bold = pango::AttrList::new();
        bold.insert(pango::AttrInt::new_weight(pango::Weight::Bold));
        title_label.set_attributes(Some(&bold));

        let artist_label = build_track_label();
        artist_label.add_css_class("dim-label");

        let track_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        track_box.append(&title_label);
        track_box.append(&artist_label);
        track_box.set_valign(gtk4::Align::Center);
        track_box.set_width_request(TRACK_INFO_WIDTH);

        let play_pause_button = gtk4::Button::from_icon_name(ICON_PLAY);
        play_pause_button.set_tooltip_text(Some(strings::PLAY));
        play_pause_button.set_valign(gtk4::Align::Center);

        let position_label = gtk4::Label::new(Some(ZERO_TIME_LABEL));
        let duration_label = gtk4::Label::new(Some(ZERO_TIME_LABEL));

        let scale = gtk4::Scale::new(gtk4::Orientation::Horizontal, None::<&gtk4::Adjustment>);
        scale.set_range(0.0, 1.0);
        scale.set_draw_value(false);
        scale.set_hexpand(true);
        scale.set_valign(gtk4::Align::Center);
        scale.set_tooltip_text(Some(strings::PLAYBACK_POSITION));

        let center_box = gtk4::Box::new(gtk4::Orientation::Horizontal, CENTER_BOX_SPACING);
        center_box.append(&play_pause_button);
        center_box.append(&position_label);
        center_box.append(&scale);
        center_box.append(&duration_label);
        center_box.set_hexpand(true);
        center_box.set_valign(gtk4::Align::Center);

        let volume_button =
            gtk4::ScaleButton::new(VOLUME_MIN, VOLUME_MAX, VOLUME_STEP, &VOLUME_ICONS);
        volume_button.set_value(VOLUME_DEFAULT);
        volume_button.set_tooltip_text(Some(strings::VOLUME));
        volume_button.set_valign(gtk4::Align::Center);

        let bar = gtk4::ActionBar::new();
        bar.pack_start(&track_box);
        bar.set_center_widget(Some(&center_box));
        bar.pack_end(&volume_button);
        // No track loaded yet — nothing to play, pause, or seek until the
        // player reports a non-stopped state (see `set_state`).
        bar.set_sensitive(false);

        Self {
            bar,
            title_label,
            artist_label,
            play_pause_button,
            position_label,
            duration_label,
            scale,
            volume_button,
            updating_scale: Rc::new(Cell::new(false)),
        }
    }

    /// The root widget to embed via `ToolbarView::add_bottom_bar`.
    pub fn widget(&self) -> &gtk4::ActionBar {
        &self.bar
    }

    /// Shows `title`/`artist` in the left-hand labels. Called on row
    /// activation with data already in hand from the `Track` (see
    /// `player_controller.rs`) — no extra DB query needed.
    pub fn set_track(&self, title: &str, artist: &str) {
        self.title_label.set_text(title);
        self.artist_label.set_text(artist);
    }

    /// Clears the track labels back to empty — used when playback stops with
    /// no track active. Queueing (auto-advance) is a later stage; see the
    /// module doc comment in `track_list.rs`.
    pub fn clear_track(&self) {
        self.title_label.set_text("");
        self.artist_label.set_text("");
    }

    /// Applies a `PlaybackState`: swaps the play/pause icon and tooltip, and
    /// keeps the whole bar insensitive while stopped — a stopped bar has no
    /// active track to seek, pause, or adjust volume for.
    pub fn set_state(&self, state: PlaybackState) {
        let is_playing = state == PlaybackState::Playing;
        self.play_pause_button
            .set_icon_name(if is_playing { ICON_PAUSE } else { ICON_PLAY });
        self.play_pause_button.set_tooltip_text(Some(if is_playing {
            strings::PAUSE
        } else {
            strings::PLAY
        }));
        self.bar.set_sensitive(state != PlaybackState::Stopped);
        if state == PlaybackState::Stopped {
            self.set_position(0, 0);
        }
    }

    /// Updates the seek scale and time labels for a `Position` event.
    /// Reentrancy-safe against `connect_seek`'s handler: GTK fires
    /// `value-changed` synchronously from `set_value`, and `updating_scale`
    /// is `true` for that whole call, so the handler recognizes this as a
    /// programmatic update rather than a user drag and skips seeking.
    pub fn set_position(&self, position_ms: i64, duration_ms: i64) {
        let duration_ms = duration_ms.max(0);
        let position_ms = position_ms.clamp(0, duration_ms);

        self.updating_scale.set(true);
        // A zero-length range would make the scale's drag handle meaningless
        // (and GTK dislikes a max == min range); floor it at 1 ms.
        self.scale.set_range(0.0, duration_ms.max(1) as f64);
        self.scale.set_value(position_ms as f64);
        self.updating_scale.set(false);

        self.position_label.set_text(&format_duration(position_ms));
        self.duration_label.set_text(&format_duration(duration_ms));
    }

    /// Wires the play/pause button; `f` is called on every click with no
    /// arguments — the caller (which owns the `Player`) decides what
    /// "toggle" means and reports the resulting state back via `set_state`.
    pub fn connect_play_pause<F: Fn() + 'static>(&self, f: F) {
        self.play_pause_button.connect_clicked(move |_| f());
    }

    /// Wires the seek scale: `f` is called with the target position in
    /// milliseconds only when the *user* moves the scale, never when
    /// `set_position`/`set_state` update it programmatically.
    pub fn connect_seek<F: Fn(i64) + 'static>(&self, f: F) {
        let updating_scale = self.updating_scale.clone();
        self.scale.connect_value_changed(move |scale| {
            if updating_scale.get() {
                return;
            }
            f(scale.value() as i64);
        });
    }

    /// Wires the volume button: `f` is called with a `0.0..=1.0` value on
    /// every user change.
    pub fn connect_volume_changed<F: Fn(f64) + 'static>(&self, f: F) {
        self.volume_button
            .connect_value_changed(move |_, value| f(value));
    }
}

impl Default for PlayerBar {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds a start-aligned, ellipsizing label — the shared shape of the title
/// and artist labels, differing only in the attribute/CSS class the caller
/// adds afterwards (bold weight vs. `dim-label`).
fn build_track_label() -> gtk4::Label {
    let label = gtk4::Label::new(None);
    label.set_halign(gtk4::Align::Start);
    label.set_ellipsize(pango::EllipsizeMode::End);
    label.set_xalign(0.0);
    label
}
