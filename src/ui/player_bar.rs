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
//!    `set_range`) entirely, and ending the drag fires exactly one seek to
//!    wherever the user left the handle. That guard's `pressed` handler
//!    must run at GTK's CAPTURE propagation phase, not the default BUBBLE —
//!    see the comment on `click.set_propagation_phase(..)` in `connect_seek`
//!    for why (a plain click-to-jump on the trough otherwise fires a seek
//!    on press too, before `dragging` is set, doubling up with the seek on
//!    release).
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
//! - **More end-of-drag signals, idempotently** (`connect_seek`):
//!   `released`, `cancel`, `unpaired-release`, *and* `stopped` (the latter
//!   gated on the gesture no longer being active — see the comment at its
//!   connection for why) are all treated as end-of-drag.
//!   `unpaired-release` catches releases the gesture couldn't match to its
//!   press; a non-active `stopped` catches resets where neither `released`
//!   nor `cancel` was delivered. Because several of these can fire for the
//!   same interaction (e.g. `released` then `stopped`), the shared handler
//!   is idempotent: it swaps `dragging` to `false` and only seeks if the
//!   swap found `true`, so one interaction still produces exactly one seek.
//! - **Self-healing in `set_position`**: even if *no* end signal ever
//!   arrives, `set_position` cross-checks a set `dragging` flag against
//!   `Gesture::is_active()` on the guard's own gesture. `dragging == true`
//!   while the gesture is not active means the guard is stale — it is
//!   reset (with a `warn!`) and the update proceeds. Worst case, a stuck
//!   guard now costs one 500 ms tick instead of the rest of the session.
//!   `set_state(Stopped)`, `set_track`, and `clear_track` additionally
//!   clear the guard unconditionally: a track change or stop always ends
//!   any drag-in-progress as far as this bar is concerned.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::pango;
use gtk4::prelude::*;

use crate::format::format_duration;
use crate::player::PlaybackState;
use crate::queue::Repeat;
use crate::ui::strings;

