//! Fullscreen chrome for the audio-reactive visualizer — the immersive
//! header + bottom bar described in the design mock (`fullscreen-detail-
//! reference.md`'s "Task 7b: Fullscreen chrome per design"). A descendant of
//! `song_visualizer`, so it reaches straight into [`SongVisualizer`]'s
//! private fields and the parent module's construction helpers
//! (`drawing_area`, `mode_controls`, `install_chrome_autohide`, `render::
//! draw_scene`, …) rather than re-exposing them as a public API only this
//! module would ever call.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::playback::PlaybackState;
use reprise_core::visuals::VisualEngine;

use crate::ui::strings;
use crate::ui::style::buttons;

use super::{
    accent_rgb, glow_area, gpu_visuals_enabled, install_chrome_autohide, mode_controls,
    register_area, render, FullscreenChrome, PlayerHooks, SongVisualizer, SEEK_SCALE_MAX,
};

/// Dark vignette color (design mock's `#0f101c`) painted under the engine's
/// scene, as two overlaid `cairo::RadialGradient`s: a flat center wash at
/// 0.45 alpha, plus an outer ring ramping to 0.87 alpha that only shows past
/// the wash's radius — so the center reads close to 0.45 and the corners
/// read close to 0.87, matching the mock's "0.45 center → 0.87 edge" without
/// a hard seam.
const VIGNETTE_RGB: (f64, f64, f64) = (15.0 / 255.0, 16.0 / 255.0, 28.0 / 255.0);
const VIGNETTE_CENTER_ALPHA: f64 = 0.45;
const VIGNETTE_EDGE_ALPHA: f64 = 0.87;

const COVER_THUMB_SIZE: i32 = 84;
const VOLUME_SCALE_WIDTH: i32 = 110;
const HEADER_MARGIN_TOP: i32 = 28;
const HEADER_MARGIN_SIDE: i32 = 28;
const BOTTOM_MARGIN: i32 = 28;

const ICON_VOLUME_MUTED: &str = "audio-volume-muted-symbolic";
const ICON_VOLUME_HIGH: &str = "audio-volume-high-symbolic";

/// Seek step for the ←/→ keys, in milliseconds.
const SEEK_STEP_MS: i64 = 5_000;
/// Volume step for the ↑/↓ keys.
const VOLUME_STEP: f64 = 0.05;

/// Accessor for one [`PlayerHooks`] slot, used to wire a transport button to
/// its callback without repeating the field type at every call site.
type HookSlot = fn(&PlayerHooks) -> &Rc<dyn Fn()>;

