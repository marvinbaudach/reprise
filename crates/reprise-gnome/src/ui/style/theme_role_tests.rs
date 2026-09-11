use super::*;
use crate::ui::style::accent::AccentSource;

#[test]
fn contrast_1_secondary_text_meets_ratio() {
    fn rgb(hex: &str) -> [f64; 3] {
        let hex = hex.strip_prefix('#').expect("palette color starts with #");
        [0, 2, 4].map(|offset| {
            f64::from(u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap()) / 255.0
        })
    }

    fn linear(channel: f64) -> f64 {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }

    fn luminance(color: [f64; 3]) -> f64 {
        0.2126 * linear(color[0]) + 0.7152 * linear(color[1]) + 0.0722 * linear(color[2])
    }

    fn contrast(foreground: &str, background: &str, alpha: f64) -> f64 {
        let foreground = rgb(foreground);
        let background = rgb(background);
        let composed =
            [0, 1, 2].map(|index| foreground[index] * alpha + background[index] * (1.0 - alpha));
        let (lighter, darker) = {
            let foreground = luminance(composed);
            let background = luminance(background);
            if foreground > background {
                (foreground, background)
            } else {
                (background, foreground)
            }
        };
        (lighter + 0.05) / (darker + 0.05)
    }

    const SECONDARY_ALPHA: f64 = 0.70;
    const MINIMUM_RATIO: f64 = 4.5;
    for theme in Theme::all() {
        for (is_dark, palette) in [(true, theme.palette()), (false, theme.light_palette())] {
            let css = theme_css(theme, is_dark, AccentSource::App);
            assert!(css.contains(&format!(
                "@define-color reprise_primary_fg_color alpha({}, 0.95);",
                palette.fg
            )));
            assert!(css.contains(&format!(
                "@define-color reprise_secondary_fg_color alpha({}, 0.7);",
                palette.fg
            )));
            let tertiary = if is_dark { 0.70 } else { 0.65 };
            assert!(css.contains(&format!(
                "@define-color reprise_tertiary_fg_color alpha({}, {tertiary});",
                palette.fg
            )));
            assert!(css.contains(&format!(
                "@define-color reprise_hint_fg_color alpha({}, 0.5);",
                palette.fg
            )));
            for (role, surface) in [
                ("status line", palette.sidebar_bg),
                ("column headers", palette.view_bg),
                ("sidebar sections", palette.sidebar_bg),
                ("card metadata", palette.card_bg),
                ("popover content", palette.popover_bg),
                ("dialog content", palette.dialog_bg),
            ] {
                let ratio = contrast(palette.fg, surface, SECONDARY_ALPHA);
                assert!(
                    ratio >= MINIMUM_RATIO,
                    "{theme:?} {role} contrast {ratio:.2}:1 is below {MINIMUM_RATIO}:1"
                );
            }
        }
    }
}

#[test]
fn contrast_3_secondary_surfaces_use_verified_level() {
    // Per selector, not per module. Asking only whether the role appears
    // *somewhere* in a stylesheet made this test blind: reverting
    // `.new-release-header` to a local `opacity: 0.55` — the original
    // 3.62:1 bug — left it green, because sibling classes in the same
    // module still mentioned the role.
    for (role, css, selector) in [
        (
            "status line",
            crate::ui::track_content::css(),
            ".reprise-list-status-bar",
        ),
        (
            "column headers",
            crate::ui::track_list_header_style::css(),
            "> header label",
        ),
        (
            "sidebar sections",
            crate::ui::library_chrome::css(),
            ".reprise-library-sidebar .caption-heading",
        ),
        (
            "updates section headers",
            crate::ui::updates::css(),
            ".new-release-header",
        ),
    ] {
        let rules = css
            .split(selector)
            .nth(1)
            .and_then(|rest| rest.split('}').next())
            .unwrap_or_else(|| panic!("{role}: no rules for {selector}"));

        assert!(
            rules.contains("@reprise_secondary_fg_color"),
            "{role} ({selector}) did not consume the verified secondary level"
        );
        assert!(
            !rules.contains("opacity:"),
            "{role} ({selector}) dims text locally instead of using the level"
        );
    }

    // NR-34 deliberately gives the compact Updates metadata a stronger
    // 0.78 level while retaining the verified secondary colour role.
    let updates_css = crate::ui::updates::css();
    let meta = updates_css
        .split(".new-release-meta")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .expect("updates card meta rules");
    assert!(meta.contains("@reprise_secondary_fg_color"));
    assert!(meta.contains("opacity: 0.78"));
}
