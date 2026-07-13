//! Compact playback surface fed by the same `PlayerController::sync_*` path
//! as the library bar and Now Playing page.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::{gdk, prelude::*};
use reprise_core::format::format_duration;
use reprise_core::playback::PlaybackState;
use reprise_core::queue::Repeat;

use super::compact_player_state::{normalized_position, volume_percent, CompactPresentation};
use super::cover_loader::CoverLoader;
use super::player_bar::{
    ICON_NEXT, ICON_PAUSE, ICON_PLAY, ICON_PREVIOUS, ICON_REPEAT_ALL, ICON_REPEAT_ONE,
    ICON_SHUFFLE, REPEAT_OFF_CSS_CLASS,
};
use super::player_bar_seek::{
    should_apply_position_tick, should_clear_drag_guard_on_track_change,
    should_finish_observer_cancel, should_finish_observer_stop, should_self_heal,
    should_update_range,
};
use super::strings;

const COVER_SIZE: i32 = 64;
const INFO_WIDTH: i32 = 150;
const ZERO_TIME: &str = "0:00";
const VOLUME_ICONS: [&str; 4] = [
    "audio-volume-muted-symbolic",
    "audio-volume-low-symbolic",
    "audio-volume-medium-symbolic",
    "audio-volume-high-symbolic",
];

pub(super) struct CompactPlayer {
    root: gtk4::Box,
    cover: gtk4::Image,
    title: gtk4::Label,
    artist: gtk4::Label,
    shuffle: gtk4::ToggleButton,
    previous: gtk4::Button,
    play_pause: gtk4::Button,
    next: gtk4::Button,
    repeat: gtk4::Button,
    position: gtk4::Label,
    duration: gtk4::Label,
    scale: gtk4::Scale,
    volume: gtk4::ScaleButton,
    presentation: RefCell<CompactPresentation>,
    updating_scale: Rc<Cell<bool>>,
    dragging: Rc<Cell<bool>>,
    pointer_down: Rc<Cell<bool>>,
    seek_gesture: RefCell<Option<gtk4::GestureClick>>,
    last_duration_ms: Cell<i64>,
    updating_shuffle: Rc<Cell<bool>>,
    updating_volume: Rc<Cell<bool>>,
}

impl CompactPlayer {
    pub(super) fn new() -> Self {
        let cover = gtk4::Image::new();
        cover.set_pixel_size(COVER_SIZE);
        CoverLoader::set_placeholder(&cover);

        let title = track_label();
        title.add_css_class("heading");
        let artist = track_label();
        artist.add_css_class("dim-label");
        let info = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        info.set_width_request(INFO_WIDTH);
        info.append(&title);
        info.append(&artist);

        let shuffle = gtk4::ToggleButton::builder()
            .icon_name(ICON_SHUFFLE)
            .tooltip_text(strings::text(strings::SHUFFLE))
            .build();
        let previous = icon_button(ICON_PREVIOUS, strings::PREVIOUS);
        let play_pause = icon_button(ICON_PLAY, strings::PLAY);
        play_pause.add_css_class("circular");
        play_pause.add_css_class("suggested-action");
        let next = icon_button(ICON_NEXT, strings::NEXT);
        let repeat = icon_button(ICON_REPEAT_ALL, strings::REPEAT);

        let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        for button in [
            shuffle.upcast_ref::<gtk4::Widget>(),
            previous.upcast_ref(),
            play_pause.upcast_ref(),
            next.upcast_ref(),
            repeat.upcast_ref(),
        ] {
            controls.append(button);
        }

        let position = gtk4::Label::new(Some(ZERO_TIME));
        let duration = gtk4::Label::new(Some(ZERO_TIME));
        let scale = gtk4::Scale::new(gtk4::Orientation::Horizontal, None::<&gtk4::Adjustment>);
        scale.set_range(0.0, 1.0);
        scale.set_draw_value(false);
        scale.set_hexpand(true);
        scale.set_tooltip_text(Some(&strings::text(strings::PLAYBACK_POSITION)));
        let seek = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        seek.append(&position);
        seek.append(&scale);
        seek.append(&duration);
        let center = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        center.set_hexpand(true);
        center.append(&controls);
        center.append(&seek);

        let volume = gtk4::ScaleButton::new(0.0, 1.0, 0.05, &VOLUME_ICONS);
        volume.set_value(1.0);
        volume.set_tooltip_text(Some(&strings::text(strings::VOLUME)));

        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        root.set_margin_top(12);
        root.set_margin_bottom(12);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.append(&cover);
        root.append(&info);
        root.append(&center);
        root.append(&volume);
        root.set_sensitive(false);

        let compact = Self {
            root,
            cover,
            title,
            artist,
            shuffle,
            previous,
            play_pause,
            next,
            repeat,
            position,
            duration,
            scale,
            volume,
            presentation: RefCell::new(CompactPresentation::default()),
            updating_scale: Rc::new(Cell::new(false)),
            dragging: Rc::new(Cell::new(false)),
            pointer_down: Rc::new(Cell::new(false)),
            seek_gesture: RefCell::new(None),
            last_duration_ms: Cell::new(0),
            updating_shuffle: Rc::new(Cell::new(false)),
            updating_volume: Rc::new(Cell::new(false)),
        };
        compact.set_repeat_indicator(Repeat::Off);
        compact.refresh_sensitivity();
        compact
    }

