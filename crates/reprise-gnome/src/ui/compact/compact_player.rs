//! Single 430×76 px Mini-Player view.
//!
//! `CompactPlayer` wraps an `Rc<Inner>` so it is cheap to clone and can be
//! stored in both `PlayerController` and `MinimalView` without lifetime
//! gymnastics. All public methods are `pub(in crate::ui)` (visible within the `ui`
//! module) to match the existing GTK component conventions in this codebase.
//!
//! ## Callback discipline
//! Restore/preferences/always-on-top/quit callbacks are routed through the
//! `CompactMenu` action group so they fire identically whether triggered by
//! the overlay button, the right-click menu, or a keyboard shortcut. This
//! also keeps `activate_restore_for_test` working at no extra cost.
//!
//! ## RefCell discipline
//! No `Ref`/`RefMut` is held across a GTK call; every borrow is cloned out
//! before the owning `RefCell` lock is released.

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::gdk;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::AnimationExt;
use reprise_core::playback::PlaybackState;

use super::compact_player_layouts::{build_mini, MiniWidgets};
use super::compact_player_menu::{self, CompactMenu};
use super::compact_player_scroll;
use super::cover_loader::CoverLoader;
use super::motion;
use super::strings;
use super::style;

const ICON_PLAY: &str = "media-playback-start-symbolic";
const ICON_PAUSE: &str = "media-playback-pause-symbolic";

/// How long the volume feedback bar stays visible after a scroll event.
const VOL_BAR_LINGER_MS: u64 = 800;
/// Delay before the hover overlay fades out after the pointer leaves.
const HOVER_HIDE_DELAY_MS: u64 = 1000;

// ── Inner state ─────────────────────────────────────────────────────────────

struct Inner {
    widgets: MiniWidgets,
    menu: CompactMenu,
    /// Last-known playback duration, used to convert waveform seek fractions
    /// to absolute millisecond positions for the controller.
    current_duration_ms: Cell<i64>,
    /// Last-known volume (0.0–1.0), used as the scroll base for the next step.
    current_volume: Cell<f64>,
    /// Pending hide-timer handle for the volume bar.
    vol_bar_hide_source: RefCell<Option<gtk4::glib::SourceId>>,
    /// Pending hide-timer handle for the hover overlay.
    hover_hide_source: RefCell<Option<gtk4::glib::SourceId>>,
    /// The active title/artist crossfade half, replaced with skip semantics.
    current_track_animation: Rc<RefCell<Option<adw::TimedAnimation>>>,
    track_animation_generation: Rc<Cell<u64>>,
}

// ── Public type ─────────────────────────────────────────────────────────────

/// Mini-Player: a single compact 430×76 px widget.
///
/// Clone-cheap (wraps `Rc<Inner>`); both `PlayerController` and `MinimalView`
/// can own a copy without needing `Weak` references to each other.
pub(in crate::ui) struct CompactPlayer(Rc<Inner>);

impl Clone for CompactPlayer {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

// ── Construction ─────────────────────────────────────────────────────────────

impl CompactPlayer {
    pub(in crate::ui) fn new() -> Self {
        let widgets = build_mini();
        let menu = CompactMenu::build();

        // Install the mini-player CSS (once per process; no-op if GTK has no
        // default display yet, matching the rest of the style pipeline).
        style::install();

        // Install the "compact" action group on the card so menu accelerators
        // and keyboard shortcuts resolve correctly.
        widgets
            .card
            .insert_action_group("compact", Some(&menu.action_group));

        let inner = Rc::new(Inner {
            widgets,
            menu,
            current_duration_ms: Cell::new(0),
            current_volume: Cell::new(1.0),
            vol_bar_hide_source: RefCell::new(None),
            hover_hide_source: RefCell::new(None),
            current_track_animation: Rc::new(RefCell::new(None)),
            track_animation_generation: Rc::new(Cell::new(0)),
        });

        wire_hover(&inner);
        wire_double_click(&inner);
        wire_right_click(&inner);
        wire_keyboard_menu(&inner);
        wire_chrome_buttons(&inner);

        Self(inner)
    }
}

// ── Widget accessors ─────────────────────────────────────────────────────────

impl CompactPlayer {
    /// The root `WindowHandle` — mount this in `MinimalView`'s toast overlay.
    pub(in crate::ui) fn handle(&self) -> &gtk4::WindowHandle {
        &self.0.widgets.root
    }

