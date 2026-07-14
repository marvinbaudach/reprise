//! The full-width Library player bar: track title/artist, transport controls,
//! a click-to-seek waveform, playback modes, and volume.
//!
//! `PlayerBar` owns every widget it displays; callers (see
//! `player_controller.rs`) only interact with it through the `set_*`/
//! `connect_*` methods below, never by reaching into its widgets directly.
//! The seek waveform (see `waveform_seek.rs`) has its own click gesture, so —
//! unlike the previous `GtkScale` — there is no programmatic-vs-user or
//! ticks-during-drag feedback-loop guard to maintain here: `set_position`
//! just redraws the played fraction on every tick, and `connect_seek` fires
//! only on a real user click, never on a programmatic position update.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;

use crate::ui::cover_loader::CoverLoader;
use crate::ui::player_bar_layout::{self, PlayerBarWidgets, VOLUME_MAX, VOLUME_MIN};
use crate::ui::strings;
use crate::ui::waveform_seek::WaveformSeek;
use reprise_core::format::format_duration;
use reprise_core::playback::PlaybackState;
use reprise_core::queue::Repeat;

// `pub(super)` (Task 8): `now_playing.rs` reuses these icon names/CSS class
// for its own transport row (DRY) rather than a second, drifting copy.
pub(super) const ICON_PLAY: &str = "media-playback-start-symbolic";
pub(super) const ICON_PAUSE: &str = "media-playback-pause-symbolic";
pub(super) const ICON_SHUFFLE: &str = "media-playlist-shuffle-symbolic";
pub(super) const ICON_PREVIOUS: &str = "media-skip-backward-symbolic";
pub(super) const ICON_NEXT: &str = "media-skip-forward-symbolic";
pub(super) const ICON_REPEAT_ALL: &str = "media-playlist-repeat-symbolic";
pub(super) const ICON_REPEAT_ONE: &str = "media-playlist-repeat-song-symbolic";
/// Applied to the repeat button while `Repeat::Off`, so it reads as inactive
/// without a third icon asset — the same generic "de-emphasize" style class
/// `artist_label` already uses (see `PlayerBar::new`), which GTK's Adwaita
/// theme renders as reduced-opacity text/icon content on any widget.
pub(super) const REPEAT_OFF_CSS_CLASS: &str = "dim-label";

/// Cover/track-info click callback storage (Task 8) — factored out to
/// satisfy clippy's `type_complexity` lint; see `on_expand`'s doc comment.
type ExpandCallback = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

pub struct PlayerBar {
    bar: gtk4::ActionBar,
    /// The currently-playing track's album cover thumbnail, packed at the
    /// start of the bar. Fed by `player_controller.rs`'s `CoverLoader` — this
    /// struct only owns/exposes the widget, never resolves or decodes covers
    /// itself (see `cover_loader.rs`).
    cover: gtk4::Image,
    title_label: gtk4::Label,
    artist_label: gtk4::Label,
    shuffle_button: gtk4::ToggleButton,
    prev_button: gtk4::Button,
    play_pause_button: gtk4::Button,
    next_button: gtk4::Button,
    repeat_button: gtk4::Button,
    position_label: gtk4::Label,
    duration_label: gtk4::Label,
    waveform: WaveformSeek,
    volume_button: gtk4::ScaleButton,
    /// Current track duration (ms) from the latest `set_position`, so
    /// `connect_seek` can turn the waveform's 0..1 fraction into a target ms.
    duration_ms: Rc<Cell<i64>>,
    playback_state: Cell<PlaybackState>,
    queue_has_tracks: Cell<bool>,
    /// True for the duration of `set_shuffle_indicator`'s `set_active`
    /// call, so `connect_shuffle_toggled`'s handler can tell a programmatic
    /// set (MPRIS `Shuffle` write) apart from a real user click. See the
    /// module doc comment's `Update (Stage 3 Task 10)` note.
    updating_shuffle: Rc<Cell<bool>>,
    /// Same guard shape as `updating_shuffle`, for `set_volume_indicator`/
    /// `connect_volume_changed`.
    updating_volume: Rc<Cell<bool>>,
    /// Callback for clicking the cover/track-info area (Task 8); `window.rs`
    /// sets it, post-construction, to push the Now-Playing page. Shared with
    /// `new()`'s gesture; cloned out of the borrow before calling — never
    /// invoked while the borrow is held.
    on_expand: ExpandCallback,
}

