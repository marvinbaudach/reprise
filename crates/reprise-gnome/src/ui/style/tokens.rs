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
