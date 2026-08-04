//! Cover-derived player accent ("Follow album cover").
//!
//! The dominant, colorful tone of the current album cover is extracted off-main
//! and installed in a dedicated high-priority provider as two colours — so the
//! waveform and the play button take on the album's hue, while selection and
//! toggles (which read `@accent_color`) keep the theme accent.
//!
//! When a cover is missing, grayscale, or too dark/washed-out to read on the
//! dark surfaces, the override is cleared and the theme's own
//! `@reprise_player_accent` applies again. That static fallback is the selected
//! theme's accent; the palette remains its single source of truth.

use std::cell::RefCell;

#[cfg(test)]
use super::cover_accent_oklab::oklch_clamp;
use super::cover_accent_oklab::{is_usable, oklch_light};
pub(in crate::ui) use super::cover_accent_oklab::{scale_chroma, Rgb};

// ---------------------------------------------------------------------------
// CSS provider
// ---------------------------------------------------------------------------

/// The `@define-color` override for `palette`, or empty (fall back to the theme's
/// own colours) when there is no usable cover accent.
///
/// `reprise_player_accent` is ink: it fills the waveform and the play button, so
/// it stays inside the muted chroma band those controls were tuned for.
/// `reprise_cover_light` is the same hue lifted into the light band — it is a
/// translucent seam on the cover's edge, and at the accent's chroma it measured
/// as invisible on real artwork.
fn accent_css(accent: Option<Rgb>) -> String {
    match accent {
        Some(accent) if is_usable(&accent) => {
            let light = oklch_light(accent);
            format!(
                "@define-color reprise_player_accent #{:02x}{:02x}{:02x};\n\
                 @define-color reprise_cover_light #{:02x}{:02x}{:02x};",
                accent.r, accent.g, accent.b, light.r, light.g, light.b,
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
pub(in crate::ui) fn set_cover_accent(accent: Option<Rgb>) {
    ACCENT_PROVIDER.with(|slot| {
        if let Some(provider) = slot.borrow().as_ref() {
            provider.load_from_string(&accent_css(accent));
        }
    });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chroma_scaling_is_draw_local_and_leaves_provider_state_untouched() {
        ACCENT_PROVIDER.with(|slot| slot.borrow_mut().take());

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
        // The seam's colour rides along: same hue, chroma lifted, because the
        // accent's own band is tuned for ink and is invisible as a one-pixel
        // translucent line.
        assert!(css.contains("@define-color reprise_cover_light"));
        assert!(accent_css(None).is_empty());
        // Pure gray should clear to empty.
        assert!(accent_css(Some(Rgb {
            r: 128,
            g: 128,
            b: 128,
        }))
        .is_empty());
    }

    // MOT-1/MOT-3: the accent provider carries the colour and nothing else. It
    // deliberately imposes no duration, because the properties that carry the
    // accent (background-color, box-shadow) are the same ones the accent-bearing
    // widgets already transition at their own token — the play button declares
    // Micro for exactly these. A CSS property has one transition, and it cannot
    // tell an accent change from a hover, so a duration written here would
    // override the widget's own token instead of adding to it.
    #[test]
    fn mot_1_accent_provider_imposes_no_duration_of_its_own() {
        let css = accent_css(Some(Rgb {
            r: 46,
            g: 200,
            b: 166,
        }));

        assert!(css.contains("@define-color reprise_player_accent #2ec8a6"));
        assert!(
            !css.contains("transition"),
            "the accent provider must not override the tokens its consumers declare: {css:?}"
        );
        assert!(
            accent_css(None).is_empty(),
            "clearing the override must leave nothing behind"
        );
    }
}
