//! Row chrome for the Plugins page.
//!
//! `docs/plans/plugins-online-content-master-hierarchy.md`, third draft. The
//! second draft dissolved the card and moved the expander chevron into a
//! reserved gutter left of the title, so that row titles and group headings
//! shared one left edge. The third draft puts the card back — the five online
//! plugins are explicitly asked for as *one* card with hairlines between the
//! rows — and with it the gutter's whole reason to exist: subordination is now
//! carried by the 18px indent and the rail, and the group headings no longer
//! share an edge with an indented card. So the chevron returns to libadwaita's
//! own trailing slot, and the only alignment work left is the one thing that
//! keeps every switch on one right edge: rows without a chevron reserve its
//! width (`SET-14b`).
//!
//! What stays here is the page's own chrome: the expanded settings of a plugin
//! must read as *contents of that plugin*, one step further in and on a
//! slightly lifted surface, not as further plugins floating at the same level.

use gtk4::prelude::*;
use libadwaita as adw;

/// Set on the Plugins page; every rule in [`css`] is scoped to it.
pub(in crate::ui) const PLUGINS_PAGE_CLASS: &str = "reprise-plugin-page";
/// Set on every expandable plugin row, so its nested settings can be addressed
/// without depending on a libadwaita-internal style class.
pub(in crate::ui) const EXPANDER_ROW_CLASS: &str = "reprise-plugin-expander";

/// How far a plugin's own settings sit inside its row.
const NESTED_INDENT_PX: u32 = 24;
/// The lift that marks the nested surface as belonging to the row above it.
const NESTED_SURFACE_ALPHA: f32 = 0.03;

/// A chevron-sized hole on a row that never expands.
///
/// libadwaita puts the expander arrow *after* the enable area, so a plain
/// switch row is one arrow narrower than an expander row and its switch would
/// sit further right. The placeholder gives it the same trailing width.
pub(in crate::ui) fn switch_alignment_placeholder() -> gtk4::Image {
    gtk4::Image::builder()
        .icon_name("pan-down-symbolic")
        .accessible_role(gtk4::AccessibleRole::Presentation)
        .opacity(0.0)
        .can_target(false)
        .can_focus(false)
        .build()
}

/// Marks an expandable plugin row so [`css`] can reach its nested settings.
pub(in crate::ui) fn mark_expander(row: &adw::ExpanderRow) {
    row.add_css_class(EXPANDER_ROW_CLASS);
}

pub(in crate::ui) fn css() -> String {
    format!(
        "/* --- Plugins rows: a plugin's settings live inside the plugin --- */ \
         .{PLUGINS_PAGE_CLASS} .{EXPANDER_ROW_CLASS} list {{ \
           background-color: alpha(@window_fg_color, {NESTED_SURFACE_ALPHA}); }} \
         .{PLUGINS_PAGE_CLASS} .{EXPANDER_ROW_CLASS} list > row {{ \
           padding-left: {NESTED_INDENT_PX}px; }}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_11a_expanded_settings_read_as_contents_of_their_plugin() {
        let css = css();

        assert!(css.contains(&format!(".{EXPANDER_ROW_CLASS} list")));
        assert!(css.contains(&format!(
            "background-color: alpha(@window_fg_color, {NESTED_SURFACE_ALPHA})"
        )));
        assert!(css.contains(&format!("padding-left: {NESTED_INDENT_PX}px")));
    }

    #[test]
    fn set_11a_the_card_and_its_hairlines_are_left_to_libadwaita() {
        let css = css();

        // The second draft's overrides are gone: a boxed list already is the
        // one card with hairlines the third draft asks for.
        assert!(!css.contains("background-color: transparent"));
        assert!(!css.contains("border-radius: 0"));
        assert!(!css.contains("expander-row-arrow"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn set_14b_the_alignment_placeholder_is_presentation_only() {
        gtk4::init().unwrap();
        let placeholder = switch_alignment_placeholder();

        assert_eq!(
            placeholder.accessible_role(),
            gtk4::AccessibleRole::Presentation
        );
        assert_eq!(placeholder.opacity(), 0.0);
        assert!(!placeholder.can_target());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn the_plugin_chrome_css_parses_without_gtk_errors() {
        gtk4::init().unwrap();
        let errors = crate::ui::style::css_parse_errors(&css());
        assert!(
            errors.is_empty(),
            "GTK reported CSS parsing errors: {errors:?}"
        );
    }
}
