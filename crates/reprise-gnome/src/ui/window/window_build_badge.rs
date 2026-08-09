use gtk4::prelude::*;

/// A badge naming this build, for anything that is not the shipped release.
///
/// A session regularly has several binaries in play at once — the installed
/// app, a debug build from a worktree, one with a diagnostic switched on —
/// and telling them apart by the window alone was guesswork. The badge sits
/// in the header bar rather than in the window title because the title is
/// rewritten on every navigation (`Music`, an album name, …) while this
/// survives.
///
/// `None` for a release build with no diagnostics active: the shipped app
/// carries no badge at all.
pub(super) fn build() -> Option<gtk4::Widget> {
    let text = build_kind_label(
        cfg!(debug_assertions),
        std::env::var_os("REPRISE_DEBUG_SCROLL").is_some(),
    )?;
    let label = gtk4::Label::new(Some(&text));
    label.add_css_class("reprise-build-badge");
    label.set_tooltip_text(Some(
        "This is not the installed release build. Diagnostics may be active.",
    ));
    Some(label.upcast())
}

/// What the badge should read, given what is actually different about this
/// build. `None` means "nothing to say" — a plain release build.
fn build_kind_label(is_debug_build: bool, scroll_diagnostic: bool) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    if is_debug_build {
        parts.push("DEBUG");
    }
    if scroll_diagnostic {
        parts.push("SCROLL-LOG");
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join(" \u{b7} "))
}

#[cfg(test)]
mod tests {
    use super::build_kind_label;

    /// The shipped app must stay unmarked — a badge on the release build
    /// would be noise in every screenshot and bug report.
    #[test]
    fn a_release_build_without_diagnostics_carries_no_badge() {
        assert_eq!(build_kind_label(false, false), None);
    }

    #[test]
    fn the_badge_names_what_is_actually_different() {
        assert_eq!(build_kind_label(true, false).as_deref(), Some("DEBUG"));
        assert_eq!(
            build_kind_label(false, true).as_deref(),
            Some("SCROLL-LOG"),
            "a release build can still have a diagnostic switched on"
        );
        assert_eq!(
            build_kind_label(true, true).as_deref(),
            Some("DEBUG · SCROLL-LOG")
        );
    }
}