impl PlayerBar {
    pub fn new() -> Self {
        let PlayerBarWidgets {
            root: bar,
            info_box,
            cover,
            title_label,
            artist_label,
            shuffle_button,
            prev_button,
            play_pause_button,
            next_button,
            repeat_button,
            position_label,
            duration_label,
            waveform,
            volume_button,
            ..
        } = player_bar_layout::build();

        let on_expand: ExpandCallback = Rc::new(RefCell::new(None));
        let expand_click = gtk4::GestureClick::new();
        let expand_click_on_expand = on_expand.clone();
        // Clone-out-before-call: see the `on_expand` field's doc comment.
        expand_click.connect_released(move |_, _, _, _| {
            let callback = expand_click_on_expand.borrow().clone();
            if let Some(callback) = callback {
                callback();
            }
        });
        info_box.add_controller(expand_click);

        let bar = Self {
            bar,
            cover,
            title_label,
            artist_label,
            shuffle_button,
            prev_button,
            play_pause_button,
            next_button,
            repeat_button,
            position_label,
            duration_label,
            waveform,
            volume_button,
            duration_ms: Rc::new(Cell::new(0)),
            playback_state: Cell::new(PlaybackState::Stopped),
            queue_has_tracks: Cell::new(false),
            updating_shuffle: Rc::new(Cell::new(false)),
            updating_volume: Rc::new(Cell::new(false)),
            on_expand,
        };
        // Starts at Repeat::Off — matches Queue::default() (see queue.rs).
        bar.set_repeat_indicator(Repeat::Off);
        bar
    }

    /// The root widget to place in the full-width Library player-bar shell.
    pub fn widget(&self) -> &gtk4::ActionBar {
        &self.bar
    }

    /// The cover thumbnail widget — `player_controller.rs` feeds it via
    /// `CoverLoader::load_into` after `set_track`.
    pub fn cover_image(&self) -> &gtk4::Image {
        &self.cover
    }

    /// Resets the cover back to the placeholder icon — used when playback
    /// stops with no track active (see `clear_track`).
    pub fn clear_cover(&self) {
        CoverLoader::set_placeholder(&self.cover);
    }

