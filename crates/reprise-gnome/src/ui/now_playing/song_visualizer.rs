//! Audio-reactive song visuals for the Now Playing Audio Character page.
//!
//! All reactive state (eased spectrum bands, envelopes, water, dust, impact
//! overlay, accent palette) and the per-mode geometry live in
//! `reprise_core::visuals::VisualEngine` — a portable core the GUI never has
//! to reimplement. This module owns the inline canvas + mode picker shell;
//! the fullscreen overlay's chrome (header, transport, seek, volume, mode
//! pills) lives in `song_visualizer/fullscreen.rs`, which reaches back into
//! this module's private helpers (`drawing_area`, `mode_controls`, the
//! [`FullscreenChrome`] live-update handles, …) as a descendant module. Both
//! turn the engine's [`reprise_core::visuals::Scene`] into pixels via
//! [`render`].

mod fullscreen;
mod render;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::playback::{PlaybackState, SpectrumFrame};
use reprise_core::visuals::{VisualEngine, VisualMode};

use crate::ui::style::buttons;
use crate::ui::{motion, strings};

const DRAW_HEIGHT: i32 = 220;
/// Edge length (px) the cover texture is rasterized down to before feeding
/// the engine's secondary-accent palette extraction — cheap and plenty for a
/// hue/saturation sample.
const COVER_PALETTE_EDGE: i32 = 32;
const COVER_PALETTE_PIXELS: usize = (COVER_PALETTE_EDGE * COVER_PALETTE_EDGE) as usize;
/// Width (px) the cover texture is rasterized down to for the fullscreen
/// backdrop. Small enough that the `Picture`'s bilinear upscaling reads as a
/// soft wash of the cover's dominant colors — a fast, GPU-independent
/// "fake blur" — rather than a sharp thumbnail.
const BACKDROP_WIDTH: i32 = 24;
/// The fullscreen seek `Scale`'s range top; its value is always
/// `fraction * SEEK_SCALE_MAX`.
const SEEK_SCALE_MAX: f64 = 1000.0;

/// Player actions (and the volume slider's starting value) the fullscreen
/// overlay drives. Wired from the player controller; the widget works
/// standalone without hooks installed (tests) since `hooks` starts `None`.
pub(in crate::ui) struct PlayerHooks {
    pub(in crate::ui) previous: Rc<dyn Fn()>,
    pub(in crate::ui) play_pause: Rc<dyn Fn()>,
    pub(in crate::ui) stop: Rc<dyn Fn()>,
    pub(in crate::ui) next: Rc<dyn Fn()>,
    pub(in crate::ui) seek_to_ms: Rc<dyn Fn(i64)>,
    pub(in crate::ui) set_volume: Rc<dyn Fn(f64)>,
    pub(in crate::ui) initial_volume: f64,
}

/// Live fullscreen-chrome widgets, present only while the overlay window is
/// open. One struct rather than a handful of parallel `Option` fields on
/// [`SongVisualizer`] because every member shares the same lifecycle: built
/// in `fullscreen::build`, populated immediately via the `apply_*` methods
/// below, mirrored on every subsequent `set_*` call, and dropped together in
/// the fullscreen window's `connect_destroy` handler.
struct FullscreenChrome {
    title: gtk4::Label,
    subtitle: gtk4::Label,
    state: gtk4::Label,
    track_pos: gtk4::Label,
    next_up: gtk4::Label,
    cover_thumb: gtk4::Picture,
    backdrop: gtk4::Picture,
    play_pause: gtk4::Button,
    time_cur: gtk4::Label,
    time_total: gtk4::Label,
    timecode: gtk4::Label,
    seek: gtk4::Scale,
    /// Guards the seek `Scale`'s `change-value` handler against a feedback
    /// loop with `apply_position`'s own programmatic `set_value` — set
    /// around that write, checked (and ignored) by the handler in
    /// `fullscreen.rs::wire_seek`.
    seek_updating: Rc<Cell<bool>>,
}

