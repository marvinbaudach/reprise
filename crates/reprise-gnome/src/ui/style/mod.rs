//! The single application-wide CSS provider.
//!
//! Feature modules own their CSS class names and contribute one plain
//! `css()` string section; this module composes those sections, installs
//! them exactly once per process, and keeps the tunable design values in
//! [`tokens`]. Widget constructors must never install providers themselves —
//! per-widget installation used to add one display-global provider on every
//! rebuild.

pub(super) mod theme;
pub(super) mod tokens;

use std::cell::Cell;

thread_local! {
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
}

/// One entry per feature that ships app-authored CSS.
fn app_css() -> String {
    [
        theme::theme_css(theme::Theme::DEFAULT),
        super::browse_bar::css(),
        super::column_layout_editor::css(),
        super::list_density::css(),
        super::lyrics_view::css(),
        super::preference_choice_cards::css(),
        super::rating::css(),
        super::track_list_header_style::css(),
        super::track_list_row_interaction::css(),
    ]
    .join("\n")
}

/// Installs the composed app CSS on the default display, once per process.
/// Safe to call from every window/test entry point; calls before GTK has a
/// default display (e.g. plain unit tests) are ignored without arming the
/// once-guard, so a later call after `gtk4::init` still installs.
pub(super) fn install() {
    INSTALLED.with(|installed| {
        if installed.get() {
            return;
        }
        let Some(display) = gtk4::gdk::Display::default() else {
            return;
        };
        installed.set(true);
        let provider = gtk4::CssProvider::new();
        provider.load_from_string(&app_css());
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn app_css_contains_every_feature_section() {
        let css = super::app_css();

        for marker in [
            "@define-color window_bg_color",
            "@define-color accent_bg_color",
            ".reprise-filter-chip",
            ".reprise-column-drop-before",
            ".reprise-track-cell.reprise-density-comfortable",
            ".lyrics-line-active",
            "checkbutton.reprise-choice-card",
            ".reprise-rating-inline-star",
            ".reprise-track-list > header label",
            ".reprise-reorder-target",
        ] {
            assert!(css.contains(marker), "missing section marker: {marker}");
        }
    }

    #[test]
    fn install_without_a_display_does_not_arm_the_once_guard() {
        // Plain unit-test processes have no GDK display; install() must stay
        // re-runnable so the first real (xvfb) caller still gets the CSS.
        if gtk4::gdk::Display::default().is_some() {
            return;
        }
        super::install();
        super::install();
        super::INSTALLED.with(|installed| assert!(!installed.get()));
    }
}
