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
//!    playback position out from under the user's cursor. A raw capture-
//!    phase event controller brackets the physical pointer lifetime even if
//!    GtkRange denies the accompanying `GestureClick` observer. While
//!    `dragging` is `true`, `set_position` skips `set_value` and `set_range`;
//!    release fires exactly one seek to the final handle position.
//!
//! **The `dragging` guard must never trust GTK to clear it.** A field bug
//! (found on a real desktop, invisible to headless testing): a pointer
//! interaction with the scale set `dragging` via the capture-phase
//! `pressed` handler, but neither `released` nor `cancel` was ever
//! delivered to this observing gesture afterwards — plausibly because
//! GtkRange's own internal drag gesture claimed the event sequence
//! mid-gesture and the denied observer's teardown signals never arrived on
//! that stack. The guard stayed `true` forever: every later `set_position`
//! skipped the scale update (frozen progress bar) and the release-side
//! seek never fired (dead seeking). Two layers of defense now exist:
//!
//! - **Independent raw pointer lifetime plus idempotent gesture fallbacks**
//!   (`connect_seek`): raw primary-button/touch release, `released`,
//!   `cancel`, `unpaired-release`, and `stopped` all feed one handler.
//!   `cancel`/`stopped` are ignored while the raw pointer is still down,
//!   because GtkRange may have claimed the observer rather than ended the
//!   physical drag. Multiple end signals still produce exactly one seek.
//! - **Self-healing in `set_position`**: even if *no* end signal ever
//!   arrives, `set_position` cross-checks a set `dragging` flag against
//!   both the raw pointer state and `Gesture::is_active()`. Only when all
//!   observers are inactive is the guard stale and reset. This extra raw
//!   check is what prevents a denied observer from re-enabling position
//!   ticks while the user is still holding the handle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::{gdk, pango, prelude::*};

use crate::ui::cover_loader::CoverLoader;
use crate::ui::player_bar_seek::{
    should_apply_position_tick, should_clear_drag_guard_on_track_change,
    should_finish_observer_cancel, should_finish_observer_stop, should_self_heal,
    should_update_range,
};
use crate::ui::strings;
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

/// Pixel size of the player bar's cover thumbnail — matches
/// `reprise_core::cover::ThumbnailSize::Bar`.
const COVER_PIXEL_SIZE: i32 = 48;