    /// The cover `Image` widget, fed by `CoverLoader` in `now_playing_wiring`.
    pub(in crate::ui) fn cover_image(&self) -> &gtk4::Image {
        &self.0.widgets.cover
    }
}

// ── State setters (called from `now_playing_wiring`) ─────────────────────────

impl CompactPlayer {
    /// Crossfades the title and artist labels when the track changes. The
    /// central motion helper follows the system animation setting; the cover
    /// is managed by `CoverLoader` asynchronously.
    pub(in crate::ui) fn set_track(&self, title: &str, artist: &str) {
        start_label_crossfade(&self.0, title.to_owned(), artist.to_owned());
    }

    /// Resets all displayed metadata to the empty state.
    pub(in crate::ui) fn clear_track(&self) {
        let generation = self.0.track_animation_generation.get().wrapping_add(1);
        self.0.track_animation_generation.set(generation);
        let previous = self.0.current_track_animation.borrow_mut().take();
        if let Some(previous) = previous {
            previous.skip();
        }
        self.0.widgets.title_label.set_text("");
        self.0.widgets.artist_label.set_text("");
        self.0.current_duration_ms.set(0);
        self.0.widgets.waveform.set_fraction_smooth(0.0);
        self.set_cover_placeholder();
    }

    /// Updates the play/pause icon and the menu's play label.
    pub(in crate::ui) fn set_state(&self, state: PlaybackState) {
        let is_playing = state == PlaybackState::Playing;
        self.0
            .widgets
            .play_pause_button
            .set_icon_name(if is_playing { ICON_PAUSE } else { ICON_PLAY });
        self.0.widgets.waveform.set_paused(!is_playing);
        self.0
            .widgets
            .play_pause_button
            .set_tooltip_text(Some(&strings::text(if is_playing {
                strings::TOOLTIP_PAUSE
            } else {
                strings::TOOLTIP_PLAY
            })));
        self.0.menu.set_playing(is_playing);
    }

    /// Advances the waveform seek bar. Stores `duration_ms` for seek-fraction
    /// conversion.
    pub(in crate::ui) fn set_position(&self, position_ms: i64, duration_ms: i64) {
        let dur = duration_ms.max(0);
        self.0.current_duration_ms.set(dur);
        self.0.widgets.waveform.set_duration(dur);
        let fraction = if dur > 0 {
            position_ms.clamp(0, dur) as f64 / dur as f64
        } else {
            0.0
        };
        self.0.widgets.waveform.set_fraction_smooth(fraction);
    }

    /// Enables or disables the play/pause button.
    pub(in crate::ui) fn set_transport_enabled(&self, enabled: bool) {
        self.0.widgets.play_pause_button.set_sensitive(enabled);
    }

    /// Stores the current volume and shows the visual feedback bar briefly.
    pub(in crate::ui) fn set_volume_indicator(&self, volume: f64) {
        let v = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.0.current_volume.set(v);
    }

    /// Resets the cover to the placeholder image.
    pub(in crate::ui) fn set_cover_placeholder(&self) {
        CoverLoader::set_placeholder(&self.0.widgets.cover);
    }
}

// ── Callback wiring (called from `compact_mode_controls` and
//    `player_controller_wiring`) ──────────────────────────────────────────────

impl CompactPlayer {
    /// Replaces the callback fired by "Restore Full Window" — the overlay
    /// button, the X button, the menu action, and double-clicking the cover
    /// or title all route through this.
    pub(in crate::ui) fn set_on_restore(&self, callback: Rc<dyn Fn()>) {
        self.0.menu.set_on_restore(callback);
    }

    pub(in crate::ui) fn set_on_preferences(&self, callback: Rc<dyn Fn()>) {
        self.0.menu.set_on_preferences(callback);
    }

    pub(in crate::ui) fn set_on_always_on_top(&self, callback: Rc<dyn Fn(bool)>) {
        self.0.menu.set_on_always_on_top(callback);
    }