impl FullscreenChrome {
    fn apply_track_meta(&self, title: &str, subtitle: &str) {
        self.title.set_label(title);
        self.subtitle.set_label(subtitle);
        self.subtitle.set_visible(!subtitle.is_empty());
    }

    fn apply_playback_state(&self, playback: PlaybackState) {
        self.play_pause
            .set_icon_name(if playback == PlaybackState::Playing {
                "media-playback-pause-symbolic"
            } else {
                "media-playback-start-symbolic"
            });
        let text = match playback {
            PlaybackState::Playing => strings::SONG_VISUALS_STATE_PLAYING,
            PlaybackState::Paused => strings::SONG_VISUALS_STATE_PAUSED,
            PlaybackState::Stopped => strings::SONG_VISUALS_STATE_STOPPED,
        };
        self.state.set_label(&strings::text(text).to_uppercase());
    }

    fn apply_position(&self, position_ms: i64, duration_ms: i64) {
        self.time_cur.set_label(&format_time(position_ms));
        self.time_total.set_label(&format_time(duration_ms));
        self.timecode.set_label(&format!(
            "{} / {}",
            format_time(position_ms),
            format_time(duration_ms)
        ));
        self.seek_updating.set(true);
        self.seek
            .set_value(seek_fraction(position_ms, duration_ms) * SEEK_SCALE_MAX);
        self.seek_updating.set(false);
    }

    fn apply_cover(&self, texture: Option<&gtk4::gdk::Texture>) {
        match texture {
            Some(texture) => {
                self.cover_thumb.set_paintable(Some(texture));
                self.cover_thumb.set_visible(true);
            }
            None => {
                self.cover_thumb.set_paintable(gtk4::gdk::Paintable::NONE);
                self.cover_thumb.set_visible(false);
            }
        }
        match texture.and_then(|texture| backdrop_texture(texture, BACKDROP_WIDTH)) {
            Some(blurred) => {
                self.backdrop.set_paintable(Some(&blurred));
                self.backdrop.set_visible(true);
            }
            None => {
                self.backdrop.set_paintable(gtk4::gdk::Paintable::NONE);
                self.backdrop.set_visible(false);
            }
        }
    }

    fn apply_next_up(&self, line: Option<&str>) {
        self.next_up.set_label(line.unwrap_or_default());
        self.next_up.set_visible(line.is_some());
    }

    fn apply_queue_position(&self, index: usize, total: usize) {
        self.track_pos.set_label(&strings::formatted(
            strings::SONG_VISUALS_TRACK_POS,
            &[
                ("index", &(index + 1).to_string()),
                ("total", &total.to_string()),
            ],
        ));
    }
}

#[derive(Clone)]
pub(in crate::ui) struct SongVisualizer {
    root: gtk4::Box,
    area: gtk4::DrawingArea,
    areas: Rc<RefCell<Vec<gtk4::glib::WeakRef<gtk4::DrawingArea>>>>,
    engine: Rc<RefCell<VisualEngine>>,
    /// Mirrored outside the engine (which has no getter) so `set_spectrum`
    /// can gate on "are we actually playing" without borrowing it.
    playback: Rc<Cell<PlaybackState>>,
    panel_active: Rc<Cell<bool>>,
    fullscreen_active: Rc<Cell<bool>>,
    fullscreen_window: Rc<RefCell<Option<gtk4::glib::WeakRef<gtk4::Window>>>>,
    tick_id: Rc<RefCell<Option<gtk4::TickCallbackId>>>,
    /// Current track title + subtitle, mirrored into the fullscreen header.
    meta: Rc<RefCell<(String, String)>>,
    hooks: Rc<RefCell<Option<PlayerHooks>>>,
    /// Latest position tick (`position_ms`, `duration_ms`), mirrored into the
    /// fullscreen seek row.
    position: Rc<Cell<(i64, i64)>>,
    /// Latest cover texture, mirrored into the fullscreen backdrop and cover
    /// thumbnail. Also drives the engine's secondary accent (see
    /// `set_cover`).
    cover: Rc<RefCell<Option<gtk4::gdk::Texture>>>,
    /// Pre-formatted "Up next: …" line, mirrored into the fullscreen queue
    /// strip.
    next_up: Rc<RefCell<Option<String>>>,
    /// `(index, total)` within the up-next queue, mirrored into the
    /// fullscreen "TRACK i / n" label.
    queue_position: Rc<Cell<(usize, usize)>>,
    /// Live fullscreen-chrome widget handles, `Some` only while the overlay
    /// is open — see [`FullscreenChrome`].
    fullscreen_chrome: Rc<RefCell<Option<FullscreenChrome>>>,
}

