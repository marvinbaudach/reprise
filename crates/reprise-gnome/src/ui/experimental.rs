//! The "Experimental features" master gate (INST-11, Decision 11).
//!
//! A persisted, default-off switch that decides whether the AI surface is
//! visible at all: today the AI badge (INST-10) and the "Hide AI music" filter
//! (FIL-7), both of which mark tracks the CLI/MCP frontends can still produce.
//!
//! This used to live inside `ui::instrumental` alongside the conversion view
//! and the out-of-process worker supervisor. Those were removed from the GTK
//! frontend; the gate itself is not instrumental-specific and stays, so the
//! remaining AI surface keeps a single source of truth for its visibility.
//!
//! Deliberately just a key plus its accessors. The live-transition and
//! open-settings hooks that used to sit here existed only to start/stop the
//! instrumental worker runtime and to build or tear down the conversion page;
//! they went with it. Callers that need the toggle to take effect immediately
//! refresh the surface they own — see `ui::preferences::preference_experimental`,
//! which refreshes the sidebar right where it persists the new value.

use reprise_core::db::Db;
use reprise_core::library::settings;

/// The persisted settings key gating the AI surface (INST-11, Decision 11). A
/// bespoke key rather than a `reprise_core::modules` descriptor: this is a
/// master gate over a whole surface (not a plain plugin toggle), and the plan
/// scopes it to the frontend's `ui/preferences`. Default off.
pub(in crate::ui) const EXPERIMENTAL_ENABLED_KEY: &str = "experimental_features.enabled";

/// Whether the "Experimental features" master switch is on. Every gated
/// surface (AI badge, "Hide AI music" filter) reads this before showing
/// anything (INST-11 / FIL-7). Defaults to `false` and tolerates a read error
/// by staying hidden — the safe default for an experimental gate.
pub(in crate::ui) fn experimental_enabled(db: &Db) -> bool {
    settings::get_bool(db, EXPERIMENTAL_ENABLED_KEY, false).unwrap_or(false)
}

/// Persists the master switch. Wrapping `settings::set_bool` keeps the key a
/// single source of truth for both the preferences toggle and every reader.
pub(in crate::ui) fn set_experimental_enabled(
    db: &Db,
    enabled: bool,
) -> Result<(), rusqlite::Error> {
    settings::set_bool(db, EXPERIMENTAL_ENABLED_KEY, enabled)
}
