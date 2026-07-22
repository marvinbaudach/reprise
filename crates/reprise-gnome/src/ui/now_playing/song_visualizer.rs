//! Audio-reactive song visuals for the Now Playing Audio Character page.
//!
//! All reactive state (eased spectrum bands, envelopes, water, dust, impact
//! overlay, accent palette) and the per-mode geometry live in
//! `reprise_core::visuals::VisualEngine` — a portable core the GUI never has
//! to reimplement. This module owns the inline canvas + mode picker shell
//! embedded in the Now Playing panel. It turns the engine's
//! [`reprise_core::visuals::Scene`] into pixels via [`render`], through a
//! Cairo `DrawingArea` driven by the tick loop and `queue_registered_areas`.

mod render;
mod song_visualizer_util;

use song_visualizer_util::downscale_cover_rgba;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
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

#[derive(Clone)]
pub(in crate::ui) struct SongVisualizer {
    root: gtk4::Box,
    /// The inline Cairo `DrawingArea`, upcast to `gtk4::Widget`. Used only as
    /// the tick loop's `add_tick_callback` host; drawing itself goes through
    /// `areas`.
    area: gtk4::Widget,
    areas: Rc<RefCell<Vec<gtk4::glib::WeakRef<gtk4::Widget>>>>,
    engine: Rc<RefCell<VisualEngine>>,
    /// Mirrored outside the engine (which has no getter) so `set_spectrum`
    /// can gate on "are we actually playing" without borrowing it.
    playback: Rc<Cell<PlaybackState>>,
    panel_active: Rc<Cell<bool>>,
    tick_id: Rc<RefCell<Option<gtk4::TickCallbackId>>>,
}

impl SongVisualizer {
    pub(in crate::ui) fn new() -> Self {
        let engine = Rc::new(RefCell::new(VisualEngine::new()));
        let areas = Rc::new(RefCell::new(Vec::new()));
        let area = build_canvas(&engine);
        register_area(&areas, &area);

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        root.add_css_class("reprise-song-visuals");
        root.append(&area);
        root.append(&mode_controls(&engine, &areas));
        Self {
            root,
            area,
            areas,
            engine,
            playback: Rc::new(Cell::new(PlaybackState::Stopped)),
            panel_active: Rc::new(Cell::new(false)),
            tick_id: Rc::new(RefCell::new(None)),
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    /// Feeds the engine's secondary accent: the texture is rasterized down
    /// to a small RGBA sample and handed to `VisualEngine::set_cover_pixels`,
    /// or cleared on `None`.
    pub(in crate::ui) fn set_cover(&self, texture: Option<&gtk4::gdk::Texture>) {
        match texture {
            Some(texture) => match downscale_cover_rgba(texture, COVER_PALETTE_EDGE) {
                Some(rgba) => self
                    .engine
                    .borrow_mut()
                    .set_cover_pixels(&rgba, COVER_PALETTE_PIXELS),
                None => self.engine.borrow_mut().clear_cover(),
            },
            None => self.engine.borrow_mut().clear_cover(),
        }
        queue_registered_areas(&self.areas);
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

    fn is_active(&self) -> bool {
        self.panel_active.get()
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
        let slot = self.tick_id.clone();
        // Decouple the sim's advance from the render frame rate: each engine
        // tick is a fixed 1/60 s step, but the frame clock slows to the render
        // rate when the canvas can't keep up — so at, say, 20 fps we advance
        // ~3 steps per frame to keep the animation at real-time speed instead
        // of running in slow motion. Capped so a hitch never spirals into a
        // burst of catch-up work.
        let last_frame_us = Cell::new(0i64);
        let id = self.area.add_tick_callback(move |_, frame_clock| {
            if !panel_active.get() || !motion::animations_enabled() {
                *slot.borrow_mut() = None;
                return gtk4::glib::ControlFlow::Break;
            }
            let now = frame_clock.frame_time();
            let previous = last_frame_us.replace(now);
            let steps = if previous == 0 {
                1
            } else {
                (((now - previous) as f64 / 16_667.0).round() as i32).clamp(1, 4)
            };
            let mut settled = true;
            for _ in 0..steps {
                settled = engine.borrow_mut().tick();
            }
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

/// Builds the inline Cairo `DrawingArea` canvas, upcast to `gtk4::Widget` so
/// every other helper here (`register_area`, the tick loop's
/// `add_tick_callback`) works uniformly.
fn build_canvas(engine: &Rc<RefCell<VisualEngine>>) -> gtk4::Widget {
    drawing_area(engine).upcast()
}

fn drawing_area(engine: &Rc<RefCell<VisualEngine>>) -> gtk4::DrawingArea {
    let area = gtk4::DrawingArea::builder()
        .height_request(DRAW_HEIGHT)
        .hexpand(true)
        .accessible_role(gtk4::AccessibleRole::Img)
        .build();
    area.add_css_class("reprise-song-visual-canvas");
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
    areas: &Rc<RefCell<Vec<gtk4::glib::WeakRef<gtk4::Widget>>>>,
    area: &impl IsA<gtk4::Widget>,
) {
    let weak = gtk4::glib::WeakRef::new();
    weak.set(Some(area.upcast_ref::<gtk4::Widget>()));
    areas.borrow_mut().push(weak);
}

fn queue_registered_areas(areas: &Rc<RefCell<Vec<gtk4::glib::WeakRef<gtk4::Widget>>>>) {
    areas.borrow_mut().retain(|weak| {
        let Some(area) = weak.upgrade() else {
            return false;
        };
        area.queue_draw();
        true
    });
}

/// Reads `widget`'s resolved CSS `color` (the app accent, via
/// `@reprise_player_accent`) as an `(r, g, b)` triple the engine can use for
/// its accent-driven fills.
fn accent_rgb(widget: &impl IsA<gtk4::Widget>) -> (f32, f32, f32) {
    let color = widget.color();
    (color.red(), color.green(), color.blue())
}

/// The picker's user-facing label for one visual mode.
fn mode_label(mode: VisualMode) -> &'static str {
    match mode {
        VisualMode::Grid => strings::SONG_VISUALS_MODE_GRID,
        VisualMode::Bars => strings::SONG_VISUALS_MODE_BARS,
        VisualMode::Flow => strings::SONG_VISUALS_MODE_FLOW,
        VisualMode::Pulse => strings::SONG_VISUALS_MODE_PULSE,
        VisualMode::Particles => strings::SONG_VISUALS_MODE_PARTICLES,
        VisualMode::Neon => strings::SONG_VISUALS_MODE_NEON,
    }
}

/// Builds the grouped mode-toggle row: one [`gtk4::ToggleButton`] per
/// [`VisualMode`], wrapped in a [`gtk4::FlowBox`] so it reflows at narrow
/// widths instead of overflowing. Each call builds a fresh, independent row
/// that reads the engine's current mode at construction time.
fn mode_controls(
    engine: &Rc<RefCell<VisualEngine>>,
    areas: &Rc<RefCell<Vec<gtk4::glib::WeakRef<gtk4::Widget>>>>,
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

pub(in crate::ui) fn css() -> String {
    ".reprise-song-visuals { margin: 0 18px 12px; }\n\
     .reprise-song-visual-canvas {\
       color: @reprise_player_accent;\
       background-color: alpha(#ffffff, 0.025);\
       border: 1px solid alpha(@reprise_player_accent, 0.14);\
       border-radius: 24px;\
     }\n\
     .reprise-song-visual-modes { margin-top: 2px; }"
        .to_owned()
}

#[cfg(test)]
#[path = "song_visualizer_tests.rs"]
mod tests;
