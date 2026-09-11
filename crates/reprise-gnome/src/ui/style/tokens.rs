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

/// Corner radius of a filter-bar chip and of "+ Add filter" beside it.
pub(in crate::ui) const RADIUS_CHIP: &str = "8px";

/// Resting surface alpha of a filter chip (over `@window_fg_color`).
///
/// The chip used to sit on `@accent_bg_color`: at 0.22/0.32 its label measured
/// 4.17:1 and 3.37:1 — below AA — and even after that was pulled back to
/// 0.14/0.18 (see the retired `CHIP_BG_ALPHA`), the label still bounded the
/// fill. The redesign drops the accent tint from the chip entirely — only the
/// left edge and the search icon still carry `@accent_color` — so the surface
/// is neutral instead and the bound that used to cap it no longer applies.
pub(in crate::ui) const CHIP_SURFACE_ALPHA: &str = "0.07";

/// Border alpha of a filter chip (over `@window_fg_color`). See
/// [`CHIP_SURFACE_ALPHA`] for why the chip moved off the accent surface.
pub(in crate::ui) const CHIP_BORDER_ALPHA: &str = "0.14";

/// Hover background alpha of a chip's own × remove button (over
/// `@window_fg_color`) — a glyph with a hover state of its own, distinct from
/// the chip surface it sits on.
pub(in crate::ui) const CHIP_REMOVE_HOVER_BG_ALPHA: &str = "0.12";

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

/// Hairline colour on dark surfaces, kept literal so the dark appearance is
/// unchanged when the light appearance receives its own edge colour.
pub(in crate::ui) const HAIRLINE_DARK: &str = concat!("rgba(255, ", "255, 255, 0.06)");

/// Hairline colour on light surfaces. The dark twin cannot be reused because
/// a translucent white edge disappears against the near-white palettes.
pub(in crate::ui) const HAIRLINE_LIGHT: &str = "rgba(0, 0, 6, 0.09)";

/// Strong hairline colour on dark surfaces, preserved from the existing table header.
pub(in crate::ui) const HAIRLINE_STRONG_DARK: &str = concat!("rgba(255, ", "255, 255, 0.07)");

/// Strong hairline colour on light surfaces. Its white dark twin would vanish
/// against the light table background.
pub(in crate::ui) const HAIRLINE_STRONG_LIGHT: &str = "rgba(0, 0, 6, 0.11)";

/// Subtle row rule on dark surfaces, preserved from the existing track table.
pub(in crate::ui) const RULE_DARK: &str = concat!("rgba(255, ", "255, 255, 0.045)");

/// Subtle row rule on light surfaces. Its white dark twin has no visible edge
/// against the near-white table.
pub(in crate::ui) const RULE_LIGHT: &str = "rgba(0, 0, 6, 0.055)";

/// Floating-pill border on dark surfaces, preserved from the library summary.
pub(in crate::ui) const PILL_BORDER_DARK: &str = concat!("rgba(255, ", "255, 255, 0.10)");

/// Floating-pill border on light surfaces. Its white dark twin disappears on
/// the lifted white pill surface.
pub(in crate::ui) const PILL_BORDER_LIGHT: &str = "rgba(0, 0, 6, 0.14)";

/// Floating-pill surface on dark palettes, preserved from the existing sidebar fill.
pub(in crate::ui) const PILL_BG_DARK: &str = "@sidebar_bg_color";

/// Floating-pill surface on light palettes. The dark twin is too close to the
/// light table beneath the overlay, so the card surface supplies elevation.
pub(in crate::ui) const PILL_BG_LIGHT: &str = "@card_bg_color";

/// Corner radius for layered redesign surfaces (cards, panels).
pub(in crate::ui) const RADIUS_SURFACE: &str = "12px";

/// Hover background alpha for flat interactive elements (over `@accent_bg_color`).
pub(in crate::ui) const HOVER_BG_ALPHA: &str = "0.10";

/// Foreground-tint alpha for flat hover feedback in the light appearance. The
/// dark accent twin would brighten light rows instead of darkening them.
pub(in crate::ui) const HOVER_BG_LIGHT_ALPHA: &str = "0.045";

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

/// Resting checked-toggle alpha in the light appearance. The dark twin is too
/// loud beside the brand accent on near-white surfaces.
pub(in crate::ui) const BTN_CHECKED_FILL_LIGHT_ALPHA: &str = "0.14";

/// Checked + hover: brighter fill, same state display.
pub(in crate::ui) const BTN_CHECKED_FILL_HOVER_ALPHA: &str = "0.22";

