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
