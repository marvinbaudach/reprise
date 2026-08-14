//! Named dark theme palettes for the redesign.
//!
//! Each theme is a [`Palette`] for surface and text colors. [`theme_css`]
//! combines that palette with the selected app or system accent source. Since
//! every Adwaita widget resolves these named colors at draw time, either choice
//! recolors the whole app immediately without coupling accent to album art.
//!
//! The dark palettes follow design frame 14a's surface hierarchy: the central
//! table is darkest, side panels sit one step above it, and the header bar is
//! another step brighter. Cards remain brighter than their panel surface;
//! popovers sit between cards and dialogs.
//!
//! The concrete color values below are extracted from the design frames and
//! are deliberately approximate; a later pass tunes them against the exact
//! canonical palettes (Perpetual Rain, Night Terrain, …).

/// Settings key persisting the selected theme's [`Theme::id`].
pub(in crate::ui) const THEME_SETTING_KEY: &str = "ui.theme";

/// A selectable dark theme. `id()` is the stable persistence key; the enum
/// order is the picker order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::ui) enum Theme {
    PerpetualRain,
    NightTerrain,
    MutedBloom,
}

/// The color values a theme maps onto libadwaita's named colors. Every field
/// is emitted by [`theme_css`] — keep them in sync so no value is dead.
pub(in crate::ui) struct Palette {
    pub(in crate::ui) window_bg: &'static str,
    pub(in crate::ui) view_bg: &'static str,
    pub(in crate::ui) card_bg: &'static str,
    pub(in crate::ui) headerbar_bg: &'static str,
    pub(in crate::ui) sidebar_bg: &'static str,
    pub(in crate::ui) popover_bg: &'static str,
    pub(in crate::ui) dialog_bg: &'static str,
    pub(in crate::ui) fg: &'static str,
    pub(in crate::ui) dim_fg: &'static str,
}

impl Palette {
    /// Every surface a widget can sit on, in no particular order.
    pub(in crate::ui::style) fn surfaces(&self) -> [&'static str; 7] {
        [
            self.window_bg,
            self.view_bg,
            self.card_bg,
            self.headerbar_bg,
            self.sidebar_bg,
            self.popover_bg,
            self.dialog_bg,
        ]
    }

    /// The surface an accent foreground contrasts *worst* against: the
    /// lightest one when the text is light-on-dark, the darkest one when it is
    /// dark-on-light. Derived by measuring the palette rather than by naming a
    /// field, so retuning a colour cannot silently invalidate the choice.
    ///
    /// Accent-tinted chip fills count as surfaces here even though they have no
    /// hex literal. The tint drags a surface far toward the accent — in the dark
    /// palettes it triples the luminance of the lightest plain surface — so chip
    /// text, not popover text, is the real worst case. Measuring only the named
    /// surfaces left accent text on chips at 3.4:1 while every test passed.
    pub(in crate::ui::style) fn critical_accent_surface(
        &self,
        is_dark: bool,
        accent: [u8; 3],
    ) -> [u8; 3] {
        use super::color_math::{composite, parse_hex_rgb, relative_luminance};

        let chip_tint: f64 = super::tokens::CHIP_BG_HOVER_ALPHA
            .parse()
            .expect("the chip tint token is a decimal fraction");

        self.surfaces()
            .into_iter()
            .flat_map(|hex| {
                let surface = parse_hex_rgb(hex).expect("palette colour must use #RRGGBB");
                [surface, composite(accent, surface, chip_tint)]
            })
            .reduce(|worst, surface| {
                let take_surface = if is_dark {
                    relative_luminance(surface) > relative_luminance(worst)
                } else {
                    relative_luminance(surface) < relative_luminance(worst)
                };
                if take_surface {
                    surface
                } else {
                    worst
                }
            })
            .expect("the palette always has surfaces")
    }
}

impl Theme {
    /// The theme a fresh install starts on.
    pub(in crate::ui) const DEFAULT: Theme = Theme::PerpetualRain;

