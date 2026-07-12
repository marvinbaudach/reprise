//! The Now-Playing full view (Amberol-style, Task 8): a big cover, prominent
//! title/artist/album, a seek scale, and transport controls, opened by
//! clicking the player bar. This widget owns no playback/seek state of its
//! own — it binds to the SAME `PlayerController` actions and is fed from the
//! SAME state as the bar; see `now_playing_wiring.rs`'s `sync_*` fan-out and
//! `wire_now_playing_controls` for how one controller feeds both widgets
//! without a second playback/seek path (same discipline as the MPRIS mirror
//! in `mpris_mirror.rs`).
//!
//! The seek scale reuses `player_bar.rs`'s drag-guard shape (`updating_scale`
//! / `dragging`, cross-checked against the gesture's own `is_active()`)
//! since this is an independent `gtk4::Scale` widget with its own drag
//! state — see that module's doc comment for the full rationale and the
//! field bug that shape defends against; the comments here are condensed,
//! not the full story.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

use crate::ui::cover_loader::CoverLoader;
use crate::ui::player_bar::{
    ICON_NEXT, ICON_PAUSE, ICON_PLAY, ICON_PREVIOUS, ICON_REPEAT_ALL, ICON_REPEAT_ONE,
    ICON_SHUFFLE, REPEAT_OFF_CSS_CLASS,
};
use crate::ui::strings;
use reprise_core::format::format_duration;
use reprise_core::playback::PlaybackState;
use reprise_core::queue::Repeat;

/// Display size of the cover `gtk4::Image`; fed a `ThumbnailSize::Full`
/// (1024px) texture (see `now_playing_wiring.rs`'s `sync_cover`), so it
/// stays crisp when enlarged or on HiDPI — the texture is only ever
/// downscaled for display, never upscaled.
const COVER_DISPLAY_SIZE: i32 = 320;
const ZERO_TIME_LABEL: &str = "0:00";
const PAGE_TITLE: &str = "Now Playing";
const CONTENT_WIDTH: i32 = 420;
const CONTENT_SPACING: i32 = 12;

pub struct NowPlayingView {
    page: adw::NavigationPage,
    cover: gtk4::Image,
    title_label: gtk4::Label,
    artist_album_label: gtk4::Label,
    shuffle_button: gtk4::ToggleButton,
    prev_button: gtk4::Button,
    play_pause_button: gtk4::Button,
    next_button: gtk4::Button,
    repeat_button: gtk4::Button,
    position_label: gtk4::Label,
    duration_label: gtk4::Label,
    scale: gtk4::Scale,
    /// See `player_bar.rs`'s field of the same name for the full rationale;
    /// this is an independent guard for this widget's own `scale`.
    updating_scale: Rc<Cell<bool>>,
    dragging: Rc<Cell<bool>>,
    seek_gesture: RefCell<Option<gtk4::GestureClick>>,
    last_duration_ms: Cell<i64>,
    updating_shuffle: Rc<Cell<bool>>,
}