    /// Disables the always-on-top action (grays out the menu item).
    pub(in crate::ui) fn set_always_on_top_enabled(&self, enabled: bool) {
        self.0.menu.always_on_top_action.set_enabled(enabled);
    }

    /// Sets the always-on-top action state (check mark in the menu).
    pub(in crate::ui) fn set_always_on_top_active(&self, active: bool) {
        self.0
            .menu
            .always_on_top_action
            .set_state(&active.to_variant());
    }

    pub(in crate::ui) fn set_on_quit(&self, callback: Rc<dyn Fn()>) {
        self.0.menu.set_on_quit(callback);
    }

    /// Wires play/pause to both the overlay button and the menu action.
    pub(in crate::ui) fn connect_play_pause(&self, callback: impl Fn() + 'static) {
        let callback: Rc<dyn Fn()> = Rc::new(callback);
        let cb1 = Rc::clone(&callback);
        self.0
            .widgets
            .play_pause_button
            .connect_clicked(move |_| cb1());
        self.0.menu.set_on_play_pause(callback);
    }

    /// Wires the waveform's drag-to-seek gesture. The fraction (0..1) is
    /// converted to milliseconds using the last-known duration.
    pub(in crate::ui) fn connect_seek(&self, callback: impl Fn(i64) + 'static) {
        let inner = Rc::clone(&self.0);
        self.0.widgets.waveform.connect_seek(move |fraction| {
            let dur_ms = inner.current_duration_ms.get();
            if dur_ms > 0 {
                callback((fraction * dur_ms as f64) as i64);
            }
        });
    }

    /// Installs a scroll controller on the card for volume adjustment.
    /// Calls `on_volume_change` with the new clamped level and briefly shows
    /// the accent-coloured feedback bar at the top edge.
    pub(in crate::ui) fn connect_volume_changed(&self, callback: impl Fn(f64) + 'static) {
        let inner = Rc::clone(&self.0);
        let inner2 = Rc::clone(&self.0);
        let callback: Rc<dyn Fn(f64)> = Rc::new(callback);
        let card_widget: gtk4::Widget = self.0.widgets.card.clone().upcast();
        compact_player_scroll::install(
            &card_widget,
            Rc::new(move || inner.current_volume.get()),
            Rc::new(move |volume| {
                callback(volume);
                show_volume_bar(&inner2);
            }),
        );
    }

    /// Routes "Previous" from the right-click menu to the controller.
    pub(in crate::ui) fn connect_previous(&self, callback: impl Fn() + 'static) {
        self.0.menu.set_on_previous(Rc::new(callback));
    }

    /// Routes "Next" from the right-click menu to the controller.
    pub(in crate::ui) fn connect_next(&self, callback: impl Fn() + 'static) {
        self.0.menu.set_on_next(Rc::new(callback));
    }