impl SongVisualizer {
    pub(in crate::ui) fn new() -> Self {
        let engine = Rc::new(RefCell::new(VisualEngine::new()));
        let areas = Rc::new(RefCell::new(Vec::new()));
        let area = drawing_area(&engine, DRAW_HEIGHT, "reprise-song-visual-canvas");
        register_area(&areas, &area);

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        root.add_css_class("reprise-song-visuals");
        root.append(&area);
        root.append(&mode_controls(&engine, &areas, &[]));
        Self {
            root,
            area,
            areas,
            engine,
            playback: Rc::new(Cell::new(PlaybackState::Stopped)),
            panel_active: Rc::new(Cell::new(false)),
            fullscreen_active: Rc::new(Cell::new(false)),
            fullscreen_window: Rc::new(RefCell::new(None)),
            tick_id: Rc::new(RefCell::new(None)),
            meta: Rc::new(RefCell::new((String::new(), String::new()))),
            hooks: Rc::new(RefCell::new(None)),
            position: Rc::new(Cell::new((0, 0))),
            cover: Rc::new(RefCell::new(None)),
            next_up: Rc::new(RefCell::new(None)),
            queue_position: Rc::new(Cell::new((0, 0))),
            fullscreen_chrome: Rc::new(RefCell::new(None)),
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    /// Mirrors the current track's title and subtitle into the fullscreen
    /// header. No-op on the inline canvas (which sits beside the panel's own
    /// metadata); only the immersive fullscreen view shows it.
    pub(in crate::ui) fn set_track_meta(&self, title: &str, subtitle: &str) {
        *self.meta.borrow_mut() = (title.to_owned(), subtitle.to_owned());
        if let Some(chrome) = self.fullscreen_chrome.borrow().as_ref() {
            chrome.apply_track_meta(title, subtitle);
        }
    }

    /// Wires the fullscreen transport buttons, seek scale, and volume slider
    /// to player actions.
    pub(in crate::ui) fn set_player_hooks(&self, hooks: PlayerHooks) {
        *self.hooks.borrow_mut() = Some(hooks);
    }

    /// Mirrors the live playback position into the fullscreen seek row (time
    /// labels, timecode, and the seek scale itself).
    pub(in crate::ui) fn set_position(&self, position_ms: i64, duration_ms: i64) {
        self.position.set((position_ms, duration_ms));
        if let Some(chrome) = self.fullscreen_chrome.borrow().as_ref() {
            chrome.apply_position(position_ms, duration_ms);
        }
    }

    /// Mirrors the current cover into the fullscreen backdrop and cover
    /// thumbnail AND feeds the engine's secondary accent: the texture is
    /// rasterized down to a small RGBA sample and handed to
    /// `VisualEngine::set_cover_pixels`, or cleared on `None`.
    pub(in crate::ui) fn set_cover(&self, texture: Option<gtk4::gdk::Texture>) {
        match &texture {
            Some(texture) => match downscale_cover_rgba(texture, COVER_PALETTE_EDGE) {
                Some(rgba) => self
                    .engine
                    .borrow_mut()
                    .set_cover_pixels(&rgba, COVER_PALETTE_PIXELS),
                None => self.engine.borrow_mut().clear_cover(),
            },
            None => self.engine.borrow_mut().clear_cover(),
        }
        if let Some(chrome) = self.fullscreen_chrome.borrow().as_ref() {
            chrome.apply_cover(texture.as_ref());
        }
        *self.cover.borrow_mut() = texture;
        queue_registered_areas(&self.areas);
    }

    /// Mirrors the pre-formatted "Up next: …" line into the fullscreen queue
    /// strip.
    pub(in crate::ui) fn set_next_up(&self, line: Option<String>) {
        if let Some(chrome) = self.fullscreen_chrome.borrow().as_ref() {
            chrome.apply_next_up(line.as_deref());
        }
        *self.next_up.borrow_mut() = line;
    }

    /// Mirrors the up-next queue position (`index`, `total`) into the
    /// fullscreen "TRACK i / n" label.
    pub(in crate::ui) fn set_queue_position(&self, index: usize, total: usize) {
        self.queue_position.set((index, total));
        if let Some(chrome) = self.fullscreen_chrome.borrow().as_ref() {
            chrome.apply_queue_position(index, total);
        }
    }

    /// A new track started: resets the engine's clock, water surface, and
    /// impact overlay so ripples/sparks from the previous track don't bleed
    /// into the new one.
    pub(in crate::ui) fn note_track_changed(&self) {
        self.engine.borrow_mut().note_track_changed();
        queue_registered_areas(&self.areas);
    }

    pub(in crate::ui) fn set_profile(&self, dimensions: &[u8; 4]) {
        self.engine.borrow_mut().set_static_profile(dimensions);
        self.settle_or_animate();
    }

    pub(in crate::ui) fn clear_profile(&self) {
        self.engine.borrow_mut().clear_static_profile();
        self.settle_or_animate();
    }

    pub(in crate::ui) fn set_spectrum(&self, frame: SpectrumFrame) {
        if !motion::animations_enabled() || self.playback.get() != PlaybackState::Playing {
            return;
        }
        self.engine.borrow_mut().ingest(&frame);
        self.ensure_tick();
    }

    pub(in crate::ui) fn set_playback_state(&self, playback: PlaybackState) {
        if let Some(chrome) = self.fullscreen_chrome.borrow().as_ref() {
            chrome.apply_playback_state(playback);
        }
        self.playback.set(playback);
        let animations_enabled = motion::animations_enabled();
        {
            let mut engine = self.engine.borrow_mut();
            engine.set_playing(playback == PlaybackState::Playing);
            if !animations_enabled {
                engine.snap_to_static();
            }
        }
        if playback == PlaybackState::Playing || animations_enabled {
            self.ensure_tick();
        } else {
            self.stop_tick();
            queue_registered_areas(&self.areas);
        }
    }

    pub(in crate::ui) fn set_active(&self, active: bool) {
        self.panel_active.set(active);
        if self.is_active() {
            self.ensure_tick();
        } else {
            self.stop_tick();
        }
    }

    pub(in crate::ui) fn close_fullscreen(&self) {
        let window = self
            .fullscreen_window
            .borrow_mut()
            .take()
            .and_then(|window| window.upgrade());
        if let Some(window) = window {
            window.close();
        }
    }

    pub(in crate::ui) fn toggle_fullscreen(&self, parent: &adw::ApplicationWindow) {
        let existing = self
            .fullscreen_window
            .borrow_mut()
            .take()
            .and_then(|window| window.upgrade());
        if let Some(window) = existing {
            window.close();
            return;
        }

        let window = fullscreen::build(self, parent);
        self.fullscreen_active.set(true);
        *self.fullscreen_window.borrow_mut() = Some(window.downgrade());
        self.ensure_tick();
        window.present();
    }

    fn is_active(&self) -> bool {
        self.panel_active.get() || self.fullscreen_active.get()
    }

    /// After a static-profile change: ease toward it over the tick loop when
    /// motion is allowed, otherwise snap straight to rest and repaint once.
    fn settle_or_animate(&self) {
        if motion::animations_enabled() {
            self.ensure_tick();
        } else {
            self.engine.borrow_mut().snap_to_static();
            queue_registered_areas(&self.areas);
        }
    }

    fn ensure_tick(&self) {
        if !self.is_active() || !motion::animations_enabled() || self.tick_id.borrow().is_some() {
            return;
        }
        let engine = self.engine.clone();
        let areas = self.areas.clone();
        let panel_active = self.panel_active.clone();
        let fullscreen_active = self.fullscreen_active.clone();
        let slot = self.tick_id.clone();
        let id = self.area.add_tick_callback(move |_, _| {
            if (!panel_active.get() && !fullscreen_active.get()) || !motion::animations_enabled() {
                *slot.borrow_mut() = None;
                return gtk4::glib::ControlFlow::Break;
            }
            let settled = engine.borrow_mut().tick();
            queue_registered_areas(&areas);
            if settled {
                *slot.borrow_mut() = None;
                gtk4::glib::ControlFlow::Break
            } else {
                gtk4::glib::ControlFlow::Continue
            }
        });
        *self.tick_id.borrow_mut() = Some(id);
    }

    fn stop_tick(&self) {
        if let Some(id) = self.tick_id.borrow_mut().take() {
            id.remove();
        }
    }
}

fn drawing_area(
    engine: &Rc<RefCell<VisualEngine>>,
    height_request: i32,
    css_class: &str,
) -> gtk4::DrawingArea {
    let area = gtk4::DrawingArea::builder()
        .height_request(height_request)
        .hexpand(true)
        .accessible_role(gtk4::AccessibleRole::Img)
        .build();
    area.add_css_class(css_class);
    area.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::SONG_VISUALS_ACCESSIBLE,
    ))]);
    let engine = engine.clone();
    area.set_draw_func(move |area, cr, width, height| {
        let accent = accent_rgb(area);
        engine.borrow_mut().set_accent(accent);
        let scene = engine.borrow().scene(width as f32, height as f32);
        render::draw_scene(cr, &scene);
    });
    area
}