/// Checked + pressed, and the loudest accent tint in the app — so this is the
/// value [`ACCENT_TINT_CEILING`] is pinned to, and it may not exceed it.
///
/// It used to be 0.38, chosen for press feedback alone while
/// `@reprise_accent_text_color` was still derived against the much quieter chip
/// tint. The label on this fill therefore measured 2.97:1 in the dark palettes
/// with the entire contrast suite green — nothing modelled the surface the text
/// actually landed on. Following the precedent the filter chip set — see
/// [`CHIP_SURFACE_ALPHA`], which now records that measurement — the fill
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

/// Top inset from the settled Now Playing panel specification.
pub(in crate::ui) const NOW_PLAYING_HEAD_TOP: i32 = 50;
/// Cover edge from the settled Now Playing panel specification.
pub(in crate::ui) const NOW_PLAYING_COVER_SIZE: i32 = 184;
/// Gap between the cover and title in the settled panel specification.
pub(in crate::ui) const NOW_PLAYING_COVER_TO_TITLE: i32 = 34;
/// The artwork band ends exactly where the title begins.
pub(in crate::ui) const NOW_PLAYING_ARTWORK_BAND: i32 =
    NOW_PLAYING_HEAD_TOP + NOW_PLAYING_COVER_SIZE + NOW_PLAYING_COVER_TO_TITLE;
/// Peak alpha of the accent glow. At 0.15 the subtitle clears 4.5:1 over the
/// panel surface plus glow for both pure-white and pure-black accents, making
/// the cap safe for any accent colour. The 0.17 boundary leaves no margin.
pub(in crate::ui) const NOW_PLAYING_GLOW_ALPHA: &str = "0.15";
/// Peak glow alpha in the light appearance. The dark twin is too luminous on
/// the near-white sidebar, so light uses only a restrained accent bloom.
pub(in crate::ui) const NOW_PLAYING_GLOW_LIGHT_ALPHA: &str = "0.05";
/// Segment-control height from the settled Now Playing panel specification.
pub(in crate::ui) const NOW_PLAYING_SEGMENT_HEIGHT: i32 = 30;
/// Outer segment-control radius from the settled panel specification.
pub(in crate::ui) const NOW_PLAYING_SEGMENT_RADIUS: &str = "7px";
/// Checked-segment radius from the settled panel specification.
pub(in crate::ui) const NOW_PLAYING_SEGMENT_INNER_RADIUS: &str = "5px";
/// Distance the settled panel's list rule fades at each end.
pub(in crate::ui) const NOW_PLAYING_LIST_RULE_RUN_OUT: i32 = 34;
/// Gap above the settled panel's list rule.
pub(in crate::ui) const NOW_PLAYING_LIST_RULE_ABOVE: i32 = 20;
/// Gap below the settled panel's list rule.
pub(in crate::ui) const NOW_PLAYING_LIST_RULE_BELOW: i32 = 8;
/// Share of the settled panel width occupied by its left artwork fade.
pub(in crate::ui) const NOW_PLAYING_LEFT_FADE_SHARE: f64 = 0.22;
/// Tertiary album tone from the settled panel specification in dark mode.
pub(in crate::ui) const TERTIARY_TEXT_ALPHA_DARK: f64 = 0.55;
/// Stronger light-mode album tone needed to retain the specification's 4.5:1.
pub(in crate::ui) const TERTIARY_TEXT_ALPHA_LIGHT: f64 = 0.65;
pub(in crate::ui) const NOW_PLAYING_PILL_BG_ALPHA: &str = "0.06";
pub(in crate::ui) const NOW_PLAYING_PILL_ACTIVE_ALPHA: &str = "0.14";
/// Active Now Playing tab surface in the light appearance. The translucent
/// foreground dark twin reads as a tint rather than a lifted tab on light.
pub(in crate::ui) const NOW_PLAYING_TAB_ACTIVE_BG_LIGHT: &str = "@view_bg_color";
pub(in crate::ui) const NOW_PLAYING_TITLE_SIZE: &str = "15px";
pub(in crate::ui) const NOW_PLAYING_SUBTITLE_SIZE: &str = "12px";
pub(in crate::ui) const NOW_PLAYING_FOOTER_SIZE: &str = "10.5px";
pub(in crate::ui) const NOW_PLAYING_QUEUE_COVER_SIZE: i32 = 32;
pub(in crate::ui) const NOW_PLAYING_QUEUE_TITLE_SIZE: &str = "13.5px";

// --- Appearance-specific edges and shadows ---

/// Mini-player card surface in dark mode, preserved from the existing glass.
pub(in crate::ui) const MINI_CARD_BG_DARK: &str = "rgba(34, 34, 34, 0.92)";
/// Mini-player card surface in light mode. The fixed near-black dark twin
/// contradicts the light waveform and text, so light follows the player bar.
pub(in crate::ui) const MINI_CARD_BG_LIGHT: &str = "alpha(@headerbar_bg_color, 0.92)";