    /// All themes in picker order.
    pub(in crate::ui) fn all() -> [Theme; 3] {
        [Theme::PerpetualRain, Theme::NightTerrain, Theme::MutedBloom]
    }

    /// Stable persistence key (never reused across renamed themes).
    pub(in crate::ui) fn id(self) -> &'static str {
        match self {
            Theme::PerpetualRain => "perpetual-rain",
            Theme::NightTerrain => "night-terrain",
            Theme::MutedBloom => "muted-bloom",
        }
    }

    /// Inverse of [`Self::id`]; unknown ids fall back to `None` so the caller can
    /// choose [`Self::DEFAULT`].
    pub(in crate::ui) fn from_id(id: &str) -> Option<Theme> {
        Theme::all().into_iter().find(|theme| theme.id() == id)
    }

    /// Human-readable name shown in the picker (English UI copy).
    pub(in crate::ui) fn display_name(self) -> &'static str {
        match self {
            Theme::PerpetualRain => "Perpetual Rain",
            Theme::NightTerrain => "Night Terrain",
            Theme::MutedBloom => "Muted Bloom",
        }
    }

    pub(in crate::ui) fn palette(self) -> Palette {
        match self {
            Theme::PerpetualRain => Palette {
                window_bg: "#16181b",
                view_bg: "#1b1e22",
                card_bg: "#272d33",
                headerbar_bg: "#262b31",
                sidebar_bg: "#22262b",
                popover_bg: "#2f353d",
                dialog_bg: "#353b44",
                fg: "#e7e9ec",
                dim_fg: "#9198a0",
            },
            Theme::NightTerrain => Palette {
                window_bg: "#13161c",
                view_bg: "#191d25",
                card_bg: "#252b37",
                headerbar_bg: "#242a35",
                sidebar_bg: "#20252f",
                popover_bg: "#2d3441",
                dialog_bg: "#333a48",
                fg: "#e4e7ec",
                dim_fg: "#8b93a1",
            },
            Theme::MutedBloom => Palette {
                window_bg: "#1a1518",
                view_bg: "#201a1e",
                card_bg: "#2d252c",
                headerbar_bg: "#2c242c",
                sidebar_bg: "#282027",
                popover_bg: "#362e37",
                dialog_bg: "#3a343c",
                fg: "#ece6ea",
                dim_fg: "#a2949c",
            },
        }
    }

    pub(in crate::ui) fn light_palette(self) -> Palette {
        match self {
            Theme::PerpetualRain => Palette {
                window_bg: "#f4f5f7",
                view_bg: "#fafbfc",
                card_bg: "#ffffff",
                headerbar_bg: "#e8eaed",
                sidebar_bg: "#eceef1",
                popover_bg: "#ffffff",
                dialog_bg: "#f0f2f5",
                fg: "#1a1c1f",
                dim_fg: "#6b7280",
            },
            Theme::NightTerrain => Palette {
                window_bg: "#f2f4f8",
                view_bg: "#f8f9fc",
                card_bg: "#ffffff",
                headerbar_bg: "#e4e8ef",
                sidebar_bg: "#e8ecf2",
                popover_bg: "#ffffff",
                dialog_bg: "#edf0f6",
                fg: "#181b22",
                dim_fg: "#636d7e",
            },
            Theme::MutedBloom => Palette {
                window_bg: "#f6f3f5",
                view_bg: "#fbf9fa",
                card_bg: "#ffffff",
                headerbar_bg: "#ede8eb",
                sidebar_bg: "#f0ebee",
                popover_bg: "#ffffff",
                dialog_bg: "#f2eef1",
                fg: "#1f1a1d",
                dim_fg: "#7a6e75",
            },
        }
    }
}

