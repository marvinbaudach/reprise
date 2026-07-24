//! Cover-derived player accent ("Follow album cover").
//!
//! The dominant, colorful tone of the current album cover is extracted
//! off-main and installed as an override of `@reprise_player_accent` in a
//! dedicated high-priority provider — so the waveform and the play button
//! (which read that color) take on the album's hue, while selection and
//! toggles (which read `@accent_color`) keep the theme accent.
//!
//! When a cover is missing, grayscale, or too dark/washed-out to read on the
//! dark surfaces, the override is cleared and the theme's own
//! `@reprise_player_accent` applies again. That static fallback is the selected
//! theme's accent; the palette remains its single source of truth.

use std::cell::RefCell;

use gtk4::prelude::IsA;
use libadwaita::prelude::AnimationExt;

use crate::ui::motion;

/// Edge length the cover is scaled to before sampling — small enough to be
/// cheap, large enough to be representative.
const SAMPLE_EDGE: i32 = 32;

/// An extracted 8-bit color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) struct Rgb {
    pub(in crate::ui) r: u8,
    pub(in crate::ui) g: u8,
    pub(in crate::ui) b: u8,
}

// ---------------------------------------------------------------------------
// OKLab / OKLCH conversion helpers
// ---------------------------------------------------------------------------

/// sRGB channel (0–255) → linear light.
fn to_linear(channel: u8) -> f64 {
    let c = f64::from(channel) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light → sRGB channel (0–255), clamped.
fn from_linear(c: f64) -> u8 {
    let srgb = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (srgb.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Linear RGB → OKLab `(L, a, b)`.
fn linear_rgb_to_oklab(lr: f64, lg: f64, lb: f64) -> (f64, f64, f64) {
    // Linear RGB → LMS (matrix from Björn Ottosson's OKLab blog)
    let l = 0.412_221_470_8 * lr + 0.536_332_536_3 * lg + 0.051_445_992_9 * lb;
    let m = 0.211_903_498_2 * lr + 0.680_699_545_1 * lg + 0.107_396_956_6 * lb;
    let s = 0.088_302_461_9 * lr + 0.281_718_837_6 * lg + 0.629_978_700_5 * lb;

    // Cube root
    let l = l.cbrt();
    let m = m.cbrt();
    let s = s.cbrt();

    // LMS → Lab
    let lab_l = 0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s;
    let lab_a = 1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s;
    let lab_b = 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s;

    (lab_l, lab_a, lab_b)
}

/// OKLab `(L, a, b)` → linear RGB `(r, g, b)`.
fn oklab_to_linear_rgb(lab_l: f64, lab_a: f64, lab_b: f64) -> (f64, f64, f64) {
    // Lab → LMS (inverse matrix)
    let l = lab_l + 0.396_337_777_3 * lab_a + 0.215_803_757_9 * lab_b;
    let m = lab_l - 0.105_561_346_2 * lab_a - 0.063_854_174_7 * lab_b;
    let s = lab_l - 0.089_484_177_5 * lab_a - 1.291_485_548_0 * lab_b;

    // Cube (inverse of cube root)
    let l = l * l * l;
    let m = m * m * m;
    let s = s * s * s;

    // LMS → linear RGB (inverse matrix)
    let r = 4.076_741_661_3 * l - 3.307_711_590_8 * m + 0.230_969_929_5 * s;
    let g = -1.268_437_973_0 * l + 2.609_757_401_1 * m - 0.341_319_427_9 * s;
    let b = -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s;

    (r, g, b)
}

/// Scales an sRGB color's OKLCH chroma while preserving its lightness and hue.
/// Inputs and outputs use Cairo's normalized 0..1 channel range. This is pure
/// color math and deliberately does not touch the global cover-accent provider.
pub(in crate::ui) fn scale_chroma(r: f64, g: f64, b: f64, factor: f64) -> (f64, f64, f64) {
    let to_channel = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    let (l, a, b) = linear_rgb_to_oklab(
        to_linear(to_channel(r)),
        to_linear(to_channel(g)),
        to_linear(to_channel(b)),
    );
    let factor = factor.clamp(0.0, 1.0);
    let (lr, lg, lb) = oklab_to_linear_rgb(l, a * factor, b * factor);
    (
        f64::from(from_linear(lr)) / 255.0,
        f64::from(from_linear(lg)) / 255.0,
        f64::from(from_linear(lb)) / 255.0,
    )
}

// ---------------------------------------------------------------------------
// OKLCH clamping
// ---------------------------------------------------------------------------

/// Chroma is clamped into this band. The floor lifts washed-out or dark covers
/// (e.g. a dark red album sleeve) to a saturated accent instead of a muted
/// near-gray tan; the ceiling keeps vivid covers from glaring. A cover whose
/// dominant chroma is below the near-gray gate (0.03) gets no accent at all —
/// the theme fallback applies instead.
const CHROMA_FLOOR: f64 = 0.08;
const CHROMA_CEIL: f64 = 0.13;

/// Converts `color` to OKLCH, clamps L to [0.55, 0.75] and C into
/// `[CHROMA_FLOOR, CHROMA_CEIL]`, and returns the result as `Rgb`. Returns
/// `None` if chroma < 0.03 (near-gray — use the theme fallback instead).
pub(super) fn oklch_clamp(color: Rgb) -> Option<Rgb> {
    let lr = to_linear(color.r);
    let lg = to_linear(color.g);
    let lb = to_linear(color.b);

    let (l, a, b) = linear_rgb_to_oklab(lr, lg, lb);

    let c = (a * a + b * b).sqrt();
    let h = b.atan2(a);

    if c < 0.03 {
        return None; // near-gray
    }

    // Clamp L into the readable band; clamp C into [floor, ceil] so even a
    // washed-out or dark cover yields a punchy — not muted — accent.
    let l_clamped = l.clamp(0.55, 0.75);
    let c_clamped = c.clamp(CHROMA_FLOOR, CHROMA_CEIL);

    // Back to Lab
    let a_out = c_clamped * h.cos();
    let b_out = c_clamped * h.sin();

    let (lr_out, lg_out, lb_out) = oklab_to_linear_rgb(l_clamped, a_out, b_out);

    Some(Rgb {
        r: from_linear(lr_out),
        g: from_linear(lg_out),
        b: from_linear(lb_out),
    })
}

// ---------------------------------------------------------------------------
// Median-cut
// ---------------------------------------------------------------------------

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

/// Median-cut dominant accent: splits the pixel set into up to 8 buckets,
/// picks the bucket with max `population × oklch_chroma`, and OKLCH-clamps
/// its average RGB. Returns `None` for near-gray or transparent covers.
fn dominant_accent(pixels: &[u8], channels: usize) -> Option<Rgb> {
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

    // Score each bucket by population × chroma.
    let best = buckets.iter().filter_map(|bucket| {
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

        let lr = to_linear(avg.r);
        let lg = to_linear(avg.g);
        let lb = to_linear(avg.b);
        let (_, a, b) = linear_rgb_to_oklab(lr, lg, lb);
        let chroma = (a * a + b * b).sqrt();

        Some((n * chroma, avg))
    });

    let (_, best_rgb) = best.max_by(|(s1, _), (s2, _)| s1.partial_cmp(s2).unwrap())?;
    oklch_clamp(best_rgb)
}

/// Whether `color` is colorful enough to use as an accent. Uses OKLCH chroma
/// (C ≥ 0.03) — effectively the same gate `oklch_clamp` applies, expressed
/// as a predicate so callers that already have an `Rgb` can check it without
/// re-running the full conversion.
fn is_usable(color: &Rgb) -> bool {
    let lr = to_linear(color.r);
    let lg = to_linear(color.g);
    let lb = to_linear(color.b);
    let (_, a, b) = linear_rgb_to_oklab(lr, lg, lb);
    let chroma = (a * a + b * b).sqrt();
    chroma >= 0.03
}

// ---------------------------------------------------------------------------
// CSS provider
// ---------------------------------------------------------------------------

/// The `@define-color` override for `color`, or empty (fall back to the theme's
/// own `reprise_player_accent`) when there is no usable cover accent.
fn accent_css(color: Option<Rgb>) -> String {
    match color {
        Some(c) if is_usable(&c) => {
            format!(
                "@define-color reprise_player_accent #{:02x}{:02x}{:02x};",
                c.r, c.g, c.b
            )
        }
        _ => String::new(),
    }
}

thread_local! {
    /// Override provider for the cover accent, kept so it can be reloaded per
    /// track. Sits above the theme provider so its `reprise_player_accent`
    /// wins when set, and falls back to the theme's when cleared (empty).
    static ACCENT_PROVIDER: RefCell<Option<gtk4::CssProvider>> = const { RefCell::new(None) };

    /// The currently-running cross-fade animation. Held here to prevent GC
    /// between ticks. Replaced (and thus dropped) on each new fade.
    static CURRENT_ANIMATION: RefCell<Option<libadwaita::TimedAnimation>> =
        const { RefCell::new(None) };
}

/// Installs the (initially empty) cover-accent override provider just above
/// application priority so it overrides the theme's `reprise_player_accent`.
pub(super) fn install(display: &gtk4::gdk::Display) {
    let provider = gtk4::CssProvider::new();
    gtk4::style_context_add_provider_for_display(
        display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
    );
    ACCENT_PROVIDER.with(|slot| *slot.borrow_mut() = Some(provider));
}

/// Applies (or clears, with `None`) the cover-derived player accent. A no-op
/// before [`install`] has run.
pub(in crate::ui) fn set_cover_accent(color: Option<Rgb>) {
    ACCENT_PROVIDER.with(|slot| {
        if let Some(provider) = slot.borrow().as_ref() {
            provider.load_from_string(&accent_css(color));
        }
    });
}

// ---------------------------------------------------------------------------
// Cross-fade
// ---------------------------------------------------------------------------

/// Linear interpolation between two u8 values at position `t` ∈ [0, 1].
fn lerp(a: u8, b: u8, t: f64) -> u8 {
    (f64::from(a) + (f64::from(b) - f64::from(a)) * t)
        .round()
        .clamp(0.0, 255.0) as u8
}

/// Resolves the selected theme's static accent without duplicating a fallback
/// literal in the cover pipeline.
fn theme_fallback_rgb() -> Rgb {
    let accent = super::CURRENT_THEME.with(|slot| slot.get().palette().accent);
    let hex = accent
        .strip_prefix('#')
        .filter(|hex| hex.len() == 6)
        .expect("theme accent must use #RRGGBB");
    let channel = |offset| {
        u8::from_str_radix(&hex[offset..offset + 2], 16)
            .expect("theme accent must use hexadecimal channels")
    };
    Rgb {
        r: channel(0),
        g: channel(2),
        b: channel(4),
    }
}

fn accent_during_fade(
    old: Option<Rgb>,
    new: Option<Rgb>,
    fallback: Rgb,
    value: f64,
) -> Option<Rgb> {
    if new.is_none() && value >= 1.0 {
        return None;
    }
    let old = old.unwrap_or(fallback);
    let new = new.unwrap_or(fallback);
    Some(Rgb {
        r: lerp(old.r, new.r, value),
        g: lerp(old.g, new.g, value),
        b: lerp(old.b, new.b, value),
    })
}

/// Animates the cover accent from `old` to `new` with the Ambient token.
/// The central motion helper follows the system animation setting. A `None`
/// argument uses the selected palette's theme accent as the interpolation
/// endpoint; clearing the override after the fade exposes the same color.
pub(in crate::ui) fn cross_fade_accent(
    old: Option<Rgb>,
    new: Option<Rgb>,
    widget: &impl IsA<gtk4::Widget>,
) {
    if old == new {
        let previous = CURRENT_ANIMATION.with(|slot| slot.borrow_mut().take());
        if let Some(previous) = previous {
            previous.skip();
        }
        set_cover_accent(new);
        return;
    }

    let fallback = theme_fallback_rgb();
    let target = libadwaita::CallbackAnimationTarget::new(move |value| {
        set_cover_accent(accent_during_fade(old, new, fallback, value));
    });

    let animation = motion::timed(widget, 0.0, 1.0, motion::AMBIENT, target);
    CURRENT_ANIMATION.with(|slot| motion::replace_animation(slot, animation.clone()));
    animation.play();
}

// ---------------------------------------------------------------------------
// Public extraction entry-point
// ---------------------------------------------------------------------------

/// Extracts the dominant accent from a cover image file. Runs off-main (decodes
/// a scaled pixbuf and reads its pixels); returns a `Send` [`Rgb`] for the main
/// thread to apply via [`set_cover_accent`] or [`cross_fade_accent`]. `None` on
/// any decode failure or a non-colorful cover.
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(r: u8, g: u8, b: u8, count: usize) -> Vec<u8> {
        std::iter::repeat_n([r, g, b], count).flatten().collect()
    }

    // --- median-cut tests (Task 8) ---

    #[test]
    fn median_cut_picks_vivid_cluster() {
        // 90% gray pixels, 10% bright red -> should pick the red cluster.
        let mut pixels = solid(130, 130, 130, 90);
        pixels.extend(solid(220, 40, 40, 10));
        let accent = dominant_accent(&pixels, 3).expect("red cluster");
        assert!(accent.r > 180, "expected red-dominant, got {accent:?}");
    }

    #[test]
    fn oklch_clamp_lifts_low_chroma_to_the_floor() {
        let chroma_of = |c: Rgb| {
            let (_, a, b) = linear_rgb_to_oklab(to_linear(c.r), to_linear(c.g), to_linear(c.b));
            (a * a + b * b).sqrt()
        };
        // A washed-out warm tone at C≈0.05: above the near-gray gate but below
        // the saturation floor — a dark/muted cover's dominant bucket.
        let (lr, lg, lb) = oklab_to_linear_rgb(0.65, 0.05 * 0.707, 0.05 * 0.707);
        let muted = Rgb {
            r: from_linear(lr),
            g: from_linear(lg),
            b: from_linear(lb),
        };
        let c_in = chroma_of(muted);
        assert!(
            (0.03..CHROMA_FLOOR).contains(&c_in),
            "test input chroma {c_in} is not in the boost band"
        );
        let out = oklch_clamp(muted).expect("usable, not near-gray");
        assert!(
            chroma_of(out) >= CHROMA_FLOOR - 0.005,
            "low chroma {} was not lifted to the {CHROMA_FLOOR} floor",
            chroma_of(out)
        );
    }

    #[test]
    fn oklch_clamp_limits_lightness_and_chroma() {
        // Pure red is very saturated; clamping should produce a muted mid-L color.
        let clamped = oklch_clamp(Rgb { r: 255, g: 0, b: 0 }).expect("red not near-gray");
        // After clamping, the result should be a muted, mid-lightness red-ish color.
        assert!(
            clamped.r > 100 && clamped.r < 230,
            "unexpected clamped red: {clamped:?}"
        );
    }

    #[test]
    fn chroma_scaling_is_draw_local_and_leaves_provider_state_untouched() {
        ACCENT_PROVIDER.with(|slot| slot.borrow_mut().take());
        CURRENT_ANIMATION.with(|slot| slot.borrow_mut().take());

        let original = (0.8, 0.2, 0.1);
        let unchanged = scale_chroma(original.0, original.1, original.2, 1.0);
        assert!((unchanged.0 - original.0).abs() <= 1.0 / 255.0);
        assert!((unchanged.1 - original.1).abs() <= 1.0 / 255.0);
        assert!((unchanged.2 - original.2).abs() <= 1.0 / 255.0);

        let gray = scale_chroma(original.0, original.1, original.2, 0.0);
        assert!((gray.0 - gray.1).abs() <= 1.0 / 255.0);
        assert!((gray.1 - gray.2).abs() <= 1.0 / 255.0);

        // Chroma scaling is pure draw-time math: it neither replaces nor
        // reloads the application-wide cover-accent provider.
        ACCENT_PROVIDER.with(|slot| assert!(slot.borrow().is_none()));
        CURRENT_ANIMATION.with(|slot| assert!(slot.borrow().is_none()));
    }

    #[test]
    fn near_gray_falls_back_to_none() {
        let result = dominant_accent(&solid(128, 126, 130, 100), 3);
        // Either returns None directly, or returns a color that is_usable rejects.
        assert!(result.is_none() || !is_usable(&result.unwrap()));
    }

    // --- legacy / regression tests kept from before median-cut ---

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
    fn usable_accepts_vivid_and_rejects_gray() {
        // A vivid red after OKLCH clamping should be usable.
        let vivid = oklch_clamp(Rgb {
            r: 220,
            g: 90,
            b: 40,
        })
        .expect("orange not gray");
        assert!(is_usable(&vivid));
        // Pure mid-gray is not usable.
        assert!(!is_usable(&Rgb {
            r: 128,
            g: 128,
            b: 128
        }));
    }

    #[test]
    fn accent_css_overrides_when_usable_and_is_empty_otherwise() {
        // A vivid, clamped color should produce a CSS override.
        let vivid = oklch_clamp(Rgb {
            r: 220,
            g: 90,
            b: 40,
        })
        .expect("orange not gray");
        let css = accent_css(Some(vivid));
        assert!(
            css.contains("@define-color reprise_player_accent"),
            "expected CSS override, got: {css:?}"
        );
        assert!(accent_css(None).is_empty());
        // Pure gray should clear to empty.
        assert!(accent_css(Some(Rgb {
            r: 128,
            g: 128,
            b: 128
        }))
        .is_empty());
    }

    #[test]
    fn lerp_interpolates_correctly() {
        assert_eq!(lerp(0, 200, 0.0), 0);
        assert_eq!(lerp(0, 200, 1.0), 200);
        assert_eq!(lerp(0, 200, 0.5), 100);
        assert_eq!(lerp(100, 200, 0.5), 150);
    }

    #[test]
    fn fade_to_theme_fallback_clears_cover_override_at_endpoint() {
        let cover = Some(Rgb {
            r: 200,
            g: 80,
            b: 40,
        });
        let fallback = Rgb {
            r: 51,
            g: 201,
            b: 163,
        };

        assert!(accent_during_fade(cover, None, fallback, 0.5).is_some());
        assert_eq!(accent_during_fade(cover, None, fallback, 1.0), None);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mot_6_replacing_an_accent_fade_skips_the_previous_animation() {
        gtk4::init().unwrap();
        let settings = gtk4::Settings::default().unwrap();
        let previous_setting = settings.is_gtk_enable_animations();
        settings.set_gtk_enable_animations(true);
        let label = gtk4::Label::new(None);
        let red = Some(Rgb {
            r: 220,
            g: 40,
            b: 40,
        });
        let blue = Some(Rgb {
            r: 40,
            g: 40,
            b: 220,
        });
        let green = Some(Rgb {
            r: 40,
            g: 220,
            b: 40,
        });

        cross_fade_accent(red, blue, &label);
        let first = CURRENT_ANIMATION.with(|slot| slot.borrow().as_ref().unwrap().clone());
        cross_fade_accent(blue, green, &label);

        assert_eq!(first.state(), libadwaita::AnimationState::Finished);
        CURRENT_ANIMATION.with(|slot| {
            let animation = slot.borrow();
            let animation = animation.as_ref().unwrap();
            assert_eq!(animation.duration(), motion::AMBIENT_MS);
            assert_eq!(animation.easing(), motion::AMBIENT_EASING);
            assert!(animation.follows_enable_animations_setting());
        });

        settings.set_gtk_enable_animations(previous_setting);
    }
}