    pub(super) fn cover_image(&self) -> &gtk4::Image {
        &self.cover
    }

    pub(super) fn set_track(&self, title: &str, artist: &str, album: &str) {
        if should_clear_drag_guard_on_track_change(
            self.pointer_down.get(),
            self.seek_gesture_is_active(),
        ) {
            self.dragging.set(false);
        }
        {
            let mut presentation = self.presentation.borrow_mut();
            presentation.title = title.to_string();
            presentation.artist = artist.to_string();
            presentation.album = album.to_string();
        }
        self.title.set_text(title);
        self.artist.set_text(artist);
    }

    pub(super) fn clear_track(&self) {
        if should_clear_drag_guard_on_track_change(
            self.pointer_down.get(),
            self.seek_gesture_is_active(),
        ) {
            self.dragging.set(false);
        }
        self.presentation.borrow_mut().clear_track();
        self.title.set_text("");
        self.artist.set_text("");
        self.position.set_text(ZERO_TIME);
        self.duration.set_text(ZERO_TIME);
        CoverLoader::set_placeholder(&self.cover);
    }

    pub(super) fn set_state(&self, state: PlaybackState) {
        self.presentation.borrow_mut().set_playback_state(state);
        let is_playing = state == PlaybackState::Playing;
        self.play_pause
            .set_icon_name(if is_playing { ICON_PAUSE } else { ICON_PLAY });
        self.play_pause
            .set_tooltip_text(Some(&strings::text(if is_playing {
                strings::PAUSE
            } else {
                strings::PLAY
            })));
        if state == PlaybackState::Stopped {
            self.pointer_down.set(false);
            self.dragging.set(false);
            self.set_position(0, 0);
        }
        self.refresh_sensitivity();
    }

    pub(super) fn set_position(&self, position_ms: i64, duration_ms: i64) {
        let (position_ms, duration_ms) = normalized_position(position_ms, duration_ms);
        {
            let mut presentation = self.presentation.borrow_mut();
            presentation.position_ms = position_ms;
            presentation.duration_ms = duration_ms;
        }
        if should_self_heal(
            self.dragging.get(),
            self.pointer_down.get(),
            self.seek_gesture_is_active(),
        ) {
            tracing::warn!("compact-player drag guard was stuck; self-healing");
            self.dragging.set(false);
        }
        if should_apply_position_tick(self.dragging.get()) {
            self.updating_scale.set(true);
            if should_update_range(self.last_duration_ms.get(), duration_ms) {
                self.last_duration_ms.set(duration_ms);
                self.scale.set_range(0.0, duration_ms.max(1) as f64);
            }
            self.scale.set_value(position_ms as f64);
            self.updating_scale.set(false);
        }
        self.position.set_text(&format_duration(position_ms));
        self.duration.set_text(&format_duration(duration_ms));
    }

    pub(super) fn set_transport_enabled(&self, enabled: bool) {
        self.presentation.borrow_mut().transport_enabled = enabled;
        self.previous.set_sensitive(enabled);
        self.next.set_sensitive(enabled);
        self.refresh_sensitivity();
    }

    pub(super) fn set_shuffle_indicator(&self, active: bool) {
        self.presentation.borrow_mut().shuffled = active;
        self.updating_shuffle.set(true);
        self.shuffle.set_active(active);
        self.updating_shuffle.set(false);
    }

    pub(super) fn set_repeat_indicator(&self, repeat: Repeat) {
        self.presentation.borrow_mut().repeat = repeat;
        let (icon, off) = match repeat {
            Repeat::Off => (ICON_REPEAT_ALL, true),
            Repeat::All => (ICON_REPEAT_ALL, false),
            Repeat::One => (ICON_REPEAT_ONE, false),
        };
        self.repeat.set_icon_name(icon);
        if off {
            self.repeat.add_css_class(REPEAT_OFF_CSS_CLASS);
        } else {
            self.repeat.remove_css_class(REPEAT_OFF_CSS_CLASS);
        }
    }

    pub(super) fn set_volume_indicator(&self, volume: f64) {
        let volume = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.presentation.borrow_mut().volume_percent = volume_percent(volume);
        self.updating_volume.set(true);
        self.volume.set_value(volume);
        self.updating_volume.set(false);
    }

