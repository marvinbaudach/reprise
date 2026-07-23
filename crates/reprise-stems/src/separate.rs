//! The runtime-free separation orchestration: drive a track through the fixed
//! htdemucs geometry segment by segment, reduce each segment's stems to the
//! instrumental, and blend the segments back with the Demucs overlap-add
//! window.
//!
//! It is generic over an **inference function** (`infer`) rather than calling
//! onnxruntime directly, so the whole control flow this package must get right
//! — cancel honoured *between* chunks, monotonic per-chunk progress,
//! deterministic stitching, source reduction, shape validation — is unit-tested
//! in the default build with a synthetic `infer`, no model and no native lib.
//! `crate::ort_backend` supplies the real `infer` that runs the ONNX session.

use reprise_core::stem_separation::{ProgressPermille, StemError, PROGRESS_COMPLETE};

use crate::chunk::{fade_window, Geometry};

/// Guard against dividing by a zero summed weight. Every in-range sample is
/// covered by at least one segment whose transition weight is strictly
/// positive, so this only defends against pathological geometries.
const WEIGHT_EPSILON: f32 = 1e-8;

/// One segment of interleaved-free (planar) mix audio handed to `infer`:
/// `channels * segment` f32 laid out `[ch0 samples…, ch1 samples…]`.
///
/// `infer` must return the model's stems as `source_count * channels *
/// segment` f32 laid out `[source][channel][sample]`, i.e. index
/// `(source * channels + channel) * segment + sample`.
pub type InferFn<'a> = dyn FnMut(&[f32]) -> Result<Vec<f32>, StemError> + 'a;

/// Separates `input` (planar channels, each `total_samples` long) into its
/// instrumental (every source except the vocals, summed), returning planar
/// channels of the same length.
///
/// Contract mirrored from [`reprise_core::stem_separation`]:
/// * `cancel` is polled **before every segment** and once more after the last;
///   on cancel it returns [`StemError::Cancelled`] immediately and the caller
///   writes no output.
/// * `progress` is reported **after** each segment completes, in permille of
///   the whole job, non-decreasing and ending at exactly [`PROGRESS_COMPLETE`].
/// * the result is deterministic for identical `input` and a deterministic
///   `infer` (fixed iteration order, no nondeterministic reductions here).
pub fn separate_instrumental(
    input: &[Vec<f32>],
    geometry: &Geometry,
    progress: &mut dyn FnMut(ProgressPermille),
    cancel: &dyn Fn() -> bool,
    infer: &mut InferFn<'_>,
) -> Result<Vec<Vec<f32>>, StemError> {
    let channels = input.len();
    let total_samples = input.first().map_or(0, Vec::len);
    if channels == 0 || total_samples == 0 {
        return Err(StemError::Backend(
            "no audio samples to separate".to_string(),
        ));
    }
    if input.iter().any(|c| c.len() != total_samples) {
        return Err(StemError::Backend(
            "input channels have unequal length".to_string(),
        ));
    }

    let segment = geometry.segment;
    let starts = geometry.segment_starts(total_samples);
    let segment_count = starts.len();
    // Boundary-aware windows: the whole track's leading edge (first segment) and
    // trailing edge (last segment) keep full weight, so the very first/last
    // samples reconstruct exactly instead of being forced to zero. Interior
    // seams keep the reference crossfade. At most three distinct windows.
    let overlap = geometry.overlap();
    let multi = segment_count > 1;
    let first_window = fade_window(segment, overlap, false, multi);
    let interior_window = fade_window(segment, overlap, true, true);
    let last_window = fade_window(segment, overlap, multi, false);

    let mut accumulator = vec![vec![0.0f32; total_samples]; channels];
    let mut summed_weight = vec![0.0f32; total_samples];
    let expected_stems = geometry.source_count * channels * segment;

    for (index, &start) in starts.iter().enumerate() {
        // Cancel is honoured at the segment boundary, before the expensive
        // inference — a cancelled run does no further work and leaves no output.
        if cancel() {
            return Err(StemError::Cancelled);
        }

        let mix = pack_segment(input, channels, segment, start, total_samples);
        let stems = infer(&mix)?;
        if stems.len() != expected_stems {
            return Err(StemError::Backend(format!(
                "inference returned {} values, expected {} ({}×{}×{})",
                stems.len(),
                expected_stems,
                geometry.source_count,
                channels,
                segment
            )));
        }

        // The first segment does not fade in and the last does not fade out;
        // everything between uses the interior crossfade.
        let weights = if index == 0 {
            &first_window
        } else if index == segment_count - 1 {
            &last_window
        } else {
            &interior_window
        };
        accumulate_instrumental(
            &mut accumulator,
            &mut summed_weight,
            &stems,
            weights,
            geometry,
            channels,
            segment,
            start,
            total_samples,
        );

        // Progress AFTER the segment; the final segment reports exactly
        // PROGRESS_COMPLETE. Non-decreasing by construction.
        let done = (index + 1) as u64 * u64::from(PROGRESS_COMPLETE) / segment_count as u64;
        progress(done as ProgressPermille);
    }

    // A cancel that arrives at the very end still wins, before we hand back the
    // (soon-to-be-written) render.
    if cancel() {
        return Err(StemError::Cancelled);
    }

    for channel in &mut accumulator {
        for (sample, weight) in channel.iter_mut().zip(&summed_weight) {
            *sample /= weight.max(WEIGHT_EPSILON);
        }
    }
    Ok(accumulator)
}

