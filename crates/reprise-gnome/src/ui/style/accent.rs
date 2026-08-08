use std::cell::Cell;

/// Reprise's brand accent: the thick barline of the repeat-sign logo.
///
/// Lifted at compile time out of `data/brand/palette.toml` by the crate's
/// build script, so the brand has exactly one maintained source. It stays a
/// plain constant — the accent is needed while the theme CSS is built at
/// startup, long before anything may touch the filesystem.
pub(in crate::ui) const APP_ACCENT: &str = env!("REPRISE_APP_ACCENT");

/// Dark foreground for text and glyphs on [`APP_ACCENT`]. Its 11.16:1 WCAG
/// contrast ratio is comfortably above the 4.5:1 AA requirement for text.
const APP_ACCENT_FG: &str = "#04140f";

/// Settings key persisting the selected [`AccentSource`].
pub(in crate::ui) const ACCENT_SOURCE_SETTING_KEY: &str = "ui.accent-source";

/// The source used for libadwaita's accent roles and Rust-side accent readers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum AccentSource {
    App,
    System,
}

thread_local! {
    static CURRENT_SOURCE: Cell<AccentSource> = const { Cell::new(AccentSource::DEFAULT) };
}

impl AccentSource {
    /// A fresh install starts with Reprise's own brand accent.
    pub(in crate::ui) const DEFAULT: Self = Self::App;

    /// Stable persistence key.
    pub(in crate::ui) const fn id(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::System => "system",
        }
    }

    /// Restores a persisted source, falling back safely for unknown values.
    pub(in crate::ui) fn from_id(id: &str) -> Self {
        match id {
            "system" => Self::System,
            "app" => Self::App,
            _ => Self::DEFAULT,
        }
    }
}

pub(in crate::ui) fn current() -> AccentSource {
    CURRENT_SOURCE.with(Cell::get)
}

/// The effective color for every Rust-side accent reader.
pub(in crate::ui) fn accent_rgba() -> gtk4::gdk::RGBA {
    match current() {
        AccentSource::App => gtk4::gdk::RGBA::parse(APP_ACCENT)
            .expect("the compile-time Reprise accent must be valid #RRGGBB"),
        AccentSource::System => libadwaita::StyleManager::default().accent_color_rgba(),
    }
}

/// The AA threshold every accent foreground must clear (CONTRAST-5).
pub(in crate::ui::style) const ACCENT_TEXT_MINIMUM_RATIO: f64 = 4.5;

fn accent_text_rgb(accent: &str, background: &str, is_dark: bool) -> [u8; 3] {
    use super::color_math::{ensure_contrast_by_lightness, max_contrast_monochrome, parse_hex_rgb};

    let accent = parse_hex_rgb(accent).expect("accent color must use #RRGGBB");
    let background = parse_hex_rgb(background).expect("surface color must use #RRGGBB");
    // A sufficiently saturated accent cannot always reach the ratio by
    // lightness alone. Legibility outranks staying on-hue, so that case drops
    // the tint entirely rather than shipping text nobody can read.
    ensure_contrast_by_lightness(accent, background, is_dark, ACCENT_TEXT_MINIMUM_RATIO)
        .unwrap_or_else(|| max_contrast_monochrome(background))
}

fn rgb_hex(color: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
}

/// Text/glyph accent derived from the effective accent for this appearance.
/// Unlike libadwaita's surface-oriented accent roles, this role is measured
/// directly against the appearance's critical surface before it enters
/// app-authored CSS.
pub(super) fn accent_text_color(source: AccentSource, background: &str, is_dark: bool) -> String {
    let accent = match source {
        AccentSource::App => APP_ACCENT.to_owned(),
        AccentSource::System if gtk4::is_initialized_main_thread() => {
            let rgba = libadwaita::StyleManager::default().accent_color_rgba();
            rgb_hex([
                (rgba.red() * 255.0).round() as u8,
                (rgba.green() * 255.0).round() as u8,
                (rgba.blue() * 255.0).round() as u8,
            ])
        }
        // `theme_css` is also a headless pure-test seam. Installed theme CSS
        // is only built after GTK initialization, where the branch above reads
        // the actual system RGBA and tracks later change notifications.
        AccentSource::System => APP_ACCENT.to_owned(),
    };
    rgb_hex(accent_text_rgb(&accent, background, is_dark))
}

pub(super) fn set_current(source: AccentSource) {
    CURRENT_SOURCE.with(|current| current.set(source));
}

/// Foreground for an app-authored accent surface. System accents leave this
/// role to libadwaita's matching `accent_fg_color`.
pub(in crate::ui) const fn accent_fg(source: AccentSource) -> Option<&'static str> {
    match source {
        AccentSource::App => Some(APP_ACCENT_FG),
        AccentSource::System => None,
    }
}

