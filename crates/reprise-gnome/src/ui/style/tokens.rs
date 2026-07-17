//! Tunable design values shared by the app-authored CSS sections.
//!
//! Every alpha, thickness, and density height that a design pass would want
//! to adjust lives here; the structural selectors stay with the feature that
//! owns the CSS classes (see [`super::app_css`]'s section list).

/// Foreground alpha for sortable track-table column titles — quieter than
/// song metadata without looking disabled.
pub(in crate::ui) const HEADER_TEXT_ALPHA: &str = "0.78";

/// Resting background alpha for filter chips (over `@accent_bg_color`).
pub(in crate::ui) const CHIP_BG_ALPHA: &str = "0.22";

/// Hover background alpha for filter chips.
pub(in crate::ui) const CHIP_BG_HOVER_ALPHA: &str = "0.32";

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
pub(in crate::ui) const ROW_MIN_HEIGHT_COMPACT: i32 = 12;

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

// --- Artists master/detail view (see `super::super::artist_view_css`) ---

/// Muted secondary-text alpha (over `@window_fg_color`) shared by the Artists
/// view's count, list meta, eyebrow, empty hint, and top-track play/duration
/// labels.
pub(in crate::ui) const MUTED_TEXT_ALPHA: &str = "0.45";

/// Resting fill alpha for the subtle non-accent pills/buttons (Shuffle, the ⋮
/// menu) — a barely-there wash over `@window_fg_color`.
pub(in crate::ui) const SUBTLE_FILL_ALPHA: &str = "0.09";

/// Hover fill alpha for those same subtle pills/buttons.
pub(in crate::ui) const SUBTLE_FILL_HOVER_ALPHA: &str = "0.14";

/// Initials color for the gradient avatars — near-white so it reads on any
/// per-artist gradient (list row and hero both).
pub(in crate::ui) const AVATAR_INITIALS_COLOR: &str = "rgba(255, 255, 255, 0.95)";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_css_uses_the_micro_motion_token() {
        assert_eq!(format!("{TRANSITION}"), "150ms ease-out");
    }
}