const ICON_PLAY: &str = "media-playback-start-symbolic";
const ICON_PAUSE: &str = "media-playback-pause-symbolic";
const ICON_SHUFFLE: &str = "media-playlist-shuffle-symbolic";
const ICON_PREVIOUS: &str = "media-skip-backward-symbolic";
const ICON_NEXT: &str = "media-skip-forward-symbolic";
const ICON_REPEAT_ALL: &str = "media-playlist-repeat-symbolic";
const ICON_REPEAT_ONE: &str = "media-playlist-repeat-song-symbolic";
/// Applied to the repeat button while `Repeat::Off`, so it reads as inactive
/// without a third icon asset — the same generic "de-emphasize" style class
/// `artist_label` already uses (see `PlayerBar::new`), which GTK's Adwaita
/// theme renders as reduced-opacity text/icon content on any widget.
const REPEAT_OFF_CSS_CLASS: &str = "dim-label";

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

        // Transport strip, mockup order (design mockup 7a): Shuffle | Prev |
        // Play | Next | Repeat, centered around the seek scale.
        let shuffle_button = gtk4::ToggleButton::builder()
            .icon_name(ICON_SHUFFLE)
            .tooltip_text(strings::SHUFFLE)
            .valign(gtk4::Align::Center)
            .build();

        let prev_button = gtk4::Button::from_icon_name(ICON_PREVIOUS);
        prev_button.set_tooltip_text(Some(strings::PREVIOUS));
        prev_button.set_valign(gtk4::Align::Center);
        // No queue to step through until a view has been activated at least
        // once (see `set_transport_enabled`, called by
        // `player_controller::PlayerController::play_from_view`).
        prev_button.set_sensitive(false);

        let play_pause_button = gtk4::Button::from_icon_name(ICON_PLAY);
        play_pause_button.set_tooltip_text(Some(strings::PLAY));
        play_pause_button.set_valign(gtk4::Align::Center);

        let next_button = gtk4::Button::from_icon_name(ICON_NEXT);
        next_button.set_tooltip_text(Some(strings::NEXT));
        next_button.set_valign(gtk4::Align::Center);
        next_button.set_sensitive(false);

        let repeat_button = gtk4::Button::from_icon_name(ICON_REPEAT_ALL);
        repeat_button.set_tooltip_text(Some(strings::REPEAT));
        repeat_button.set_valign(gtk4::Align::Center);

        let position_label = gtk4::Label::new(Some(ZERO_TIME_LABEL));
        let duration_label = gtk4::Label::new(Some(ZERO_TIME_LABEL));

        let scale = gtk4::Scale::new(gtk4::Orientation::Horizontal, None::<&gtk4::Adjustment>);
        scale.set_range(0.0, 1.0);
        scale.set_draw_value(false);
        scale.set_hexpand(true);
        scale.set_valign(gtk4::Align::Center);
        scale.set_tooltip_text(Some(strings::PLAYBACK_POSITION));

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
        volume_button.set_tooltip_text(Some(strings::VOLUME));
        volume_button.set_valign(gtk4::Align::Center);

        let bar = gtk4::ActionBar::new();
        bar.pack_start(&track_box);
        bar.set_center_widget(Some(&center_box));
        bar.pack_end(&volume_button);
        // No track loaded yet — nothing to play, pause, or seek until the
        // player reports a non-stopped state (see `set_state`).
        bar.set_sensitive(false);

        let bar = Self {
            bar,
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
            seek_gesture: RefCell::new(None),
            last_duration_ms: Cell::new(0),
        };
        // Starts at Repeat::Off — matches Queue::default() (see queue.rs).
        bar.set_repeat_indicator(Repeat::Off);
        bar
    }

    /// The root widget to embed via `ToolbarView::add_bottom_bar`.
    pub fn widget(&self) -> &gtk4::ActionBar {
        &self.bar
    }

    /// Shows `title`/`artist` in the left-hand labels. Called on row
    /// activation with data already in hand from the `Track` (see
    /// `player_controller.rs`) — no extra DB query needed.
    pub fn set_track(&self, title: &str, artist: &str) {
        // Only clear the drag guard if no gesture is currently active.
        // If the user is mid-drag while a track auto-advances (e.g. EOS),
        // keep the guard set so the next position tick doesn't yank the
        // handle — the gesture's own end-of-drag signals will clear it.
        if should_clear_drag_guard_on_track_change(self.seek_gesture_is_active()) {
            self.dragging.set(false);
        }
        self.title_label.set_text(title);
        self.artist_label.set_text(artist);
    }

    /// Clears the track labels back to empty — used when playback stops with
    /// no track active. Queueing (auto-advance) is a later stage; see the
    /// module doc comment in `track_list.rs`.
    pub fn clear_track(&self) {
        // Same gesture-activity guard as `set_track` — only clear the drag
        // guard if no drag is currently in progress.
        if should_clear_drag_guard_on_track_change(self.seek_gesture_is_active()) {
            self.dragging.set(false);
        }
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
    ///
    /// Before honoring `dragging`, the guard is cross-checked against the
    /// gesture that owns it: a set flag whose gesture is no longer active
    /// is a stuck guard (the field bug in the module doc comment) and is
    /// self-healed here rather than trusted.
    pub fn set_position(&self, position_ms: i64, duration_ms: i64) {
        let duration_ms = duration_ms.max(0);
        let position_ms = position_ms.clamp(0, duration_ms);

        if should_self_heal(self.dragging.get(), self.seek_gesture_is_active()) {
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
    /// "When the drag ends" deliberately spans four signals (`released`,
    /// `cancel`, `unpaired-release`, `stopped`), funneled through one
    /// idempotent handler — see the module doc comment for the field bug
    /// that forced this redundancy.
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
        // exactly one seek to wherever the scale's value ended up. Four
        // signals funnel here (`released`, `cancel`, `unpaired-release`,
        // `stopped`) because the field bug in the module doc comment showed
        // that on a real desktop stack `released`/`cancel` alone are not
        // guaranteed to arrive for this observing gesture; several of the
        // four can fire for one interaction, so the handler is idempotent —
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
            let scale = self.scale.downgrade();
            let f = Rc::clone(&f);
            Rc::new(move || {
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

        let released_end_drag = Rc::clone(&end_drag);
        click.connect_released(move |_, _, _, _| released_end_drag());

        let cancel_end_drag = Rc::clone(&end_drag);
        click.connect_cancel(move |_, _| cancel_end_drag());

        let unpaired_end_drag = Rc::clone(&end_drag);
        click.connect_unpaired_release(move |_, _, _, _, _| unpaired_end_drag());

        // `stopped` needs a gate the other three don't: GestureClick also
        // emits it *mid-drag*, the moment the pointer exceeds the
        // multi-click distance threshold (the press stops counting toward
        // double-clicks) — while the sequence is still active and the user
        // is still dragging. Ending the drag there would reintroduce the
        // exact mid-drag handle-yanking the guard exists to prevent, so
        // `stopped` only ends the drag when the gesture has genuinely let
        // go of its sequence (`!is_active()` — e.g. a reset after the
        // sequence was taken away without `cancel` ever being delivered).
        let stopped_end_drag = Rc::clone(&end_drag);
        click.connect_stopped(move |gesture| {
            if !gesture.is_active() {
                stopped_end_drag();
            }
        });

        // Kept for `set_position`'s self-heal cross-check (see the module
        // doc comment); the borrow is confined to this one statement.
        *self.seek_gesture.borrow_mut() = Some(click.clone());

        self.scale.add_controller(click);
    }

    /// Wires the volume button: `f` is called with a `0.0..=1.0` value on
    /// every user change.
    pub fn connect_volume_changed<F: Fn(f64) + 'static>(&self, f: F) {
        self.volume_button
            .connect_value_changed(move |_, value| f(value));
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
    /// state on every user click. Nothing in this codebase sets
    /// `shuffle_button`'s state programmatically (yet — Stage 2 settings
    /// persistence may), so unlike `connect_seek`'s `updating_scale` guard
    /// there is no reentrancy hazard to guard against here today: `connect_
    /// toggled` only ever fires from a real user click. If a future caller
    /// does call `shuffle_button.set_active` programmatically, add an
    /// `updating_*`-style guard Cell at that point, following the same
    /// pattern documented on `updating_scale` above — `queue::Queue::
    /// set_shuffle` is itself idempotent for a same-value call, so a missing
    /// guard would cause at worst a redundant no-op reshuffle, not a loop.
    pub fn connect_shuffle_toggled<F: Fn(bool) + 'static>(&self, f: F) {
        self.shuffle_button
            .connect_toggled(move |button| f(button.is_active()));
    }

    /// Wires the repeat button; `f` is called on every click with no
    /// arguments — the caller cycles the repeat mode and reports the new
    /// value back via `set_repeat_indicator`.
    pub fn connect_repeat_clicked<F: Fn() + 'static>(&self, f: F) {
        self.repeat_button.connect_clicked(move |_| f());
    }

    /// Enables/disables the previous/next buttons — insensitive whenever the
    /// queue is empty (nothing to step to), independent of the rest of the
    /// bar's Playing/Paused/Stopped-driven sensitivity (`set_state`).
    pub fn set_transport_enabled(&self, enabled: bool) {
        self.prev_button.set_sensitive(enabled);
        self.next_button.set_sensitive(enabled);
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

/// Whether a position tick should move the scale's handle — pulled out of
/// `set_position` as a pure predicate purely so the drag guard (see the
/// module doc comment) can be unit tested without spinning up any GTK
/// widgets, following the same `empty_state_for`-style pattern already used
/// in `track_list.rs`.
fn should_apply_position_tick(dragging: bool) -> bool {
    !dragging
}

/// Whether `set_track`/`clear_track` should clear the `dragging` flag: only
/// when the seek gesture is not currently active. If the user is mid-drag
/// while a track auto-advances (e.g. EOS), keeping the flag set prevents
/// the next `set_position` tick from yanking the handle; the gesture's
/// own end-of-drag signals will clear it properly.
fn should_clear_drag_guard_on_track_change(gesture_active: bool) -> bool {
    !gesture_active
}

/// Whether `set_position` should treat the `dragging` guard as stuck and
/// reset it: the flag says a drag is in progress, but the gesture that owns
/// the flag isn't actually tracking any pointer sequence — so no end-of-drag
/// signal is ever going to clear it (the field bug in the module doc
/// comment). Pure for the same testability reason as
/// `should_apply_position_tick`.
fn should_self_heal(dragging: bool, gesture_active: bool) -> bool {
    dragging && !gesture_active
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
    fn self_heal_only_when_guard_set_but_gesture_inactive() {
        // The stuck-guard field bug: flag set, gesture long done.
        assert!(should_self_heal(true, false));
        // A genuine drag in progress must NOT be healed away.
        assert!(!should_self_heal(true, true));
        // No guard set — nothing to heal, regardless of gesture state.
        assert!(!should_self_heal(false, false));
        assert!(!should_self_heal(false, true));
    }

    #[test]
    fn drag_guard_clears_only_when_gesture_inactive() {
        // Gesture still active (user mid-drag): guard must stay set.
        assert!(!should_clear_drag_guard_on_track_change(true));
        // Gesture no longer active: guard can be safely cleared.
        assert!(should_clear_drag_guard_on_track_change(false));
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
