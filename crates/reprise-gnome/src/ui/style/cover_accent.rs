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

use gtk4::prelude::IsA;
use libadwaita::prelude::AnimationExt;

use crate::ui::motion;

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
pub(in crate::ui) fn set_cover_accent(accent: Option<Rgb>) {
    ACCENT_PROVIDER.with(|slot| {
        if let Some(provider) = slot.borrow().as_ref() {
            provider.load_from_string(&accent_css(accent));
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn lerp_interpolates_correctly() {
        assert_eq!(lerp(0, 200, 0.0), 0);
        assert_eq!(lerp(0, 200, 1.0), 200);
        assert_eq!(lerp(0, 200, 0.5), 100);
        assert_eq!(lerp(100, 200, 0.5), 150);
    }

    #[test]
    fn fade_to_theme_fallback_clears_all_three_overrides_at_the_endpoint() {
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