/// Builds the fullscreen overlay window: backdrop → canvas → header → bottom
/// bar, wired to `visualizer`'s live state. `SongVisualizer::toggle_fullscreen`
/// owns opening/closing (window lifetime, `fullscreen_active`); this function
/// only constructs the widget tree and its live-update/destroy plumbing.
pub(super) fn build(visualizer: &SongVisualizer, parent: &adw::ApplicationWindow) -> gtk4::Window {
    let area = fullscreen_canvas(&visualizer.engine);
    register_area(&visualizer.areas, &area);

    let backdrop = gtk4::Picture::new();
    backdrop.set_content_fit(gtk4::ContentFit::Cover);
    backdrop.set_can_target(false);
    backdrop.set_hexpand(true);
    backdrop.set_vexpand(true);
    backdrop.add_css_class("reprise-fs-backdrop");

    let (header, title, subtitle, state, timecode) = header_row();
    let (
        bottom,
        cover_thumb,
        track_pos,
        next_up,
        time_cur,
        time_total,
        seek,
        seek_updating,
        play_pause,
        volume,
        mode_row,
    ) = bottom_bar(visualizer);

    let chrome = FullscreenChrome {
        title,
        subtitle,
        state,
        track_pos,
        next_up,
        cover_thumb,
        backdrop: backdrop.clone(),
        play_pause,
        time_cur,
        time_total,
        timecode,
        seek,
        seek_updating,
    };
    let (title_text, subtitle_text) = visualizer.meta.borrow().clone();
    let (position_ms, duration_ms) = visualizer.position.get();
    let (queue_index, queue_total) = visualizer.queue_position.get();
    let next_up_text = visualizer.next_up.borrow().clone();
    let cover_texture = visualizer.cover.borrow().clone();
    chrome.apply_track_meta(&title_text, &subtitle_text);
    chrome.apply_playback_state(visualizer.playback.get());
    chrome.apply_queue_position(queue_index, queue_total);
    chrome.apply_next_up(next_up_text.as_deref());
    chrome.apply_position(position_ms, duration_ms);
    chrome.apply_cover(cover_texture.as_ref());
    *visualizer.fullscreen_chrome.borrow_mut() = Some(chrome);

    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&backdrop));
    overlay.add_overlay(&area);
    overlay.add_overlay(&header);
    overlay.add_overlay(&bottom);

    let window = gtk4::Window::builder()
        .title(strings::text(strings::SONG_VISUALS))
        .transient_for(parent)
        .decorated(false)
        .child(&overlay)
        .build();
    window.add_css_class("reprise-song-visual-fullscreen");

    let playback = visualizer.playback.clone();
    let playing: Rc<dyn Fn() -> bool> = Rc::new(move || playback.get() == PlaybackState::Playing);
    let wake = install_chrome_autohide(
        &window,
        &overlay,
        header.upcast_ref::<gtk4::Widget>(),
        bottom.upcast_ref::<gtk4::Widget>(),
        &playing,
    );

    let key = gtk4::EventControllerKey::new();
    let window_weak = window.downgrade();
    let hooks = visualizer.hooks.clone();
    let position = visualizer.position.clone();
    let volume_weak = volume.downgrade();
    let mode_row_weak = mode_row.downgrade();
    key.connect_key_pressed(move |_, key, _, _| {
        // Every key wakes the chrome, even ones we don't otherwise handle
        // (e.g. Tab) — reaching for the keyboard at all should feel like
        // reaching for the mouse.
        wake();
        match key {
            gtk4::gdk::Key::Escape
            | gtk4::gdk::Key::F11
            | gtk4::gdk::Key::F
            | gtk4::gdk::Key::f => {
                if let Some(window) = window_weak.upgrade() {
                    window.close();
                }
                gtk4::glib::Propagation::Stop
            }
            gtk4::gdk::Key::space => {
                fire_hook(&hooks, |h| &h.play_pause);
                gtk4::glib::Propagation::Stop
            }
            gtk4::gdk::Key::n | gtk4::gdk::Key::N => {
                fire_hook(&hooks, |h| &h.next);
                gtk4::glib::Propagation::Stop
            }
            gtk4::gdk::Key::p | gtk4::gdk::Key::P => {
                fire_hook(&hooks, |h| &h.previous);
                gtk4::glib::Propagation::Stop
            }
            gtk4::gdk::Key::Left => {
                seek_relative(&hooks, &position, -SEEK_STEP_MS);
                gtk4::glib::Propagation::Stop
            }
            gtk4::gdk::Key::Right => {
                seek_relative(&hooks, &position, SEEK_STEP_MS);
                gtk4::glib::Propagation::Stop
            }
            gtk4::gdk::Key::Up => {
                adjust_volume(&volume_weak, VOLUME_STEP);
                gtk4::glib::Propagation::Stop
            }
            gtk4::gdk::Key::Down => {
                adjust_volume(&volume_weak, -VOLUME_STEP);
                gtk4::glib::Propagation::Stop
            }
            // Fall through for anything else — digits 1-8 select a mode;
            // everything else (Tab, Shift, …) is left to proceed so normal
            // keyboard focus navigation among the chrome's widgets still
            // works.
            other => match digit_mode_index(other) {
                Some(index) => {
                    if let Some(mode_row) = mode_row_weak.upgrade() {
                        select_mode_pill(&mode_row, index);
                    }
                    gtk4::glib::Propagation::Stop
                }
                None => gtk4::glib::Propagation::Proceed,
            },
        }
    });
    window.add_controller(key);

    let visualizer = visualizer.clone();
    window.connect_destroy(move |_| {
        visualizer.fullscreen_active.set(false);
        visualizer.fullscreen_window.borrow_mut().take();
        visualizer.fullscreen_chrome.borrow_mut().take();
        if !visualizer.panel_active.get() {
            if let Some(id) = visualizer.tick_id.borrow_mut().take() {
                id.remove();
            }
        }
    });

    window.fullscreen();
    window
}

