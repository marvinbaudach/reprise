//! The bottom player bar: track title/artist, transport controls, a seek
//! scale, and volume. A `gtk::ActionBar` embedded via
//! `adw::ToolbarView::add_bottom_bar` in `window.rs`.
//!
//! `PlayerBar` owns every widget it displays; callers (see
//! `player_controller.rs`) only interact with it through the `set_*`/
//! `connect_*` methods below, never by reaching into its widgets directly.
//! That keeps the seek scale's two feedback-loop guards private
//! implementation details instead of something every caller has to remember
//! to respect:
//!
//! 1. **Programmatic-vs-user updates** (`updating_scale`): `set_position`
//!    (called from position ticks/track changes) and the user moving the
//!    scale both end up firing `value-changed`, and only the latter should
//!    trigger a seek. `updating_scale` is `true` for the whole duration of
//!    `set_position`'s `gtk::Range::set_value` call, so `connect_seek`'s
//!    handler can tell the two apart.
//! 2. **Ticks-during-drag** (`dragging`): position ticks arrive every
//!    500 ms regardless of what the user is doing. Without tracking whether
//!    the user currently has the pointer down on the scale, a tick mid-drag
//!    would call `set_value` and visibly yank the handle back to the actual
//!    playback position out from under the user's cursor. A `GestureClick`
//!    added alongside the scale's own built-in slider dragging (see
//!    `connect_seek`) brackets "pointer down" .. "pointer up"; while
//!    `dragging` is `true`, `set_position` skips `set_value` (and
//!    `set_range`) entirely, and releasing fires exactly one seek to
//!    wherever the user left the handle.

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
    /// True while the user has the pointer down on `scale` (between a
    /// `GestureClick` press and its matching release/cancel — wired in
    /// `connect_seek`). Checked by `set_position` so a mid-drag position
    /// tick can't yank the handle. See the module doc comment.
    dragging: Rc<Cell<bool>>,
    /// The `duration_ms` passed to the most recent `set_position` call, so
    /// it can skip `scale.set_range` when the duration hasn't actually
    /// changed (it's static per track — this is only ever a real change on
    /// a track switch, not on every 500 ms tick).
    last_duration_ms: Cell<i64>,
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
            dragging: Rc::new(Cell::new(false)),
            last_duration_ms: Cell::new(0),
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
            // Force-clear any stale drag state: e.g. the track finishes or
            // errors out while the user still has the pointer down on the
            // scale, so there's no `released`/`cancel` to clear it via the
            // normal `connect_seek` path. Without this, `set_position` below
            // would (correctly, per its own contract) skip resetting the
            // handle because it thinks a drag is still in progress.
            self.dragging.set(false);
            self.set_position(0, 0);
        }
    }

    /// Updates the seek scale and time labels for a `Position` event.
    ///
    /// Two guards apply here (see the module doc comment for the full
    /// rationale): reentrancy against `connect_seek`'s handler
    /// (`updating_scale`, set for the duration of *both* `set_range` and
    /// `set_value` below — `set_range` alone can fire `value-changed` too,
    /// by clamping the current value into the new bounds, so it needs the
    /// same guard as `set_value`), and suppression of the handle-yanking
    /// `set_range`/`set_value` calls while the user is actively dragging
    /// (`dragging`). The time labels are deliberately *not* suppressed
    /// while dragging: they keep showing the true elapsed/remaining
    /// playback time, which stays accurate and doesn't visibly jump around
    /// the way the handle would.
    pub fn set_position(&self, position_ms: i64, duration_ms: i64) {
        let duration_ms = duration_ms.max(0);
        let position_ms = position_ms.clamp(0, duration_ms);

        if should_apply_position_tick(self.dragging.get()) {
            self.updating_scale.set(true);
            if should_update_range(self.last_duration_ms.get(), duration_ms) {
                self.last_duration_ms.set(duration_ms);
                // A zero-length range would make the scale's drag handle
                // meaningless (and GTK dislikes a max == min range); floor
                // it at 1 ms.
                self.scale.set_range(0.0, duration_ms.max(1) as f64);
            }
            self.scale.set_value(position_ms as f64);
            self.updating_scale.set(false);
        }

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
    /// milliseconds whenever the user changes it — but never for
    /// programmatic updates (`set_position`/`set_state`, guarded by
    /// `updating_scale`), and, for pointer drags/clicks specifically, only
    /// once, on release (guarded by `dragging`; see the module doc comment).
    /// Concretely: a pointer drag or click-to-position jump seeks exactly
    /// once, to wherever the user let go; keyboard (arrow-key) or
    /// scroll-wheel adjustments — which never touch `dragging` — still seek
    /// immediately on each `value-changed`, same as before this fix.
    pub fn connect_seek<F: Fn(i64) + 'static>(&self, f: F) {
        let f = Rc::new(f);

        let updating_scale = self.updating_scale.clone();
        let dragging = self.dragging.clone();
        let value_changed_f = Rc::clone(&f);
        self.scale.connect_value_changed(move |scale| {
            if updating_scale.get() || dragging.get() {
                return;
            }
            value_changed_f(scale.value() as i64);
        });

        // A `GestureClick` added alongside the scale's own built-in
        // slider-drag handling. GTK4 lets multiple independent (ungrouped)
        // controllers observe the same widget — one gesture claiming a
        // sequence doesn't deny others on the same widget — so this purely
        // brackets "pointer down" .. "pointer up" without interfering with
        // how the handle actually moves; that's still entirely GtkRange's
        // own doing.
        let click = gtk4::GestureClick::new();

        let press_dragging = self.dragging.clone();
        click.connect_pressed(move |_, _, _, _| {
            press_dragging.set(true);
        });

        // Shared by `released` and `cancel`: both end the drag the same
        // way — clear the flag and fire exactly one seek to wherever the
        // scale's value ended up. A `cancel` (sequence denied, e.g. an
        // ancestor widget claims it as a scroll) is rare for this scale
        // (it doesn't sit inside anything that pans), but handling it keeps
        // `dragging` from ever getting stuck `true` if it happens.
        let end_drag: Rc<dyn Fn()> = {
            let dragging = self.dragging.clone();
            let scale = self.scale.downgrade();
            let f = Rc::clone(&f);
            Rc::new(move || {
                dragging.set(false);
                if let Some(scale) = scale.upgrade() {
                    f(scale.value() as i64);
                }
            })
        };

        let released_end_drag = Rc::clone(&end_drag);
        click.connect_released(move |_, _, _, _| released_end_drag());

        let cancel_end_drag = Rc::clone(&end_drag);
        click.connect_cancel(move |_, _| cancel_end_drag());

        self.scale.add_controller(click);
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

/// Whether a position tick should move the scale's handle — pulled out of
/// `set_position` as a pure predicate purely so the drag guard (see the
/// module doc comment) can be unit tested without spinning up any GTK
/// widgets, following the same `empty_state_for`-style pattern already used
/// in `track_list.rs`.
fn should_apply_position_tick(dragging: bool) -> bool {
    !dragging
}

/// Whether `set_position` needs to touch `scale.set_range` at all — only
/// when the duration actually changed since the last call (it's static per
/// track, so this is normally `false` on every tick but the first after a
/// track change). Pure for the same reason as `should_apply_position_tick`.
fn should_update_range(last_duration_ms: i64, duration_ms: i64) -> bool {
    duration_ms != last_duration_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_tick_applies_when_not_dragging() {
        assert!(should_apply_position_tick(false));
    }

    #[test]
    fn position_tick_suppressed_while_dragging() {
        assert!(!should_apply_position_tick(true));
    }

    #[test]
    fn range_updates_when_duration_changes() {
        assert!(should_update_range(0, 180_000));
        assert!(should_update_range(180_000, 0));
    }

    #[test]
    fn range_update_skipped_when_duration_unchanged() {
        assert!(!should_update_range(180_000, 180_000));
        assert!(!should_update_range(0, 0));
    }
}