impl NowPlayingView {
    pub fn new() -> Self {
        let cover = gtk4::Image::new();
        cover.set_pixel_size(COVER_DISPLAY_SIZE);
        cover.add_css_class("now-playing-cover");
        CoverLoader::set_placeholder(&cover);

        let title_label = gtk4::Label::new(None);
        title_label.add_css_class("title-1");
        title_label.set_wrap(true);
        title_label.set_justify(gtk4::Justification::Center);

        let artist_album_label = gtk4::Label::new(None);
        artist_album_label.add_css_class("dim-label");
        artist_album_label.add_css_class("title-4");
        artist_album_label.set_wrap(true);
        artist_album_label.set_justify(gtk4::Justification::Center);

        let shuffle_button = gtk4::ToggleButton::builder()
            .icon_name(ICON_SHUFFLE)
            .tooltip_text(strings::text(strings::SHUFFLE))
            .valign(gtk4::Align::Center)
            .build();

        let prev_button = gtk4::Button::from_icon_name(ICON_PREVIOUS);
        prev_button.set_tooltip_text(Some(&strings::text(strings::PREVIOUS)));
        prev_button.add_css_class("circular");
        // No queue until a view has been activated — mirrors the bar's own
        // initial state (see `PlayerBar::new`), synced from then on via
        // `sync_transport_enabled`.
        prev_button.set_sensitive(false);

        let play_pause_button = gtk4::Button::from_icon_name(ICON_PLAY);
        play_pause_button.set_tooltip_text(Some(&strings::text(strings::PLAY)));
        play_pause_button.add_css_class("circular");
        play_pause_button.add_css_class("suggested-action");

        let next_button = gtk4::Button::from_icon_name(ICON_NEXT);
        next_button.set_tooltip_text(Some(&strings::text(strings::NEXT)));
        next_button.add_css_class("circular");
        next_button.set_sensitive(false);

        let repeat_button = gtk4::Button::from_icon_name(ICON_REPEAT_ALL);
        repeat_button.set_tooltip_text(Some(&strings::text(strings::REPEAT)));

        let position_label = gtk4::Label::new(Some(ZERO_TIME_LABEL));
        let duration_label = gtk4::Label::new(Some(ZERO_TIME_LABEL));

        let scale = gtk4::Scale::new(gtk4::Orientation::Horizontal, None::<&gtk4::Adjustment>);
        scale.set_range(0.0, 1.0);
        scale.set_draw_value(false);
        scale.set_hexpand(true);
        scale.set_tooltip_text(Some(&strings::text(strings::PLAYBACK_POSITION)));

        let seek_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        seek_row.append(&position_label);
        seek_row.append(&scale);
        seek_row.append(&duration_label);

        let transport_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 16);
        transport_row.set_halign(gtk4::Align::Center);
        transport_row.append(&shuffle_button);
        transport_row.append(&prev_button);
        transport_row.append(&play_pause_button);
        transport_row.append(&next_button);
        transport_row.append(&repeat_button);

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, CONTENT_SPACING);
        content.set_valign(gtk4::Align::Center);
        content.set_halign(gtk4::Align::Center);
        content.set_margin_top(24);
        content.set_margin_bottom(24);
        content.set_margin_start(24);
        content.set_margin_end(24);
        content.set_width_request(CONTENT_WIDTH);
        content.append(&cover);
        content.append(&title_label);
        content.append(&artist_album_label);
        content.append(&seek_row);
        content.append(&transport_row);

        // A slim header, title hidden: its only job here is to give
        // `adw::NavigationView` a back button (see that type's "Header Bar
        // Integration" doc) — the page's own title still drives the
        // back-button's accessible label.
        let header = adw::HeaderBar::new();
        header.set_show_title(false);
        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        toolbar_view.set_content(Some(&content));

        let page = adw::NavigationPage::new(&toolbar_view, PAGE_TITLE);

        let view = Self {
            page,
            cover,
            title_label,
            artist_album_label,
            shuffle_button,
            prev_button,
            play_pause_button,
            next_button,
            repeat_button,
            position_label,
            duration_label,
            scale,
            updating_scale: Rc::new(Cell::new(false)),
            dragging: Rc::new(Cell::new(false)),
            seek_gesture: RefCell::new(None),
            last_duration_ms: Cell::new(0),
            updating_shuffle: Rc::new(Cell::new(false)),
        };
        // Starts at Repeat::Off, matching the bar (see `PlayerBar::new`).
        view.set_repeat_indicator(Repeat::Off);
        view
    }

    /// The page to add/push on an `adw::NavigationView` (see
    /// `now_playing_wiring.rs`'s window-side helpers).
    pub fn widget(&self) -> &adw::NavigationPage {
        &self.page
    }

    /// The cover widget — `now_playing_wiring.rs`'s `sync_cover` feeds it via
    /// `CoverLoader::load_into` at `ThumbnailSize::Full`, its own generation
    /// token (separate from the bar's).
    pub fn cover_image(&self) -> &gtk4::Image {
        &self.cover
    }

    pub fn clear_cover(&self) {
        CoverLoader::set_placeholder(&self.cover);
    }

    pub fn set_track(&self, title: &str, artist: &str, album: &str) {
        if should_clear_drag_guard(self.seek_gesture_is_active()) {
            self.dragging.set(false);
        }
        self.title_label.set_text(title);
        self.artist_album_label
            .set_text(&format!("{artist} — {album}"));
    }

    pub fn clear_track(&self) {
        if should_clear_drag_guard(self.seek_gesture_is_active()) {
            self.dragging.set(false);
        }
        self.title_label.set_text("");
        self.artist_album_label.set_text("");
        self.clear_cover();
    }

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
        if state == PlaybackState::Stopped {
            self.dragging.set(false);
            self.set_position(0, 0);
        }
    }

    /// Same guard shape as `player_bar.rs`'s `set_position` — see that
    /// method's doc comment for the full drag/self-heal rationale.
    pub fn set_position(&self, position_ms: i64, duration_ms: i64) {
        let duration_ms = duration_ms.max(0);
        let position_ms = position_ms.clamp(0, duration_ms);

        if self.dragging.get() && !self.seek_gesture_is_active() {
            tracing::warn!("now-playing drag guard was stuck; self-healing");
            self.dragging.set(false);
        }

        if !self.dragging.get() {
            self.updating_scale.set(true);
            if duration_ms != self.last_duration_ms.get() {
                self.last_duration_ms.set(duration_ms);
                self.scale.set_range(0.0, duration_ms.max(1) as f64);
            }
            self.scale.set_value(position_ms as f64);
            self.updating_scale.set(false);
        }

        self.position_label.set_text(&format_duration(position_ms));
        self.duration_label.set_text(&format_duration(duration_ms));
    }

    fn seek_gesture_is_active(&self) -> bool {
        self.seek_gesture
            .borrow()
            .as_ref()
            .is_some_and(gtk4::prelude::GestureExt::is_active)
    }

    pub fn set_transport_enabled(&self, enabled: bool) {
        self.prev_button.set_sensitive(enabled);
        self.next_button.set_sensitive(enabled);
    }

    pub fn connect_play_pause<F: Fn() + 'static>(&self, f: F) {
        self.play_pause_button.connect_clicked(move |_| f());
    }

    pub fn connect_previous<F: Fn() + 'static>(&self, f: F) {
        self.prev_button.connect_clicked(move |_| f());
    }

    pub fn connect_next<F: Fn() + 'static>(&self, f: F) {
        self.next_button.connect_clicked(move |_| f());
    }

    pub fn connect_shuffle_toggled<F: Fn(bool) + 'static>(&self, f: F) {
        let updating_shuffle = self.updating_shuffle.clone();
        self.shuffle_button.connect_toggled(move |button| {
            if updating_shuffle.get() {
                return;
            }
            f(button.is_active());
        });
    }

    pub fn set_shuffle_indicator(&self, active: bool) {
        self.updating_shuffle.set(true);
        self.shuffle_button.set_active(active);
        self.updating_shuffle.set(false);
    }

    pub fn connect_repeat_clicked<F: Fn() + 'static>(&self, f: F) {
        self.repeat_button.connect_clicked(move |_| f());
    }

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

    /// Wires the seek scale — condensed version of `player_bar.rs`'s
    /// `connect_seek`: same guard shape (`updating_scale`/`dragging`), same
    /// capture-phase `GestureClick` bracketing "pointer down" .. "pointer
    /// up", same idempotent end-of-drag handler across `released`/`cancel`/
    /// `unpaired_release`. See that method's doc comment for the full
    /// rationale (including the field bug this defends against) — not
    /// repeated here to keep this file's size down.
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

        let click = gtk4::GestureClick::new();
        click.set_propagation_phase(gtk4::PropagationPhase::Capture);

        let press_dragging = self.dragging.clone();
        click.connect_pressed(move |_, _, _, _| {
            press_dragging.set(true);
        });

        let end_drag: Rc<dyn Fn()> = {
            let dragging = self.dragging.clone();
            let scale = self.scale.downgrade();
            let f = Rc::clone(&f);
            Rc::new(move || {
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

        *self.seek_gesture.borrow_mut() = Some(click.clone());
        self.scale.add_controller(click);
    }
}

impl Default for NowPlayingView {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether `set_track`/`clear_track` should clear the `dragging` flag — same
/// predicate/rationale as `player_bar.rs`'s function of the same name.
fn should_clear_drag_guard(gesture_active: bool) -> bool {
    !gesture_active
}
