//! Named dark theme palettes for the redesign.
//!
//! Each theme is a [`Palette`] that overrides libadwaita's named colors via
//! `@define-color` (see [`theme_css`]). Because every Adwaita widget resolves
//! those named colors at draw time, swapping the palette recolors the whole
//! app at once — the mechanism the redesign's live theme picker will drive.
//!
//! The dark palettes follow design frame 14a's surface hierarchy: the central
//! table is darkest, side panels sit one step above it, and the header bar is
//! another step brighter. Cards remain brighter than their panel surface.
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
    /// Teal selection/toggle accent.
    pub(in crate::ui) accent: &'static str,
    pub(in crate::ui) accent_fg: &'static str,
    /// Warm play/waveform accent — the static fallback until the P5
    /// cover-derived accent subsystem drives it per track.
    pub(in crate::ui) player_accent: &'static str,
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
                popover_bg: "#404650",
                dialog_bg: "#353b44",
                fg: "#e7e9ec",
                dim_fg: "#9198a0",
                accent: "#33c9a3",
                accent_fg: "#04140f",
                player_accent: "#e8703a",
            },
            Theme::NightTerrain => Palette {
                window_bg: "#13161c",
                view_bg: "#191d25",
                card_bg: "#252b37",
                headerbar_bg: "#242a35",
                sidebar_bg: "#20252f",
                popover_bg: "#3e4452",
                dialog_bg: "#333a48",
                fg: "#e4e7ec",
                dim_fg: "#8b93a1",
                accent: "#4db6a9",
                accent_fg: "#05130f",
                player_accent: "#d98a3d",
            },
            Theme::MutedBloom => Palette {
                window_bg: "#1a1518",
                view_bg: "#201a1e",
                card_bg: "#2d252c",
                headerbar_bg: "#2c242c",
                sidebar_bg: "#282027",
                popover_bg: "#463c48",
                dialog_bg: "#3a343c",
                fg: "#ece6ea",
                dim_fg: "#a2949c",
                accent: "#c98bd0",
                accent_fg: "#180612",
                player_accent: "#e08a5a",
            },
        }
    }

    pub(in crate::ui) fn light_palette(self) -> Palette {
        let dark = self.palette();
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
                accent: dark.accent,
                accent_fg: dark.accent_fg,
                player_accent: dark.player_accent,
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
                accent: dark.accent,
                accent_fg: dark.accent_fg,
                player_accent: dark.player_accent,
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
                accent: dark.accent,
                accent_fg: dark.accent_fg,
                player_accent: dark.player_accent,
            },
        }
    }
}

/// Produces the `@define-color` overrides that map `theme`'s palette onto the
/// libadwaita named colors, plus two `reprise_*` colors the app's own CSS
/// reads. Installed at application priority so it wins over Adwaita's defaults.
pub(in crate::ui) fn theme_css(theme: Theme, is_dark: bool) -> String {
    let p = if is_dark {
        theme.palette()
    } else {
        theme.light_palette()
    };
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
         @define-color accent_bg_color {acc};\n\
         @define-color accent_fg_color {accfg};\n\
         @define-color accent_color {acc};\n\
         @define-color reprise_dim_fg_color {dim};\n\
         @define-color reprise_player_accent {play};\n",
        win = p.window_bg,
        fg = p.fg,
        view = p.view_bg,
        hb = p.headerbar_bg,
        sb = p.sidebar_bg,
        card = p.card_bg,
        pop = p.popover_bg,
        dlg = p.dialog_bg,
        acc = p.accent,
        accfg = p.accent_fg,
        dim = p.dim_fg,
        play = p.player_accent,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn theme_css_defines_core_named_colors() {
        let css = theme_css(Theme::PerpetualRain, true);
        for name in [
            "@define-color window_bg_color",
            "@define-color view_bg_color",
            "@define-color accent_bg_color",
            "@define-color reprise_player_accent",
        ] {
            assert!(css.contains(name), "missing color definition: {name}");
        }
    }

    #[test]
    fn distinct_themes_produce_distinct_css() {
        assert_ne!(
            theme_css(Theme::PerpetualRain, true),
            theme_css(Theme::NightTerrain, true)
        );
        assert_ne!(
            theme_css(Theme::NightTerrain, true),
            theme_css(Theme::MutedBloom, true)
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
    fn dark_palettes_follow_14a_surface_hierarchy() {
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
        }
    }
}
