//! Appearance-aware values for the named colours emitted by `theme_css`.

use super::tokens as t;

pub(super) struct ThemeTokens {
    pub(super) hairline: &'static str,
    pub(super) hairline_strong: &'static str,
    pub(super) rule: &'static str,
    pub(super) pill_border: &'static str,
    pub(super) pill_bg: &'static str,
    pub(super) hover_bg: String,
    pub(super) now_playing_tint: String,
    pub(super) now_playing_glow: String,
    pub(super) cover_edge: String,
    pub(super) cover_shadow: String,
    pub(super) tab_active_bg: String,
    pub(super) tab_active_shadow: String,
    pub(super) toggle_checked_fill: String,
    pub(super) play_glow_near: String,
    pub(super) play_glow_far: String,
    pub(super) play_glow_near_hover: String,
    pub(super) play_glow_far_hover: String,
    pub(super) play_ring: String,
    pub(super) play_drop: String,
    pub(super) play_drop_hover: String,
}

impl ThemeTokens {
    pub(super) fn for_appearance(is_dark: bool) -> Self {
        let select = |dark, light| if is_dark { dark } else { light };
        Self {
            hairline: select(t::HAIRLINE_DARK, t::HAIRLINE_LIGHT),
            hairline_strong: select(t::HAIRLINE_STRONG_DARK, t::HAIRLINE_STRONG_LIGHT),
            rule: select(t::RULE_DARK, t::RULE_LIGHT),
            pill_border: select(t::PILL_BORDER_DARK, t::PILL_BORDER_LIGHT),
            pill_bg: select(t::PILL_BG_DARK, t::PILL_BG_LIGHT),
            hover_bg: if is_dark {
                format!("alpha(@accent_bg_color, {})", t::HOVER_BG_ALPHA)
            } else {
                format!("alpha(@window_fg_color, {})", t::HOVER_BG_LIGHT_ALPHA)
            },
            now_playing_tint: if is_dark {
                format!("alpha(@accent_color, {})", t::NOW_PLAYING_TINT_DARK_ALPHA)
            } else {
                format!(
                    "alpha(@accent_bg_color, {})",
                    t::NOW_PLAYING_TINT_LIGHT_ALPHA
                )
            },
            now_playing_glow: format!(
                "alpha(@reprise_player_accent, {})",
                select(t::NOW_PLAYING_GLOW_ALPHA, t::NOW_PLAYING_GLOW_LIGHT_ALPHA)
            ),
            cover_edge: if is_dark {
                format!("alpha(@sidebar_fg_color, {})", t::COVER_EDGE_DARK_ALPHA)
            } else {
                format!("alpha(@window_fg_color, {})", t::COVER_EDGE_LIGHT_ALPHA)
            },
            cover_shadow: format!(
                "alpha(#000000, {})",
                select(t::COVER_SHADOW_DARK_ALPHA, t::COVER_SHADOW_LIGHT_ALPHA)
            ),
            tab_active_bg: if is_dark {
                format!(
                    "alpha(@sidebar_fg_color, {})",
                    t::NOW_PLAYING_PILL_ACTIVE_ALPHA
                )
            } else {
                t::NOW_PLAYING_TAB_ACTIVE_BG_LIGHT.to_owned()
            },
            tab_active_shadow: format!(
                "alpha(#000000, {})",
                select(
                    t::TAB_ACTIVE_SHADOW_DARK_ALPHA,
                    t::TAB_ACTIVE_SHADOW_LIGHT_ALPHA,
                )
            ),
            toggle_checked_fill: format!(
                "alpha(@accent_bg_color, {})",
                select(t::BTN_CHECKED_FILL_ALPHA, t::BTN_CHECKED_FILL_LIGHT_ALPHA)
            ),
            play_glow_near: format!(
                "alpha(@reprise_player_accent, {})",
                select(t::PLAY_GLOW_NEAR_DARK_ALPHA, t::PLAY_GLOW_NEAR_LIGHT_ALPHA)
            ),
            play_glow_far: format!(
                "alpha(@reprise_player_accent, {})",
                select(t::PLAY_GLOW_FAR_DARK_ALPHA, t::PLAY_GLOW_FAR_LIGHT_ALPHA)
            ),
            play_glow_near_hover: format!(
                "alpha(@reprise_player_accent, {})",
                select(
                    t::PLAY_GLOW_NEAR_HOVER_DARK_ALPHA,
                    t::PLAY_GLOW_NEAR_HOVER_LIGHT_ALPHA,
                )
            ),
            play_glow_far_hover: format!(
                "alpha(@reprise_player_accent, {})",
                select(
                    t::PLAY_GLOW_FAR_HOVER_DARK_ALPHA,
                    t::PLAY_GLOW_FAR_HOVER_LIGHT_ALPHA,
                )
            ),
            play_ring: format!(
                "alpha(@window_fg_color, {})",
                select(t::PLAY_RING_DARK_ALPHA, t::PLAY_RING_LIGHT_ALPHA)
            ),
            play_drop: format!(
                "alpha(#000000, {})",
                select(t::PLAY_DROP_DARK_ALPHA, t::PLAY_DROP_LIGHT_ALPHA)
            ),
            play_drop_hover: format!(
                "alpha(#000000, {})",
                select(
                    t::PLAY_DROP_HOVER_DARK_ALPHA,
                    t::PLAY_DROP_HOVER_LIGHT_ALPHA,
                )
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{accent::AccentSource, theme};

    #[test]
    fn dark_appearance_tokens_reproduce_every_replaced_literal() {
        let definitions = [
            "@define-color reprise_pill_bg @sidebar_bg_color;",
            "@define-color reprise_hover_bg alpha(@accent_bg_color, 0.10);",
            "@define-color reprise_now_playing_tint alpha(@accent_color, 0.09);",
            "@define-color reprise_now_playing_glow alpha(@reprise_player_accent, 0.15);",
            "@define-color reprise_cover_edge alpha(@sidebar_fg_color, 0.12);",
            "@define-color reprise_cover_shadow alpha(#000000, 0);",
            "@define-color reprise_tab_active_bg alpha(@sidebar_fg_color, 0.14);",
            "@define-color reprise_tab_active_shadow alpha(#000000, 0);",
            "@define-color reprise_toggle_checked_fill alpha(@accent_bg_color, 0.18);",
            "@define-color reprise_play_glow_near alpha(@reprise_player_accent, 0.60);",
            "@define-color reprise_play_glow_far alpha(@reprise_player_accent, 0.35);",
            "@define-color reprise_play_glow_near_hover alpha(@reprise_player_accent, 0.75);",
            "@define-color reprise_play_glow_far_hover alpha(@reprise_player_accent, 0.48);",
            "@define-color reprise_play_ring alpha(@window_fg_color, 0);",
            "@define-color reprise_play_drop alpha(#000000, 0.36);",
            "@define-color reprise_play_drop_hover alpha(#000000, 0.34);",
        ];
        for selected_theme in theme::Theme::all() {
            for source in [AccentSource::App, AccentSource::System] {
                let css = theme::theme_css(selected_theme, true, source);
                for definition in definitions {
                    assert!(
                        css.contains(definition),
                        "{selected_theme:?} {source:?}: {definition}"
                    );
                }
            }
        }
    }
}