/// The fullscreen canvas: same engine-driven scene as the inline canvas, but
/// with the dark vignette (see [`VIGNETTE_RGB`]) painted first so the scene
/// always reads over a moody backdrop rather than the accent-tinted CSS
/// background alone. Built as a GSK `glow_area::GlowArea` (real GPU blur) or
/// the Cairo `DrawingArea` fallback per `gpu_visuals_enabled()` — mirrors
/// `song_visualizer::build_canvas`, but kept local to this module since the
/// vignette pre-paint is fullscreen-only chrome, not something the inline
/// canvas needs.
fn fullscreen_canvas(engine: &Rc<RefCell<VisualEngine>>) -> gtk4::Widget {
    const CSS_CLASS: &str = "reprise-song-visual-fullscreen-canvas";
    if gpu_visuals_enabled() {
        let area = glow_area::GlowArea::new(engine.clone(), -1, true, CSS_CLASS);
        area.set_pre_paint(|cr, width, height| {
            paint_vignette(cr, f64::from(width), f64::from(height));
        });
        area.upcast()
    } else {
        let area = gtk4::DrawingArea::builder()
            .height_request(-1)
            .hexpand(true)
            .vexpand(true)
            .accessible_role(gtk4::AccessibleRole::Img)
            .build();
        area.add_css_class(CSS_CLASS);
        area.update_property(&[gtk4::accessible::Property::Label(&strings::text(
            strings::SONG_VISUALS_ACCESSIBLE,
        ))]);
        let engine = engine.clone();
        area.set_draw_func(move |area, cr, width, height| {
            paint_vignette(cr, f64::from(width), f64::from(height));
            let accent = accent_rgb(area);
            engine.borrow_mut().set_accent(accent);
            let scene = engine.borrow().scene(width as f32, height as f32);
            render::draw_scene(cr, &scene);
        });
        area.upcast()
    }
}

fn paint_vignette(cr: &gtk4::cairo::Context, width: f64, height: f64) {
    let (r, g, b) = VIGNETTE_RGB;
    let cx = width / 2.0;
    let cy = height / 2.0;
    let wash_r = (width.min(height) * 0.5).max(1.0);
    let edge_r = (width.max(height) * 0.75).max(1.0);

    let wash = gtk4::cairo::RadialGradient::new(cx, cy, 0.0, cx, cy, wash_r);
    wash.add_color_stop_rgba(0.0, r, g, b, VIGNETTE_CENTER_ALPHA);
    wash.add_color_stop_rgba(1.0, r, g, b, VIGNETTE_CENTER_ALPHA);
    if cr.set_source(&wash).is_ok() {
        cr.rectangle(0.0, 0.0, width, height);
        let _ = cr.fill();
    }

    let edge = gtk4::cairo::RadialGradient::new(cx, cy, wash_r * 0.6, cx, cy, edge_r);
    edge.add_color_stop_rgba(0.0, r, g, b, 0.0);
    edge.add_color_stop_rgba(1.0, r, g, b, VIGNETTE_EDGE_ALPHA);
    if cr.set_source(&edge).is_ok() {
        cr.rectangle(0.0, 0.0, width, height);
        let _ = cr.fill();
    }
}

/// Builds the header: timecode at the start, a centered state/title/meta
/// column — a [`gtk4::CenterBox`] keeps the column truly centered regardless
/// of the timecode's width. Returns the row plus the four labels that need
/// live updates.
#[allow(clippy::type_complexity)]
fn header_row() -> (
    gtk4::CenterBox,
    gtk4::Label,
    gtk4::Label,
    gtk4::Label,
    gtk4::Label,
) {
    let timecode = gtk4::Label::new(None);
    timecode.add_css_class("reprise-fs-timecode");
    timecode.set_halign(gtk4::Align::Start);
    timecode.set_margin_start(HEADER_MARGIN_SIDE);

    let state = gtk4::Label::new(None);
    state.add_css_class("reprise-fs-state");

    let title = gtk4::Label::new(None);
    title.add_css_class("reprise-fs-title");
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);

    let subtitle = gtk4::Label::new(None);
    subtitle.add_css_class("reprise-fs-meta");
    subtitle.set_ellipsize(gtk4::pango::EllipsizeMode::End);

    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    column.set_halign(gtk4::Align::Center);
    column.append(&state);
    column.append(&title);
    column.append(&subtitle);

    let header = gtk4::CenterBox::new();
    header.set_start_widget(Some(&timecode));
    header.set_center_widget(Some(&column));
    header.set_valign(gtk4::Align::Start);
    header.set_margin_top(HEADER_MARGIN_TOP);
    header.add_css_class("reprise-song-visual-chrome");
    header.add_css_class("reprise-fs-header-scrim");

    (header, title, subtitle, state, timecode)
}

