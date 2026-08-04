//! Toolkit-neutral OKLab and OKLCH colour arithmetic.

/// An extracted 8-bit sRGB colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Normalized sRGB channel to linear light.
pub fn srgb_channel_to_linear(channel: f64) -> f64 {
    let channel = channel.clamp(0.0, 1.0);
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear-light channel to normalized sRGB.
pub fn linear_channel_to_srgb(channel: f64) -> f64 {
    let srgb = if channel <= 0.003_130_8 {
        channel * 12.92
    } else {
        1.055 * channel.powf(1.0 / 2.4) - 0.055
    };
    srgb.clamp(0.0, 1.0)
}

/// Eight-bit sRGB channel to linear light.
pub fn to_linear(channel: u8) -> f64 {
    srgb_channel_to_linear(f64::from(channel) / 255.0)
}

/// Linear light to an eight-bit sRGB channel.
pub fn from_linear(channel: f64) -> u8 {
    (linear_channel_to_srgb(channel) * 255.0).round() as u8
}

/// Linear RGB to OKLab `(L, a, b)`.
pub fn linear_rgb_to_oklab(lr: f64, lg: f64, lb: f64) -> (f64, f64, f64) {
    let l = 0.412_221_470_8 * lr + 0.536_332_536_3 * lg + 0.051_445_992_9 * lb;
    let m = 0.211_903_498_2 * lr + 0.680_699_545_1 * lg + 0.107_396_956_6 * lb;
    let s = 0.088_302_461_9 * lr + 0.281_718_837_6 * lg + 0.629_978_700_5 * lb;

    let l = l.cbrt();
    let m = m.cbrt();
    let s = s.cbrt();

    (
        0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
    )
}

/// OKLab `(L, a, b)` to linear RGB.
pub fn oklab_to_linear_rgb(lab_l: f64, lab_a: f64, lab_b: f64) -> (f64, f64, f64) {
    let l = lab_l + 0.396_337_777_3 * lab_a + 0.215_803_757_9 * lab_b;
    let m = lab_l - 0.105_561_346_2 * lab_a - 0.063_854_174_7 * lab_b;
    let s = lab_l - 0.089_484_177_5 * lab_a - 1.291_485_548_0 * lab_b;

    let l = l * l * l;
    let m = m * m * m;
    let s = s * s * s;

    (
        4.076_741_661_3 * l - 3.307_711_590_8 * m + 0.230_969_929_5 * s,
        -1.268_437_973_0 * l + 2.609_757_401_1 * m - 0.341_319_427_9 * s,
        -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s,
    )
}

/// Normalized sRGB to OKLab `(L, a, b)`.
pub fn srgb_to_oklab(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    linear_rgb_to_oklab(
        srgb_channel_to_linear(r),
        srgb_channel_to_linear(g),
        srgb_channel_to_linear(b),
    )
}

/// OKLab `(L, a, b)` to normalized sRGB.
pub fn oklab_to_srgb(l: f64, a: f64, b: f64) -> (f64, f64, f64) {
    let (r, g, b) = oklab_to_linear_rgb(l, a, b);
    (
        linear_channel_to_srgb(r),
        linear_channel_to_srgb(g),
        linear_channel_to_srgb(b),
    )
}

/// Scales an sRGB colour's OKLCH chroma while preserving lightness and hue.
pub fn scale_chroma(r: f64, g: f64, b: f64, factor: f64) -> (f64, f64, f64) {
    let (l, a, b) = srgb_to_oklab(r, g, b);
    let factor = factor.clamp(0.0, 1.0);
    oklab_to_srgb(l, a * factor, b * factor)
}

pub const CHROMA_FLOOR: f64 = 0.08;
pub const CHROMA_CEIL: f64 = 0.13;

pub fn oklch_clamp(color: Rgb) -> Option<Rgb> {
    let (l, a, b) = linear_rgb_to_oklab(to_linear(color.r), to_linear(color.g), to_linear(color.b));
    let c = (a * a + b * b).sqrt();
    if c < 0.03 {
        return None;
    }
    let hue = b.atan2(a);
    let chroma = c.clamp(CHROMA_FLOOR, CHROMA_CEIL);
    let (r, g, b) =
        oklab_to_linear_rgb(l.clamp(0.55, 0.75), chroma * hue.cos(), chroma * hue.sin());
    Some(Rgb {
        r: from_linear(r),
        g: from_linear(g),
        b: from_linear(b),
    })
}

pub const LIGHT_CHROMA_FLOOR: f64 = 0.16;
pub const LIGHT_CHROMA_CEIL: f64 = 0.30;

pub fn oklch_light(color: Rgb) -> Rgb {
    let (l, a, b) = linear_rgb_to_oklab(to_linear(color.r), to_linear(color.g), to_linear(color.b));
    let chroma = (a * a + b * b).sqrt();
    let hue = b.atan2(a);
    let chroma = chroma.clamp(LIGHT_CHROMA_FLOOR, LIGHT_CHROMA_CEIL);
    let (r, g, b) = oklab_to_linear_rgb(l, chroma * hue.cos(), chroma * hue.sin());
    Rgb {
        r: from_linear(r),
        g: from_linear(g),
        b: from_linear(b),
    }
}

pub fn is_usable(color: &Rgb) -> bool {
    let (_, a, b) = linear_rgb_to_oklab(to_linear(color.r), to_linear(color.g), to_linear(color.b));
    (a * a + b * b).sqrt() >= 0.03
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_lifts_low_chroma_to_the_floor() {
        let chroma_of = |colour: Rgb| {
            let (_, a, b) = linear_rgb_to_oklab(
                to_linear(colour.r),
                to_linear(colour.g),
                to_linear(colour.b),
            );
            (a * a + b * b).sqrt()
        };
        let (r, g, b) = oklab_to_linear_rgb(0.65, 0.05 * 0.707, 0.05 * 0.707);
        let muted = Rgb {
            r: from_linear(r),
            g: from_linear(g),
            b: from_linear(b),
        };
        assert!((0.03..CHROMA_FLOOR).contains(&chroma_of(muted)));
        let output = oklch_clamp(muted).expect("usable colour");
        assert!(chroma_of(output) >= CHROMA_FLOOR - 0.005);
    }

    #[test]
    fn clamp_limits_lightness_and_chroma() {
        let clamped = oklch_clamp(Rgb { r: 255, g: 0, b: 0 }).expect("usable red");
        assert!(clamped.r > 100 && clamped.r < 230);
    }

    #[test]
    fn usability_accepts_vivid_and_rejects_gray() {
        let vivid = oklch_clamp(Rgb {
            r: 220,
            g: 90,
            b: 40,
        })
        .expect("usable orange");
        assert!(is_usable(&vivid));
        assert!(!is_usable(&Rgb {
            r: 128,
            g: 128,
            b: 128
        }));
    }
}
