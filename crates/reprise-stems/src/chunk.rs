//! Chunking geometry for segment-wise htdemucs inference.
//!
//! htdemucs is a fixed-length model: its ONNX input is `[1, 2, SEGMENT]` — a
//! stereo waveform of exactly [`HTDEMUCS_SEGMENT`] samples at 44.1 kHz (the
//! STFT lives inside the graph, so it is pure waveform I/O). A whole track is
//! therefore processed as a sequence of **overlapping** fixed segments and the
//! per-segment outputs are blended back together (see [`crate::separate`]).
//!
//! This module owns the pure, dependency-free geometry — where each segment
//! starts and the Demucs transition window used to weight the overlap-add. It
//! carries no audio and no runtime, so every rule here is unit-tested in the
//! default build without onnxruntime.

/// htdemucs native sample rate. The model is trained at 44.1 kHz; any other
/// source rate is resampled to this before inference.
pub const HTDEMUCS_SAMPLE_RATE: u32 = 44_100;

/// htdemucs fixed segment length in samples (`≈ 7.8 s` at 44.1 kHz) — the
/// third axis of the model's `[1, 2, 343980]` input tensor.
pub const HTDEMUCS_SEGMENT: usize = 343_980;

/// The four htdemucs sources, in the order the ONNX `stems` output emits them:
/// `[drums, bass, other, vocals]` (the canonical Demucs v4 order).
pub const HTDEMUCS_SOURCE_COUNT: usize = 4;

/// Index of the `vocals` source in the htdemucs output. The instrumental is
/// every source **except** this one (drums + bass + other), per Beschluss 19.
pub const HTDEMUCS_VOCAL_SOURCE: usize = 3;

/// Segment overlap fraction (Demucs default). Consecutive segments share this
/// fraction of their length; the shared span is cross-faded by the transition
/// window so segment seams are inaudible.
pub const OVERLAP: f64 = 0.25;

/// The fixed geometry a separation run is driven by. Small and `Copy` so tests
/// can build a tiny geometry (a handful of samples) instead of the real
/// 343 980-sample htdemucs segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    /// Samples per inference segment (the model's fixed input length).
    pub segment: usize,
    /// Hop between consecutive segment starts (`< segment`, so segments
    /// overlap).
    pub stride: usize,
    /// Number of sources the model emits per segment.
    pub source_count: usize,
    /// Which source is the vocals (excluded from the instrumental sum).
    pub vocal_source: usize,
}

impl Geometry {
    /// The production htdemucs geometry (44.1 kHz, 343 980-sample segments,
    /// overlap 0.25, 4 sources with vocals last).
    pub fn htdemucs() -> Self {
        Self::new(
            HTDEMUCS_SEGMENT,
            OVERLAP,
            HTDEMUCS_SOURCE_COUNT,
            HTDEMUCS_VOCAL_SOURCE,
        )
    }

    /// Builds a geometry from a segment length and overlap fraction. The stride
    /// is `round(segment * (1 - overlap))`, clamped to `1..=segment` so a chunk
    /// always advances yet segments always overlap (mirrors Demucs `apply.py`).
    ///
    /// Panics only on a nonsensical zero segment — callers use the fixed model
    /// constants, never user input.
    pub fn new(segment: usize, overlap: f64, source_count: usize, vocal_source: usize) -> Self {
        assert!(segment > 0, "segment length must be positive");
        let raw = (segment as f64 * (1.0 - overlap)).round() as usize;
        let stride = raw.clamp(1, segment);
        Self {
            segment,
            stride,
            source_count,
            vocal_source,
        }
    }

    /// The start sample of every segment needed to cover `total_samples`.
    ///
    /// Starts step by `stride` while still inside the track, so the last
    /// segment begins at the largest multiple of `stride` below
    /// `total_samples` and (because `segment > stride`) reaches past the end,
    /// which the caller zero-pads. A track of `0` samples yields no segments;
    /// any non-empty track yields at least one.
    pub fn segment_starts(&self, total_samples: usize) -> Vec<usize> {
        if total_samples == 0 {
            return Vec::new();
        }
        let mut starts = Vec::new();
        let mut start = 0;
        while start < total_samples {
            starts.push(start);
            start += self.stride;
        }
        starts
    }

    /// Number of segments (and therefore inference calls) for `total_samples`.
    pub fn segment_count(&self, total_samples: usize) -> usize {
        if total_samples == 0 {
            return 0;
        }
        // ceil over the stride, but always at least one segment.
        1 + total_samples.saturating_sub(1) / self.stride
    }

    /// The overlap length in samples — the span two consecutive segments share
    /// (`segment - stride`). This is the fade length of the overlap-add window.
    pub fn overlap(&self) -> usize {
        self.segment - self.stride
    }
}

/// The overlap-add window for one segment: a **trapezoid** that is `1.0`
/// across the middle with a linear fade-in over the first `overlap` samples
/// and a mirrored fade-out over the last `overlap` samples.
///
/// This is a faithful port of the model's own reference `infer.py`
/// (`StemSplitio/htdemucs-onnx`, MIT), whose parity-vs-PyTorch claim depends on
/// exactly this window:
///
/// ```python
/// w = np.ones(n); fade = np.linspace(0, 1, overlap)
/// w[:overlap] = fade; w[-overlap:] = fade[::-1]
/// ```
///
/// Segments are accumulated weighted by this window and divided by the summed
/// weight (`out /= max(weight, eps)`), so an interior sample covered by a
/// single segment is reproduced exactly while shared spans cross-fade. As in
/// the reference, `linspace(0, 1, overlap)[0] == 0`, so the very first window
/// sample is `0` — a one-sample boundary quirk the reference shares.
pub fn fade_window(segment: usize, overlap: usize) -> Vec<f32> {
    debug_assert!(segment > 0);
    let mut window = vec![1.0f32; segment];
    let fade = linspace_0_1(overlap.min(segment));
    for (i, &f) in fade.iter().enumerate() {
        window[i] = f;
    }
    for (i, &f) in fade.iter().enumerate() {
        // Mirror onto the tail: window[segment-1-i] = fade[i].
        window[segment - 1 - i] = f;
    }
    window
}

