//! Audio-reactive Bars visual for the Now Playing Visual page.
//!
//! All reactive state (eased spectrum bands, impact overlay and accent
//! palette) and the Bars geometry live in
//! `reprise_core::visuals::VisualEngine` — a portable core the GUI never has
//! to reimplement. This module owns the inline canvas embedded in the Now
//! Playing panel. It turns the engine's
//! [`reprise_core::visuals::Scene`] into pixels via [`render`], through a
//! Cairo `DrawingArea` driven by the tick loop and `queue_registered_areas`.

mod render;
mod song_visualizer_util;

use song_visualizer_util::downscale_cover_rgba;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::playback::{BassPressure, PlaybackState, SpectrumFrame};
use reprise_core::visuals::VisualEngine;

use crate::ui::{motion, strings};

const DRAW_HEIGHT: i32 = 220;
/// Edge length (px) the cover texture is rasterized down to before feeding
/// the engine's secondary-accent palette extraction — cheap and plenty for a
/// hue/saturation sample.
const COVER_PALETTE_EDGE: i32 = 32;
const COVER_PALETTE_PIXELS: usize = (COVER_PALETTE_EDGE * COVER_PALETTE_EDGE) as usize;
/// Redraw interval (µs) while no audio is playing — the resting breath runs at
/// ~30 Hz instead of the full render rate.
const IDLE_FRAME_INTERVAL_US: i64 = 33_000;
/// Refresh interval (µs) of the analysis readout. The measurement updates at
/// the frame rate, but numbers changing 60×/s are unreadable — and re-laying
/// out four labels that often is pure waste.
const READOUT_INTERVAL_US: i64 = 100_000;
/// Below this the bass band carries nothing worth printing as a number.
const READOUT_SILENCE_DBFS: f32 = -90.0;

#[derive(Clone)]
pub(in crate::ui) struct SongVisualizer {
    root: gtk4::Box,
    /// The inline Cairo `DrawingArea`, upcast to `gtk4::Widget`. Used only as
    /// the tick loop's `add_tick_callback` host; drawing itself goes through
    /// `areas`.
    area: gtk4::Widget,
    areas: Rc<RefCell<Vec<gtk4::glib::WeakRef<gtk4::Widget>>>>,
    engine: Rc<RefCell<VisualEngine>>,
    /// Shows what the glow layer is currently reacting to (AC-23).
    readout: AnalysisReadout,
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

