//! OKLab/OKLCH colour maths for the cover palette.
//!
//! Split out of `cover_accent.rs` so the extraction, the provider and the
//! palette each stay reviewable on their own. Nothing here touches GTK or any
//! global state — it is pure arithmetic over sRGB bytes.

/// An extracted 8-bit color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) struct Rgb {
    pub(in crate::ui) r: u8,
    pub(in crate::ui) g: u8,
    pub(in crate::ui) b: u8,
}

/// sRGB channel (0–255) → linear light.
pub(in crate::ui::style) fn to_linear(channel: u8) -> f64 {
    let c = f64::from(channel) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light → sRGB channel (0–255), clamped.
pub(in crate::ui::style) fn from_linear(c: f64) -> u8 {
    let srgb = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (srgb.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Linear RGB → OKLab `(L, a, b)`.
pub(in crate::ui::style) fn linear_rgb_to_oklab(lr: f64, lg: f64, lb: f64) -> (f64, f64, f64) {
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
pub(in crate::ui::style) fn oklab_to_linear_rgb(
    lab_l: f64,
    lab_a: f64,
    lab_b: f64,
) -> (f64, f64, f64) {
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

/// Chroma is clamped into this band. The floor lifts washed-out or dark covers
/// (e.g. a dark red album sleeve) to a saturated accent instead of a muted
/// near-gray tan; the ceiling keeps vivid covers from glaring. A cover whose
/// dominant chroma is below the near-gray gate (0.03) gets no accent at all —
/// the theme fallback applies instead.
pub(in crate::ui::style) const CHROMA_FLOOR: f64 = 0.08;
pub(in crate::ui::style) const CHROMA_CEIL: f64 = 0.13;

/// Converts `color` to OKLCH, clamps L to [0.55, 0.75] and C into
/// `[CHROMA_FLOOR, CHROMA_CEIL]`, and returns the result as `Rgb`. Returns
/// `None` if chroma < 0.03 (near-gray — use the theme fallback instead).
pub(in crate::ui::style) fn oklch_clamp(color: Rgb) -> Option<Rgb> {
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

/// Chroma band for surfaces that are *light* rather than ink.
///
/// [`CHROMA_CEIL`] is tuned for the player accent, which fills the waveform and
/// the play button: at 0.13 a vivid cover cannot glare in a control the eye
/// rests on. The cover's edge seam is the opposite case — one translucent pixel
/// at 18–50 % alpha, lying on the artwork's own edge above a dark panel. At the
/// accent's chroma it measured as invisible on real covers, which is why it
/// gets its own, wider band instead of the seam simply being drawn brighter:
/// more alpha of a muted tone is still a muted tone.
pub(in crate::ui::style) const LIGHT_CHROMA_FLOOR: f64 = 0.16;
pub(in crate::ui::style) const LIGHT_CHROMA_CEIL: f64 = 0.30;

/// The same colour as `color`, lifted into the light band: hue kept, lightness
/// kept, chroma renormalised into `[LIGHT_CHROMA_FLOOR, LIGHT_CHROMA_CEIL]`.
///
/// Deliberately takes an already-clamped accent rather than a raw bucket
/// average: hue is the part that has to come from the artwork, and re-reading
/// the pixels for one more colour would be a second source for the same fact.
pub(in crate::ui::style) fn oklch_light(color: Rgb) -> Rgb {
    let (l, a, b) = linear_rgb_to_oklab(to_linear(color.r), to_linear(color.g), to_linear(color.b));
    let c = (a * a + b * b).sqrt();
    let h = b.atan2(a);
    let c_light = c.clamp(LIGHT_CHROMA_FLOOR, LIGHT_CHROMA_CEIL);
    let (lr, lg, lb) = oklab_to_linear_rgb(l, c_light * h.cos(), c_light * h.sin());
    Rgb {
        r: from_linear(lr),
        g: from_linear(lg),
        b: from_linear(lb),
    }
}

/// Whether `color` is colorful enough to use as an accent. Uses OKLCH chroma
/// (C ≥ 0.03) — effectively the same gate `oklch_clamp` applies, expressed
/// as a predicate so callers that already have an `Rgb` can check it without
/// re-running the full conversion.
pub(in crate::ui::style) fn is_usable(color: &Rgb) -> bool {
    let lr = to_linear(color.r);
    let lg = to_linear(color.g);
    let lb = to_linear(color.b);
    let (_, a, b) = linear_rgb_to_oklab(lr, lg, lb);
    let chroma = (a * a + b * b).sqrt();
    chroma >= 0.03
}

/// OKLCH hue of a colour, in radians.
pub(in crate::ui::style) fn hue_of(color: Rgb) -> f64 {
    let (_, a, b) = linear_rgb_to_oklab(to_linear(color.r), to_linear(color.g), to_linear(color.b));
    b.atan2(a)
}

/// Shortest angular distance between two hues, in radians (0..=π).
pub(in crate::ui::style) fn hue_distance(a: f64, b: f64) -> f64 {
    let distance = (a - b).rem_euclid(std::f64::consts::TAU);
    distance.min(std::f64::consts::TAU - distance)
}

/// `color` rotated by `radians` in OKLCH, keeping L and C.
pub(in crate::ui::style) fn hue_rotated(color: Rgb, radians: f64) -> Rgb {
    let (l, a, b) = linear_rgb_to_oklab(to_linear(color.r), to_linear(color.g), to_linear(color.b));
    let chroma = (a * a + b * b).sqrt();
    let hue = b.atan2(a) + radians;
    let (r, g, b) = oklab_to_linear_rgb(l, chroma * hue.cos(), chroma * hue.sin());
    Rgb {
        r: from_linear(r),
        g: from_linear(g),
        b: from_linear(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
