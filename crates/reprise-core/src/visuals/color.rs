//! Color conversion utilities: HSL ↔ RGB, hue shifts, and cover palette extraction.

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

pub fn secondary_accent(
    rgba: &[u8],
    pixel_count: usize,
    primary: (f32, f32, f32),
) -> Option<(f32, f32, f32)> {
    #[derive(Default, Clone, Copy)]
    struct Bucket {
        w: f32,
        r: f32,
        g: f32,
        b: f32,
    }
    let mut buckets = [Bucket::default(); 12];
    for pixel in 0..pixel_count.min(rgba.len() / 4) {
        let o = pixel * 4;
        let (r, g, b) = (
            f32::from(rgba[o]),
            f32::from(rgba[o + 1]),
            f32::from(rgba[o + 2]),
        );
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let sat = if max > 0.0 { (max - min) / max } else { 0.0 };
        let value = max / 255.0;
        let weight = sat.powf(1.6) * value * value;
        if weight < 0.01 {
            continue;
        }
        let hue = rgb_hue((r / 255.0, g / 255.0, b / 255.0));
        let bucket = &mut buckets[((hue / 30.0) as usize).min(11)];
        bucket.w += weight;
        bucket.r += r * weight;
        bucket.g += g * weight;
        bucket.b += b * weight;
    }
    let primary_bucket = ((rgb_hue(primary) / 30.0) as usize).min(11);
    let top_weight = buckets.iter().map(|b| b.w).fold(0.0_f32, f32::max);
    if top_weight < 0.5 {
        return None;
    }
    let mut order: Vec<usize> = (0..12).filter(|&i| buckets[i].w > 0.0).collect();
    order.sort_by(|&a, &b| buckets[b].w.total_cmp(&buckets[a].w));
    order
        .into_iter()
        .find(|&i| {
            let distance = (i as i32 - primary_bucket as i32).unsigned_abs() as usize;
            distance.min(12 - distance) >= 2 && buckets[i].w >= top_weight * 0.16
        })
        .map(|i| {
            let bucket = buckets[i];
            let (r, g, b) = (
                bucket.r / bucket.w,
                bucket.g / bucket.w,
                bucket.b / bucket.w,
            );
            let max = r.max(g).max(b);
            let k = if max > 0.0 { 208.0 / max } else { 1.0 };
            (
                (r * k).min(255.0) / 255.0,
                (g * k).min(255.0) / 255.0,
                (b * k).min(255.0) / 255.0,
            )
        })
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
    fn secondary_accent_finds_a_distinct_second_hue() {
        // Half saturated red pixels, half saturated cyan-blue: primary red → secondary ≈ blue.
        let mut rgba = Vec::new();
        for _ in 0..64 {
            rgba.extend_from_slice(&[220, 30, 30, 255]);
        }
        for _ in 0..40 {
            rgba.extend_from_slice(&[30, 120, 220, 255]);
        }
        let secondary = secondary_accent(&rgba, 104, (0.86, 0.12, 0.12)).expect("distinct hue");
        let hue = rgb_hue(secondary);
        assert!((170.0..=250.0).contains(&hue), "got {hue}");
    }

    #[test]
    fn secondary_accent_none_for_monochrome_covers() {
        let mut rgba = Vec::new();
        for _ in 0..100 {
            rgba.extend_from_slice(&[200, 40, 40, 255]);
        }
        assert!(secondary_accent(&rgba, 100, (0.78, 0.16, 0.16)).is_none());
    }
}