/// CSS class applied to the player bar's cover `gtk4::Image`, for styling
/// hooks (rounded corners etc.) without reaching into `PlayerBar`'s widgets.
const COVER_CSS_CLASS: &str = "player-bar-cover";

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
    scale: gtk4::Scale,
    volume_button: gtk4::ScaleButton,
    /// True while `set_position`/`set_state` are the ones setting `scale`'s
    /// value, so `connect_seek`'s handler can tell a programmatic update
    /// apart from the user dragging the scale. See the module doc comment.
    updating_scale: Rc<Cell<bool>>,
    /// True while the user has the pointer down on `scale` (between a
    /// `GestureClick` press and whichever end-of-drag signal arrives first —
    /// wired in `connect_seek`). Checked by `set_position` so a mid-drag
    /// position tick can't yank the handle. See the module doc comment,
    /// including why this guard is deliberately never trusted to be cleared
    /// by GTK alone.
    dragging: Rc<Cell<bool>>,
    /// Raw physical button/touch state from an `EventControllerLegacy`.
    /// Unlike an observing `GestureClick`, this remains true if GtkRange's
    /// internal gesture claims the sequence while the pointer is held.
    pointer_down: Rc<Cell<bool>>,
    /// The seek guard's `GestureClick`, kept so `set_position` can
    /// cross-check a set `dragging` flag against `Gesture::is_active()` and
    /// self-heal a stuck guard (see the module doc comment). `None` until
    /// `connect_seek` wires it; holding the gesture here creates no
    /// reference cycle — the gesture's closures only capture `Cell`s and a
    /// weak `scale` reference, never `PlayerBar` itself.
    seek_gesture: RefCell<Option<gtk4::GestureClick>>,
    /// The `duration_ms` passed to the most recent `set_position` call, so
    /// it can skip `scale.set_range` when the duration hasn't actually
    /// changed (it's static per track — this is only ever a real change on
    /// a track switch, not on every 500 ms tick).
    last_duration_ms: Cell<i64>,
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
        let cover = gtk4::Image::new();
        cover.set_pixel_size(COVER_PIXEL_SIZE);
        cover.add_css_class(COVER_CSS_CLASS);
        CoverLoader::set_placeholder(&cover);

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

        // Transport strip, mockup order (design mockup 7a): Shuffle | Prev |
        // Play | Next | Repeat, centered around the seek scale.
        let shuffle_button = gtk4::ToggleButton::builder()
            .icon_name(ICON_SHUFFLE)
            .tooltip_text(strings::text(strings::SHUFFLE))
            .valign(gtk4::Align::Center)
            .build();

        let prev_button = gtk4::Button::from_icon_name(ICON_PREVIOUS);
        prev_button.set_tooltip_text(Some(&strings::text(strings::PREVIOUS)));
        prev_button.set_valign(gtk4::Align::Center);
        // No queue to step through until a view has been activated at least
        // once (see `set_transport_enabled`, called by
        // `player_controller::PlayerController::play_from_view`).
        prev_button.set_sensitive(false);

        let play_pause_button = gtk4::Button::from_icon_name(ICON_PLAY);
        play_pause_button.set_tooltip_text(Some(&strings::text(strings::PLAY)));
        play_pause_button.set_valign(gtk4::Align::Center);

        let next_button = gtk4::Button::from_icon_name(ICON_NEXT);
        next_button.set_tooltip_text(Some(&strings::text(strings::NEXT)));
        next_button.set_valign(gtk4::Align::Center);
        next_button.set_sensitive(false);

        let repeat_button = gtk4::Button::from_icon_name(ICON_REPEAT_ALL);
        repeat_button.set_tooltip_text(Some(&strings::text(strings::REPEAT)));
        repeat_button.set_valign(gtk4::Align::Center);

        let position_label = gtk4::Label::new(Some(ZERO_TIME_LABEL));
        let duration_label = gtk4::Label::new(Some(ZERO_TIME_LABEL));

        let scale = gtk4::Scale::new(gtk4::Orientation::Horizontal, None::<&gtk4::Adjustment>);
        scale.set_range(0.0, 1.0);
        scale.set_draw_value(false);
        scale.set_hexpand(true);
        scale.set_valign(gtk4::Align::Center);
        scale.set_tooltip_text(Some(&strings::text(strings::PLAYBACK_POSITION)));

        let center_box = gtk4::Box::new(gtk4::Orientation::Horizontal, CENTER_BOX_SPACING);
        center_box.append(&shuffle_button);
        center_box.append(&prev_button);
        center_box.append(&play_pause_button);
        center_box.append(&position_label);
        center_box.append(&scale);
        center_box.append(&duration_label);
        center_box.append(&next_button);
        center_box.append(&repeat_button);
        center_box.set_hexpand(true);
        center_box.set_valign(gtk4::Align::Center);

        let volume_button =
            gtk4::ScaleButton::new(VOLUME_MIN, VOLUME_MAX, VOLUME_STEP, &VOLUME_ICONS);
        volume_button.set_value(VOLUME_DEFAULT);
        volume_button.set_tooltip_text(Some(&strings::text(strings::VOLUME)));
        volume_button.set_valign(gtk4::Align::Center);

        // Task 8: cover + track_box (no buttons) share one clickable area —
        // NOT the whole `ActionBar`, which would swallow button clicks.
        let info_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        info_box.append(&cover);
        info_box.append(&track_box);

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

        let bar = gtk4::ActionBar::new();
        bar.pack_start(&info_box);
        bar.set_center_widget(Some(&center_box));
        bar.pack_end(&volume_button);
        // No track loaded yet — nothing to play, pause, or seek until the
        // player reports a non-stopped state (see `set_state`).
        bar.set_sensitive(false);

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
            scale,
            volume_button,
            updating_scale: Rc::new(Cell::new(false)),
            dragging: Rc::new(Cell::new(false)),
            pointer_down: Rc::new(Cell::new(false)),
            seek_gesture: RefCell::new(None),
            last_duration_ms: Cell::new(0),
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

    /// The root widget to embed via `ToolbarView::add_bottom_bar`.
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
        // Only clear the drag guard if neither observer is currently active.
        // If the user is mid-drag while a track auto-advances (e.g. EOS),
        // keep the guard set so the next position tick doesn't yank the
        // handle — the raw/gesture end signals will clear it.
        if should_clear_drag_guard_on_track_change(
            self.pointer_down.get(),
            self.seek_gesture_is_active(),
        ) {
            self.dragging.set(false);
        }
        self.title_label.set_text(title);
        self.artist_label.set_text(artist);
    }

    /// Clears the track labels back to empty — used when playback stops with
    /// no track active. Queueing (auto-advance) is a later stage; see the
    /// module doc comment in `track_list.rs`.
    pub fn clear_track(&self) {
        // Same observer-activity guard as `set_track` — only clear the drag
        // guard if no drag is currently in progress.
        if should_clear_drag_guard_on_track_change(
            self.pointer_down.get(),
            self.seek_gesture_is_active(),
        ) {
            self.dragging.set(false);
        }
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
            // Force-clear any stale drag state: e.g. the track finishes or
            // errors out while the user still has the pointer down on the
            // scale, so there's no `released`/`cancel` to clear it via the
            // normal `connect_seek` path. Without this, `set_position` below
            // would (correctly, per its own contract) skip resetting the
            // handle because it thinks a drag is still in progress.
            self.pointer_down.set(false);
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
    ///
    /// Before honoring `dragging`, the guard is cross-checked against both
    /// raw physical-down state and gesture activity. Only a set flag with
    /// neither observer active is stale and self-healed.
    pub fn set_position(&self, position_ms: i64, duration_ms: i64) {
        let duration_ms = duration_ms.max(0);
        let position_ms = position_ms.clamp(0, duration_ms);

        if should_self_heal(
            self.dragging.get(),
            self.pointer_down.get(),
            self.seek_gesture_is_active(),
        ) {
            tracing::warn!("drag guard was stuck; self-healing");
            self.dragging.set(false);
        }

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

    /// Whether the seek guard's gesture is currently tracking a pointer
    /// sequence. `false` when `connect_seek` hasn't wired a gesture (there
    /// is nothing that could legitimately be holding `dragging` then, so a
    /// set flag is stale by definition). The borrow is hoisted into this
    /// one expression and dropped before the caller does anything else.
    fn seek_gesture_is_active(&self) -> bool {
        self.seek_gesture
            .borrow()
            .as_ref()
            .is_some_and(gtk4::prelude::GestureExt::is_active)
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

    /// Wires the seek scale: `f` is called with the target position in
    /// milliseconds whenever the user changes it — but never for
    /// programmatic updates (`set_position`/`set_state`, guarded by
    /// `updating_scale`), and, for pointer drags/clicks specifically, only
    /// once, when the drag ends (guarded by `dragging`; see the module doc
    /// comment and the capture-phase note on the `GestureClick` below).
    /// Concretely: a pointer drag or click-to-position jump seeks exactly
    /// once, to wherever the user let go — including a plain click on the
    /// trough, which GTK jumps to synchronously on press when
    /// `gtk-primary-button-warps-slider` is set (the GNOME default); keyboard
    /// (arrow-key) and scroll-wheel adjustments — which never touch
    /// `dragging` — still seek immediately on each `value-changed`.
    ///
    /// "When the drag ends" is observed by both raw pointer/touch release
    /// and gesture teardown fallbacks, all funneled through one idempotent
    /// handler — see the module doc comment for the field bugs that forced
    /// this redundancy.
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

        // A `GestureClick` added alongside the scale's built-in drag. It is
        // a fallback observer only: GtkRange may claim/deny its sequence,
        // which is why the raw controller below owns physical-down truth.
        let click = gtk4::GestureClick::new();
        click.set_button(gdk::BUTTON_PRIMARY);

        // CAPTURE, not the default BUBBLE: GtkRange registers its own
        // internal click gesture on `scale` first, also at BUBBLE phase, and
        // (with the GNOME default `gtk-primary-button-warps-slider=true`) a
        // primary click on the trough jumps the handle synchronously inside
        // that internal gesture's press handler
        // (`gtk_range_click_gesture_pressed` in upstream gtk/gtkrange.c),
        // firing `value-changed` on press — *before* a same-phase (BUBBLE)
        // `pressed` handler here would get to set `dragging`. A single event
        // is dispatched through capture phase (root→target), then bubble
        // phase (target→root), in that order, so a CAPTURE-phase controller
        // on this same widget always runs before any BUBBLE-phase one for
        // the same press. That flips `dragging` true first, so the
        // synchronous jump-on-press's `value-changed` is correctly
        // suppressed by `connect_value_changed` below instead of sneaking
        // through as an extra seek. Deliberately not
        // `set_state(EventSequenceState::Claimed)`-ing the press: GTK's own
        // click-to-jump / drag-start handling must still run normally
        // afterward — this gesture only ever *observes*, never consumes.
        click.set_propagation_phase(gtk4::PropagationPhase::Capture);

        let press_dragging = self.dragging.clone();
        click.connect_pressed(move |_, _, _, _| {
            press_dragging.set(true);
        });

        // Shared by every end-of-drag signal: clear the flag and fire
        // exactly one seek to wherever the scale's value ended up. Raw
        // release and all gesture fallbacks funnel here because no single
        // observer is reliable on every GTK stack; several can fire for
        // one interaction, so the handler is idempotent —
        // `Cell::replace(false)` both clears the flag and tells us whether
        // this call is the first (and therefore the one that seeks).
        //
        // Reading `scale.value()` here is safe even though this whole
        // gesture (including this `released` callback) runs at CAPTURE
        // phase, i.e. *before* GtkRange's own BUBBLE-phase release handling
        // for the same release event: GTK dispatches one event at a time,
        // fully through capture→target→bubble, before the next event is
        // even generated. Whatever moves the value for this gesture — the
        // click-to-jump on press, or the continuous updates GtkRange applies
        // as separate motion events arrive during an actual drag — happens
        // as earlier, already-fully-dispatched events, strictly before this
        // release event exists at all. So by the time *any* handler for
        // `released` runs, ours included, `scale.value()` already holds
        // GTK's final applied position; our phase relative to GtkRange's own
        // release handler (which doesn't touch the value — it only ends the
        // internal drag/grab state) doesn't matter here.
        let end_drag: Rc<dyn Fn()> = {
            let dragging = self.dragging.clone();
            let pointer_down = self.pointer_down.clone();
            let scale = self.scale.downgrade();
            let f = Rc::clone(&f);
            Rc::new(move || {
                pointer_down.set(false);
                // Idempotent: only the first end signal for an interaction
                // finds `true` here and seeks; the rest are no-ops.
                if !dragging.replace(false) {
                    return;
                }
                if let Some(scale) = scale.upgrade() {
                    f(scale.value() as i64);
                }
            })
        };

        // GtkRange's internal drag can deny the observing GestureClick
        // while the physical button is still held. This raw capture-phase
        // controller independently brackets the real button/touch lifetime
        // and cannot be denied by gesture arbitration. It never consumes
        // the event or interferes with GtkRange's own handling.
        let raw = gtk4::EventControllerLegacy::new();
        raw.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let raw_pointer_down = self.pointer_down.clone();
        let raw_dragging = self.dragging.clone();
        let raw_end_drag = Rc::clone(&end_drag);
        raw.connect_event(move |_, event| {
            let primary_button = event
                .downcast_ref::<gdk::ButtonEvent>()
                .is_some_and(|button| button.button() == gdk::BUTTON_PRIMARY);
            match event.event_type() {
                gdk::EventType::ButtonPress if primary_button => {
                    raw_pointer_down.set(true);
                    raw_dragging.set(true);
                }
                gdk::EventType::TouchBegin => {
                    raw_pointer_down.set(true);
                    raw_dragging.set(true);
                }
                gdk::EventType::ButtonRelease if primary_button => raw_end_drag(),
                gdk::EventType::TouchEnd | gdk::EventType::TouchCancel => raw_end_drag(),
                _ => {}
            }
            gtk4::glib::Propagation::Proceed
        });
        self.scale.add_controller(raw);

        let released_end_drag = Rc::clone(&end_drag);
        click.connect_released(move |_, _, _, _| released_end_drag());

        let cancel_end_drag = Rc::clone(&end_drag);
        let cancel_pointer_down = self.pointer_down.clone();
        click.connect_cancel(move |_, _| {
            if should_finish_observer_cancel(cancel_pointer_down.get()) {
                cancel_end_drag();
            }
        });

        let unpaired_end_drag = Rc::clone(&end_drag);
        click.connect_unpaired_release(move |_, _, _, _, _| unpaired_end_drag());

        // `stopped` needs a gate the other fallbacks don't: GestureClick also
        // emits it *mid-drag*, the moment the pointer exceeds the
        // multi-click distance threshold (the press stops counting toward
        // double-clicks) — while the sequence is still active and the user
        // is still dragging. Ending the drag there would reintroduce the
        // exact mid-drag handle-yanking the guard exists to prevent, so
        // `stopped` only ends the drag after both the raw pointer and gesture
        // are inactive (e.g. a reset after no `cancel` was delivered).
        let stopped_end_drag = Rc::clone(&end_drag);
        let stopped_pointer_down = self.pointer_down.clone();
        click.connect_stopped(move |gesture| {
            if should_finish_observer_stop(stopped_pointer_down.get(), gesture.is_active()) {
                stopped_end_drag();
            }
        });

        // Kept for `set_position`'s self-heal cross-check (see the module
        // doc comment); the borrow is confined to this one statement.
        *self.seek_gesture.borrow_mut() = Some(click.clone());

        self.scale.add_controller(click);
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
        self.scale.set_sensitive(state != PlaybackState::Stopped);
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