/// Builds the bottom bar's five rows (cover/queue/volume, seek, transport,
/// mode pills, hint) and returns the row plus every widget that needs live
/// updates or event wiring — including the volume `Scale` and the mode-pill
/// `FlowBox`, which the fullscreen key controller (↑/↓ and digits 1–8) also
/// drives.
#[allow(clippy::type_complexity)]
fn bottom_bar(
    visualizer: &SongVisualizer,
) -> (
    gtk4::Box,
    gtk4::Picture,
    gtk4::Label,
    gtk4::Label,
    gtk4::Label,
    gtk4::Label,
    gtk4::Scale,
    Rc<Cell<bool>>,
    gtk4::Button,
    gtk4::Scale,
    gtk4::FlowBox,
) {
    let cover_thumb = gtk4::Picture::new();
    cover_thumb.set_content_fit(gtk4::ContentFit::Cover);
    cover_thumb.set_overflow(gtk4::Overflow::Hidden);
    cover_thumb.add_css_class("reprise-fs-cover-thumb");
    cover_thumb.set_size_request(COVER_THUMB_SIZE, COVER_THUMB_SIZE);

    let track_pos = gtk4::Label::new(None);
    track_pos.add_css_class("dim-label");
    track_pos.set_halign(gtk4::Align::Start);

    let next_up = gtk4::Label::new(None);
    next_up.add_css_class("dim-label");
    next_up.set_halign(gtk4::Align::Start);
    next_up.set_ellipsize(gtk4::pango::EllipsizeMode::End);

    let meta_column = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    meta_column.set_valign(gtk4::Align::Center);
    meta_column.append(&track_pos);
    meta_column.append(&next_up);

    let mute = gtk4::Button::from_icon_name(ICON_VOLUME_HIGH);
    mute.add_css_class("flat");
    buttons::arm(&mute, buttons::ICON_CLASS);
    mute.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::SONG_VISUALS_MUTE,
    ))]);

    let initial_volume = visualizer
        .hooks
        .borrow()
        .as_ref()
        .map_or(1.0, |hooks| hooks.initial_volume)
        .clamp(0.0, 1.0);
    let volume = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 1.0, 0.01);
    volume.set_size_request(VOLUME_SCALE_WIDTH, -1);
    volume.set_draw_value(false);
    volume.set_value(initial_volume);
    volume.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::SONG_VISUALS_VOLUME,
    ))]);
    wire_volume(&visualizer.hooks, &volume, &mute, initial_volume);

    let row1 = gtk4::Box::new(gtk4::Orientation::Horizontal, 14);
    row1.append(&cover_thumb);
    row1.append(&meta_column);
    let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    row1.append(&spacer);
    row1.append(&mute);
    row1.append(&volume);

    let time_cur = gtk4::Label::new(None);
    time_cur.add_css_class("dim-label");
    time_cur.add_css_class("reprise-fs-timecode");
    let time_total = gtk4::Label::new(None);
    time_total.add_css_class("dim-label");
    time_total.add_css_class("reprise-fs-timecode");

    let seek = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, SEEK_SCALE_MAX, 1.0);
    seek.set_hexpand(true);
    seek.set_draw_value(false);
    seek.add_css_class("reprise-fs-seek");
    seek.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::SONG_VISUALS_SEEK,
    ))]);
    let seek_updating = Rc::new(Cell::new(false));
    wire_seek(
        &visualizer.hooks,
        &visualizer.position,
        &seek,
        &seek_updating,
    );

    let row2 = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    row2.append(&time_cur);
    row2.append(&seek);
    row2.append(&time_total);

    let (row3, play_pause) = transport_row(visualizer);

    let row4 = mode_controls(&visualizer.engine, &visualizer.areas, &["reprise-fs-pill"]);

    let hint = gtk4::Label::new(Some(&strings::text(strings::SONG_VISUALS_FULLSCREEN_HINT)));
    hint.add_css_class("dim-label");

    let bottom = gtk4::Box::new(gtk4::Orientation::Vertical, 14);
    bottom.set_valign(gtk4::Align::End);
    bottom.set_margin_bottom(BOTTOM_MARGIN);
    bottom.set_margin_start(BOTTOM_MARGIN);
    bottom.set_margin_end(BOTTOM_MARGIN);
    bottom.add_css_class("reprise-song-visual-chrome");
    bottom.add_css_class("reprise-fs-bottom-scrim");
    bottom.append(&row1);
    bottom.append(&row2);
    bottom.append(&row3);
    bottom.append(&row4);
    bottom.append(&hint);

    (
        bottom,
        cover_thumb,
        track_pos,
        next_up,
        time_cur,
        time_total,
        seek,
        seek_updating,
        play_pause,
        volume,
        row4,
    )
}