    /// Test-only: fires the "restore" action as if the user clicked the button
    /// or chose it from the menu.
    #[cfg(test)]
    pub(in crate::ui) fn activate_restore_for_test(&self) {
        self.0.menu.action_group.activate_action("restore", None);
    }
}

// ── Private wiring helpers ────────────────────────────────────────────────────

/// Installs a motion controller on the card that shows/hides the hover overlay
/// (restore + close buttons). The hide is deferred by `HOVER_HIDE_DELAY_MS`
/// so a slow pointer move does not flicker.
fn wire_hover(inner: &Rc<Inner>) {
    let motion = gtk4::EventControllerMotion::new();

    motion.connect_enter({
        let inner = Rc::clone(inner);
        move |_, _, _| {
            // Cancel a pending hide so the overlay stays visible.
            if let Some(id) = inner.hover_hide_source.borrow_mut().take() {
                id.remove();
            }
            inner.widgets.hover_revealer.set_reveal_child(true);
            inner.widgets.hover_revealer.set_can_target(true);
        }
    });

    motion.connect_leave({
        let inner = Rc::clone(inner);
        move |_| {
            let inner2 = Rc::clone(&inner);
            let id = gtk4::glib::timeout_add_local_once(
                Duration::from_millis(HOVER_HIDE_DELAY_MS),
                move || {
                    inner2.widgets.hover_revealer.set_reveal_child(false);
                    inner2.widgets.hover_revealer.set_can_target(false);
                    *inner2.hover_hide_source.borrow_mut() = None;
                },
            );
            *inner.hover_hide_source.borrow_mut() = Some(id);
        }
    });

    inner.widgets.card.add_controller(motion);
}

/// Wires the overlay restore/close buttons to the "compact.restore" action,
/// so they fire the same callback as the menu item and keyboard shortcut.
fn wire_chrome_buttons(inner: &Rc<Inner>) {
    let ag = inner.menu.action_group.clone();
    inner.widgets.restore_button.connect_clicked({
        let ag = ag.clone();
        move |_| {
            ag.activate_action("restore", None);
        }
    });
    inner.widgets.close_button.connect_clicked(move |_| {
        ag.activate_action("restore", None);
    });
}

/// Double-clicking the cover image or the title label triggers restore.
fn wire_double_click(inner: &Rc<Inner>) {
    let ag = inner.menu.action_group.clone();
    // input-parity: ACC-8 keyboard=restore-button
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gdk::BUTTON_PRIMARY);
    gesture.connect_pressed(move |g, n_press, _, _| {
        if n_press == 2 {
            g.set_state(gtk4::EventSequenceState::Claimed);
            ag.activate_action("restore", None);
        }
    });
    // Add to the card; the gesture sees clicks anywhere in it.
    inner.widgets.card.add_controller(gesture);
}

/// Right-clicking anywhere on the card (outside interactive controls) opens
/// the context menu at the pointer position.
fn wire_right_click(inner: &Rc<Inner>) {
    let popover = inner.menu.popover.clone();
    // input-parity: ACC-8 keyboard=menu-shift-f10
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gdk::BUTTON_SECONDARY);
    let anchor = inner.widgets.card.clone();
    gesture.connect_pressed(move |g, _, x, y| {
        g.set_state(gtk4::EventSequenceState::Claimed);
        compact_player_menu::popup_at(&popover, anchor.upcast_ref(), Some((x as i32, y as i32)));
    });
    inner.widgets.card.add_controller(gesture);
}

/// Opens the context menu on Menu key or Shift+F10 (keyboard accessibility).
fn wire_keyboard_menu(inner: &Rc<Inner>) {
    let key_controller = gtk4::EventControllerKey::new();
    let popover = inner.menu.popover.clone();
    let anchor = inner.widgets.card.clone();
    key_controller.connect_key_pressed(move |_, keyval, _, modifier| {
        let dominated = keyval == gdk::Key::Menu
            || (keyval == gdk::Key::F10 && modifier.contains(gdk::ModifierType::SHIFT_MASK));
        if dominated {
            compact_player_menu::popup_at(&popover, anchor.upcast_ref(), None);
            gtk4::glib::Propagation::Stop
        } else {
            gtk4::glib::Propagation::Proceed
        }
    });
    inner.widgets.card.add_controller(key_controller);
}

/// Cross-fades the title and artist labels: fade out → swap text → fade in.
/// Uses one callback target for both labels; the swap and fade-in are
/// triggered by the fade-out's `done` signal.
fn start_label_crossfade(inner: &Rc<Inner>, title: String, artist: String) {
    let generation = inner.track_animation_generation.get().wrapping_add(1);
    inner.track_animation_generation.set(generation);
    let title_lbl = inner.widgets.title_label.clone();
    let artist_lbl = inner.widgets.artist_label.clone();
    let animation_slot = inner.current_track_animation.clone();
    let animation_generation = inner.track_animation_generation.clone();

    let fade_out_target = adw::CallbackAnimationTarget::new({
        let title_lbl = title_lbl.clone();
        let artist_lbl = artist_lbl.clone();
        move |value| {
            title_lbl.set_opacity(value);
            artist_lbl.set_opacity(value);
        }
    });
    let fade_out = motion::timed(&title_lbl, 1.0, 0.0, motion::STANDARD, fade_out_target);

    fade_out.connect_done(move |_| {
        title_lbl.set_text(&title);
        artist_lbl.set_text(&artist);

        if animation_generation.get() != generation {
            title_lbl.set_opacity(1.0);
            artist_lbl.set_opacity(1.0);
            return;
        }

        let fade_in_target = adw::CallbackAnimationTarget::new({
            let title_lbl = title_lbl.clone();
            let artist_lbl = artist_lbl.clone();
            move |value| {
                title_lbl.set_opacity(value);
                artist_lbl.set_opacity(value);
            }
        });
        let fade_in = motion::timed(&title_lbl, 0.0, 1.0, motion::STANDARD, fade_in_target);
        fade_in.set_duration(motion::half(motion::STANDARD));
        motion::replace_animation(&animation_slot, fade_in.clone());
        fade_in.play();
    });

    fade_out.set_duration(motion::half(motion::STANDARD));
    motion::replace_animation(&inner.current_track_animation, fade_out.clone());
    fade_out.play();
}

