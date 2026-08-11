//! Allocation-light Android boundary for the shared song visualizer.
//!
//! A scene is flattened as one record per shape:
//! `[kind, r, g, b, a, width, glow, point_count, geometry...]`.
//! `kind` is `0` for a rectangle (`x, y, w, h`), `1` for a polyline
//! (`x1, y1, ...`), and `2` for a radial glow (`cx, cy, radius`). The rectangle
//! and radial-glow `point_count` fields are respectively `4` and `3`, matching
//! the number of geometry scalars rather than a literal number of points.
//!
//! The shared scene format also carries closed-path and dash metadata. Bars do
//! not use either, so this boundary deliberately omits them rather than adding
//! fields the phone would copy on every rendered frame.

use std::sync::{Mutex, MutexGuard, PoisonError};

use reprise_core::visuals::{spectrum_frame_from_bands, Fill, Geom, Scene, VisualEngine};

const RECT_KIND: f32 = 0.0;
const POLYLINE_KIND: f32 = 1.0;
const RADIAL_GLOW_KIND: f32 = 2.0;
const RECORD_PREFIX_LEN: usize = 8;

struct VisualState {
    engine: VisualEngine,
    has_ingested: bool,
    has_analysis: bool,
    playing: bool,
}

/// One phone-local owner of the portable visual engine.
#[derive(uniffi::Object)]
pub struct AndroidVisualEngine {
    state: Mutex<VisualState>,
}

#[uniffi::export]
impl AndroidVisualEngine {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(VisualState {
                engine: VisualEngine::new(),
                has_ingested: false,
                has_analysis: false,
                playing: false,
            }),
        }
    }

    pub fn set_accent(&self, red: f32, green: f32, blue: f32) {
        self.lock()
            .engine
            .set_accent((finite_unit(red), finite_unit(green), finite_unit(blue)));
    }

    pub fn set_playing(&self, playing: bool) {
        let mut state = self.lock();
        state.playing = playing;
        let has_analysis = state.has_analysis;
        state.engine.set_playing(playing && has_analysis);
    }

    /// Starts a clean visual history for the next track.
    pub fn note_track_changed(&self) {
        let mut state = self.lock();
        state.engine.note_track_changed();
        state.engine.set_has_track(false);
        state.has_ingested = false;
        state.has_analysis = false;
    }

    /// Installs one already-smoothed spectrogram frame.
    #[allow(clippy::needless_pass_by_value)] // UniFFI cannot export borrowed slices.
    pub fn ingest_bands(&self, bands: Vec<f32>) {
        let has_analysis = !bands.is_empty();
        let frame = spectrum_frame_from_bands(&bands);
        let mut state = self.lock();
        state.engine.set_has_track(true);
        let playing = state.playing;
        state.engine.set_playing(playing && has_analysis);
        state.engine.ingest(&frame);
        state.has_ingested = true;
        state.has_analysis = has_analysis;
    }

    /// Advances the portable presentation state; `true` means it is settled.
    pub fn tick(&self) -> bool {
        self.lock().engine.tick()
    }

    /// Returns the scene in the flat format documented by this module.
    pub fn scene(&self, width: f32, height: f32) -> Vec<f32> {
        let state = self.lock();
        if !state.has_ingested
            || !width.is_finite()
            || !height.is_finite()
            || width <= 0.0
            || height <= 0.0
        {
            return Vec::new();
        }
        encode_scene(&state.engine.scene(width, height))
    }
}

impl Default for AndroidVisualEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AndroidVisualEngine {
    fn lock(&self) -> MutexGuard<'_, VisualState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn finite_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub(crate) fn encode_scene(scene: &Scene) -> Vec<f32> {
    let geometry_len = scene
        .shapes
        .iter()
        .map(|shape| match &shape.geom {
            Geom::Rect { .. } => 4,
            Geom::Polyline { points, .. } => points.len() * 2,
            Geom::RadialGlow { .. } => 3,
        })
        .sum::<usize>();
    let mut buffer = Vec::with_capacity(scene.shapes.len() * RECORD_PREFIX_LEN + geometry_len);

    for shape in &scene.shapes {
        let Fill::Solid(color) = &shape.fill;
        let (kind, point_count) = match &shape.geom {
            Geom::Rect { .. } => (RECT_KIND, 4),
            Geom::Polyline { points, .. } => (POLYLINE_KIND, points.len()),
            Geom::RadialGlow { .. } => (RADIAL_GLOW_KIND, 3),
        };
        buffer.extend_from_slice(&[
            kind,
            color.r,
            color.g,
            color.b,
            color.a,
            shape.width,
            shape.glow,
            point_count as f32,
        ]);
        match &shape.geom {
            Geom::Rect { x, y, w, h } => buffer.extend_from_slice(&[*x, *y, *w, *h]),
            Geom::Polyline { points, .. } => {
                for (x, y) in points {
                    buffer.extend_from_slice(&[*x, *y]);
                }
            }
            Geom::RadialGlow { cx, cy, r } => buffer.extend_from_slice(&[*cx, *cy, *r]),
        }
    }

    buffer
}
