//! What the seek bar's colour says, and what it must never say.
//!
//! Split out of `waveform_seek_tests.rs` to keep that file under the project's
//! 800-line cap. These cover SEEK-1: the curve is averaged over a window of
//! seconds before anything draws it, and both sides of the playhead carry the
//! result.

use super::super::*;
use super::{accent_rgb, composited_luminance};

/// A curve that alternates every point — the beat-to-beat swing that made
/// neighbouring bars land a third of the axis apart.
fn jittering_curve(len: usize) -> Vec<u8> {
    (0..len)
        .map(|index| if index % 2 == 0 { 40 } else { 210 })
        .collect()
}

#[test]
fn seek_1_the_colour_curve_is_averaged_before_anything_draws_it() {
    // The bar paints from `colour_curve`, never from `raw_centroid`. Without
    // the averaging step in between, neighbouring bars are cyan and magenta
    // inside two seconds of music.
    let mut state = State {
        raw_centroid: jittering_curve(600),
        duration_ms: 300_000,
        ..State::default()
    };
    rebuild_colour_curve(&mut state);

    let swing = |curve: &[u8]| {
        curve
            .windows(2)
            .map(|pair| u32::from(pair[0].abs_diff(pair[1])))
            .max()
            .unwrap_or(0)
    };
    assert_eq!(
        swing(&state.raw_centroid),
        170,
        "the raw curve still swings"
    );
    assert!(
        swing(&state.colour_curve) * 10 < swing(&state.raw_centroid),
        "the drawn curve is not averaged: {}",
        swing(&state.colour_curve)
    );
    assert_eq!(state.colour_curve.len(), state.raw_centroid.len());
}

#[test]
fn seek_1_the_window_is_seconds_wide_so_a_resize_cannot_change_it() {
    // The failure this pins: a window defined in display bars smooths a narrow
    // window differently from a wide one, so dragging the window edge would
    // change how the track reads. The curve is built before any width exists,
    // so resampling it at two widths must agree.
    let mut state = State {
        raw_peaks: vec![128u8; 600],
        raw_centroid: jittering_curve(600),
        duration_ms: 300_000,
        ..State::default()
    };
    rebuild_colour_curve(&mut state);
    let curve = state.colour_curve.clone();

    ensure_resampled(&mut state, 400);
    let narrow = state.shaped_centroid.clone();
    ensure_resampled(&mut state, 1600);
    let wide = state.shaped_centroid.clone();

    assert_eq!(state.colour_curve, curve, "the source curve was re-derived");
    assert_ne!(narrow.len(), wide.len(), "sanity: two different bar counts");
    // Both rasters sample one curve, so their averages agree.
    let mean = |bars: &[f32]| f64::from(bars.iter().sum::<f32>()) / bars.len() as f64;
    assert!((mean(&narrow) - mean(&wide)).abs() < 0.01);
}

#[test]
fn seek_1_the_curve_is_rebuilt_when_the_duration_arrives_and_not_per_frame() {
    // Peaks, curve and duration arrive independently. Until the duration is
    // known there is no timescale, so the raw curve is handed through; the
    // moment it lands the averaged one takes over.
    let mut state = State {
        raw_centroid: jittering_curve(600),
        duration_ms: 0,
        ..State::default()
    };
    rebuild_colour_curve(&mut state);
    assert_eq!(
        state.colour_curve, state.raw_centroid,
        "with no timescale the curve passes through untouched"
    );

    state.duration_ms = 300_000;
    rebuild_colour_curve(&mut state);
    assert_ne!(state.colour_curve, state.raw_centroid);
}

#[test]
fn seek_1_the_single_colour_bar_drops_the_curve_and_marks_the_sections() {
    let mut curve = vec![40u8; 600];
    for value in curve[300..].iter_mut() {
        *value = 220;
    }
    let mut state = State {
        raw_centroid: curve,
        duration_ms: 300_000,
        colouring: SeekColouring::Solid,
        ..State::default()
    };
    rebuild_colour_curve(&mut state);

    assert!(
        state.colour_curve.is_empty(),
        "an absent curve is what draws the accent everywhere below"
    );
    assert_eq!(state.section_marks.len(), 1, "{:?}", state.section_marks);
    assert!((state.section_marks[0] - 0.5).abs() < 0.02);

    // Switching back restores the fill and drops the hairlines: the spectral
    // bar shows the same structure as colour, so marks would say it twice.
    state.colouring = SeekColouring::Frequency;
    rebuild_colour_curve(&mut state);
    assert!(!state.colour_curve.is_empty());
    assert!(state.section_marks.is_empty());
}

