//! Row chrome for the Plugins page: the leading chevron gutter and the flat
//! row surface the online-content drafts ask for.
//!
//! `docs/plans/plugins-online-content-master-hierarchy.md`, second draft: the
//! card fill goes, rows run over the full width and are separated by hairlines
//! only, and the expander chevron moves from behind the switch into a gutter
//! left of the title. The gutter is reserved on **every** row — also on rows
//! that never open — so all row titles keep one left edge, and the group
//! headings are indented by the same amount so they share that edge.

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

/// Set on the Plugins page; every rule in [`css`] is scoped to it.
pub(in crate::ui) const FLAT_ROWS_CLASS: &str = "reprise-plugin-rows";
/// Set on the online-content master row: it carries the heading the group no
/// longer prints a second time.
pub(in crate::ui) const MASTER_ROW_CLASS: &str = "reprise-online-master";

const CHEVRON_COLLAPSED_ICON: &str = "pan-end-symbolic";
const CHEVRON_EXPANDED_ICON: &str = "pan-down-symbolic";

/// Measured against libadwaita 1.9 on 2026-08-22: with the gutter icon in
/// place a row title starts 42px right of an un-indented group heading, so the
/// heading needs exactly that indent to land on the same left edge.
const HEADING_INDENT_PX: u32 = 42;

/// The hairline between two rows, as a fraction of the foreground colour.
const HAIRLINE_ALPHA: f32 = 0.12;

fn gutter_image(icon: Option<&str>) -> gtk4::Image {
    let image = gtk4::Image::builder()
        .accessible_role(gtk4::AccessibleRole::Presentation)
        .can_target(false)
        .can_focus(false)
        .build();
    match icon {
        Some(icon) => image.set_icon_name(Some(icon)),
        // No icon, but the same footprint: the gutter stays open so titles of
        // rows that never expand start where the expandable ones start.
        None => image.set_icon_name(Some(CHEVRON_COLLAPSED_ICON)),
    }
    if icon.is_none() {
        image.set_opacity(0.0);
    }
    image
}

/// Keeps the gutter free on a row that has no chevron of its own.
pub(in crate::ui) fn reserve_gutter(row: &impl IsA<adw::ActionRow>) {
    row.as_ref().add_prefix(&gutter_image(None));
}

/// Moves the chevron of an expandable row into the gutter. libadwaita keeps
/// its own arrow behind the switch — [`css`] makes that one invisible while
/// leaving its slot in place, which is what keeps every switch on one right
/// edge (`SET-14`).
pub(in crate::ui) fn attach_chevron(row: &adw::ExpanderRow) {
    let chevron = gutter_image(Some(CHEVRON_COLLAPSED_ICON));
    row.add_prefix(&chevron);
    let chevron = chevron.downgrade();
    row.connect_expanded_notify(move |row| {
        let Some(chevron) = chevron.upgrade() else {
            return;
        };
        chevron.set_icon_name(Some(if row.is_expanded() {
            CHEVRON_EXPANDED_ICON
        } else {
            CHEVRON_COLLAPSED_ICON
        }));
    });
}

pub(in crate::ui) fn css() -> String {
    format!(
        "/* --- Plugins rows: no card, hairlines, one left edge --- */ \
         .{FLAT_ROWS_CLASS} .boxed-list {{ \
           background-color: transparent; \
           background-image: none; \
           border: none; \
           box-shadow: none; \
           border-radius: 0; }} \
         .{FLAT_ROWS_CLASS} .boxed-list > row {{ \
           background-color: transparent; \
           background-image: none; }} \
         .{FLAT_ROWS_CLASS} .boxed-list > row:not(:first-child) {{ \
           border-top: 1px solid alpha(@window_fg_color, {HAIRLINE_ALPHA}); }} \
         .{FLAT_ROWS_CLASS} box.labels {{ margin-left: {HEADING_INDENT_PX}px; }} \
         .{FLAT_ROWS_CLASS} image.expander-row-arrow {{ opacity: 0; }} \
         .{MASTER_ROW_CLASS} label.title {{ \
           font-weight: bold; \
           font-size: 1.2em; }}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_14a_the_chevron_gutter_is_reserved_on_every_row_kind() {
        let css = css();

        assert!(css.contains(&format!(".{FLAT_ROWS_CLASS} image.expander-row-arrow")));
        assert!(css.contains("opacity: 0"));
    }

    #[test]
    fn set_11_the_flat_rows_drop_the_card_and_keep_a_hairline() {
        let css = css();

        assert!(css.contains(&format!(".{FLAT_ROWS_CLASS} .boxed-list")));
        assert!(css.contains("background-color: transparent"));
        assert!(css.contains("border-top: 1px solid alpha(@window_fg_color, 0.12)"));
    }

    #[test]
    fn set_11_group_headings_share_the_left_edge_with_row_titles() {
        assert!(css().contains(&format!("box.labels {{ margin-left: {HEADING_INDENT_PX}px")));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn the_gutter_placeholder_is_presentation_only() {
        gtk4::init().unwrap();
        let placeholder = gutter_image(None);

        assert_eq!(
            placeholder.accessible_role(),
            gtk4::AccessibleRole::Presentation
        );
        assert_eq!(placeholder.opacity(), 0.0);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn the_chevron_follows_the_expansion_state() {
        gtk4::init().unwrap();
        let row = adw::ExpanderRow::new();
        attach_chevron(&row);
        let chevron = row
            .first_child()
            .and_then(|child| find_chevron(&child))
            .expect("the expander row must carry a gutter chevron");

        assert_eq!(chevron.icon_name().as_deref(), Some(CHEVRON_COLLAPSED_ICON));
        row.set_expanded(true);
        assert_eq!(chevron.icon_name().as_deref(), Some(CHEVRON_EXPANDED_ICON));
        row.set_expanded(false);
        assert_eq!(chevron.icon_name().as_deref(), Some(CHEVRON_COLLAPSED_ICON));
    }

    #[cfg(test)]
    fn find_chevron(widget: &gtk4::Widget) -> Option<gtk4::Image> {
        if let Ok(image) = widget.clone().downcast::<gtk4::Image>() {
            if image
                .icon_name()
                .is_some_and(|name| name == CHEVRON_COLLAPSED_ICON || name == CHEVRON_EXPANDED_ICON)
                && image.opacity() > 0.0
            {
                return Some(image);
            }
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            if let Some(found) = find_chevron(&current) {
                return Some(found);
            }
            child = current.next_sibling();
        }
        None
    }
}