    pub(super) fn connect_play_pause(&self, f: impl Fn() + 'static) {
        self.play_pause.connect_clicked(move |_| f());
    }

    pub(super) fn connect_previous(&self, f: impl Fn() + 'static) {
        self.previous.connect_clicked(move |_| f());
    }

    pub(super) fn connect_next(&self, f: impl Fn() + 'static) {
        self.next.connect_clicked(move |_| f());
    }

    pub(super) fn connect_shuffle_toggled(&self, f: impl Fn(bool) + 'static) {
        let updating = self.updating_shuffle.clone();
        self.shuffle.connect_toggled(move |button| {
            if !updating.get() {
                f(button.is_active());
            }
        });
    }

    pub(super) fn connect_repeat_clicked(&self, f: impl Fn() + 'static) {
        self.repeat.connect_clicked(move |_| f());
    }

    pub(super) fn connect_volume_changed(&self, f: impl Fn(f64) + 'static) {
        let updating = self.updating_volume.clone();
        self.volume.connect_value_changed(move |_, value| {
            if !updating.get() {
                f(value);
            }
        });
    }

    pub(super) fn connect_seek(&self, f: impl Fn(i64) + 'static) {
        let f = Rc::new(f);
        let updating = self.updating_scale.clone();
        let dragging = self.dragging.clone();
        let changed = f.clone();
        self.scale.connect_value_changed(move |scale| {
            if !updating.get() && !dragging.get() {
                changed(scale.value() as i64);
            }
        });

        let click = gtk4::GestureClick::new();
        click.set_button(gdk::BUTTON_PRIMARY);
        click.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let press_dragging = self.dragging.clone();
        click.connect_pressed(move |_, _, _, _| press_dragging.set(true));

        let end_drag: Rc<dyn Fn()> = {
            let dragging = self.dragging.clone();
            let pointer_down = self.pointer_down.clone();
            let scale = self.scale.downgrade();
            Rc::new(move || {
                pointer_down.set(false);
                if !dragging.replace(false) {
                    return;
                }
                if let Some(scale) = scale.upgrade() {
                    f(scale.value() as i64);
                }
            })
        };

        let raw = gtk4::EventControllerLegacy::new();
        raw.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let raw_pointer_down = self.pointer_down.clone();
        let raw_dragging = self.dragging.clone();
        let raw_end = end_drag.clone();
        raw.connect_event(move |_, event| {
            let primary = event
                .downcast_ref::<gdk::ButtonEvent>()
                .is_some_and(|button| button.button() == gdk::BUTTON_PRIMARY);
            match event.event_type() {
                gdk::EventType::ButtonPress if primary => {
                    raw_pointer_down.set(true);
                    raw_dragging.set(true);
                }
                gdk::EventType::TouchBegin => {
                    raw_pointer_down.set(true);
                    raw_dragging.set(true);
                }
                gdk::EventType::ButtonRelease if primary => raw_end(),
                gdk::EventType::TouchEnd | gdk::EventType::TouchCancel => raw_end(),
                _ => {}
            }
            gtk4::glib::Propagation::Proceed
        });
        self.scale.add_controller(raw);

        let released = end_drag.clone();
        click.connect_released(move |_, _, _, _| released());
        let cancel = end_drag.clone();
        let cancel_pointer_down = self.pointer_down.clone();
        click.connect_cancel(move |_, _| {
            if should_finish_observer_cancel(cancel_pointer_down.get()) {
                cancel();
            }
        });
        let unpaired = end_drag.clone();
        click.connect_unpaired_release(move |_, _, _, _, _| unpaired());
        let stopped = end_drag;
        let stopped_pointer_down = self.pointer_down.clone();
        click.connect_stopped(move |gesture| {
            if should_finish_observer_stop(stopped_pointer_down.get(), gesture.is_active()) {
                stopped();
            }
        });
        *self.seek_gesture.borrow_mut() = Some(click.clone());
        self.scale.add_controller(click);
    }

    fn seek_gesture_is_active(&self) -> bool {
        self.seek_gesture
            .borrow()
            .as_ref()
            .is_some_and(gtk4::prelude::GestureExt::is_active)
    }

    fn refresh_sensitivity(&self) {
        let presentation = self.presentation.borrow();
        let sensitive = super::player_bar_state::bar_should_be_sensitive(
            presentation.state,
            presentation.transport_enabled,
        );
        self.root.set_sensitive(sensitive);
        self.play_pause.set_sensitive(sensitive);
        self.scale
            .set_sensitive(presentation.state != PlaybackState::Stopped);
    }
}

fn track_label() -> gtk4::Label {
    let label = gtk4::Label::new(None);
    label.set_xalign(0.0);
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    label
}

fn icon_button(icon: &str, tooltip: &str) -> gtk4::Button {
    let button = gtk4::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(&strings::text(tooltip)));
    button
}