/// Produces the `@define-color` overrides that map `theme`'s palette onto the
/// libadwaita named colors, plus the `reprise_player_accent` alias the app's
/// own CSS reads. Installed at application priority so it wins over Adwaita's
/// defaults.
pub(in crate::ui) fn theme_css(
    theme: Theme,
    is_dark: bool,
    source: super::accent::AccentSource,
) -> String {
    use super::tokens::{HINT_TEXT_ALPHA, PRIMARY_TEXT_ALPHA, SECONDARY_TEXT_ALPHA};

    let p = if is_dark {
        theme.palette()
    } else {
        theme.light_palette()
    };
    let accent_css = super::accent::css_overrides(source);
    // Accent foregrounds are used app-wide, so the role is derived against the
    // palette's worst-case surface rather than against any one widget's.
    let accent = super::accent::effective_accent_rgb(source);
    let accent_text = super::accent::accent_text_color(
        accent,
        p.critical_accent_surface(is_dark, accent),
        is_dark,
    );
    let category_css = super::category_colors::theme_definitions(is_dark);
    format!(
        "@define-color window_bg_color {win};\n\
         @define-color window_fg_color {fg};\n\
         @define-color view_bg_color {view};\n\
         @define-color view_fg_color {fg};\n\
         @define-color headerbar_bg_color {hb};\n\
         @define-color headerbar_fg_color {fg};\n\
         @define-color sidebar_bg_color {sb};\n\
         @define-color sidebar_fg_color {fg};\n\
         @define-color card_bg_color {card};\n\
         @define-color card_fg_color {fg};\n\
         @define-color popover_bg_color {pop};\n\
         @define-color popover_fg_color {fg};\n\
         @define-color dialog_bg_color {dlg};\n\
         @define-color dialog_fg_color {fg};\n\
         {category_css}\
         {accent_css}\
         @define-color reprise_accent_text_color {accent_text};\n\
         @define-color reprise_primary_fg_color alpha({fg}, {primary_alpha});\n\
         @define-color reprise_secondary_fg_color alpha({fg}, {secondary_alpha});\n\
         @define-color reprise_hint_fg_color alpha({fg}, {hint_alpha});\n\
         @define-color reprise_dim_fg_color {dim};\n\
         @define-color reprise_player_accent @accent_color;\n",
        win = p.window_bg,
        fg = p.fg,
        view = p.view_bg,
        hb = p.headerbar_bg,
        sb = p.sidebar_bg,
        card = p.card_bg,
        pop = p.popover_bg,
        dlg = p.dialog_bg,
        category_css = category_css,
        accent_css = accent_css,
        accent_text = accent_text,
        primary_alpha = PRIMARY_TEXT_ALPHA,
        secondary_alpha = SECONDARY_TEXT_ALPHA,
        hint_alpha = HINT_TEXT_ALPHA,
        dim = p.dim_fg,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::style::accent::{AccentSource, APP_ACCENT};

    #[test]
    fn default_theme_is_listed() {
        assert!(Theme::all().contains(&Theme::DEFAULT));
    }

    #[test]
    fn ids_round_trip_and_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for theme in Theme::all() {
            assert_eq!(Theme::from_id(theme.id()), Some(theme));
            assert!(seen.insert(theme.id()), "duplicate id: {}", theme.id());
            assert!(!theme.display_name().is_empty());
        }
        assert_eq!(Theme::from_id("does-not-exist"), None);
    }

    #[test]
    fn app_accent_css_defines_the_brand_roles_and_player_alias() {
        let css = theme_css(Theme::PerpetualRain, true, AccentSource::App);
        for name in [
            "@define-color window_bg_color",
            "@define-color view_bg_color",
        ] {
            assert!(css.contains(name), "missing color definition: {name}");
        }
        // The accent roles are asserted through `APP_ACCENT`, not a repeated
        // literal: `data/brand/palette.toml` is the one maintained source.
        for definition in [
            format!("@define-color accent_color {APP_ACCENT};"),
            format!("@define-color accent_bg_color {APP_ACCENT};"),
            "@define-color accent_fg_color #04140f;".to_string(),
            "@define-color reprise_accent_text_color ".to_string(),
            "@define-color reprise_player_accent @accent_color;".to_string(),
        ] {
            assert!(
                css.contains(&definition),
                "missing definition: {definition}"
            );
        }
    }

    #[test]
    fn system_accent_css_leaves_adwaita_roles_undefined_and_keeps_player_alias() {
        let css = theme_css(Theme::PerpetualRain, true, AccentSource::System);
        for name in ["accent_color", "accent_bg_color", "accent_fg_color"] {
            assert!(
                !css.contains(&format!("@define-color {name}")),
                "system accent must leave {name} to libadwaita"
            );
        }
        assert!(css.contains("@define-color reprise_accent_text_color #"));
        assert!(css.contains("@define-color reprise_player_accent @accent_color;"));
    }

    #[test]
    fn distinct_themes_produce_distinct_css() {
        assert_ne!(
            theme_css(Theme::PerpetualRain, true, AccentSource::App),
            theme_css(Theme::NightTerrain, true, AccentSource::App)
        );
        assert_ne!(
            theme_css(Theme::NightTerrain, true, AccentSource::App),
            theme_css(Theme::MutedBloom, true, AccentSource::App)
        );
    }

    #[test]
    fn dialog_bg_is_distinct_from_card_and_window() {
        for theme in Theme::all() {
            let p = theme.palette();
            assert_ne!(p.dialog_bg, p.card_bg, "{theme:?} dialog_bg == card_bg");
            assert_ne!(p.dialog_bg, p.window_bg, "{theme:?} dialog_bg == window_bg");
        }
    }

    #[test]
    fn dark_surface_ladder_places_popovers_between_cards_and_dialogs() {
        fn luminance(hex: &str) -> f64 {
            let hex = hex.strip_prefix('#').expect("palette color starts with #");
            let linear = |offset| {
                let channel = f64::from(
                    u8::from_str_radix(&hex[offset..offset + 2], 16)
                        .expect("palette color uses hexadecimal channels"),
                ) / 255.0;
                if channel <= 0.04045 {
                    channel / 12.92
                } else {
                    ((channel + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(0) + 0.7152 * linear(2) + 0.0722 * linear(4)
        }

        for theme in Theme::all() {
            let palette = theme.palette();
            let card = luminance(palette.card_bg);
            let popover = luminance(palette.popover_bg);
            let dialog = luminance(palette.dialog_bg);

            assert!(
                card < popover,
                "{theme:?}: card must be darker than popover"
            );
            assert!(
                popover < dialog,
                "{theme:?}: popover must be darker than dialog"
            );
        }
    }

    #[test]
    fn style_2_side_surfaces_sit_above_the_table() {
        fn channel_sum(hex: &str) -> u16 {
            let hex = hex.strip_prefix('#').expect("palette color starts with #");
            [0, 2, 4]
                .into_iter()
                .map(|offset| u16::from_str_radix(&hex[offset..offset + 2], 16).unwrap())
                .sum()
        }

        for theme in Theme::all() {
            let p = theme.palette();
            let view = channel_sum(p.view_bg);
            let sidebar = channel_sum(p.sidebar_bg);
            let headerbar = channel_sum(p.headerbar_bg);
            let card = channel_sum(p.card_bg);

            assert!(
                view < sidebar,
                "{theme:?}: view must be darker than sidebar"
            );
            assert!(
                sidebar < headerbar,
                "{theme:?}: sidebar must be darker than headerbar"
            );
            assert!(
                sidebar < card,
                "{theme:?}: sidebar must be darker than cards"
            );

            let light = theme.light_palette();
            assert!(
                channel_sum(light.sidebar_bg) < channel_sum(light.view_bg),
                "{theme:?}: light sidebar must be darker than the table"
            );
        }
    }

    #[test]
    fn player_accent_alias_is_source_independent_in_both_appearances() {
        for theme in Theme::all() {
            for is_dark in [true, false] {
                for source in [AccentSource::App, AccentSource::System] {
                    let css = theme_css(theme, is_dark, source);
                    assert!(css.contains("@define-color reprise_player_accent @accent_color;"));
                }
            }
        }
    }

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
            let composed = [0, 1, 2]
                .map(|index| foreground[index] * alpha + background[index] * (1.0 - alpha));
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
}