/// Shows the accent-coloured volume bar at the top edge of the card, then
/// hides it after `VOL_BAR_LINGER_MS`. Cancels any in-flight hide timer so
/// rapid scroll steps don't flicker.
fn show_volume_bar(inner: &Rc<Inner>) {
    inner.widgets.volume_bar.set_opacity(1.0);

    if let Some(id) = inner.vol_bar_hide_source.borrow_mut().take() {
        id.remove();
    }

    let bar = inner.widgets.volume_bar.clone();
    let inner2 = Rc::clone(inner);
    let id =
        gtk4::glib::timeout_add_local_once(Duration::from_millis(VOL_BAR_LINGER_MS), move || {
            bar.set_opacity(0.0);
            *inner2.vol_bar_hide_source.borrow_mut() = None;
        });
    *inner.vol_bar_hide_source.borrow_mut() = Some(id);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mini_player_has_root_handle_and_cover_image() {
        if gtk4::init().is_err() {
            return;
        }
        let player = CompactPlayer::new();
        assert!(player.handle().parent().is_none()); // not yet in a window
                                                     // cover_image() returns the image widget
        assert!(player.cover_image().is::<gtk4::Image>());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn restore_action_fires_callback() {
        if gtk4::init().is_err() {
            return;
        }
        let player = CompactPlayer::new();
        let fired = Rc::new(Cell::new(false));
        let fired2 = fired.clone();
        player.set_on_restore(Rc::new(move || fired2.set(true)));
        player.activate_restore_for_test();
        assert!(fired.get());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mot_6_compact_track_change_replaces_the_running_animation_slot() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let settings = gtk4::Settings::default().unwrap();
        let previous = settings.is_gtk_enable_animations();
        settings.set_gtk_enable_animations(true);

        let player = CompactPlayer::new();
        let window = gtk4::Window::new();
        window.set_child(Some(player.handle()));
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        player.0.widgets.title_label.set_text("Before");
        player.set_track("First", "First artist");
        player.set_track("Second", "Second artist");

        assert_eq!(player.0.widgets.title_label.text(), "First");
        assert_eq!(player.0.widgets.artist_label.text(), "First artist");
        assert_eq!(player.0.widgets.title_label.opacity(), 1.0);
        {
            let animation = player.0.current_track_animation.borrow();
            let animation = animation.as_ref().unwrap();
            assert_eq!(animation.duration(), motion::half(motion::STANDARD));
            assert_eq!(animation.easing(), motion::STANDARD_EASING);
            assert!(animation.follows_enable_animations_setting());
        }

        settings.set_gtk_enable_animations(previous);
        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mot_5_compact_player_state_propagates_pause_to_waveform() {
        gtk4::init().unwrap();
        let player = CompactPlayer::new();

        player.set_state(PlaybackState::Playing);
        assert_eq!(
            player.0.widgets.waveform.desaturation_target_for_test(),
            0.0
        );
        player.set_state(PlaybackState::Paused);
        assert_eq!(
            player.0.widgets.waveform.desaturation_target_for_test(),
            1.0
        );
        player.set_state(PlaybackState::Playing);
        assert_eq!(
            player.0.widgets.waveform.desaturation_target_for_test(),
            0.0
        );
        player.set_state(PlaybackState::Stopped);
        assert_eq!(
            player.0.widgets.waveform.desaturation_target_for_test(),
            1.0
        );
    }
}