/// Mini-player card edge in dark mode, preserved from the existing hairline.
pub(in crate::ui) const MINI_CARD_EDGE_DARK: &str = "alpha(white, 0.09)";
/// Mini-player card edge in light mode. The white dark twin disappears on a
/// pale floating surface, so light uses the matching raised-surface edge.
pub(in crate::ui) const MINI_CARD_EDGE_LIGHT: &str = "rgba(0, 0, 6, 0.14)";

/// Mini-player cover edge in dark mode, preserved from the existing inset.
pub(in crate::ui) const MINI_COVER_EDGE_DARK: &str = "alpha(white, 0.08)";
/// Mini-player cover edge in light mode. The white dark twin disappears on
/// pale artwork and surfaces, so light uses a restrained black inset.
pub(in crate::ui) const MINI_COVER_EDGE_LIGHT: &str = "alpha(#000000, 0.10)";

/// Mini-player artist alpha in dark mode, preserved from the existing label.
pub(in crate::ui) const MINI_ARTIST_ALPHA: &str = "0.6";
/// Mini-player artist alpha in light mode. The dark level cannot clear AA on
/// the translucent light card, so light uses the verified secondary level.
pub(in crate::ui) const MINI_ARTIST_LIGHT_ALPHA: &str = "0.70";

/// Running-row tint alpha on dark surfaces, preserved from the existing rule.
pub(in crate::ui) const NOW_PLAYING_TINT_DARK_ALPHA: &str = "0.09";
/// Running-row tint alpha on light surfaces. The dark twin uses the derived
/// text accent there, so light instead tints with the raw accent background.
pub(in crate::ui) const NOW_PLAYING_TINT_LIGHT_ALPHA: &str = "0.12";

/// Settled cover shadow strength, shared by dark and light appearances.
pub(in crate::ui) const COVER_SHADOW_DARK_ALPHA: &str = "0.42";
pub(in crate::ui) const COVER_SHADOW_LIGHT_ALPHA: &str = "0.42";

/// Active-tab shadow alpha in dark mode: zero preserves the existing flat tab.
pub(in crate::ui) const TAB_ACTIVE_SHADOW_DARK_ALPHA: &str = "0";
/// Active-tab shadow alpha in light mode. The transparent dark twin cannot
/// express the lifted active tab against the pale strip.
pub(in crate::ui) const TAB_ACTIVE_SHADOW_LIGHT_ALPHA: &str = "0.14";

/// Mini play-button glow alpha in dark mode, preserved from the existing glow.
pub(in crate::ui) const MINI_PLAY_GLOW_DARK_ALPHA: &str = "0.40";
/// Mini play-button glow alpha in light mode. The dark twin becomes glare, so
/// the colour is fully transparent while retaining the shadow layer.
pub(in crate::ui) const MINI_PLAY_GLOW_LIGHT_ALPHA: &str = "0";
/// Mini play-button hover glow alpha in dark mode, preserved from the existing hover state.
pub(in crate::ui) const MINI_PLAY_GLOW_HOVER_DARK_ALPHA: &str = "0.60";
/// Mini play-button hover glow alpha in light mode. The dark twin becomes glare,
/// so the colour is fully transparent while retaining the shadow layer.
pub(in crate::ui) const MINI_PLAY_GLOW_HOVER_LIGHT_ALPHA: &str = "0";

/// Near play-button glow alpha in dark mode, preserved from the existing glow.
pub(in crate::ui) const PLAY_GLOW_NEAR_DARK_ALPHA: &str = "0.60";
/// Near play-button glow alpha in light mode. The dark twin becomes glare, so
/// the colour is fully transparent while retaining the shadow layer.
pub(in crate::ui) const PLAY_GLOW_NEAR_LIGHT_ALPHA: &str = "0";
/// Far play-button glow alpha in dark mode, preserved from the existing glow.
pub(in crate::ui) const PLAY_GLOW_FAR_DARK_ALPHA: &str = "0.35";
/// Far play-button glow alpha in light mode. The dark twin becomes glare, so
/// the colour is fully transparent while retaining the shadow layer.
pub(in crate::ui) const PLAY_GLOW_FAR_LIGHT_ALPHA: &str = "0";
/// Near hover-glow alpha in dark mode, preserved from the existing hover state.
pub(in crate::ui) const PLAY_GLOW_NEAR_HOVER_DARK_ALPHA: &str = "0.75";
/// Near hover-glow alpha in light mode. The dark twin becomes glare, so the
/// colour is transparent while the transition keeps matched geometry.
pub(in crate::ui) const PLAY_GLOW_NEAR_HOVER_LIGHT_ALPHA: &str = "0";
/// Far hover-glow alpha in dark mode, preserved from the existing hover state.
pub(in crate::ui) const PLAY_GLOW_FAR_HOVER_DARK_ALPHA: &str = "0.48";
/// Far hover-glow alpha in light mode. The dark twin becomes glare, so the
/// colour is transparent while the transition keeps matched geometry.
pub(in crate::ui) const PLAY_GLOW_FAR_HOVER_LIGHT_ALPHA: &str = "0";