#[test]
fn seek_1_the_two_sides_carry_one_colour_and_differ_only_in_opacity() {
    use super::render::{bar_fill, BarSide};
    use reprise_view::spectral_colour::spectral_colour;

    // This is the change: the colour used to stop at the playhead, which put
    // it exactly where it was not needed — behind the listener. Both sides
    // carry it now, and progress is the step between 1.0 and 0.34.
    let spectral = spectral_colour(0.3);
    let accent = accent_rgb();
    let (played, played_alpha) =
        bar_fill(SeekColouring::Frequency, BarSide::Played, spectral, accent);
    let (coming, coming_alpha) =
        bar_fill(SeekColouring::Frequency, BarSide::Coming, spectral, accent);
    assert_eq!(played, spectral);
    assert_eq!(
        coming, spectral,
        "the coming side must not fall back to grey"
    );
    assert_eq!(played_alpha, 1.0);
    assert_eq!(coming_alpha, UNPLAYED_ALPHA);
    assert_eq!(UNPLAYED_ALPHA, 0.34);

    // The seek preview sits between the two, so it reads as "this much would
    // be played" rather than as a third state.
    let (_, preview_alpha) = bar_fill(
        SeekColouring::Frequency,
        BarSide::HoverPreview,
        spectral,
        accent,
    );
    assert!(coming_alpha < preview_alpha && preview_alpha < played_alpha);
}

#[test]
fn seek_1_buffered_media_reads_as_ahead_of_what_has_not_arrived() {
    use super::render::{bar_fill, BarSide};
    use reprise_view::spectral_colour::spectral_colour;

    // Buffered-but-unplayed remote media has one job: to say "this much is
    // here already". It only says that while it is visibly *more* than the
    // part that has not arrived and visibly *less* than the played part.
    //
    // The regression this pins is not hypothetical. The alpha arrived as 0.24,
    // chosen against a coming side of 0.12; when the coming side became 0.34
    // the buffered segment silently fell *behind* the thing it is ahead of,
    // and nothing failed. Only the ordering is pinned here, not the numbers —
    // the numbers may be retuned, the order may not.
    let spectral = spectral_colour(0.4);
    let accent = accent_rgb();
    for colouring in [SeekColouring::Frequency, SeekColouring::Solid] {
        let luminance = |side| {
            let (colour, alpha) = bar_fill(colouring, side, spectral, accent);
            composited_luminance(colour, alpha)
        };
        let coming = luminance(BarSide::Coming);
        let buffered = luminance(BarSide::Buffered);
        let played = luminance(BarSide::Played);
        assert!(
            coming < buffered && buffered < played,
            "{colouring:?}: coming {coming:.4}, buffered {buffered:.4}, played {played:.4}"
        );
    }
}

#[test]
fn the_single_colour_bar_keeps_its_grey_coming_side() {
    use super::render::{bar_fill, BarSide};
    use reprise_view::spectral_colour::spectral_colour;

    let spectral = spectral_colour(0.3);
    let accent = accent_rgb();
    assert_eq!(
        bar_fill(SeekColouring::Solid, BarSide::Played, spectral, accent),
        (accent, 1.0)
    );
    assert_eq!(
        bar_fill(SeekColouring::Solid, BarSide::Coming, spectral, accent),
        (SOLID_UNPLAYED, 1.0)
    );
    // The preview is a step *lighter* than the grey, never a dimmed copy of
    // it: dimming reads as further away, and the preview is nearer.
    let (preview, _) = bar_fill(
        SeekColouring::Solid,
        BarSide::HoverPreview,
        spectral,
        accent,
    );
    assert!(preview.0 > SOLID_UNPLAYED.0 && preview.1 > SOLID_UNPLAYED.1);
}

#[test]
fn ac_24_the_progress_boundary_is_legible_at_both_ends_of_the_axis() {
    use reprise_view::spectral_colour::spectral_colour;

    // Measured requirement, not a matter of taste: the played part must differ
    // from the coming part by at least 3:1 in luminance, so the boundary reads
    // without relying on hue — which is what a red/green-blind user, or a
    // glance, actually has.
    //
    // Both sides now carry the *same* colour, so the ratio has to hold at every
    // point of the axis, and the two ends are where it is tightest: the deep
    // end is dark to begin with, the bright end is close to saturating.
    //
    // This is also what retired the bass term on the played side. A floor of
    // 0.74 there would put this ratio at 2.1:1 at the deep end — the step the
    // boundary now rests on would be eaten by the music.
    for step in 0..=10 {
        let position = f64::from(step) / 10.0;
        let colour = spectral_colour(position);
        let played = composited_luminance(colour, 1.0);
        let coming = composited_luminance(colour, UNPLAYED_ALPHA);
        let ratio = (played.max(coming) + 0.05) / (played.min(coming) + 0.05);
        assert!(
            ratio >= 3.0,
            "at {position:.1} the played/coming luminance ratio is only {ratio:.2}:1"
        );
    }
}

#[test]
fn a_bass_intro_is_still_visible_on_the_coming_side() {
    use reprise_view::spectral_colour::spectral_colour;

    // The other end of the same measurement: at 0.34 the deep, dark stretches
    // of a bass intro must still separate from the bar's own background, or
    // the first seconds of such a track read as an empty bar.
    let background = composited_luminance((0.0, 0.0, 0.0), 0.0);
    let deepest = composited_luminance(spectral_colour(0.0), UNPLAYED_ALPHA);
    let ratio = (deepest + 0.05) / (background + 0.05);
    assert!(
        ratio >= 1.5,
        "the deep end vanishes into the background at {ratio:.2}:1"
    );
}
