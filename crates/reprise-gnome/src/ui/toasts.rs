//! Single construction point for transient notifications. Every plain
//! informational toast goes through `show` so (a) the adw::Toast type
//! appears in exactly one non-test file outside bespoke cases, and (b) a
//! future libadwaita API change or a second notification backend (e.g.
//! XDG portal notifications when the window is closed, spec "Hintergrund-
//! Wiedergabe") edits one function. Sites that need buttons, custom
//! timeouts, or priorities keep building their own Toast locally — this
//! helper is for the 90% case, not a wrapper around the whole API.

use libadwaita as adw;

pub(super) fn show(overlay: &adw::ToastOverlay, text: &str) {
    overlay.add_toast(adw::Toast::new(text));
}

/// Redesign toast chrome: a fully-rounded dark pill with soft elevation and an
/// accent-coloured action label. Installed app-wide by [`super::style`]; the
/// action reads `@accent_color`, so it follows the theme accent.
pub(super) fn css() -> String {
    use crate::ui::style::tokens::SURFACE_SHADOW;
    format!(
        ".toast {{ border-radius: 9999px; box-shadow: {SURFACE_SHADOW}; }}\n\
         .toast button.text-button {{ color: @accent_color; font-weight: bold; }}"
    )
}
