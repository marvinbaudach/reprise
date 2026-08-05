//! The seek bar's drawing state and the two derivations that feed it: the
//! width-dependent resample, and the time-averaged colour curve.
//!
//! Split out of `waveform_seek.rs` to keep that file under the project's
//! 800-line cap. The widget owns this struct behind one `RefCell`; nothing
//! outside `player_bar` sees it.

use super::*;

/// Ensure `state.display_peaks` is up to date for the given `width`.
/// Re-aggregates from the cached `raw_peaks` (never re-decodes) when the
/// width changed or the cache is empty.
pub(in crate::ui::player_bar::waveform_seek) fn ensure_resampled(state: &mut State, width: i32) {
    if state.last_display_width != 0
        && state.last_display_width != width
        && state.crossfade_progress < 1.0
    {
        state.previous_bars.clear();
        state.previous_centroid.clear();
        state.crossfade_progress = 1.0;
        state.crossfade_start_us = 0;
    }
    if state.raw_peaks.is_empty() {
        state.display_peaks.clear();
        state.shaped_centroid.clear();
        return;
    }
    if state.last_display_width != width || state.display_peaks.is_empty() {
        let count = resolve_bar_count(state.bar_count_override, width);
        state.display_peaks = shape_display_peaks(&state.raw_peaks, count);
        state.shaped_centroid = shape_centroid(&state.colour_curve, count);
        state.last_display_width = width;
    }
}

/// Rebuilds everything derived from the raw colour curve: the time-averaged
/// curve the bar is actually painted from, and the section marks the
/// single-colour bar draws instead of it.
///
/// The averaging window is defined in seconds, so it needs the track duration
/// and nothing else — in particular not the widget width. Both inputs arrive
/// independently (`set_analysis` and `set_duration`, in either order), so this
/// runs whenever either of them changes and never per frame: the result is
/// constant for the whole track.
pub(in crate::ui::player_bar::waveform_seek) fn rebuild_colour_curve(state: &mut State) {
    let duration_s = state.duration_ms as f64 / 1_000.0;
    let smoothed = smooth_centroid_over_seconds(&state.raw_centroid, duration_s, CENTROID_WINDOW_S);
    state.section_marks = match state.colouring {
        SeekColouring::Solid => section_boundaries(&smoothed, duration_s),
        SeekColouring::Frequency => Vec::new(),
    };
    state.colour_curve = match state.colouring {
        // The single-colour bar draws its played side in the accent, which is
        // exactly what an absent curve already means everywhere below.
        SeekColouring::Solid => Vec::new(),
        SeekColouring::Frequency => smoothed,
    };
    // `previous_bars`/`previous_centroid` describe the track being faded out
    // and are deliberately left alone: they belong to the curve that was
    // already shaped, not to this one.
    state.shaped_centroid.clear();
    state.display_peaks.clear();
    waveform_surface::invalidate(state);
}

