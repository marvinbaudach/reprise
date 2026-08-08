//! Pure color transformations used while drawing accent-colored widgets.

/// sRGB channel (0–255) to linear light.
pub(in crate::ui::style) fn to_linear(channel: u8) -> f64 {
    let channel = f64::from(channel) / 255.0;
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light to an sRGB channel (0–255), clamped.
pub(in crate::ui::style) fn from_linear(channel: f64) -> u8 {
    let srgb = if channel <= 0.003_130_8 {
        channel * 12.92
    } else {
        1.055 * channel.powf(1.0 / 2.4) - 0.055
    };
    (srgb.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// WCAG relative luminance for an opaque sRGB color.
pub(in crate::ui::style) fn relative_luminance(color: [u8; 3]) -> f64 {
    0.2126 * to_linear(color[0]) + 0.7152 * to_linear(color[1]) + 0.0722 * to_linear(color[2])
}

/// WCAG contrast ratio between two opaque sRGB colors.
pub(in crate::ui::style) fn contrast_ratio(first: [u8; 3], second: [u8; 3]) -> f64 {
    let first = relative_luminance(first);
    let second = relative_luminance(second);
    let (lighter, darker) = if first > second {
        (first, second)
    } else {
        (second, first)
    };
    (lighter + 0.05) / (darker + 0.05)
}

/// Linear RGB to OKLab `(L, a, b)`.
pub(in crate::ui::style) fn linear_rgb_to_oklab(
    linear_red: f64,
    linear_green: f64,
    linear_blue: f64,
) -> (f64, f64, f64) {
    let l = 0.412_221_470_8 * linear_red
        + 0.536_332_536_3 * linear_green
        + 0.051_445_992_9 * linear_blue;
    let m = 0.211_903_498_2 * linear_red
        + 0.680_699_545_1 * linear_green
        + 0.107_396_956_6 * linear_blue;
    let s = 0.088_302_461_9 * linear_red
        + 0.281_718_837_6 * linear_green
        + 0.629_978_700_5 * linear_blue;

    let l = l.cbrt();
    let m = m.cbrt();
    let s = s.cbrt();

    (
        0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
    )
}

/// OKLab `(L, a, b)` to linear RGB `(r, g, b)`.
pub(in crate::ui::style) fn oklab_to_linear_rgb(
    lightness: f64,
    green_red: f64,
    blue_yellow: f64,
) -> (f64, f64, f64) {
    let l = lightness + 0.396_337_777_3 * green_red + 0.215_803_757_9 * blue_yellow;
    let m = lightness - 0.105_561_346_2 * green_red - 0.063_854_174_7 * blue_yellow;
    let s = lightness - 0.089_484_177_5 * green_red - 1.291_485_548_0 * blue_yellow;

    let l = l * l * l;
    let m = m * m * m;
    let s = s * s * s;

    (
        4.076_741_661_3 * l - 3.307_711_590_8 * m + 0.230_969_929_5 * s,
        -1.268_437_973_0 * l + 2.609_757_401_1 * m - 0.341_319_427_9 * s,
        -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s,
    )
}

/// Parses a `#RRGGBB` string. Palette and accent values are authored as hex,
/// so every consumer of those needs this.
pub(in crate::ui::style) fn parse_hex_rgb(hex: &str) -> Option<[u8; 3]> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    Some([
        u8::from_str_radix(&hex[0..2], 16).ok()?,
        u8::from_str_radix(&hex[2..4], 16).ok()?,
        u8::from_str_radix(&hex[4..6], 16).ok()?,
    ])
}

/// Black or white, whichever contrasts more against `background`. This is the
/// most readable foreground that exists for a surface, so it is what callers
/// fall back to when tinted text cannot reach their threshold at all.
pub(in crate::ui::style) fn max_contrast_monochrome(background: [u8; 3]) -> [u8; 3] {
    const WHITE: [u8; 3] = [255, 255, 255];
    const BLACK: [u8; 3] = [0, 0, 0];
    if contrast_ratio(WHITE, background) >= contrast_ratio(BLACK, background) {
        WHITE
    } else {
        BLACK
    }
}

/// Moves only OKLab lightness until `color` reaches `minimum_ratio` against
/// `background`. Dark appearances search toward white; light appearances
/// search toward black. A color that already passes is returned unchanged.
///
/// Returns `None` when lightness alone cannot get there. That is not a
/// theoretical case: chroma is held constant while lightness moves, so a
/// saturated color leaves the sRGB gamut and gets channel-clamped, and the
/// clamped endpoint can fall short of the target. Binary-searching against an
/// endpoint that never satisfies the predicate would return a failing color
/// while looking successful — the one outcome a contrast guarantee must not
/// produce.
pub(in crate::ui::style) fn ensure_contrast_by_lightness(
    color: [u8; 3],
    background: [u8; 3],
    lighten: bool,
    minimum_ratio: f64,
) -> Option<[u8; 3]> {
    if contrast_ratio(color, background) >= minimum_ratio {
        return Some(color);
    }

    let (lightness, green_red, blue_yellow) = linear_rgb_to_oklab(
        to_linear(color[0]),
        to_linear(color[1]),
        to_linear(color[2]),
    );
    let candidate = |candidate_lightness: f64| {
        let (red, green, blue) = oklab_to_linear_rgb(candidate_lightness, green_red, blue_yellow);
        [from_linear(red), from_linear(green), from_linear(blue)]
    };

    let extreme = if lighten { 1.0 } else { 0.0 };
    if contrast_ratio(candidate(extreme), background) < minimum_ratio {
        return None;
    }

    let (mut failing, mut passing) = (lightness, extreme);
    for _ in 0..32 {
        let middle = (failing + passing) / 2.0;
        if contrast_ratio(candidate(middle), background) >= minimum_ratio {
            passing = middle;
        } else {
            failing = middle;
        }
    }
    Some(candidate(passing))
}

/// Scales an sRGB color's OKLCH chroma while preserving lightness and hue.
/// Inputs and outputs use Cairo's normalized 0..1 channel range.
pub(in crate::ui) fn scale_chroma(red: f64, green: f64, blue: f64, factor: f64) -> (f64, f64, f64) {
    let to_channel = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    let (lightness, green_red, blue_yellow) = linear_rgb_to_oklab(
        to_linear(to_channel(red)),
        to_linear(to_channel(green)),
        to_linear(to_channel(blue)),
    );
    let factor = factor.clamp(0.0, 1.0);
    let (linear_red, linear_green, linear_blue) =
        oklab_to_linear_rgb(lightness, green_red * factor, blue_yellow * factor);
    (
        f64::from(from_linear(linear_red)) / 255.0,
        f64::from(from_linear(linear_green)) / 255.0,
        f64::from(from_linear(linear_blue)) / 255.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lightness_search_reports_failure_instead_of_returning_a_failing_colour() {
        // A saturated magenta against a mid-tone pink: holding chroma while
        // lightness climbs leaves the sRGB gamut, so even pure-white lightness
        // clamps to a colour that only reaches ~2.5:1. The old search trusted
        // that endpoint and returned it as if it had succeeded.
        let accent = [251, 8, 255];
        let surface = [221, 40, 104];
        assert!(
            contrast_ratio(accent, surface) < 4.5,
            "premise: starts short"
        );

        assert_eq!(
            ensure_contrast_by_lightness(accent, surface, true, 4.5),
            None,
            "lightness alone cannot reach the ratio here and must not pretend otherwise"
        );

        // The fallback is what a caller uses instead, and it is genuinely the
        // best available foreground for that surface.
        let fallback = max_contrast_monochrome(surface);
        assert!(contrast_ratio(fallback, surface) >= contrast_ratio(accent, surface));
    }

    #[test]
    fn lightness_search_result_always_meets_the_requested_ratio() {
        // Sweep instead of spot-checking: every returned colour must clear the
        // bar, and every `None` must be a case where even the extreme fails.
        for red in (0..=255u16).step_by(51) {
            for green in (0..=255u16).step_by(51) {
                for blue in (0..=255u16).step_by(51) {
                    let colour = [red as u8, green as u8, blue as u8];
                    for surface in [[47, 53, 61], [232, 234, 237], [128, 128, 128]] {
                        for lighten in [true, false] {
                            match ensure_contrast_by_lightness(colour, surface, lighten, 4.5) {
                                Some(derived) => assert!(
                                    contrast_ratio(derived, surface) >= 4.5,
                                    "{colour:?} on {surface:?} returned {derived:?} below 4.5"
                                ),
                                None => {
                                    let extreme = if lighten { [255, 255, 255] } else { [0, 0, 0] };
                                    assert!(
                                        contrast_ratio(extreme, surface) < 4.5
                                            || contrast_ratio(colour, surface) < 4.5,
                                        "{colour:?} on {surface:?} gave up too early"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn parsing_rejects_anything_that_is_not_six_hex_digits() {
        assert_eq!(parse_hex_rgb("#4FDBD4"), Some([0x4f, 0xdb, 0xd4]));
        for malformed in ["4FDBD4", "#4FD", "#4FDBD", "#4FDBD44", "#GGGGGG", ""] {
            assert_eq!(parse_hex_rgb(malformed), None, "accepted {malformed:?}");
        }
    }

    #[test]
    fn chroma_scaling_is_pure_draw_local_math() {
        let original = (0.8, 0.2, 0.1);
        let unchanged = scale_chroma(original.0, original.1, original.2, 1.0);
        assert!((unchanged.0 - original.0).abs() <= 1.0 / 255.0);
        assert!((unchanged.1 - original.1).abs() <= 1.0 / 255.0);
        assert!((unchanged.2 - original.2).abs() <= 1.0 / 255.0);

        let gray = scale_chroma(original.0, original.1, original.2, 0.0);
        assert!((gray.0 - gray.1).abs() <= 1.0 / 255.0);
        assert!((gray.1 - gray.2).abs() <= 1.0 / 255.0);
    }
}