/// `numpy.linspace(0.0, 1.0, n)`: `n` points from `0.0` to `1.0` inclusive.
/// `n == 0` is empty and `n == 1` is `[0.0]`, matching numpy exactly.
fn linspace_0_1(n: usize) -> Vec<f32> {
    match n {
        0 => Vec::new(),
        1 => vec![0.0],
        _ => (0..n).map(|k| k as f32 / (n - 1) as f32).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn htdemucs_geometry_matches_the_model_card() {
        let g = Geometry::htdemucs();
        assert_eq!(g.segment, 343_980);
        // stride = round(343980 * 0.75) = 257985.
        assert_eq!(g.stride, 257_985);
        assert_eq!(g.source_count, 4);
        assert_eq!(g.vocal_source, 3);
    }

    #[test]
    fn stride_is_smaller_than_segment_so_segments_overlap() {
        let g = Geometry::new(1000, 0.25, 4, 3);
        assert_eq!(g.stride, 750);
        assert!(g.stride < g.segment);
    }

    #[test]
    fn zero_overlap_still_advances_a_full_segment() {
        let g = Geometry::new(1000, 0.0, 4, 3);
        assert_eq!(g.stride, 1000);
    }

    #[test]
    fn full_overlap_clamps_stride_to_one_not_zero() {
        // overlap 1.0 would give stride 0 (an infinite loop); it is clamped.
        let g = Geometry::new(1000, 1.0, 4, 3);
        assert_eq!(g.stride, 1);
    }

    #[test]
    fn empty_track_has_no_segments() {
        let g = Geometry::new(100, 0.25, 4, 3);
        assert_eq!(g.segment_starts(0), Vec::<usize>::new());
        assert_eq!(g.segment_count(0), 0);
    }

    #[test]
    fn a_track_shorter_than_a_segment_is_one_padded_segment() {
        let g = Geometry::new(100, 0.25, 4, 3); // stride 75
        assert_eq!(g.segment_starts(40), vec![0]);
        assert_eq!(g.segment_count(40), 1);
    }

    #[test]
    fn segment_starts_step_by_stride_and_cover_the_tail() {
        let g = Geometry::new(100, 0.25, 4, 3); // stride 75
                                                // 200 samples: starts 0, 75, 150 (150 < 200 < 225=150+segment).
        assert_eq!(g.segment_starts(200), vec![0, 75, 150]);
        assert_eq!(g.segment_count(200), 3);
        // The last segment [150, 250) reaches past 200 — the caller zero-pads.
        let last = *g.segment_starts(200).last().unwrap();
        assert!(last + g.segment >= 200, "last segment must cover the end");
    }

    #[test]
    fn segment_count_agrees_with_segment_starts_len() {
        let g = Geometry::new(343_980, 0.25, 4, 3);
        for n in [
            1usize, 100, 257_985, 343_980, 343_981, 1_000_000, 12_345_678,
        ] {
            assert_eq!(g.segment_count(n), g.segment_starts(n).len(), "n = {n}");
        }
    }

    #[test]
    fn fade_window_is_a_trapezoid_of_the_right_length_and_range() {
        for (segment, overlap) in [(8usize, 2usize), (100, 25), (343_980, 85_995)] {
            let w = fade_window(segment, overlap);
            assert_eq!(w.len(), segment, "length must equal the segment");
            let max = w.iter().copied().fold(0.0f32, f32::max);
            assert!((max - 1.0).abs() < 1e-6, "plateau must reach 1.0");
            // Interior (past the fades) is a flat 1.0.
            assert!((w[segment / 2] - 1.0).abs() < 1e-6, "middle is the plateau");
            assert!(w.iter().all(|&x| (0.0..=1.0).contains(&x)));
        }
    }

    #[test]
    fn fade_window_matches_the_reference_fade_in_and_out() {
        // overlap 2 -> fade = linspace(0,1,2) = [0, 1]; ones between.
        let w = fade_window(8, 2);
        assert_eq!(w, vec![0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]);
    }

    #[test]
    fn fade_window_first_sample_is_zero_like_the_reference() {
        // linspace(0,1,overlap)[0] == 0, so window[0] == 0 (reference quirk).
        let w = fade_window(100, 25);
        assert_eq!(w[0], 0.0);
        assert_eq!(*w.last().unwrap(), 0.0);
        assert!((w[25] - 1.0).abs() < 1e-6, "plateau begins after the fade");
    }

    #[test]
    fn fade_window_htdemucs_overlap_is_a_quarter_segment() {
        let g = Geometry::htdemucs();
        assert_eq!(g.overlap(), 85_995);
        assert_eq!(g.overlap(), g.segment / 4);
    }

    #[test]
    fn zero_overlap_window_is_all_ones() {
        assert_eq!(fade_window(4, 0), vec![1.0, 1.0, 1.0, 1.0]);
    }
}