        let readout = AnalysisReadout::new();
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        root.add_css_class("reprise-song-visuals");
        root.append(&area);
        root.append(&readout.root);
        Self {
            root,
            area,
            areas,
            engine,
            readout,
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

    /// A new track started: resets the Bars envelope and impact overlay so
    /// motion from the previous track does not bleed into the new one.
    pub(in crate::ui) fn note_track_changed(&self) {
        self.engine.borrow_mut().note_track_changed();
        self.readout.set(self.engine.borrow().bass_pressure());
        queue_registered_areas(&self.areas);
    }

    /// Whether the player holds a track at all. A loaded but resting track
    /// breathes (AC-11); an empty player keeps the canvas empty.
    pub(in crate::ui) fn set_has_track(&self, has_track: bool) {
        self.engine.borrow_mut().set_has_track(has_track);
        if !motion::animations_enabled() {
            self.engine.borrow_mut().snap_to_static();
        }
        self.ensure_tick();
        queue_registered_areas(&self.areas);
    }

    pub(in crate::ui) fn set_spectrum(&self, frame: SpectrumFrame) {
        if !motion::animations_enabled() || self.playback.get() != PlaybackState::Playing {
            return;
        }
        self.engine.borrow_mut().ingest(&frame);
        self.readout.update(frame.bass_pressure());
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
            self.readout.set(self.engine.borrow().bass_pressure());
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

    fn ensure_tick(&self) {
        if !self.is_active() || !motion::animations_enabled() || self.tick_id.borrow().is_some() {
            return;
        }
        let engine = self.engine.clone();
        let areas = self.areas.clone();
        let panel_active = self.panel_active.clone();
        let playback = self.playback.clone();
        let readout = self.readout.clone();
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
            let previous = last_frame_us.get();
            // The idle breath is slow by design — redrawing it at the full
            // render rate would burn frames for motion nobody can see. The
            // fixed engine step below keeps it real-time regardless.
            if playback.get() != PlaybackState::Playing
                && previous != 0
                && now - previous < IDLE_FRAME_INTERVAL_US
            {
                return gtk4::glib::ControlFlow::Continue;
            }
            last_frame_us.set(now);
            let steps = if previous == 0 {
                1
            } else {
                (((now - previous) as f64 / 16_667.0).round() as i32).clamp(1, 4)
            };
            let mut settled = true;
            for _ in 0..steps {
                settled = engine.borrow_mut().tick();
            }
            // Also refreshed here, so the readout follows the release once
            // playback stops and no further frames arrive.
            readout.update(engine.borrow().bass_pressure());
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

/// The four analysis numbers, in the order the readout shows them.
fn analysis_values(pressure: BassPressure) -> [String; 4] {
    let decibels = |value: f32| {
        if value <= READOUT_SILENCE_DBFS {
            "—".to_owned()
        } else {
            // Whole decibels only: at ten refreshes a second a tenth of a dB
            // is noise to the eye, and the extra glyphs cost width the 300 px
            // panel does not have.
            format!("{value:.0} dBFS")
        }
    };
    [
        decibels(pressure.level_dbfs),
        decibels(pressure.baseline_dbfs),
        format!("{:.2}", pressure.impact),
        format!("{:.2}", pressure.aura),
    ]
}

/// The live analysis under the canvas: the absolute bass level, the running
/// baseline it is measured against, and the two glow values derived from them.
/// Showing them keeps the visual's behavior traceable instead of magic.
#[derive(Clone)]
struct AnalysisReadout {
    root: gtk4::Grid,
    values: Vec<gtk4::Label>,
    last_update_us: Rc<Cell<i64>>,
}

impl AnalysisReadout {
    fn new() -> Self {
        // Two by two, not one row of four: the panel is a fixed 300 px wide
        // (NPP-1), where four columns leave ~70 px each and truncate every
        // caption. Caption and value sit side by side rather than stacked,
        // because the strip the canvas leaves free is only about one line
        // tall — stacked, the second row was silently clipped. Every label
        // ellipsizes so enlarged text shortens the words instead of widening
        // the panel.
        let root = gtk4::Grid::builder()
            .column_homogeneous(true)
            .row_spacing(2)
            .column_spacing(14)
            .accessible_role(gtk4::AccessibleRole::Group)
            .build();
        root.add_css_class("reprise-song-visual-analysis");
        root.update_property(&[gtk4::accessible::Property::Label(&strings::text(
            strings::SONG_VISUALS_ANALYSIS_ACCESSIBLE,
        ))]);

        let values = [
            strings::SONG_VISUALS_BASS,
            strings::SONG_VISUALS_BASELINE,
            strings::SONG_VISUALS_IMPACT,
            strings::SONG_VISUALS_BREAKDOWN,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, name)| {
            let cell = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            let caption = gtk4::Label::builder()
                .label(strings::text(name))
                .xalign(0.0)
                .ellipsize(gtk4::pango::EllipsizeMode::End)
                .build();
            caption.add_css_class("reprise-song-visual-analysis-name");
            let value = gtk4::Label::builder()
                .label("—")
                .xalign(1.0)
                .hexpand(true)
                .ellipsize(gtk4::pango::EllipsizeMode::End)
                .build();
            value.add_css_class("reprise-song-visual-analysis-value");
            cell.append(&caption);
            cell.append(&value);
            root.attach(&cell, (index % 2) as i32, (index / 2) as i32, 1, 1);
            value
        })
        .collect();

        Self {
            root,
            values,
            last_update_us: Rc::new(Cell::new(0)),
        }
    }

    /// Writes the numbers immediately — for state changes, where the readout
    /// must not lag behind a stop or a track switch.
    fn set(&self, pressure: BassPressure) {
        self.last_update_us.set(gtk4::glib::monotonic_time());
        for (label, value) in self.values.iter().zip(analysis_values(pressure)) {
            label.set_text(&value);
        }
    }

    /// Writes the numbers at most every [`READOUT_INTERVAL_US`] — for the live
    /// path, which fires at the frame rate.
    fn update(&self, pressure: BassPressure) {
        let now = gtk4::glib::monotonic_time();
        let last = self.last_update_us.get();
        if last != 0 && now.saturating_sub(last) < READOUT_INTERVAL_US {
            return;
        }
        self.set(pressure);
    }

    #[cfg(test)]
    fn shown_values(&self) -> Vec<String> {
        self.values
            .iter()
            .map(|label| label.text().to_string())
            .collect()
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
    let renderer = Rc::new(RefCell::new(render::SceneRenderer::default()));
    area.set_draw_func(move |area, cr, width, height| {
        let accent = accent_rgb(area);
        engine.borrow_mut().set_accent(accent);
        let scene_size = render::capped_scene_size(width, height);
        let scene = engine
            .borrow()
            .scene(scene_size.0 as f32, scene_size.1 as f32);
        renderer.borrow_mut().draw(cr, &scene, width, height);
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

pub(in crate::ui) fn css() -> String {
    ".reprise-song-visuals { margin: 0 18px 12px; }\n\
     .reprise-song-visual-canvas {\
       color: @reprise_player_accent;\
       background-color: alpha(#ffffff, 0.025);\
       border: 1px solid alpha(@reprise_player_accent, 0.14);\
       border-radius: 24px;\
     }\n\
     .reprise-song-visual-analysis { padding: 0 2px; }\n\
     .reprise-song-visual-analysis-name {\
       font-size: 0.78rem;\
       opacity: 0.5;\
     }\n\
     .reprise-song-visual-analysis-value {\
       font-size: 0.86rem;\
       font-feature-settings: \"tnum\" 1;\
       color: @reprise_player_accent;\
     }"
    .to_owned()
}

#[cfg(test)]
#[path = "song_visualizer_tests.rs"]
mod tests;
