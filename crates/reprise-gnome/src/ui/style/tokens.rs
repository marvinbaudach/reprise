//! Tunable design values shared by the app-authored CSS sections.
//!
//! Every alpha, thickness, and density height that a design pass would want
//! to adjust lives here; the structural selectors stay with the feature that
//! owns the CSS classes (see [`super::app_css`]'s section list).

/// Primary text alpha for titles, track names, and values.
pub(in crate::ui) const PRIMARY_TEXT_ALPHA: f64 = 0.95;

/// Secondary text alpha for artists, status, metadata, and column headings.
pub(in crate::ui) const SECONDARY_TEXT_ALPHA: f64 = 0.70;

/// Hint text alpha for placeholders and disabled secondary copy.
pub(in crate::ui) const HINT_TEXT_ALPHA: f64 = 0.50;

/// Resting background alpha for filter chips (over `@accent_bg_color`).
///
/// Bounded by the chip's own label: the fill drags the surface toward the
/// accent, and accent text on accent tint has nowhere to go. At the previous
/// 0.22/0.32 the label measured 4.17:1 and 3.37:1 — below AA. Lightening the
/// accent instead would need a near-white pastel that wrecks the brand colour
/// everywhere else, so the fill yields and the accent stays itself.
pub(in crate::ui) const CHIP_BG_ALPHA: &str = "0.14";

/// Hover background alpha for filter chips. See [`CHIP_BG_ALPHA`] for why this
/// is capped rather than chosen freely.
pub(in crate::ui) const CHIP_BG_HOVER_ALPHA: &str = "0.18";

/// Border alpha of the Layout preference preview cards (over
/// `@window_fg_color`).
pub(in crate::ui) const PREVIEW_BORDER_ALPHA: &str = "0.18";

/// Sidebar surface alpha inside the Layout preference preview cards.
pub(in crate::ui) const PREVIEW_SIDEBAR_ALPHA: &str = "0.16";

/// Content surface alpha inside the Layout preference preview cards.
pub(in crate::ui) const PREVIEW_CONTENT_ALPHA: &str = "0.06";

/// Thickness of the accent drop-position indicator used by both column-layout
/// and track-row reordering.
pub(in crate::ui) const DROP_INDICATOR_THICKNESS: &str = "2px";

/// Track-row content minimum height for the Comfortable density.
pub(in crate::ui) const ROW_MIN_HEIGHT_COMFORTABLE: i32 = 36;

/// Track-row content minimum height for the Standard density.
pub(in crate::ui) const ROW_MIN_HEIGHT_STANDARD: i32 = 28;

/// Track-row content minimum height for the Compact density.
///
/// This is the floor the cell content imposes, not a freely chosen one: the
/// rating stars and cover cannot shrink below it, so a smaller value would be
/// silently ignored. It sat at 12 for a while and never bound, which made
/// Compact 8px tighter than Standard while the token promised 16 — the density
/// test then hardcoded a pixel delta matching neither. Going below this needs
/// the cell content to shrink first.
pub(in crate::ui) const ROW_MIN_HEIGHT_COMPACT: i32 = 20;

/// Queue section-header content minimum height.
///
/// This is a floor, not a measured header height. It deliberately exceeds
/// the measured 34 px natural height of the Play Next button row so both the
/// button row and plain label bind to the same authored minimum. A theme or
/// large system font may still require more space; geometry measurement must
/// detect that instead of treating this token as truth.
pub(in crate::ui) const SECTION_HEADER_MIN_HEIGHT: i32 = 36;

/// Font size (px) applied to track-row text in the Compact density.
pub(in crate::ui) const COMPACT_ROW_FONT_SIZE: i32 = 10;

// --- Redesign interaction + surface vocabulary (see `super::interactions`) ---

/// Corner radius for layered redesign surfaces (cards, panels).
pub(in crate::ui) const RADIUS_SURFACE: &str = "12px";

/// Hover background alpha for flat interactive elements (over `@accent_bg_color`).
pub(in crate::ui) const HOVER_BG_ALPHA: &str = "0.10";