pub(in crate::ui::player_bar::waveform_seek) struct State {
    pub(in crate::ui::player_bar::waveform_seek) raw_peaks: Vec<u8>, // stored peaks from DB (1000 values, 0-255)
    pub(in crate::ui::player_bar::waveform_seek) raw_centroid: Vec<u8>, // matching spectral positions as stored, or empty
    /// The curve everything paints from: `raw_centroid` averaged over a window
    /// of seconds, or empty in the single-colour mode. Derived, cached per
    /// track — never recomputed while drawing.
    pub(in crate::ui::player_bar::waveform_seek) colour_curve: Vec<u8>,
    /// Positions, 0..1, of detected section boundaries. Only the single-colour
    /// bar draws them; the spectral fill shows the same structure as colour.
    pub(in crate::ui::player_bar::waveform_seek) section_marks: Vec<f64>,
    pub(in crate::ui::player_bar::waveform_seek) colouring: SeekColouring,
    pub(in crate::ui::player_bar::waveform_seek) display_peaks: Vec<DisplayBar>, // shaped to current bar count
    pub(in crate::ui::player_bar::waveform_seek) shaped_centroid: Vec<f32>, // spectral positions shaped to display bars
    pub(in crate::ui::player_bar::waveform_seek) last_display_width: i32, // width used for last resample
    pub(in crate::ui::player_bar::waveform_seek) fraction: f64,
    /// End of the contiguous remote-media buffer as a 0..1 fraction. `None`
    /// means local/live playback or an unavailable buffering query.
    pub(in crate::ui::player_bar::waveform_seek) buffered_fraction: Option<f64>,
    /// Pointer position as a 0..1 fraction while hovering — drives the
    /// seek-preview tint on unplayed bars up to the cursor.
    pub(in crate::ui::player_bar::waveform_seek) hover_fraction: Option<f64>,
    pub(in crate::ui::player_bar::waveform_seek) drag_fraction: Option<f64>,
    // Smooth interpolation.
    pub(in crate::ui::player_bar::waveform_seek) target_fraction: f64,
    pub(in crate::ui::player_bar::waveform_seek) fraction_velocity: f64, // fraction-per-microsecond
    pub(in crate::ui::player_bar::waveform_seek) last_tick_us: i64,
    // Build-up animation.
    pub(in crate::ui::player_bar::waveform_seek) build_progress: f64, // 0.0 = not started, 1.0 = complete
    pub(in crate::ui::player_bar::waveform_seek) build_start_us: i64, // 0 means not running
    // Track-change alpha crossfade.
    pub(in crate::ui::player_bar::waveform_seek) previous_bars: Vec<DisplayBar>,
    pub(in crate::ui::player_bar::waveform_seek) previous_centroid: Vec<f32>,
    pub(in crate::ui::player_bar::waveform_seek) crossfade_progress: f64, // 1.0 means no crossfade is running
    pub(in crate::ui::player_bar::waveform_seek) crossfade_start_us: i64,
    pub(in crate::ui::player_bar::waveform_seek) head_colour_target: Option<(f64, f64, f64)>,
    pub(in crate::ui::player_bar::waveform_seek) head_colour: Option<(f64, f64, f64)>,
    pub(in crate::ui::player_bar::waveform_seek) mask_surface: Option<gtk4::cairo::ImageSurface>,
    pub(in crate::ui::player_bar::waveform_seek) colour_surface: Option<gtk4::cairo::ImageSurface>,
    pub(in crate::ui::player_bar::waveform_seek) surface_key: Option<waveform_surface::SurfaceKey>,
    pub(in crate::ui::player_bar::waveform_seek) last_drawn_head_x: Option<f64>,
    pub(in crate::ui::player_bar::waveform_seek) last_drawn_colour: Option<(f64, f64, f64)>,
    pub(in crate::ui::player_bar::waveform_seek) last_drawn_hover_fraction: Option<f64>,
    pub(in crate::ui::player_bar::waveform_seek) last_drawn_drag_fraction: Option<f64>,
    // Pause desaturation animation.
    pub(in crate::ui::player_bar::waveform_seek) desaturation_progress: f64, // 0.0 = full chroma, 1.0 = paused chroma
    #[allow(dead_code)] // Consumed by the PlayerBar/Compact wiring in MOT-5 Phase B.
    pub(in crate::ui::player_bar::waveform_seek) desaturation_target: f64,
    pub(in crate::ui::player_bar::waveform_seek) min_bar_height: f64,
    pub(in crate::ui::player_bar::waveform_seek) max_bar_height: f64,
    /// Fixed bar count for the mini player (frame 1e); `None` = width-derived.
    pub(in crate::ui::player_bar::waveform_seek) bar_count_override: Option<usize>,
    /// Fill-width equal bars (mini) vs fixed-width bars (full waveform).
    pub(in crate::ui::player_bar::waveform_seek) fill_bars: bool,
    // Duration of the current track (ms), for formatted tooltip display.
    pub(in crate::ui::player_bar::waveform_seek) duration_ms: i64,
}

/// A resting state for tests to vary one field of.
///
/// Spelling every field out at each call site made the struct hard to extend:
/// adding one meant editing every test that only cared about two of them.
#[cfg(test)]
impl Default for State {
    fn default() -> Self {
        Self {
            raw_peaks: Vec::new(),
            raw_centroid: Vec::new(),
            colour_curve: Vec::new(),
            section_marks: Vec::new(),
            colouring: SeekColouring::DEFAULT,
            display_peaks: Vec::new(),
            shaped_centroid: Vec::new(),
            last_display_width: 0,
            fraction: 0.0,
            buffered_fraction: None,
            hover_fraction: None,
            drag_fraction: None,
            target_fraction: 0.0,
            fraction_velocity: 0.0,
            last_tick_us: 0,
            build_progress: 1.0,
            build_start_us: 0,
            previous_bars: Vec::new(),
            previous_centroid: Vec::new(),
            crossfade_progress: 1.0,
            crossfade_start_us: 0,
            head_colour_target: None,
            head_colour: None,
            mask_surface: None,
            colour_surface: None,
            surface_key: None,
            last_drawn_head_x: None,
            last_drawn_colour: None,
            last_drawn_hover_fraction: None,
            last_drawn_drag_fraction: None,
            desaturation_progress: 0.0,
            desaturation_target: 0.0,
            min_bar_height: MIN_BAR_HEIGHT,
            max_bar_height: MAX_BAR_HEIGHT,
            bar_count_override: None,
            fill_bars: false,
            duration_ms: 0,
        }
    }
}