    /// Sets the cover/track-info click callback (Task 8), post-construction —
    /// same injection shape as `set_toast_overlay`.
    pub fn set_on_expand<F: Fn() + 'static>(&self, f: F) {
        *self.on_expand.borrow_mut() = Some(Rc::new(f));
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
        self.clear_cover();
    }

    /// Applies a `PlaybackState`: swaps the play/pause icon and tooltip, and
    /// combines the state with queue availability when deriving sensitivity.
    /// A stopped restored queue remains playable, but its seek scale stays
    /// disabled until a track is actually loaded.
    pub fn set_state(&self, state: PlaybackState) {
        let is_playing = state == PlaybackState::Playing;
        self.play_pause_button
            .set_icon_name(if is_playing { ICON_PAUSE } else { ICON_PLAY });
        let tooltip = if is_playing {
            strings::text(strings::PAUSE)
        } else {
            strings::text(strings::PLAY)
        };
        self.play_pause_button.set_tooltip_text(Some(&tooltip));
        self.playback_state.set(state);
        self.refresh_sensitivity();
        if state == PlaybackState::Stopped {
            self.set_position(0, 0);
        }
    }

    /// Updates the waveform's played fraction and the time labels for a
    /// `Position` event. Click-to-seek only means there is no drag to guard
    /// against, so every tick simply redraws the played/unplayed split.
    pub fn set_position(&self, position_ms: i64, duration_ms: i64) {
        let duration_ms = duration_ms.max(0);
        let position_ms = position_ms.clamp(0, duration_ms);
        self.duration_ms.set(duration_ms);
        let fraction = if duration_ms > 0 {
            position_ms as f64 / duration_ms as f64
        } else {
            0.0
        };
        self.waveform.set_fraction(fraction);
        self.position_label.set_text(&format_duration(position_ms));
        self.duration_label.set_text(&format_duration(duration_ms));
    }

    /// Wires the play/pause button; `f` is called on every click with no
    /// arguments — the caller (which owns the `Player`) decides what
    /// "toggle" means and reports the resulting state back via `set_state`.
    pub fn connect_play_pause<F: Fn() + 'static>(&self, f: F) {
        self.play_pause_button.connect_clicked(move |_| f());
    }

    pub(super) fn smoke_activate_play_pause(&self) {
        self.play_pause_button.emit_clicked();
    }

    /// A cloneable handle to the seek waveform (shared `Rc` state), so an
    /// off-main peak load can hand its results back to the same widget.
    pub(super) fn waveform_handle(&self) -> WaveformSeek {
        self.waveform.clone()
    }

    /// Wires click-to-seek: `f` is called with the target position in
    /// milliseconds whenever the user clicks the waveform. The widget reports
    /// a 0..1 fraction; the latest track duration (from `set_position`)
    /// converts it to ms. No programmatic-update or drag guard is needed —
    /// the waveform never emits on a position tick, only on a real click.
    pub fn connect_seek<F: Fn(i64) + 'static>(&self, f: F) {
        let duration_ms = self.duration_ms.clone();
        self.waveform.connect_seek(move |fraction| {
            let target_ms = (fraction * duration_ms.get() as f64).round() as i64;
            f(target_ms);
        });
    }

    /// Wires the volume button: `f` is called with a `0.0..=1.0` value on
    /// every user change, but never for a programmatic set via `set_volume_
    /// indicator` (guarded by `updating_volume` — same shape as `connect_
    /// shuffle_toggled`'s `updating_shuffle`, added for the same Stage 3
    /// Task 10 reason: MPRIS `Volume` writes now set this button
    /// programmatically, and `gtk::ScaleButton::set_value` fires `value-
    /// changed` regardless of whether code or the user caused it).
    pub fn connect_volume_changed<F: Fn(f64) + 'static>(&self, f: F) {
        let updating_volume = self.updating_volume.clone();
        self.volume_button.connect_value_changed(move |_, value| {
            if updating_volume.get() {
                return;
            }
            f(value);
        });
    }

    /// Sets the volume button's value programmatically — used when an
    /// MPRIS `Volume` write changes the volume externally (Stage 3 Task
    /// 10), so the on-screen control follows. Guarded by `updating_volume`
    /// so this doesn't re-fire `connect_volume_changed`'s callback — see
    /// that method's doc comment.
    pub fn set_volume_indicator(&self, volume: f64) {
        self.updating_volume.set(true);
        self.volume_button
            .set_value(volume.clamp(VOLUME_MIN, VOLUME_MAX));
        self.updating_volume.set(false);
    }

    /// Wires the previous-track button; `f` is called on every click with no
    /// arguments — the caller (which owns the `Queue`) decides what
    /// "previous" resolves to and starts that track's playback itself.
    pub fn connect_previous<F: Fn() + 'static>(&self, f: F) {
        self.prev_button.connect_clicked(move |_| f());
    }

    /// Wires the next-track button; same shape as `connect_previous`.
    pub fn connect_next<F: Fn() + 'static>(&self, f: F) {
        self.next_button.connect_clicked(move |_| f());
    }

    /// Wires the shuffle toggle; `f` is called with the button's new active
    /// state on every user click, but never for a programmatic set via
    /// `set_shuffle_indicator` (guarded by `updating_shuffle`, same
    /// `Rc<Cell<bool>>` shape as `connect_seek`'s `updating_scale`).
    ///
    /// Originally (Stage 2 Task 4) this guard was deferred as YAGNI — nothing
    /// called `shuffle_button.set_active` programmatically yet, so `connect_
    /// toggled` only ever fired from a real user click, and `queue::Queue::
    /// set_shuffle`'s idempotence for a same-value call meant a missing guard
    /// could only cost a redundant no-op reshuffle, not a loop. Stage 3 Task
    /// 10 needs it for real: MPRIS's `Shuffle` writes now call `set_shuffle_
    /// indicator`, and without this guard that would immediately re-dispatch
    /// a `SetShuffle` command right back at the controller — this guard
    /// removes that round-trip entirely rather than continuing to rely on
    /// idempotence to make it harmless.
    pub fn connect_shuffle_toggled<F: Fn(bool) + 'static>(&self, f: F) {
        let updating_shuffle = self.updating_shuffle.clone();
        self.shuffle_button.connect_toggled(move |button| {
            if updating_shuffle.get() {
                return;
            }
            f(button.is_active());
        });
    }

    /// Sets the shuffle toggle's active state programmatically — used when
    /// an MPRIS `Shuffle` write changes the queue's shuffle state
    /// externally (Stage 3 Task 10), so the on-screen button follows.
    /// Guarded by `updating_shuffle` so this doesn't re-fire `connect_
    /// shuffle_toggled`'s callback — see that method's doc comment.
    pub fn set_shuffle_indicator(&self, active: bool) {
        self.updating_shuffle.set(true);
        self.shuffle_button.set_active(active);
        self.updating_shuffle.set(false);
    }

    /// Wires the repeat button; `f` is called on every click with no
    /// arguments — the caller cycles the repeat mode and reports the new
    /// value back via `set_repeat_indicator`.
    pub fn connect_repeat_clicked<F: Fn() + 'static>(&self, f: F) {
        self.repeat_button.connect_clicked(move |_| f());
    }

    /// Updates queue-dependent controls and the stopped bar's resumability.
    /// Previous/next remain insensitive without a current queue position;
    /// Play remains usable while stopped when a queue can be resumed.
    pub fn set_transport_enabled(&self, enabled: bool) {
        self.queue_has_tracks.set(enabled);
        self.prev_button.set_sensitive(enabled);
        self.next_button.set_sensitive(enabled);
        self.refresh_sensitivity();
    }

    fn refresh_sensitivity(&self) {
        let state = self.playback_state.get();
        let queue_has_tracks = self.queue_has_tracks.get();
        self.bar
            .set_sensitive(super::player_bar_state::bar_should_be_sensitive(
                state,
                queue_has_tracks,
            ));
        self.play_pause_button
            .set_sensitive(state != PlaybackState::Stopped || queue_has_tracks);
        self.waveform
            .widget()
            .set_sensitive(state != PlaybackState::Stopped);
    }

    /// Reflects the queue's repeat mode on the repeat button: `All`/`One`
    /// each get a distinct icon, `Off` reuses the `All` icon dimmed via
    /// `REPEAT_OFF_CSS_CLASS` (see its doc comment) rather than a third icon
    /// asset.
    pub fn set_repeat_indicator(&self, repeat: Repeat) {
        let (icon, is_off) = match repeat {
            Repeat::Off => (ICON_REPEAT_ALL, true),
            Repeat::All => (ICON_REPEAT_ALL, false),
            Repeat::One => (ICON_REPEAT_ONE, false),
        };
        self.repeat_button.set_icon_name(icon);
        if is_off {
            self.repeat_button.add_css_class(REPEAT_OFF_CSS_CLASS);
        } else {
            self.repeat_button.remove_css_class(REPEAT_OFF_CSS_CLASS);
        }
    }
}

impl Default for PlayerBar {
    fn default() -> Self {
        Self::new()
    }
}
