//! Icon names the app leans on, where the obvious name is the wrong one.
//!
//! Only names with a reason to be here. Everything else stays inline at its
//! call site, which is this codebase's habit.

/// "This is done." **Not** `emblem-ok-symbolic`, which is what six call sites
/// used to say: that name is absent from the installed Adwaita symbolic set
/// (checked against `/usr/share/icons/Adwaita/symbolic`), so GTK silently drew
/// the missing-image box instead of a checkmark. No test notices — it is a
/// string that resolves at runtime — and in a screenshot the box reads as a
/// small rectangle that looks like a layout detail.
pub(in crate::ui) const DONE: &str = "object-select-symbolic";