fn register_area(
    areas: &Rc<RefCell<Vec<gtk4::glib::WeakRef<gtk4::DrawingArea>>>>,
    area: &gtk4::DrawingArea,
) {
    let weak = gtk4::glib::WeakRef::new();
    weak.set(Some(area));
    areas.borrow_mut().push(weak);
}

fn queue_registered_areas(areas: &Rc<RefCell<Vec<gtk4::glib::WeakRef<gtk4::DrawingArea>>>>) {
    areas.borrow_mut().retain(|weak| {
        let Some(area) = weak.upgrade() else {
            return false;
        };
        area.queue_draw();
        true
    });
}

fn accent_rgb(area: &gtk4::DrawingArea) -> (f32, f32, f32) {
    let color = area.color();
    (color.red(), color.green(), color.blue())
}

/// Rasterizes `texture` down to an `edge`×`edge` RGBA byte buffer for the
/// engine's secondary-accent palette sample. Uses a `gtk4::Snapshot` →
/// `gsk::RenderNode` → cairo surface round trip (GSK does the scaling while
/// rasterizing) rather than `gdk_texture_download`'s full-resolution
/// readback — cheap regardless of the source texture's size, and avoids the
/// deprecated `gdk_pixbuf_get_from_texture`. `None` if rasterization fails
/// (an unreadable/zero-size texture); callers fall back to clearing the
/// engine's cover accent.
fn downscale_cover_rgba(texture: &gtk4::gdk::Texture, edge: i32) -> Option<Vec<u8>> {
    let snapshot = gtk4::Snapshot::new();
    let bounds = gtk4::graphene::Rect::new(0.0, 0.0, edge as f32, edge as f32);
    snapshot.append_texture(texture, &bounds);
    let node = snapshot.to_node()?;

    let mut surface =
        gtk4::cairo::ImageSurface::create(gtk4::cairo::Format::ARgb32, edge, edge).ok()?;
    {
        let cr = gtk4::cairo::Context::new(&surface).ok()?;
        node.draw(&cr);
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().ok()?;

    let edge = edge as usize;
    let mut rgba = Vec::with_capacity(edge * edge * 4);
    for y in 0..edge {
        for x in 0..edge {
            let o = y * stride + x * 4;
            if o + 3 >= data.len() {
                continue;
            }
            // Cairo's ARGB32 is premultiplied, native-endian — on the
            // little-endian hosts this app targets that's byte order
            // [B, G, R, A], same assumption `render_mode_gallery_ppm`
            // (below, in tests) makes when writing PPM output.
            rgba.push(data[o + 2]);
            rgba.push(data[o + 1]);
            rgba.push(data[o]);
            rgba.push(data[o + 3]);
        }
    }
    Some(rgba)
}

/// Rasterizes `texture` down to a `width`-px-wide (aspect-preserving) surface
/// for the fullscreen backdrop and wraps the raw bytes in a
/// `gdk::MemoryTexture` — no RGB reordering needed here (unlike
/// `downscale_cover_rgba`, which feeds a byte-order-agnostic palette
/// sampler): Cairo's premultiplied `ARGB32` byte layout on the little-endian
/// hosts this app targets is exactly GDK's `B8g8r8a8Premultiplied`, so the
/// surface's bytes are handed to `MemoryTexture` as-is. `None` if
/// rasterization fails or the source texture reports a zero size.
fn backdrop_texture(texture: &gtk4::gdk::Texture, width: i32) -> Option<gtk4::gdk::Texture> {
    let (tex_w, tex_h) = (texture.width(), texture.height());
    if tex_w <= 0 || tex_h <= 0 || width <= 0 {
        return None;
    }
    let height = ((width as f32) * (tex_h as f32) / (tex_w as f32))
        .round()
        .max(1.0) as i32;

    let snapshot = gtk4::Snapshot::new();
    let bounds = gtk4::graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
    snapshot.append_texture(texture, &bounds);
    let node = snapshot.to_node()?;

    let mut surface =
        gtk4::cairo::ImageSurface::create(gtk4::cairo::Format::ARgb32, width, height).ok()?;
    {
        let cr = gtk4::cairo::Context::new(&surface).ok()?;
        node.draw(&cr);
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let bytes = gtk4::glib::Bytes::from_owned(surface.data().ok()?.to_vec());
    Some(
        gtk4::gdk::MemoryTexture::new(
            width,
            height,
            gtk4::gdk::MemoryFormat::B8g8r8a8Premultiplied,
            &bytes,
            stride,
        )
        .upcast(),
    )
}

fn format_time(ms: i64) -> String {
    let seconds = ms.max(0) / 1_000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn seek_fraction(position_ms: i64, duration_ms: i64) -> f64 {
    if duration_ms <= 0 {
        0.0
    } else {
        (position_ms as f64 / duration_ms as f64).clamp(0.0, 1.0)
    }
}

/// The picker's user-facing label for one visual mode.
fn mode_label(mode: VisualMode) -> &'static str {
    match mode {
        VisualMode::Grid => strings::SONG_VISUALS_MODE_GRID,
        VisualMode::Bars => strings::SONG_VISUALS_MODE_BARS,
        VisualMode::Rings => strings::SONG_VISUALS_MODE_RINGS,
        VisualMode::Flow => strings::SONG_VISUALS_MODE_FLOW,
        VisualMode::Pulse => strings::SONG_VISUALS_MODE_PULSE,
        VisualMode::Particles => strings::SONG_VISUALS_MODE_PARTICLES,
        VisualMode::Neon => strings::SONG_VISUALS_MODE_NEON,
        VisualMode::Tunnel => strings::SONG_VISUALS_MODE_TUNNEL,
    }
}

/// Builds the grouped mode-toggle row: one [`gtk4::ToggleButton`] per
/// [`VisualMode`], wrapped in a [`gtk4::FlowBox`] so it reflows at narrow
/// widths instead of overflowing. Shared by the inline canvas and the
/// fullscreen overlay — each call builds a fresh, independent row that reads
/// the engine's current mode at construction time. `extra_classes` lets the
/// fullscreen overlay style its pills (`.reprise-fs-pill`) without leaking
/// that dark-surface styling onto the docked panel's copy.
fn mode_controls(
    engine: &Rc<RefCell<VisualEngine>>,
    areas: &Rc<RefCell<Vec<gtk4::glib::WeakRef<gtk4::DrawingArea>>>>,
    extra_classes: &[&str],
) -> gtk4::FlowBox {
    let flow = gtk4::FlowBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .max_children_per_line(4)
        .column_spacing(6)
        .row_spacing(6)
        .halign(gtk4::Align::Center)
        .build();
    flow.add_css_class("reprise-song-visual-modes");

    let current_mode = engine.borrow().mode();
    let mut group_leader: Option<gtk4::ToggleButton> = None;
    for mode in VisualMode::ALL {
        let button = gtk4::ToggleButton::builder()
            .label(strings::text(mode_label(mode)))
            .active(mode == current_mode)
            .build();
        button.set_widget_name(mode.id());
        button.add_css_class("flat");
        for class in extra_classes {
            button.add_css_class(class);
        }
        buttons::arm(&button, buttons::TOGGLE_CLASS);
        match &group_leader {
            Some(leader) => button.set_group(Some(leader)),
            None => group_leader = Some(button.clone()),
        }

        let engine = engine.clone();
        let areas = areas.clone();
        button.connect_toggled(move |button| {
            if !button.is_active() {
                return;
            }
            engine.borrow_mut().set_mode(mode);
            queue_registered_areas(&areas);
        });

        flow.append(&button);
    }
    flow
}

/// Fades the fullscreen chrome (header + bottom bar) out after the pointer sits
/// idle and brings it — plus the cursor — back on the next mouse move. Immersive
/// by default: the GUI only appears when you reach for it.
fn install_chrome_autohide(
    window: &gtk4::Window,
    overlay: &gtk4::Overlay,
    header: &gtk4::Widget,
    bottom: &gtk4::Widget,
) {
    const IDLE: Duration = Duration::from_millis(2500);
    let timer: Rc<RefCell<Option<gtk4::glib::SourceId>>> = Rc::new(RefCell::new(None));
    let header = header.downgrade();
    let bottom = bottom.downgrade();
    let window_weak = window.downgrade();

    let reveal = move || {
        if let Some(header) = header.upgrade() {
            header.remove_css_class("reprise-song-visual-chrome-hidden");
        }
        if let Some(bottom) = bottom.upgrade() {
            bottom.remove_css_class("reprise-song-visual-chrome-hidden");
        }
        if let Some(window) = window_weak.upgrade() {
            window.set_cursor(None);
        }
        if let Some(id) = timer.borrow_mut().take() {
            id.remove();
        }
        let header = header.clone();
        let bottom = bottom.clone();
        let window_weak = window_weak.clone();
        let timer_inner = timer.clone();
        let id = gtk4::glib::timeout_add_local_once(IDLE, move || {
            if let Some(header) = header.upgrade() {
                header.add_css_class("reprise-song-visual-chrome-hidden");
            }
            if let Some(bottom) = bottom.upgrade() {
                bottom.add_css_class("reprise-song-visual-chrome-hidden");
            }
            if let Some(window) = window_weak.upgrade() {
                window.set_cursor_from_name(Some("none"));
            }
            timer_inner.borrow_mut().take();
        });
        *timer.borrow_mut() = Some(id);
    };

    let motion = gtk4::EventControllerMotion::new();
    let reveal = Rc::new(reveal);
    let reveal_motion = reveal.clone();
    motion.connect_motion(move |_, _, _| reveal_motion());
    overlay.add_controller(motion);
    // Show once on open, then arm the idle timer.
    reveal();
}

pub(in crate::ui) fn css() -> String {
    ".reprise-song-visuals { margin: 0 18px 12px; }\n\
     .reprise-song-visual-canvas {\
       color: @reprise_player_accent;\
       background-color: alpha(#ffffff, 0.025);\
       border: 1px solid alpha(@reprise_player_accent, 0.14);\
       border-radius: 24px;\
     }\n\
     .reprise-song-visual-modes { margin-top: 2px; }\n\
     window.reprise-song-visual-fullscreen { background: #090b0c; }\n\
     .reprise-song-visual-chrome {\
       transition: opacity 260ms ease-out;\
       opacity: 1;\
     }\n\
     .reprise-song-visual-chrome-hidden { opacity: 0; }\n\
     .reprise-song-visual-fullscreen-canvas {\
       color: @reprise_player_accent;\
       background-image: radial-gradient(ellipse at center,\
         alpha(@reprise_player_accent, 0.12) 0%,\
         alpha(#090b0c, 0) 72%);\
     }\n\
     .reprise-fs-header-scrim {\
       background: linear-gradient(to bottom, alpha(#0b0c15, 0.55), alpha(#0b0c15, 0));\
     }\n\
     .reprise-fs-bottom-scrim {\
       background: linear-gradient(to top, alpha(#0b0c15, 0.6), alpha(#0b0c15, 0));\
     }\n\
     .reprise-fs-timecode {\
       font-size: 13px; letter-spacing: 0.08em; color: alpha(#ffffff, 0.45);\
     }\n\
     .reprise-fs-state {\
       font-size: 12px; letter-spacing: 0.22em;\
       color: alpha(@reprise_player_accent, 0.85);\
     }\n\
     .reprise-fs-title { font-size: 36px; font-weight: 600; color: #ffffff; }\n\
     .reprise-fs-meta { font-size: 16px; color: alpha(#ffffff, 0.7); }\n\
     .reprise-fs-backdrop { opacity: 0.45; }\n\
     .reprise-fs-cover-thumb { border-radius: 14px; }\n\
     .reprise-fs-pill {\
       border-radius: 999px;\
       background-color: alpha(#ffffff, 0.06);\
       border: 1px solid alpha(#ffffff, 0.14);\
       color: alpha(#ffffff, 0.75);\
       padding: 6px 14px;\
     }\n\
     .reprise-fs-pill:checked {\
       border-color: alpha(@reprise_player_accent, 0.8);\
       background-color: alpha(@reprise_player_accent, 0.12);\
       color: #ffffff;\
     }\n\
     .reprise-song-visual-header-title {\
       font-size: 1.6rem; font-weight: 800; color: #ffffff;\
     }\n\
     .reprise-song-visual-header-subtitle { font-size: 1.05rem; }\n\
     .reprise-song-visual-transport-btn {\
       min-width: 46px; min-height: 46px;\
       color: alpha(#ffffff, 0.82);\
       background-color: alpha(#ffffff, 0.08);\
       border: 1px solid alpha(#ffffff, 0.12);\
     }\n\
     .reprise-song-visual-transport-btn:hover {\
       color: #ffffff; background-color: alpha(#ffffff, 0.16);\
     }\n\
     .reprise-song-visual-transport-primary {\
       min-width: 56px; min-height: 56px;\
       color: #ffffff;\
       background-color: alpha(@reprise_player_accent, 0.28);\
       border-color: alpha(@reprise_player_accent, 0.85);\
     }\n\
     .reprise-song-visual-transport-primary:hover {\
       background-color: alpha(@reprise_player_accent, 0.42);\
     }"
    .to_owned()
}

#[cfg(test)]
#[path = "song_visualizer_tests.rs"]
mod tests;
