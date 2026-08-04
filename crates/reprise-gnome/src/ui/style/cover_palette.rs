//! The dominant colour of a cover.
//!
//! Median-cut splits the (32 px) cover into eight buckets and scores each by
//! `population × OKLCH chroma`. It then walks that ranking and takes the first
//! bucket that survives the near-gray gate, rather than giving up when the
//! single highest-scoring one happens to be gray.
//!
//! This briefly returned three colours, for a conic sweep of the cover's
//! palette behind the artwork. Measured against a real library that effect did
//! not work — half the covers yield no usable colour at all and most of the
//! rest are monochrome — so the sweep became a rotating copy of the blurred
//! cover and the second and third entries lost their only reader.

use super::color_math::{linear_rgb_to_oklab, to_linear};
use super::cover_accent_oklab::{oklch_clamp, Rgb};

/// Edge length the cover is scaled to before sampling — small enough to be
/// cheap, large enough to be representative.
const SAMPLE_EDGE: i32 = 32;

/// Recursively splits `pixels` along the channel with the widest range,
/// up to `depth` levels (producing up to 2^depth buckets). Returns all
/// buckets as a flat `Vec<Vec<[u8; 3]>>`.
fn median_cut_buckets(pixels: Vec<[u8; 3]>, depth: u32) -> Vec<Vec<[u8; 3]>> {
    if depth == 0 || pixels.len() <= 1 {
        return vec![pixels];
    }

    // Find the channel with the widest range.
    let (mut r_min, mut r_max) = (u8::MAX, u8::MIN);
    let (mut g_min, mut g_max) = (u8::MAX, u8::MIN);
    let (mut b_min, mut b_max) = (u8::MAX, u8::MIN);
    for &[r, g, b] in &pixels {
        r_min = r_min.min(r);
        r_max = r_max.max(r);
        g_min = g_min.min(g);
        g_max = g_max.max(g);
        b_min = b_min.min(b);
        b_max = b_max.max(b);
    }

    let r_range = r_max.saturating_sub(r_min);
    let g_range = g_max.saturating_sub(g_min);
    let b_range = b_max.saturating_sub(b_min);

    let mut sorted = pixels;
    if r_range >= g_range && r_range >= b_range {
        sorted.sort_unstable_by_key(|&[r, _, _]| r);
    } else if g_range >= b_range {
        sorted.sort_unstable_by_key(|&[_, g, _]| g);
    } else {
        sorted.sort_unstable_by_key(|&[_, _, b]| b);
    }

    let mid = sorted.len() / 2;
    let (lo, hi) = sorted.split_at(mid);

    let mut result = median_cut_buckets(lo.to_vec(), depth - 1);
    result.extend(median_cut_buckets(hi.to_vec(), depth - 1));
    result
}

