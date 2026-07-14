//! Cover-derived player accent ("Follow album cover").
//!
//! The dominant, colorful tone of the current album cover is extracted
//! off-main and installed as an override of `@reprise_player_accent` in a
//! dedicated high-priority provider — so the waveform and the play button
//! (which read that color) take on the album's hue, while selection and
//! toggles (which read `@accent_color`) keep the theme's teal.
//!
//! When a cover is missing, grayscale, or too dark/washed-out to read on the
//! dark surfaces, the override is cleared and the theme's own
//! `@reprise_player_accent` (the "Petrol" fallback) applies again.

use std::cell::RefCell;

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

/// Saturation-weighted average of `pixels` (contiguous, `channels` per pixel),
/// so colorful pixels dominate over gray ones. `None` when nothing colorful
/// enough contributes (e.g. a grayscale cover).
fn dominant_accent(pixels: &[u8], channels: usize) -> Option<Rgb> {
    if channels < 3 {
        return None;
    }
    let (mut wr, mut wg, mut wb, mut wsum) = (0f64, 0f64, 0f64, 0f64);
    for px in pixels.chunks_exact(channels) {
        if channels >= 4 && px[3] < 128 {
            continue; // skip largely transparent pixels
        }
        let (r, g, b) = (f64::from(px[0]), f64::from(px[1]), f64::from(px[2]));
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let saturation = if max <= 0.0 { 0.0 } else { (max - min) / max };
        let weight = saturation * saturation; // emphasise vivid pixels
        wr += weight * r;
        wg += weight * g;
        wb += weight * b;
        wsum += weight;
    }
    if wsum < 1.0 {
        return None;
    }
    Some(Rgb {
        r: (wr / wsum).round() as u8,
        g: (wg / wsum).round() as u8,
        b: (wb / wsum).round() as u8,
    })
}

/// Whether `color` is saturated and light enough to read as an accent on the
/// redesign's dark surfaces.
fn is_usable(color: &Rgb) -> bool {
    let max = f64::from(color.r.max(color.g).max(color.b));
    let min = f64::from(color.r.min(color.g).min(color.b));
    let saturation = if max <= 0.0 { 0.0 } else { (max - min) / max };
    let value = max / 255.0;
    saturation >= 0.25 && value >= 0.40
}

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

/// Extracts the dominant accent from a cover image file. Runs off-main (decodes
/// a scaled pixbuf and reads its pixels); returns a `Send` [`Rgb`] for the main
/// thread to apply via [`set_cover_accent`]. `None` on any decode failure or a
/// non-colorful cover.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(r: u8, g: u8, b: u8, count: usize) -> Vec<u8> {
        std::iter::repeat([r, g, b]).take(count).flatten().collect()
    }

    #[test]
    fn dominant_accent_returns_the_vivid_color() {
        let pixels = solid(220, 90, 40, 64); // warm orange
        assert_eq!(
            dominant_accent(&pixels, 3),
            Some(Rgb {
                r: 220,
                g: 90,
                b: 40
            })
        );
    }

    #[test]
    fn grayscale_cover_has_no_accent() {
        let pixels = solid(128, 128, 128, 64);
        assert_eq!(dominant_accent(&pixels, 3), None);
    }

    #[test]
    fn vivid_pixels_outweigh_gray_ones() {
        let mut pixels = solid(130, 130, 130, 60); // mostly gray
        pixels.extend(solid(40, 200, 120, 4)); // a few vivid teal
        let accent = dominant_accent(&pixels, 3).expect("some accent");
        assert!(accent.g > accent.r && accent.g > accent.b, "{accent:?}");
    }

    #[test]
    fn usable_rejects_dark_and_washed_out_colors() {
        assert!(is_usable(&Rgb {
            r: 220,
            g: 90,
            b: 40
        }));
        assert!(!is_usable(&Rgb {
            r: 30,
            g: 28,
            b: 26
        })); // too dark
        assert!(!is_usable(&Rgb {
            r: 180,
            g: 178,
            b: 176
        })); // too gray
    }

    #[test]
    fn accent_css_overrides_when_usable_and_is_empty_otherwise() {
        let css = accent_css(Some(Rgb {
            r: 220,
            g: 90,
            b: 40,
        }));
        assert!(css.contains("@define-color reprise_player_accent #dc5a28;"));
        assert!(accent_css(None).is_empty());
        assert!(accent_css(Some(Rgb {
            r: 20,
            g: 20,
            b: 20
        }))
        .is_empty());
    }
}