/// Stronger background alpha for active+hover panel toggle buttons.
pub(in crate::ui) const HOVER_BG_ALPHA_STRONG: &str = "0.18";

/// Blur radius of the accent focus glow on text inputs.
pub(in crate::ui) const FOCUS_GLOW_BLUR: &str = "10px";

/// Alpha of the accent focus glow (over `@accent_color`).
pub(in crate::ui) const FOCUS_GLOW_ALPHA: &str = "0.28";

/// Shared interaction transition (duration + easing) for hover/focus feedback.
#[derive(Clone, Copy, Debug)]
pub(in crate::ui) struct Transition;

impl std::fmt::Display for Transition {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}ms {}",
            crate::ui::motion::MICRO_MS,
            crate::ui::motion::MICRO_CSS_EASING
        )
    }
}

pub(in crate::ui) const TRANSITION: Transition = Transition;

// --- Button interaction states (see `super::buttons`, UX rules BTN-1..4) ---

/// Hover background alpha for flat/icon buttons, applied over `currentColor`
/// rather than over the accent or a literal white: an accent wash sinks into
/// the themed surface of the player bar, and a fixed white would be invisible on
/// the light palettes. BTN-4: measured on the tint, not on a null background.
pub(in crate::ui) const BTN_HOVER_ALPHA: &str = "0.08";

/// Pressed background alpha — the surface deepens as the button sinks.
pub(in crate::ui) const BTN_PRESS_ALPHA: &str = "0.14";

/// Press scale. The button sinks under the cursor so the click visibly lands.
pub(in crate::ui) const BTN_PRESS_SCALE: &str = "0.94";

/// Resting fill alpha of a checked toggle (over `@accent_bg_color`). Higher
/// than [`HOVER_BG_ALPHA`] so the on-state stays louder than any hover.
pub(in crate::ui) const BTN_CHECKED_FILL_ALPHA: &str = "0.22";

/// Checked + hover: brighter fill, same state display.
pub(in crate::ui) const BTN_CHECKED_FILL_HOVER_ALPHA: &str = "0.30";

/// Checked + pressed.
pub(in crate::ui) const BTN_CHECKED_FILL_PRESS_ALPHA: &str = "0.38";

/// Diameter of the on-state dot under a checked toggle — the second,
/// non-colour cue that keeps the state readable with colour vision deficiency.
pub(in crate::ui) const BTN_DOT_SIZE: &str = "4px";

/// Vertical placement of that dot, as a background-position percentage: just
/// inside the bottom edge, clear of a circular button's rounding.
pub(in crate::ui) const BTN_DOT_VERTICAL_POSITION: &str = "88%";

/// Keyboard focus ring width — focus is its own signal, never the hover fill.
pub(in crate::ui) const FOCUS_RING_WIDTH: &str = "2px";

/// Gap between the focus ring and the button edge.
pub(in crate::ui) const FOCUS_RING_OFFSET: &str = "1px";

/// Soft elevation shadow giving layered surfaces depth.
pub(in crate::ui) const SURFACE_SHADOW: &str = "0 2px 12px rgba(0, 0, 0, 0.28)";

/// Hairline border alpha for surfaces (over `@window_fg_color`).
pub(in crate::ui) const SURFACE_BORDER_ALPHA: &str = "0.08";

/// Stronger shadow for modal dialog surfaces (over the scrim).
pub(in crate::ui) const DIALOG_SHADOW: &str = "0 20px 60px rgba(0, 0, 0, 0.60)";

/// White hairline alpha for dialog borders (rgba white).
pub(in crate::ui) const DIALOG_BORDER_ALPHA: &str = "0.10";

/// Scrim alpha behind modal dialogs — darkens the main window so the dialog
/// pops (Libadwaita default is 0.35; we go slightly heavier for depth).
pub(in crate::ui) const SCRIM_ALPHA: &str = "0.55";