/// Play-button ring alpha in dark mode: zero preserves the existing silhouette.
pub(in crate::ui) const PLAY_RING_DARK_ALPHA: &str = "0";
/// Play-button ring alpha in light mode. The transparent dark twin leaves no
/// edge around the accent circle on a pale background.
pub(in crate::ui) const PLAY_RING_LIGHT_ALPHA: &str = "0.12";
/// Resting play-button drop alpha in dark mode, preserved from the existing shadow.
pub(in crate::ui) const PLAY_DROP_DARK_ALPHA: &str = "0.36";
/// Resting play-button drop alpha in light mode. The dark twin is too heavy on
/// a light surface, so light keeps a quieter elevation shadow.
pub(in crate::ui) const PLAY_DROP_LIGHT_ALPHA: &str = "0.18";
/// Hovered play-button drop alpha in dark mode, preserved from the existing shadow.
pub(in crate::ui) const PLAY_DROP_HOVER_DARK_ALPHA: &str = "0.34";
/// Hovered play-button drop alpha in light mode. The dark twin is too heavy on
/// a light surface, so light uses its own elevation step.
pub(in crate::ui) const PLAY_DROP_HOVER_LIGHT_ALPHA: &str = "0.22";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npp_2_now_playing_geometry_matches_the_settled_panel_spec() {
        assert_eq!(NOW_PLAYING_HEAD_TOP, 50);
        assert_eq!(NOW_PLAYING_COVER_SIZE, 184);
        assert_eq!(NOW_PLAYING_COVER_TO_TITLE, 34);
        assert_eq!(NOW_PLAYING_ARTWORK_BAND, 268);
        assert_eq!(NOW_PLAYING_SEGMENT_HEIGHT, 30);
        assert_eq!(NOW_PLAYING_SEGMENT_RADIUS, "7px");
        assert_eq!(NOW_PLAYING_SEGMENT_INNER_RADIUS, "5px");
        assert_eq!(NOW_PLAYING_LIST_RULE_RUN_OUT, 34);
        assert_eq!(NOW_PLAYING_LIST_RULE_ABOVE, 20);
        assert_eq!(NOW_PLAYING_LIST_RULE_BELOW, 8);
        assert!((NOW_PLAYING_LEFT_FADE_SHARE - 0.22).abs() < f64::EPSILON);
    }

    /// Every accent tint the app may paint, resting states included. The
    /// ceiling has to bound this list, not just the loudest single token —
    /// a tint added here without raising the ceiling ships unmeasured.
    fn accent_tint_alphas() -> Vec<(&'static str, f64)> {
        [
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
        // milder. In light appearance the flat hover is a 0.045 foreground
        // tint instead: it darkens the row and is gentler than the accent tint
        // it replaces, so it cannot lower any ratio guarded here. Treating all
        // dark hovers as foreground tints once suggested a failure at 4.40:1
        // that the app cannot actually produce.
        const ROW_HOVER_ALPHA: f64 = 0.04;
        let accent =
            parse_hex_rgb(super::super::accent::APP_ACCENT).expect("the brand accent is valid hex");

        for theme in Theme::all() {
            for (appearance, palette, is_dark) in [
                ("dark", theme.palette(), true),
                ("light", theme.light_palette(), false),
            ] {
                let foreground = parse_hex_rgb(palette.fg).expect("palette fg is valid hex");
                let button: f64 = BTN_HOVER_ALPHA.parse().expect("token is a fraction");
                let flat: f64 = if is_dark {
                    HOVER_BG_ALPHA
                } else {
                    HOVER_BG_LIGHT_ALPHA
                }
                .parse()
                .expect("token is a fraction");
                let flat_tint = if is_dark { accent } else { foreground };

                for surface in palette.surfaces() {
                    let plain = parse_hex_rgb(surface).expect("palette surface is valid hex");
                    for (what, tint, alpha) in [
                        ("row hover", foreground, ROW_HOVER_ALPHA),
                        ("button hover", foreground, button),
                        ("flat hover", flat_tint, flat),
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
