//! Tunable design values shared by the app-authored CSS sections.
//!
//! Every alpha, thickness, and row height that a design pass would want
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

/// Track-row content minimum height, and — the load-bearing part — the height
/// `ListGeometry` *assumes* a row has before a settled frame has measured one.
///
/// It must stay at or below the height rows really render at. Measured on
/// 2026-08-24 under the display harness: a track row is 34 px, and the cell
/// children carrying `.reprise-track-cell` are 18 px inside it. Raised to 36 by
/// #660 the assumption sat two pixels *above* the truth, and the centred reveal
/// then had two writers disagreeing about the same row: the seed placed row 137
/// at `137 * 36 = 4932` while GTK's own anchor placed it at `137 * 34 = 4658`,
/// each overwriting the other. Eight display tests read that as a viewport that
/// will not settle in one move.
///
/// The rule this token also feeds — `.reprise-track-cell { min-height }` in
/// `track_list_row_interaction::css` — does not bind: set to 80 for one run the
/// cells stayed 18 px and the list's `upper` stayed `200 * 34`. So the value
/// here is the geometry floor and nothing else, which is why it goes back to
/// what the default density used before #660 rather than to a taller row.
pub(in crate::ui) const ROW_MIN_HEIGHT: i32 = 28;

/// Queue section-header content minimum height.
///
/// This is a floor, not a measured header height. It deliberately exceeds
/// the measured 34 px natural height of the Play Next button row so both the
/// button row and plain label bind to the same authored minimum. A theme or
/// large system font may still require more space; geometry measurement must
/// detect that instead of treating this token as truth.
pub(in crate::ui) const SECTION_HEADER_MIN_HEIGHT: i32 = 36;

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
///
/// The whole checked ladder yields to [`ACCENT_TINT_CEILING`] — see
/// [`BTN_CHECKED_FILL_PRESS_ALPHA`]. What keeps the on-state readable is not
/// the fill's loudness alone: BTN-2 also paints the accent dot and the accent
/// foreground, and the hover it has to out-shout is
/// `alpha(currentColor, `[`BTN_HOVER_ALPHA`]`)`, a foreground wash in a
/// different hue rather than a quieter accent fill.
pub(in crate::ui) const BTN_CHECKED_FILL_ALPHA: &str = "0.18";

/// Checked + hover: brighter fill, same state display.
pub(in crate::ui) const BTN_CHECKED_FILL_HOVER_ALPHA: &str = "0.22";

/// Checked + pressed, and the loudest accent tint in the app — so this is the
/// value [`ACCENT_TINT_CEILING`] is pinned to, and it may not exceed it.
///
/// It used to be 0.38, chosen for press feedback alone while
/// `@reprise_accent_text_color` was still derived against the much quieter chip
/// tint. The label on this fill therefore measured 2.97:1 in the dark palettes
/// with the entire contrast suite green — nothing modelled the surface the text
/// actually landed on. Following the precedent [`CHIP_BG_ALPHA`] set, the fill
/// yields and the accent stays itself.
pub(in crate::ui) const BTN_CHECKED_FILL_PRESS_ALPHA: &str = "0.26";

/// Resting fill of a source-add action, and its hover and press steps. Same
/// ladder as the checked toggle and under the same ceiling — these used to be
/// literals inside `buttons::css`, where the ceiling guard could still see them
/// but nothing pointed a reader from the fill back to the budget it spends.
pub(in crate::ui) const ADD_ACTION_FILL_ALPHA: &str = "0.16";
pub(in crate::ui) const ADD_ACTION_FILL_HOVER_ALPHA: &str = "0.21";
pub(in crate::ui) const ADD_ACTION_FILL_PRESS_ALPHA: &str = "0.26";