pub(in crate::ui) fn dominant_accent(pixels: &[u8], channels: usize) -> Option<Rgb> {
    if channels < 3 {
        return None;
    }

    // Collect opaque pixels.
    let opaque: Vec<[u8; 3]> = pixels
        .chunks_exact(channels)
        .filter(|px| channels < 4 || px[3] >= 128)
        .map(|px| [px[0], px[1], px[2]])
        .collect();

    if opaque.is_empty() {
        return None;
    }

    // Median-cut into 8 buckets (3 levels deep).
    let buckets = median_cut_buckets(opaque, 3);

    // Score each bucket by population × chroma, preserving the old winner.
    let mut ranked = buckets
        .iter()
        .enumerate()
        .filter_map(|(index, bucket)| {
            if bucket.is_empty() {
                return None;
            }
            let n = bucket.len() as f64;
            let r_avg = bucket.iter().map(|p| f64::from(p[0])).sum::<f64>() / n;
            let g_avg = bucket.iter().map(|p| f64::from(p[1])).sum::<f64>() / n;
            let b_avg = bucket.iter().map(|p| f64::from(p[2])).sum::<f64>() / n;

            let avg = Rgb {
                r: r_avg.round() as u8,
                g: g_avg.round() as u8,
                b: b_avg.round() as u8,
            };
            let (_, a, b) =
                linear_rgb_to_oklab(to_linear(avg.r), to_linear(avg.g), to_linear(avg.b));
            let chroma = (a * a + b * b).sqrt();
            Some((n * chroma, index, avg))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(
        |(left_score, left_index, _), (right_score, right_index, _)| {
            right_score
                .partial_cmp(left_score)
                .unwrap()
                .then_with(|| right_index.cmp(left_index))
        },
    );

    // Walk the ranking instead of taking only the winner: a cover whose
    // largest, most colourful bucket is still near-gray usually has a smaller
    // one that is not, and the theme fallback is a worse answer than that.
    ranked
        .into_iter()
        .find_map(|(_, _, average)| oklch_clamp(average))
}

/// Extracts the dominant accent from a cover image file. Runs off-main (decodes
/// a scaled pixbuf and reads its pixels); returns a `Send` [`Rgb`] for the main
/// thread to apply. `None` on any decode failure or a non-colorful cover.
pub(in crate::ui) fn accent_from_cover_file(path: &std::path::Path) -> Option<Rgb> {
    let pixbuf =
        gtk4::gdk_pixbuf::Pixbuf::from_file_at_scale(path, SAMPLE_EDGE, SAMPLE_EDGE, false).ok()?;
    let channels = pixbuf.n_channels() as usize;
    let width = pixbuf.width() as usize;
    let height = pixbuf.height() as usize;
    let rowstride = pixbuf.rowstride() as usize;
    let bytes = pixbuf.read_pixel_bytes();
    // Strip any per-row padding into a contiguous buffer before sampling.
    let mut contiguous = Vec::with_capacity(width * height * channels);
    for y in 0..height {
        let start = y * rowstride;
        let end = start + width * channels;
        if end <= bytes.len() {
            contiguous.extend_from_slice(&bytes[start..end]);
        }
    }
    dominant_accent(&contiguous, channels)
}

/// Probe support: prints every median-cut bucket's average colour and its raw
/// OKLCH chroma, so the near-gray gate can be set from measurement instead of
/// guesswork. Not used in production.
#[cfg(test)]
fn probe_buckets(path: &std::path::Path) {
    let Ok(pixbuf) =
        gtk4::gdk_pixbuf::Pixbuf::from_file_at_scale(path, SAMPLE_EDGE, SAMPLE_EDGE, false)
    else {
        println!("{}\n  (undecodable)", path.display());
        return;
    };
    let channels = pixbuf.n_channels() as usize;
    let width = pixbuf.width() as usize;
    let rowstride = pixbuf.rowstride() as usize;
    let bytes = pixbuf.read_pixel_bytes();
    let mut contiguous = Vec::new();
    for y in 0..pixbuf.height() as usize {
        let start = y * rowstride;
        let end = start + width * channels;
        if end <= bytes.len() {
            contiguous.extend_from_slice(&bytes[start..end]);
        }
    }
    let opaque: Vec<[u8; 3]> = contiguous
        .chunks_exact(channels)
        .filter(|px| channels < 4 || px[3] >= 128)
        .map(|px| [px[0], px[1], px[2]])
        .collect();
    let mut chromas: Vec<(f64, Rgb)> = median_cut_buckets(opaque, 3)
        .iter()
        .filter(|bucket| !bucket.is_empty())
        .map(|bucket| {
            let n = bucket.len() as f64;
            let avg = Rgb {
                r: (bucket.iter().map(|p| f64::from(p[0])).sum::<f64>() / n).round() as u8,
                g: (bucket.iter().map(|p| f64::from(p[1])).sum::<f64>() / n).round() as u8,
                b: (bucket.iter().map(|p| f64::from(p[2])).sum::<f64>() / n).round() as u8,
            };
            let (_, a, b) =
                linear_rgb_to_oklab(to_linear(avg.r), to_linear(avg.g), to_linear(avg.b));
            ((a * a + b * b).sqrt(), avg)
        })
        .collect();
    chromas.sort_by(|left, right| right.0.partial_cmp(&left.0).unwrap());
    let top: Vec<String> = chromas
        .iter()
        .take(3)
        .map(|(chroma, rgb)| format!("#{:02x}{:02x}{:02x} C={chroma:.3}", rgb.r, rgb.g, rgb.b))
        .collect();
    println!("  buckets by chroma: {}", top.join("  "));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(r: u8, g: u8, b: u8, count: usize) -> Vec<u8> {
        std::iter::repeat_n([r, g, b], count).flatten().collect()
    }

    #[test]
    fn median_cut_picks_vivid_cluster() {
        // 90% gray pixels, 10% bright red -> should pick the red cluster.
        let mut pixels = solid(130, 130, 130, 90);
        pixels.extend(solid(220, 40, 40, 10));
        let accent = dominant_accent(&pixels, 3).expect("red cluster");
        assert!(accent.r > 180, "expected red-dominant, got {accent:?}");
    }

    #[test]
    fn near_gray_falls_back_to_none() {
        let result = dominant_accent(&solid(128, 126, 130, 100), 3);
        assert!(result.is_none());
    }

    #[test]
    fn grayscale_cover_has_no_accent() {
        let pixels = solid(128, 128, 128, 64);
        assert!(dominant_accent(&pixels, 3).is_none());
    }

    #[test]
    fn vivid_pixels_outweigh_gray_ones() {
        let mut pixels = solid(130, 130, 130, 60); // mostly gray
        pixels.extend(solid(40, 200, 120, 4)); // a few vivid teal
        let accent = dominant_accent(&pixels, 3).expect("some accent");
        // After OKLCH clamping the teal bucket wins, so green should dominate.
        assert!(accent.g > accent.r && accent.g > accent.b, "{accent:?}");
    }

    #[test]
    fn dominant_accent_returns_colorful_result_for_vivid_input() {
        let pixels = solid(220, 90, 40, 64); // warm orange
                                             // Result is OKLCH-clamped so exact match not expected, but should exist.
        let result = dominant_accent(&pixels, 3);
        assert!(
            result.is_some(),
            "expected a result for vivid orange pixels"
        );
    }

    #[test]
    fn a_gray_winner_does_not_cost_the_cover_its_accent() {
        // Mostly near-black filler, one small vivid family. The old code took
        // the single highest-scoring bucket and gave up when it was gray; the
        // ranking is walked instead, because a smaller colourful bucket is a
        // better answer than the theme fallback.
        let mut pixels = solid(20, 20, 24, 200); // near-black, wins on population
        pixels.extend(solid(210, 70, 60, 24)); // small but colourful
        let accent = dominant_accent(&pixels, 3).expect("the red bucket survives");
        assert!(accent.r > accent.g && accent.r > accent.b, "{accent:?}");
    }

    #[test]
    fn a_grayscale_cover_has_no_accent() {
        // Nothing to walk to: every bucket is below the near-gray gate, so the
        // theme fallback applies. Inventing a hue here is exactly the
        // dishonesty the cover-derived accent exists to avoid.
        assert!(dominant_accent(&solid(128, 128, 128, 64), 3).is_none());
    }

    /// Probe, not a regression: runs the real extraction over real artwork so
    /// "why is the light not showing" can be answered by measuring instead of
    /// reasoning. Paths come from the environment, comma-separated.
    ///
    /// ```sh
    /// REPRISE_COVERS="/path/a/cover.jpg,/path/b/cover.png" \
    ///   cargo test -p reprise-gnome --bins probe_real_covers -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "probe: set REPRISE_COVERS to real cover files"]
    fn probe_real_covers() {
        let covers = std::env::var("REPRISE_COVERS").expect("REPRISE_COVERS must be set");
        for path in covers.split(',').filter(|entry| !entry.is_empty()) {
            probe_buckets(std::path::Path::new(path));
            match accent_from_cover_file(std::path::Path::new(path)) {
                Some(accent) => {
                    println!(
                        "{path}\n  accent #{:02x}{:02x}{:02x}",
                        accent.r, accent.g, accent.b,
                    );
                }
                None => println!("{path}\n  NONE — no usable colour, the theme accent applies"),
            }
        }
    }
}