/// CSS definitions owned by the selected source. The System choice returns no
/// definitions so libadwaita's named accent colors remain authoritative.
pub(super) fn css_overrides(source: AccentSource) -> String {
    match accent_fg(source) {
        Some(foreground) => format!(
            "@define-color accent_bg_color {APP_ACCENT};\n\
             @define-color accent_fg_color {foreground};\n\
             @define-color accent_color {APP_ACCENT};\n"
        ),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(rgb: [u8; 3]) -> f64 {
        let linear = |channel: u8| {
            let channel = f64::from(channel) / 255.0;
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2])
    }

    fn contrast(foreground: [u8; 3], background: [u8; 3]) -> f64 {
        let foreground = luminance(foreground);
        let background = luminance(background);
        let (lighter, darker) = if foreground > background {
            (foreground, background)
        } else {
            (background, foreground)
        };
        (lighter + 0.05) / (darker + 0.05)
    }

    #[test]
    fn accent_source_ids_round_trip_and_unknown_ids_use_the_default() {
        for source in [AccentSource::App, AccentSource::System] {
            assert_eq!(AccentSource::from_id(source.id()), source);
        }
        assert_eq!(
            AccentSource::from_id("does-not-exist"),
            AccentSource::DEFAULT
        );
        assert_eq!(AccentSource::DEFAULT, AccentSource::App);
        assert_eq!(ACCENT_SOURCE_SETTING_KEY, "ui.accent-source");
        assert_eq!(accent_fg(AccentSource::App), Some("#04140f"));
        assert_eq!(accent_fg(AccentSource::System), None);
    }

    /// Proves the derivation rather than restating the colour: the build
    /// script lifts [`APP_ACCENT`] out of the brand palette, and this reads
    /// the same file back. The parse is deliberately written out again here
    /// so a bug in the build script cannot vouch for itself.
    #[test]
    fn app_accent_is_the_teal_the_brand_palette_declares() {
        const PALETTE: &str = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/brand/palette.toml"
        ));

        let declared = PALETTE
            .lines()
            .filter_map(|line| line.split_once('='))
            .find(|(key, _)| key.trim() == "reprise_teal")
            .map(|(_, value)| value.trim().trim_matches('"'))
            .expect("the brand palette declares reprise_teal");

        assert_eq!(APP_ACCENT, declared);
    }

    #[test]
    fn app_accent_foreground_meets_wcag_aa_for_text() {
        fn luminance(hex: &str) -> f64 {
            let hex = hex.strip_prefix('#').expect("color uses #RRGGBB");
            let linear = |offset| {
                let channel = f64::from(
                    u8::from_str_radix(&hex[offset..offset + 2], 16)
                        .expect("color uses hexadecimal channels"),
                ) / 255.0;
                if channel <= 0.04045 {
                    channel / 12.92
                } else {
                    ((channel + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(0) + 0.7152 * linear(2) + 0.0722 * linear(4)
        }

        let foreground = luminance(APP_ACCENT_FG);
        let background = luminance(APP_ACCENT);
        let ratio = (background + 0.05) / (foreground + 0.05);
        assert!(ratio >= 4.5, "app accent contrast is only {ratio:.2}:1");
        assert!((ratio - 11.164).abs() < 0.001);
    }

    #[test]
    fn contrast_5_derived_accent_text_meets_ratio_on_every_live_surface() {
        use super::super::color_math::parse_hex_rgb;
        use super::super::theme::Theme;

        // Read the palettes live. An earlier version copied the hex values into
        // this test, which would have kept passing through the very palette
        // retune the module doc says is coming.
        let accents = [
            APP_ACCENT, // the brand teal
            "#3584e4",  // GNOME blue
            "#f8f7ff",  // a near-white system accent
            "#101318",  // a near-black system accent
            "#ff3bd4",  // saturated magenta: the case lightness alone cannot fix
        ];

        for theme in Theme::all() {
            for (is_dark, palette) in [(true, theme.palette()), (false, theme.light_palette())] {
                for surface in palette.surfaces() {
                    let background = parse_hex_rgb(surface).expect("palette surface is valid hex");
                    for accent in accents {
                        // Every surface, not just the critical one: the role is
                        // painted app-wide, and the critical surface is only the
                        // worst case it must survive.
                        let critical = palette.critical_accent_surface(is_dark);
                        let derived = accent_text_rgb(accent, critical, is_dark);
                        let ratio = contrast(derived, background);
                        assert!(
                            ratio >= ACCENT_TEXT_MINIMUM_RATIO,
                            "{theme:?} {}: {accent} derived against {critical} \
                             reaches only {ratio:.2}:1 on {surface}",
                            if is_dark { "dark" } else { "light" }
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn contrast_5_the_app_accent_survives_unchanged_where_it_already_passes() {
        use super::super::color_math::parse_hex_rgb;
        use super::super::theme::Theme;

        let palette = Theme::PerpetualRain.palette();
        assert_eq!(
            accent_text_rgb(APP_ACCENT, palette.critical_accent_surface(true), true),
            parse_hex_rgb(APP_ACCENT).expect("the brand accent is valid hex"),
            "the brand teal already clears every dark surface and must not be retinted"
        );
    }

    #[test]
    fn app_source_rgba_is_the_logo_teal() {
        let previous = current();
        set_current(AccentSource::App);
        let accent = accent_rgba();
        set_current(previous);

        assert_eq!(accent.to_string(), "rgb(79,219,212)");
    }
}