/// The heaviest accent-tinted background any app surface may paint.
///
/// `theme::Palette::critical_accent_surface` derives
/// `@reprise_accent_text_color` against a surface tinted this far, so every
/// accent foreground stays above [`super::accent::ACCENT_TEXT_MINIMUM_RATIO`]
/// on the loudest tint that exists — not just on the plain surfaces. Modelling
/// only the chip tint left the checked player-bar toggle at 2.97:1 while every
/// contrast test passed, because the toggle fill is brighter than a chip's.
///
/// Raising this is not free in either direction. Too low and a louder tint
/// ships unmeasured; too high and no single foreground can serve both ends of
/// the palette any more — a heavy tint of a *light* system accent lifts a dark
/// surface into mid-grey, the lightness search runs out of gamut, and the
/// monochrome fallback then picks a foreground that fails on the plain surfaces
/// instead. Measured across the brand teal and the four extreme system accents
/// `accent::tests` exercises, and across the elevation ladder rather than the
/// bare palette: 0.28 breaks that way and 0.26 is the last value where every
/// accent still resolves and clears the ratio on every rung. Counting only the
/// bare surfaces the budget looks like 0.30 — that reading is what left accent
/// text on a tinted dialog card at 3.90:1.
///
/// `contrast_5a_accent_text_survives_every_tint_up_to_the_ceiling` holds both
/// ends, and `contrast_5a_no_app_surface_tints_past_the_ceiling` proves no CSS
/// rule exceeds it.
pub(in crate::ui) const ACCENT_TINT_CEILING: &str = "0.26";