/// Builds the transport row (previous · play/pause · stop · next), wired to
/// the stored [`PlayerHooks`]. Buttons read the callbacks at click time, so
/// they stay valid even if hooks are installed after this.
fn transport_row(visualizer: &SongVisualizer) -> (gtk4::Box, gtk4::Button) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 14);
    row.set_halign(gtk4::Align::Center);
    row.add_css_class("reprise-song-visual-transport");

    let button = |icon: &str, label: &str, primary: bool| {
        let button = gtk4::Button::from_icon_name(icon);
        button.add_css_class("circular");
        button.add_css_class("reprise-song-visual-transport-btn");
        if primary {
            button.add_css_class("reprise-song-visual-transport-primary");
        }
        button.update_property(&[gtk4::accessible::Property::Label(&strings::text(label))]);
        button
    };

    let previous = button(
        "media-skip-backward-symbolic",
        strings::SONG_VISUALS_PREVIOUS,
        false,
    );
    let play_pause_icon = if visualizer.playback.get() == PlaybackState::Playing {
        "media-playback-pause-symbolic"
    } else {
        "media-playback-start-symbolic"
    };
    let play_pause = button(play_pause_icon, strings::SONG_VISUALS_PLAY_PAUSE, true);
    let stop = button(
        "media-playback-stop-symbolic",
        strings::SONG_VISUALS_STOP,
        false,
    );
    let next = button(
        "media-skip-forward-symbolic",
        strings::SONG_VISUALS_NEXT,
        false,
    );

    let fire = |slot: HookSlot, hooks: &Rc<RefCell<Option<PlayerHooks>>>| {
        let hooks = hooks.clone();
        move || {
            if let Some(hooks) = hooks.borrow().as_ref() {
                (slot(hooks))();
            }
        }
    };
    let previous_cb = fire(|h| &h.previous, &visualizer.hooks);
    previous.connect_clicked(move |_| previous_cb());
    let play_pause_cb = fire(|h| &h.play_pause, &visualizer.hooks);
    play_pause.connect_clicked(move |_| play_pause_cb());
    let stop_cb = fire(|h| &h.stop, &visualizer.hooks);
    stop.connect_clicked(move |_| stop_cb());
    let next_cb = fire(|h| &h.next, &visualizer.hooks);
    next.connect_clicked(move |_| next_cb());

    row.append(&previous);
    row.append(&play_pause);
    row.append(&stop);
    row.append(&next);
    (row, play_pause)
}

/// Fires one `PlayerHooks` slot (see [`HookSlot`]) if hooks are installed —
/// the keyboard-shortcut equivalent of `transport_row`'s click handlers.
fn fire_hook(hooks: &Rc<RefCell<Option<PlayerHooks>>>, slot: HookSlot) {
    if let Some(hooks) = hooks.borrow().as_ref() {
        (slot(hooks))();
    }
}

/// The ←/→ keys' seek-by-`delta_ms`: reads the same `(position_ms,
/// duration_ms)` mirror the seek `Scale` uses (`SongVisualizer::position`),
/// clamps to the track's bounds, and sends the result straight to
/// `hooks.seek_to_ms` — the seek `Scale` itself catches up on the next
/// position tick via `FullscreenChrome::apply_position`.
fn seek_relative(
    hooks: &Rc<RefCell<Option<PlayerHooks>>>,
    position: &Rc<Cell<(i64, i64)>>,
    delta_ms: i64,
) {
    let (position_ms, duration_ms) = position.get();
    if duration_ms <= 0 {
        return;
    }
    let target_ms = (position_ms + delta_ms).clamp(0, duration_ms);
    if let Some(hooks) = hooks.borrow().as_ref() {
        (hooks.seek_to_ms)(target_ms);
    }
}

/// The ↑/↓ keys' volume step: nudges the volume `Scale`'s value, which fires
/// its existing `connect_value_changed` handler (`wire_volume`) and so calls
/// `hooks.set_volume` for us — this is the single write that keeps the slider
/// and the player's volume in sync.
fn adjust_volume(volume: &gtk4::glib::WeakRef<gtk4::Scale>, delta: f64) {
    let Some(volume) = volume.upgrade() else {
        return;
    };
    let new_value = (volume.value() + delta).clamp(0.0, 1.0);
    volume.set_value(new_value);
}

