//! Conversion from the model's floating-point output to the integer PCM a FLAC
//! encoder consumes. Pure and dependency-free, so the quantisation rules are
//! unit-tested in the default build.

/// Bit depth of the instrumental render. 24-bit keeps the render effectively
/// lossless relative to the model's f32 output (Beschluss 15: "einmal in
/// finaler Qualität"), while staying universally playable.
pub const RENDER_BITS_PER_SAMPLE: u32 = 24;

/// The largest positive integer sample for [`RENDER_BITS_PER_SAMPLE`] bits
/// (`2^(bits-1) - 1`). Both limbs use this magnitude so the mapping is
/// symmetric and `0.0` maps to `0`.
fn sample_peak(bits: u32) -> i64 {
    (1i64 << (bits - 1)) - 1
}

/// Maps one f32 sample in (nominally) `[-1.0, 1.0]` to a signed integer of
/// `bits` bits. Values outside `[-1, 1]` (htdemucs can overshoot slightly) are
/// hard-clipped so the render never wraps, and NaN maps to silence.
pub fn f32_to_sample(x: f32, bits: u32) -> i32 {
    let peak = sample_peak(bits) as f32;
    let clamped = if x.is_nan() { 0.0 } else { x.clamp(-1.0, 1.0) };
    // Round half-away-from-zero for a symmetric, deterministic mapping.
    (clamped * peak).round() as i32
}

/// Interleaves planar f32 channels (`[[L…], [R…]]`) into the interleaved
/// integer PCM (`[L0, R0, L1, R1, …]`) a FLAC encoder consumes. All channels
/// must be the same length; the result has `channels * frames` samples.
pub fn interleave_to_pcm(planar: &[Vec<f32>], bits: u32) -> Vec<i32> {
    let channels = planar.len();
    if channels == 0 {
        return Vec::new();
    }
    let frames = planar[0].len();
    debug_assert!(
        planar.iter().all(|c| c.len() == frames),
        "all channels must share the same frame count"
    );
    let mut out = Vec::with_capacity(channels * frames);
    for frame in 0..frames {
        for channel in planar {
            out.push(f32_to_sample(channel[frame], bits));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_map_to_the_symmetric_peak() {
        let peak = (1i32 << 23) - 1; // 24-bit
        assert_eq!(f32_to_sample(1.0, 24), peak);
        assert_eq!(f32_to_sample(-1.0, 24), -peak);
        assert_eq!(f32_to_sample(0.0, 24), 0);
    }

    #[test]
    fn out_of_range_values_are_hard_clipped_not_wrapped() {
        let peak = (1i32 << 23) - 1;
        assert_eq!(f32_to_sample(2.5, 24), peak);
        assert_eq!(f32_to_sample(-9.0, 24), -peak);
    }

    #[test]
    fn nan_maps_to_silence() {
        assert_eq!(f32_to_sample(f32::NAN, 24), 0);
    }

    #[test]
    fn conversion_is_deterministic() {
        for _ in 0..3 {
            assert_eq!(f32_to_sample(0.333_333, 24), f32_to_sample(0.333_333, 24));
        }
    }

    #[test]
    fn sixteen_bit_uses_its_own_peak() {
        assert_eq!(f32_to_sample(1.0, 16), (1i32 << 15) - 1);
    }

    #[test]
    fn interleave_orders_frames_by_channel() {
        let planar = vec![vec![1.0, 0.0], vec![-1.0, 0.0]];
        let pcm = interleave_to_pcm(&planar, 24);
        let peak = (1i32 << 23) - 1;
        // frame 0: L=+peak, R=-peak ; frame 1: 0, 0.
        assert_eq!(pcm, vec![peak, -peak, 0, 0]);
    }

    #[test]
    fn interleave_of_no_channels_is_empty() {
        assert_eq!(interleave_to_pcm(&[], 24), Vec::<i32>::new());
    }
}