/// Neutral fill of a disabled primary button, over `currentColor`.
///
/// A disabled filled button keeps no accent surface at all. Adwaita dims the
/// accent fill instead, which lands the near-black accent foreground on a
/// mid-dark tint of the accent — measured at roughly 2.5:1, the pairing that
/// made "Sync now" unreadable while it was insensitive. WCAG exempts inactive
/// controls from the ratio, so the fix is not the ratio itself: it is that the
/// *absence* of the accent surface, not a muddied version of it, is what says
/// the action is unavailable.
pub(in crate::ui) const PRIMARY_DISABLED_FILL_ALPHA: &str = "0.08";

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
/// Height of the artwork band: the cover, the bloom and the shimmer live in it
/// and the title block begins below it. 280 because the shimmer's own mask
/// reaches zero at y = 277 (SHIMMER_CENTRE_Y + 0.68 × disc radius), so the band
/// contains that falloff completely.
pub(in crate::ui) const NOW_PLAYING_ARTWORK_BAND: i32 = 280;
/// Peak alpha of the accent glow. At 0.15 the subtitle clears 4.5:1 over the
/// panel surface plus glow for both pure-white and pure-black accents, making
/// the cap safe for any accent colour. The 0.17 boundary leaves no margin.
pub(in crate::ui) const NOW_PLAYING_GLOW_ALPHA: &str = "0.15";
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

    /// Every accent tint the app may paint, resting states included. The
    /// ceiling has to bound this list, not just the loudest single token —
    /// a tint added here without raising the ceiling ships unmeasured.
    fn accent_tint_alphas() -> Vec<(&'static str, f64)> {
        [
            ("chip", CHIP_BG_ALPHA),
            ("chip:hover", CHIP_BG_HOVER_ALPHA),
            ("flat:hover", HOVER_BG_ALPHA),
            ("panel toggle:checked:hover", HOVER_BG_ALPHA_STRONG),
            ("toggle:checked", BTN_CHECKED_FILL_ALPHA),
            ("toggle:checked:hover", BTN_CHECKED_FILL_HOVER_ALPHA),
            ("toggle:checked:active", BTN_CHECKED_FILL_PRESS_ALPHA),
            ("add action", ADD_ACTION_FILL_ALPHA),
            ("add action:hover", ADD_ACTION_FILL_HOVER_ALPHA),
            ("add action:active", ADD_ACTION_FILL_PRESS_ALPHA),
            ("now playing pill:active", NOW_PLAYING_PILL_ACTIVE_ALPHA),
        ]
        .into_iter()
        .map(|(name, token)| (name, token.parse().expect("tint token is a fraction")))
        .collect()
    }

    fn elevation_rung(token: &str) -> f64 {
        token.parse().expect("elevation tint token is a fraction")
    }

    #[test]
    fn contrast_5a_the_ceiling_bounds_every_accent_tint_token() {
        let ceiling: f64 = ACCENT_TINT_CEILING.parse().expect("ceiling is a fraction");
        for (name, alpha) in accent_tint_alphas() {
            assert!(
                alpha <= ceiling,
                "the {name} tint paints at {alpha}, past the {ceiling} ceiling \
                 @reprise_accent_text_color is derived against"
            );
        }
    }

    /// The derivation is bounded at *both* ends, and only one end is a contrast
    /// floor. Raising the ceiling far enough pushes the lightened accent out of
    /// the sRGB gamut, `ensure_contrast_by_lightness` returns `None`, and the
    /// role falls back to black or white — which silently removes the brand hue
    /// from every accent foreground in the app rather than failing a test. So
    /// this asserts the ratio *and* that the answer is still the accent.
    #[test]
    fn contrast_5a_accent_text_survives_every_tint_up_to_the_ceiling() {
        use super::super::accent::{ACCENT_TEXT_MINIMUM_RATIO, APP_ACCENT};
        use super::super::color_math::{
            composite, contrast_ratio, ensure_contrast_by_lightness, parse_hex_rgb,
        };
        use super::super::theme::Theme;

        let ceiling: f64 = ACCENT_TINT_CEILING.parse().expect("ceiling is a fraction");
        let accent = parse_hex_rgb(APP_ACCENT).expect("the brand accent is valid hex");

        for theme in Theme::all() {
            for (appearance, palette, is_dark) in [
                ("dark", theme.palette(), true),
                ("light", theme.light_palette(), false),
            ] {
                let critical = palette.critical_accent_surface(is_dark, accent);
                assert!(
                    ensure_contrast_by_lightness(
                        accent,
                        critical,
                        is_dark,
                        ACCENT_TEXT_MINIMUM_RATIO
                    )
                    .is_some(),
                    "{theme:?} {appearance}: the accent text role cannot reach \
                     {ACCENT_TEXT_MINIMUM_RATIO}:1 on a {ceiling} tint by lightness alone and \
                     would fall back to monochrome, dropping the brand hue app-wide"
                );

                let text = parse_hex_rgb(&super::super::accent::accent_text_color(
                    accent, critical, is_dark,
                ))
                .expect("the derived accent text is valid hex");

                for surface in palette.surfaces() {
                    let plain = parse_hex_rgb(surface).expect("palette surface is valid hex");
                    // Walk the elevation ladder, not just the bare surface: the
                    // dialog rungs are white over the ground below them, and an
                    // accent tint on a dialog *card* is lighter than the same
                    // tint on `dialog_bg_color`. Measuring the bare surfaces
                    // alone reported this palette safe while accent text on a
                    // tinted card sat at 3.90:1.
                    for (rung, elevation) in [
                        ("plain", 0.0),
                        ("dialog headerbar", elevation_rung(DIALOG_HEADER_TINT_ALPHA)),
                        ("dialog card", elevation_rung(DIALOG_CARD_ALPHA)),
                    ] {
                        const WHITE: [u8; 3] = [255, 255, 255];
                        let ground = composite(WHITE, plain, elevation);
                        for (name, alpha) in accent_tint_alphas() {
                            let tinted = composite(accent, ground, alpha);
                            let ratio = contrast_ratio(text, tinted);
                            assert!(
                                ratio >= ACCENT_TEXT_MINIMUM_RATIO,
                                "{theme:?} {appearance}: accent text on the {rung} rung of \
                                 {surface} under the {name} tint reaches only {ratio:.2}:1"
                            );
                        }
                    }
                }
            }
        }
    }

    /// The disabled primary button is exempt from AA (WCAG excludes inactive
    /// controls), but "exempt" is what produced the 2.5:1 pairing this replaced.
    /// Its label is held to the same floor as any other text so the exemption
    /// cannot quietly become the excuse again.
    #[test]
    fn btn_5_the_disabled_primary_label_stays_readable() {
        use super::super::color_math::{composite, contrast_ratio, parse_hex_rgb};
        use super::super::theme::Theme;

        let fill: f64 = PRIMARY_DISABLED_FILL_ALPHA
            .parse()
            .expect("token is a fraction");

        for theme in Theme::all() {
            for (appearance, palette) in
                [("dark", theme.palette()), ("light", theme.light_palette())]
            {
                let foreground = parse_hex_rgb(palette.fg).expect("palette fg is valid hex");
                for surface in palette.surfaces() {
                    let plain = parse_hex_rgb(surface).expect("palette surface is valid hex");
                    // `color` is set on the same rule, so `currentColor` in the
                    // fill is the already-translucent secondary level.
                    let ground = composite(foreground, plain, SECONDARY_TEXT_ALPHA * fill);
                    let label = composite(foreground, ground, SECONDARY_TEXT_ALPHA);
                    let ratio = contrast_ratio(label, ground);
                    assert!(
                        ratio >= 4.5,
                        "{theme:?} {appearance}: the disabled primary label on {surface} \
                         reaches only {ratio:.2}:1"
                    );
                }
            }
        }
    }

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