/// Maps digit keys 1–8 to a zero-based index into `VisualMode::ALL` /
/// the mode-pill `FlowBox` (same order, see `mode_controls`).
fn digit_mode_index(key: gtk4::gdk::Key) -> Option<usize> {
    use gtk4::gdk::Key;
    match key {
        Key::_1 => Some(0),
        Key::_2 => Some(1),
        Key::_3 => Some(2),
        Key::_4 => Some(3),
        Key::_5 => Some(4),
        Key::_6 => Some(5),
        Key::_7 => Some(6),
        Key::_8 => Some(7),
        _ => None,
    }
}

/// Activates the mode pill at `index` (the digit-key equivalent of clicking
/// it): `ToggleButton::set_active` fires the same `connect_toggled` handler
/// `mode_controls` wired up, which sets the engine's mode and queues a
/// redraw — so this one call satisfies "select mode + queue redraw + reflect
/// in the picker" together.
fn select_mode_pill(mode_row: &gtk4::FlowBox, index: usize) {
    let Some(child) = mode_row.child_at_index(index as i32) else {
        return;
    };
    let Some(widget) = child.child() else {
        return;
    };
    if let Ok(button) = widget.downcast::<gtk4::ToggleButton>() {
        button.set_active(true);
    }
}

/// Wires the seek scale: user drags/clicks/keypresses fire `change-value`,
/// converted to a target ms via the latest known duration and sent to
/// `hooks.seek_to_ms`. `seek_updating` — set around `FullscreenChrome::
/// apply_position`'s own `set_value` call — breaks the feedback loop where a
/// position tick's programmatic write would otherwise re-fire a seek.
fn wire_seek(
    hooks: &Rc<RefCell<Option<PlayerHooks>>>,
    position: &Rc<Cell<(i64, i64)>>,
    seek: &gtk4::Scale,
    seek_updating: &Rc<Cell<bool>>,
) {
    let hooks = hooks.clone();
    let position = position.clone();
    let seek_updating = seek_updating.clone();
    seek.connect_change_value(move |_, _, value| {
        if seek_updating.get() {
            return gtk4::glib::Propagation::Proceed;
        }
        let duration_ms = position.get().1;
        if duration_ms > 0 {
            if let Some(hooks) = hooks.borrow().as_ref() {
                let fraction = (value / SEEK_SCALE_MAX).clamp(0.0, 1.0);
                (hooks.seek_to_ms)((fraction * duration_ms as f64).round() as i64);
            }
        }
        gtk4::glib::Propagation::Proceed
    });
}

/// Wires the volume scale to `hooks.set_volume` and the mute button as a
/// self-contained toggle (no dedicated `PlayerHooks` slot: mute is just
/// "drive the scale to 0, remembering the prior value to restore"). Mirrors
/// `player_bar::PlayerBar::connect_mute_toggled`'s shape. Known v1
/// limitation: this volume slider only tracks its own changes plus the
/// `initial_volume` it opened with — external changes (bar, MPRIS) don't
/// reach it while the overlay is open.
fn wire_volume(
    hooks: &Rc<RefCell<Option<PlayerHooks>>>,
    volume: &gtk4::Scale,
    mute: &gtk4::Button,
    initial_volume: f64,
) {
    let updating = Rc::new(Cell::new(false));
    let muted = Rc::new(Cell::new(false));
    let pre_mute_volume = Rc::new(Cell::new(initial_volume.max(0.01)));

    {
        let hooks = hooks.clone();
        let updating = updating.clone();
        volume.connect_value_changed(move |scale| {
            if updating.get() {
                return;
            }
            if let Some(hooks) = hooks.borrow().as_ref() {
                (hooks.set_volume)(scale.value());
            }
        });
    }

    let volume_weak = volume.downgrade();
    let hooks = hooks.clone();
    mute.connect_clicked(move |mute_button| {
        let Some(volume) = volume_weak.upgrade() else {
            return;
        };
        let is_muted = muted.get();
        let new_value = if is_muted {
            pre_mute_volume.get()
        } else {
            pre_mute_volume.set(volume.value().max(0.01));
            0.0
        };
        updating.set(true);
        volume.set_value(new_value);
        updating.set(false);
        mute_button.set_icon_name(if new_value <= 0.0 {
            ICON_VOLUME_MUTED
        } else {
            ICON_VOLUME_HIGH
        });
        muted.set(!is_muted);
        if let Some(hooks) = hooks.borrow().as_ref() {
            (hooks.set_volume)(new_value);
        }
    });
}
