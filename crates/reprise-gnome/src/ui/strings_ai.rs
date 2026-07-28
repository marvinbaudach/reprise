//! Labels for the AI surface that survives in the GTK frontend: the AI badge
//! (INST-10), the "Hide AI music" filter (FIL-7), and the Experimental
//! preferences page that gates both (INST-11).

use super::text;

// AI badge on a promoted instrumental track row (INST-10). A compact "AI" pill
// with the full provenance phrase as its tooltip.
pub const AI_BADGE_LABEL: &str = N_!("AI");
pub const AI_BADGE_TOOLTIP: &str = N_!("Instrumental · AI-manipulated");

// "Hide AI music" library filter (FIL-7).
pub const FILTER_HIDE_AI: &str = N_!("Hide AI music");

/// The accessible label for the AI-filter chip's remove (×) affordance.
pub fn remove_hide_ai_filter() -> String {
    format!("Remove filter: {}", text(FILTER_HIDE_AI))
}

// Experimental preferences page (INST-11/INST-12).
pub const EXPERIMENTAL_PAGE_TITLE: &str = N_!("Experimental");
pub const EXPERIMENTAL_GROUP_TITLE: &str = N_!("Experimental features");
pub const EXPERIMENTAL_GROUP_DESCRIPTION: &str =
    N_!("Unfinished features with rough edges, off by default.");
pub const EXPERIMENTAL_TOGGLE_TITLE: &str = N_!("Enable experimental features");
pub const EXPERIMENTAL_TOGGLE_SUBTITLE: &str =
    N_!("Shows AI instrumental versions across the app: the context-menu trigger, the conversion view, badges, and the \"Hide AI music\" filter.");