/// White tint alpha for dialog headerbars — one elevation step above the
/// dialog body (the "Dialog-Header" rung in the surface ladder).
pub(in crate::ui) const DIALOG_HEADER_TINT_ALPHA: &str = "0.04";

/// White tint alpha for card/list surfaces inside dialogs — higher than the
/// standard 5 % because the dialog body is already elevated.
pub(in crate::ui) const DIALOG_CARD_ALPHA: &str = "0.07";

// --- Now Playing panel (design 21a) ---

pub(in crate::ui) const NOW_PLAYING_COVER_SIZE: i32 = 168;
pub(in crate::ui) const NOW_PLAYING_GLOW_ALPHA: &str = "0.26";
pub(in crate::ui) const NOW_PLAYING_PILL_RADIUS: &str = "99px";
pub(in crate::ui) const NOW_PLAYING_PILL_BG_ALPHA: &str = "0.06";
pub(in crate::ui) const NOW_PLAYING_PILL_ACTIVE_ALPHA: &str = "0.14";
pub(in crate::ui) const NOW_PLAYING_TITLE_SIZE: &str = "15px";
pub(in crate::ui) const NOW_PLAYING_SUBTITLE_SIZE: &str = "12px";
pub(in crate::ui) const NOW_PLAYING_FOOTER_SIZE: &str = "10.5px";
pub(in crate::ui) const NOW_PLAYING_QUEUE_COVER_SIZE: i32 = 32;
pub(in crate::ui) const NOW_PLAYING_QUEUE_TITLE_SIZE: &str = "13.5px";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_css_uses_the_micro_motion_token() {
        assert_eq!(format!("{TRANSITION}"), "150ms ease-out");
    }

    #[test]
    fn contrast_3_hover_tints_leave_text_above_aa() {
        use super::super::color_math::{composite, contrast_ratio, parse_hex_rgb};
        use super::super::theme::Theme;

        // A hover tint lightens the surface *under* the text, so it eats into
        // every text level's headroom — the same text that clears 5.88:1 at
        // rest drops toward the floor once its row lights up. Measured in a
        // real menu, a hovered row cost about 1.15 points of ratio.
        //
        // Which colour the tint is made of decides how much it costs, so each
        // is modelled with its own: the row and button tints lie over
        // `currentColor`, i.e. the foreground itself and therefore the
        // strongest lightening available, while HOVER_BG_ALPHA lies over
        // `@accent_bg_color`, which is darker than the foreground and so
        // milder. Treating them all as foreground tints once suggested a
        // failure at 4.40:1 that the app cannot actually produce.
        const ROW_HOVER_ALPHA: f64 = 0.04;
        let accent =
            parse_hex_rgb(super::super::accent::APP_ACCENT).expect("the brand accent is valid hex");

        for theme in Theme::all() {
            for (appearance, palette) in
                [("dark", theme.palette()), ("light", theme.light_palette())]
            {
                let foreground = parse_hex_rgb(palette.fg).expect("palette fg is valid hex");
                let button: f64 = BTN_HOVER_ALPHA.parse().expect("token is a fraction");
                let flat: f64 = HOVER_BG_ALPHA.parse().expect("token is a fraction");

                for surface in palette.surfaces() {
                    let plain = parse_hex_rgb(surface).expect("palette surface is valid hex");
                    for (what, tint, alpha) in [
                        ("row hover", foreground, ROW_HOVER_ALPHA),
                        ("button hover", foreground, button),
                        ("flat hover", accent, flat),
                    ] {
                        let hovered = composite(tint, plain, alpha);
                        for (level, name) in [
                            (PRIMARY_TEXT_ALPHA, "primary"),
                            (SECONDARY_TEXT_ALPHA, "secondary"),
                        ] {
                            let rendered = composite(foreground, hovered, level);
                            let ratio = contrast_ratio(rendered, hovered);
                            assert!(
                                ratio >= 4.5,
                                "{theme:?} {appearance}: {name} text on {surface} under \
                                 {what} reaches only {ratio:.2}:1"
                            );
                        }
                    }
                }
            }
        }
    }
}
