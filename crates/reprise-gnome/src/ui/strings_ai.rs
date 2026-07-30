//! Labels for the AI surface that survives in the GTK frontend: the AI badge
//! (INST-10) and the "Hide AI music" filter (FIL-7). Both mark tracks the
//! CLI/MCP frontends produce; neither sits behind a settings gate any more.

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