/// Builds one zero-padded planar mix segment `[ch0…, ch1…]` starting at
/// `start`; samples past `total_samples` are silence.
fn pack_segment(
    input: &[Vec<f32>],
    channels: usize,
    segment: usize,
    start: usize,
    total_samples: usize,
) -> Vec<f32> {
    let mut mix = vec![0.0f32; channels * segment];
    for (channel_index, channel) in input.iter().enumerate().take(channels) {
        let base = channel_index * segment;
        for j in 0..segment {
            let position = start + j;
            if position >= total_samples {
                break;
            }
            mix[base + j] = channel[position];
        }
    }
    mix
}

/// Reduces one segment's stems to the instrumental (all sources except the
/// vocals) and overlap-adds it into the accumulator with the transition window.
#[allow(clippy::too_many_arguments)]
fn accumulate_instrumental(
    accumulator: &mut [Vec<f32>],
    summed_weight: &mut [f32],
    stems: &[f32],
    weights: &[f32],
    geometry: &Geometry,
    channels: usize,
    segment: usize,
    start: usize,
    total_samples: usize,
) {
    for j in 0..segment {
        let position = start + j;
        if position >= total_samples {
            break;
        }
        let weight = weights[j];
        for (channel_index, channel) in accumulator.iter_mut().enumerate().take(channels) {
            let mut instrumental = 0.0f32;
            for source in 0..geometry.source_count {
                if source == geometry.vocal_source {
                    continue;
                }
                instrumental += stems[(source * channels + channel_index) * segment + j];
            }
            channel[position] += weight * instrumental;
        }
        summed_weight[position] += weight;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    // A source_count=2, vocals-last geometry: source 0 is the instrumental,
    // source 1 the vocals. `stems = [instrumental = mix, vocals = 0]` makes the
    // reduction reproduce the mix, so a correct stitch reconstructs the input.
    fn tiny_geometry() -> Geometry {
        Geometry::new(8, 0.25, 2, 1) // segment 8, stride 6, 2 sources, vocals #1
    }

    fn stereo_ramp(total: usize) -> Vec<Vec<f32>> {
        // Offset from zero so sample 0 is non-trivial: a fade window that zeroed
        // out[0] would otherwise reconstruct vacuously (0 ≈ 0).
        let left: Vec<f32> = (0..total).map(|i| 0.5 + (i as f32) * 0.001).collect();
        let right: Vec<f32> = (0..total).map(|i| -0.3 - (i as f32) * 0.002).collect();
        vec![left, right]
    }

    // `infer` that echoes the mix as the instrumental source and silences the
    // vocals — the identity of the whole reduce+stitch path.
    fn echo_instrumental(
        channels: usize,
        segment: usize,
    ) -> impl FnMut(&[f32]) -> Result<Vec<f32>, StemError> {
        move |mix: &[f32]| {
            let mut stems = vec![0.0f32; 2 * channels * segment];
            // source 0 (instrumental) = mix ; source 1 (vocals) = 0.
            stems[..channels * segment].copy_from_slice(mix);
            Ok(stems)
        }
    }

    #[test]
    fn reconstructs_the_input_across_overlapping_segments() {
        let g = tiny_geometry();
        let input = stereo_ramp(20); // starts 0,6,12,18 -> 4 overlapping segments
        let mut infer = echo_instrumental(2, g.segment);

        let out = separate_instrumental(&input, &g, &mut |_| {}, &|| false, &mut infer).unwrap();

        assert_eq!(out.len(), 2);
        for channel in 0..2 {
            assert_eq!(out[channel].len(), 20);
            for (i, (&got, &want)) in out[channel].iter().zip(&input[channel]).enumerate() {
                assert!(
                    (got - want).abs() < 1e-4,
                    "channel {channel} sample {i}: got {got}, want {want}"
                );
            }
            // The whole-track boundaries reconstruct exactly — the old window
            // forced window[0] = 0, zeroing the very first output sample.
            assert!(
                (out[channel][0] - input[channel][0]).abs() < 1e-6,
                "first sample must reconstruct: got {}, want {}",
                out[channel][0],
                input[channel][0]
            );
            let last = input[channel].len() - 1;
            assert!(
                (out[channel][last] - input[channel][last]).abs() < 1e-6,
                "last sample must reconstruct: got {}, want {}",
                out[channel][last],
                input[channel][last]
            );
        }
    }

    #[test]
    fn the_vocal_source_is_excluded_from_the_instrumental() {
        let g = tiny_geometry();
        let input = stereo_ramp(10);
        // infer puts the mix in BOTH sources; only source 0 (non-vocal) counts,
        // so the instrumental equals the mix, not twice it.
        let mut infer = |mix: &[f32]| {
            let mut stems = vec![0.0f32; 2 * 2 * g.segment];
            stems[..2 * g.segment].copy_from_slice(mix); // source 0
            stems[2 * g.segment..].copy_from_slice(mix); // source 1 (vocals) — ignored
            Ok(stems)
        };

        let out = separate_instrumental(&input, &g, &mut |_| {}, &|| false, &mut infer).unwrap();
        for channel in 0..2 {
            for (&got, &want) in out[channel].iter().zip(&input[channel]) {
                assert!((got - want).abs() < 1e-4, "vocals must not be added in");
            }
        }
    }

    #[test]
    fn cancel_before_a_chunk_stops_promptly_with_no_output() {
        let g = tiny_geometry();
        let input = stereo_ramp(50); // many segments
        let calls = Cell::new(0u32);
        let mut infer = |mix: &[f32]| {
            calls.set(calls.get() + 1);
            Ok(echo_instrumental(2, g.segment)(mix).unwrap())
        };
        // Cancel becomes true once two segments have run.
        let err = separate_instrumental(&input, &g, &mut |_| {}, &|| calls.get() >= 2, &mut infer)
            .unwrap_err();

        assert!(matches!(err, StemError::Cancelled));
        assert!(calls.get() < g.segment_count(50) as u32, "must stop early");
    }

    #[test]
    fn progress_is_monotonic_and_ends_at_complete() {
        let g = tiny_geometry();
        let input = stereo_ramp(60);
        let reported = std::cell::RefCell::new(Vec::new());
        let mut infer = echo_instrumental(2, g.segment);

        separate_instrumental(
            &input,
            &g,
            &mut |p| reported.borrow_mut().push(p),
            &|| false,
            &mut infer,
        )
        .unwrap();

        let reported = reported.into_inner();
        assert_eq!(reported.len(), g.segment_count(60));
        assert!(
            reported.windows(2).all(|w| w[0] <= w[1]),
            "progress must be non-decreasing: {reported:?}"
        );
        assert_eq!(*reported.last().unwrap(), PROGRESS_COMPLETE);
    }

    #[test]
    fn identical_input_yields_byte_identical_output() {
        let g = tiny_geometry();
        let input = stereo_ramp(37);
        let mut a = echo_instrumental(2, g.segment);
        let mut b = echo_instrumental(2, g.segment);

        let first = separate_instrumental(&input, &g, &mut |_| {}, &|| false, &mut a).unwrap();
        let second = separate_instrumental(&input, &g, &mut |_| {}, &|| false, &mut b).unwrap();

        assert_eq!(
            first
                .iter()
                .map(|c| c.iter().map(|f| f.to_bits()).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            second
                .iter()
                .map(|c| c.iter().map(|f| f.to_bits()).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            "the stitching path must be bit-for-bit deterministic"
        );
    }

    #[test]
    fn a_wrong_inference_shape_is_a_backend_error_not_a_panic() {
        let g = tiny_geometry();
        let input = stereo_ramp(10);
        let mut infer = |_mix: &[f32]| Ok(vec![0.0f32; 3]); // nonsense length

        let err =
            separate_instrumental(&input, &g, &mut |_| {}, &|| false, &mut infer).unwrap_err();
        assert!(matches!(err, StemError::Backend(_)));
    }

    #[test]
    fn empty_input_is_a_backend_error() {
        let g = tiny_geometry();
        let mut infer = echo_instrumental(2, g.segment);
        let err = separate_instrumental(&[], &g, &mut |_| {}, &|| false, &mut infer).unwrap_err();
        assert!(matches!(err, StemError::Backend(_)));
    }

    #[test]
    fn inference_error_propagates() {
        let g = tiny_geometry();
        let input = stereo_ramp(10);
        let mut infer = |_mix: &[f32]| Err(StemError::Backend("boom".to_string()));
        let err =
            separate_instrumental(&input, &g, &mut |_| {}, &|| false, &mut infer).unwrap_err();
        assert!(matches!(err, StemError::Backend(msg) if msg == "boom"));
    }
}
