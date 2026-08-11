//! Fixed identity for music in the device-sync storage bar.

use super::accent::APP_ACCENT;

/// Music's dark-surface tone is the fixed Reprise teal. Its lowest contrast
/// against the three dark card surfaces is 8.23:1.
const MUSIC_DARK: &str = APP_ACCENT;
/// The brand teal itself is only 1.69:1 against the light card surface, so its
/// fixed light-mode counterpart is darkened to 5.02:1.
const MUSIC_LIGHT: &str = "#147C78";

/// Returns music's fixed identity tone for the current appearance.
///
/// This color deliberately does not belong to a theme
/// [`super::theme::Palette`] and does not follow the resolved accent source.
pub(in crate::ui) fn music_color(is_dark: bool) -> &'static str {
    if is_dark {
        MUSIC_DARK
    } else {
        MUSIC_LIGHT
    }
}

pub(in crate::ui) const fn music_css_class() -> &'static str {
    "reprise-sync-category-music"
}

pub(super) fn css() -> String {
    format!(
        ".{} {{ color: @reprise_sync_music_color; }}\n",
        music_css_class(),
    )
}

pub(super) fn theme_definitions(is_dark: bool) -> String {
    format!(
        "@define-color reprise_sync_music_color {};\n",
        music_color(is_dark),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn music_tone_matches_the_device_design_without_following_the_system_accent() {
        assert_eq!(music_color(true), APP_ACCENT);
        assert_eq!(music_color(false), "#147C78");
    }

    #[test]
    fn music_icon_class_resolves_through_its_mode_aware_role() {
        let css = css();
        let class = "reprise-sync-category-music";
        let role = "reprise_sync_music_color";
        assert_eq!(music_css_class(), class);
        assert!(css.contains(&format!(".{class} {{ color: @{role}; }}")));
        for is_dark in [true, false] {
            assert!(theme_definitions(is_dark)
                .contains(&format!("@define-color {role} {};", music_color(is_dark))));
        }
    }

    #[test]
    fn music_tone_keeps_text_level_contrast_on_every_card_surface() {
        for theme in super::super::theme::Theme::all() {
            for (is_dark, card_bg) in [
                (true, theme.palette().card_bg),
                (false, theme.light_palette().card_bg),
            ] {
                let ratio = contrast_ratio(music_color(is_dark), card_bg);
                assert!(
                    ratio >= 4.5,
                    "music is only {ratio:.2}:1 on {theme:?}'s card in dark={is_dark}"
                );
            }
        }
    }

    fn contrast_ratio(first: &str, second: &str) -> f64 {
        let (lighter, darker) = {
            let first = luminance(first);
            let second = luminance(second);
            if first > second {
                (first, second)
            } else {
                (second, first)
            }
        };
        (lighter + 0.05) / (darker + 0.05)
    }

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
}
