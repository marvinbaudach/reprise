//! OKLab/OKLCH colour maths for the cover palette.
//!
//! Split out of `cover_accent.rs` so the extraction, the provider and the
//! palette each stay reviewable on their own. Nothing here touches GTK or any
//! global state — it is pure arithmetic over sRGB bytes.

use super::color_math::{from_linear, linear_rgb_to_oklab, oklab_to_linear_rgb, to_linear};

/// An extracted 8-bit color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) struct Rgb {
    pub(in crate::ui) r: u8,
    pub(in crate::ui) g: u8,
    pub(in crate::ui) b: u8,
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
}
