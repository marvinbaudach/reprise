//! Audio-reactive Bars visual for the Now Playing Visual page.
//!
//! All reactive state (eased spectrum bands, impact overlay and effective
//! accent) and the Bars geometry live in
//! `reprise_core::visuals::VisualEngine` — a portable core the GUI never has
//! to reimplement. This module owns the inline canvas embedded in the Now
//! Playing panel. It turns the engine's
//! [`reprise_core::visuals::Scene`] into pixels via [`render`], through a
//! Cairo `DrawingArea` driven by the tick loop and `queue_registered_areas`.

mod render;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use reprise_core::playback::{BassPressure, PlaybackState, SpectrumFrame};
use reprise_core::visuals::VisualEngine;

use crate::ui::{motion, strings};

/// Canvas height. Trimmed by 12 px when the readout grew from four values to
/// six: the strip below is what is left of the panel, and a readout taller
/// than it is silently clipped — the failure this file's `…fits_in_the_strip…`
/// test exists to catch.
const DRAW_HEIGHT: i32 = 208;
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
    /// Monotonic time of the last audio frame handed to the portable engine.
    /// Peak caps decay from this elapsed time even while the panel is hidden.
    last_ingest_us: Rc<Cell<i64>>,
    panel_active: Rc<Cell<bool>>,
    /// Mirrored from the panel, which owns the envelope — the readout names
    /// every value the reactive light runs on, and `swell` is one of them.
    swell: Rc<Cell<f64>>,
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
            last_ingest_us: Rc::new(Cell::new(0)),
            panel_active: Rc::new(Cell::new(false)),
            swell: Rc::new(Cell::new(0.0)),
            tick_id: Rc::new(RefCell::new(None)),
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    /// A new track started: resets the Bars envelope and impact overlay so
    /// motion from the previous track does not bleed into the new one.
    pub(in crate::ui) fn note_track_changed(&self) {
        self.last_ingest_us.set(0);
        self.engine.borrow_mut().note_track_changed();
        self.readout
            .set(self.engine.borrow().bass_pressure(), self.swell.get());
        queue_registered_areas(&self.areas);
    }

    /// Whether the player holds a track at all. A loaded but resting track
    /// breathes (AC-27); an empty player keeps the canvas empty.
    pub(in crate::ui) fn set_has_track(&self, has_track: bool) {
        self.engine.borrow_mut().set_has_track(has_track);
        if !motion::animations_enabled() {
            self.engine.borrow_mut().snap_to_static();
        }
        self.ensure_tick();
        queue_registered_areas(&self.areas);
    }

    #[cfg(test)]
    pub(super) fn reports_track_for_test(&self) -> bool {
        self.engine.borrow_mut().tick();
        self.engine
            .borrow()
            .scene(548.0, 300.0)
            .shapes
            .iter()
            .any(|shape| matches!(shape.geom, reprise_core::visuals::Geom::Rect { .. }))
    }

    pub(in crate::ui) fn set_spectrum(&self, frame: SpectrumFrame) {
        if !motion::animations_enabled() || self.playback.get() != PlaybackState::Playing {
            return;
        }
        let now = gtk4::glib::monotonic_time();
        let previous = self.last_ingest_us.replace(now);
        let elapsed = if previous == 0 {
            Duration::from_micros(16_667)
        } else {
            Duration::from_micros(u64::try_from(now.saturating_sub(previous)).unwrap_or_default())
        };
        self.engine.borrow_mut().ingest((&frame, elapsed));
        self.readout.update(frame.bass_pressure(), self.swell.get());
        self.ensure_tick();
    }

    pub(in crate::ui) fn set_playback_state(&self, playback: PlaybackState) {
        self.playback.set(playback);
        if playback != PlaybackState::Playing {
            self.last_ingest_us.set(0);
        }
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
            self.readout
                .set(self.engine.borrow().bass_pressure(), self.swell.get());
            queue_registered_areas(&self.areas);
        }
    }

    /// The panel owns the envelope; the readout only reports it.
    pub(in crate::ui) fn set_swell(&self, swell: f64) {
        self.swell.set(swell);
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
        let swell = self.swell.clone();
        let slot = self.tick_id.clone();
        // Decouple the simulation from the render frame rate. The portable
        // engine consumes GTK's monotonic elapsed time directly, so a reduced
        // cadence or a missed frame changes smoothness, never wave speed.
        let last_frame_us = Cell::new(0i64);
        let id = self.area.add_tick_callback(move |_, frame_clock| {
            if !panel_active.get() || !motion::animations_enabled() {
                *slot.borrow_mut() = None;
                return gtk4::glib::ControlFlow::Break;
            }
            let now = frame_clock.frame_time();
            let previous = last_frame_us.get();
            // The idle breath is slow by design — redrawing it at the full
            // render rate would burn frames for motion nobody can see.
            if playback.get() != PlaybackState::Playing
                && previous != 0
                && now - previous < IDLE_FRAME_INTERVAL_US
            {
                return gtk4::glib::ControlFlow::Continue;
            }
            last_frame_us.set(now);
            let elapsed = if previous == 0 {
                Duration::from_micros(16_667)
            } else {
                // A frame-clock regression is treated as zero elapsed time;
                // the signed delta cannot be represented as a Duration.
                Duration::from_micros(
                    u64::try_from(now.saturating_sub(previous)).unwrap_or_default(),
                )
            };
            let settled = engine.borrow_mut().advance_by(elapsed);
            // Also refreshed here, so the readout follows the release once
            // playback stops and no further frames arrive.
            readout.update(engine.borrow().bass_pressure(), swell.get());
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

/// The six analysis numbers, in the order the readout shows them.
fn analysis_values(pressure: BassPressure, swell: f64) -> [String; 6] {
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
        // `impact` is deliberately absent: since the glow became a stage light
        // driven by `kick`, nothing reads it any more, and AC-23 asks this
        // strip to name the analysis the visual *reacts to*. It stays a
        // produced reading, just not a displayed one.
        format!("{:.2}", pressure.aura),
        format!("{:.2}", pressure.kick),
        format!("{:.2}", pressure.pressure),
        format!("{swell:.2}"),
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
        // Two by three, not one row of six: the panel is a fixed 300 px wide
        // (NPP-1), where six columns leave ~45 px each and truncate every
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
            strings::SONG_VISUALS_BREAKDOWN,
            strings::SONG_VISUALS_KICK,
            strings::SONG_VISUALS_PRESSURE,
            strings::SONG_VISUALS_SWELL,
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
    fn set(&self, pressure: BassPressure, swell: f64) {
        self.last_update_us.set(gtk4::glib::monotonic_time());
        for (label, value) in self.values.iter().zip(analysis_values(pressure, swell)) {
            label.set_text(&value);
        }
    }

    /// Writes the numbers at most every [`READOUT_INTERVAL_US`] — for the live
    /// path, which fires at the frame rate.
    fn update(&self, pressure: BassPressure, swell: f64) {
        let now = gtk4::glib::monotonic_time();
        let last = self.last_update_us.get();
        if last != 0 && now.saturating_sub(last) < READOUT_INTERVAL_US {
            return;
        }
        self.set(pressure, swell);
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
    area.set_draw_func(move |_, cr, width, height| {
        let accent = accent_rgb();
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

/// Reads the effective accent through the central Rust-side source.
fn accent_rgb() -> (f32, f32, f32) {
    let color = crate::ui::style::accent::accent_rgba();
    (color.red(), color.green(), color.blue())
}

pub(in crate::ui) fn css() -> String {
    ".reprise-song-visuals { margin: 0 18px 12px; }\n\
     .reprise-song-visual-canvas {\
       color: @reprise_player_accent;\
       background-color: alpha(@sidebar_fg_color, 0.025);\
       border: 1px solid alpha(@reprise_player_accent, 0.14);\
       border-radius: 24px;\
     }\n\
     .reprise-song-visual-analysis { padding: 0 2px; }\n\
     .reprise-song-visual-analysis-name {\
       font-size: 0.78rem;\
       color: @reprise_secondary_fg_color;\
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
