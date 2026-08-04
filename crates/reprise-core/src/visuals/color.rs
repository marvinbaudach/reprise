//! Color conversion utilities: HSL to RGB and hue shifts.

pub fn hsla_to_rgb(hue: f32, sat: f32, light: f32) -> (f32, f32, f32) {
    let hue = hue.rem_euclid(360.0);
    let c = (1.0 - (2.0 * light - 1.0).abs()) * sat;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = light - c / 2.0;
    let (r, g, b) = match (hue / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r + m, g + m, b + m)
}

pub fn rgb_hue(rgb: (f32, f32, f32)) -> f32 {
    let (r, g, b) = rgb;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    if delta < 0.001 {
        return 250.0;
    }
    let hue = if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    } * 60.0;
    hue.rem_euclid(360.0)
}

pub fn hue_shift(rgb: (f32, f32, f32), degrees: f32) -> (f32, f32, f32) {
    let (r, g, b) = rgb;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let light = (max + min) / 2.0;
    let sat = if max - min < 0.001 {
        0.0
    } else {
        (max - min) / (1.0 - (2.0 * light - 1.0).abs())
    };
    hsla_to_rgb(rgb_hue(rgb) + degrees, sat, light)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsla_roundtrip_and_hue_shift() {
        for hue in [0.0_f32, 60.0, 120.0, 200.0, 300.0] {
            let rgb = hsla_to_rgb(hue, 0.85, 0.6);
            let back = rgb_hue(rgb);
            let delta = (back - hue).abs().min(360.0 - (back - hue).abs());
            assert!(delta < 2.0, "hue {hue} → {back}");
            let shifted = rgb_hue(hue_shift(rgb, 42.0));
            let want = (hue + 42.0) % 360.0;
            let delta = (shifted - want).abs().min(360.0 - (shifted - want).abs());
            assert!(delta < 3.0, "shift {hue} → {shifted}, want {want}");
        }
    }

    #[test]
    fn visualizer_color_has_no_cover_palette_path() {
        let engine = include_str!("engine.rs");
        let color = include_str!("color.rs");
        for retired in [
            ["cover_", "accent2"].concat(),
            ["set_cover", "_pixels"].concat(),
            ["clear_", "cover"].concat(),
            ["secondary_", "accent"].concat(),
        ] {
            assert!(!engine.contains(&retired), "engine retained {retired}");
            assert!(!color.contains(&retired), "color module retained {retired}");
        }
    }
}
