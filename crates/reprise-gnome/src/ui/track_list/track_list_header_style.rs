//! Scoped visual hierarchy for the sortable track-table headers.
//!
//! GTK owns the `ColumnView` header labels, so the app cannot mark them
//! directly. A root class keeps this selector away from song cells and every
//! other column view; the rule itself is installed app-wide by
//! [`super::style`].

use gtk4::prelude::*;

const TRACK_LIST_CLASS: &str = "reprise-track-list";

/// Quieter column-title rule plus the table's own hairline separators, all
/// scoped to [`TRACK_LIST_CLASS`] roots. Both built-in `GtkColumnView`
/// separators are disabled (`track_list.rs`), so these rules fully own the
/// grid: a 1 px horizontal rule under each cell at white 4.5 % (no vertical
/// column lines at all), and a slightly stronger white 7 % rule under the
/// sortable header. The `rgba(white)` literals are deliberate — these are
/// fixed hairlines on the dark surface, not theme-tinted borders, so they
/// don't route through a palette `@`-color.
///
/// Every column carries a sorter (so its header is clickable — see
/// `track_list_columns`'s dummy-sorter comment), and GTK renders a
/// `sort-indicator` arrow in each header for that. On a column that isn't the
/// active sort, that arrow gets the `unsorted` style class; left visible it
/// paints a faint arrow on *every* column at once, which reads as a table full
/// of sorters. Hiding the `unsorted` indicator (keeping `ascending`/
/// `descending`) leaves a single arrow on the one column actually sorted, while
/// still reserving its width so headers don't shift when the sort changes.
pub(in crate::ui) fn css() -> String {
    format!(
        ".{TRACK_LIST_CLASS} > header label {{ color: @reprise_secondary_fg_color; }}\n\
         .{TRACK_LIST_CLASS} > header {{ \
           border-bottom: 1px solid rgba(255, 255, 255, 0.07); }}\n\
         .{TRACK_LIST_CLASS} sort-indicator.unsorted {{ opacity: 0; }}\n\
         .{TRACK_LIST_CLASS} > listview > row > cell {{ \
           border-left: none; border-right: none; \
           border-bottom: 1px solid rgba(255, 255, 255, 0.045); }}"
    )
}

/// Marks a column view as the track table so the scoped header rule applies.
pub(in crate::ui) fn mark(view: &gtk4::ColumnView) {
    view.add_css_class(TRACK_LIST_CLASS);
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;

    #[test]
    fn header_style_is_subtle_and_scoped_away_from_song_cells() {
        let css = super::css();

        assert!(css.contains(".reprise-track-list > header label"));
        assert!(css.contains("@reprise_secondary_fg_color"));
        assert!(!css.contains("reprise-track-cell"));
        // The inactive-column sort arrows are hidden; the active one
        // (ascending/descending) is untouched so it still shows.
        assert!(css.contains("sort-indicator.unsorted"));
        assert!(css.contains("opacity: 0"));
        assert!(!css.contains(".ascending"));
        assert!(!css.contains(".descending"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn marking_targets_only_the_track_table_root() {
        gtk4::init().unwrap();
        let view = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);
        let unrelated = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);

        super::mark(&view);

        assert!(view.has_css_class("reprise-track-list"));
        assert!(!unrelated.has_css_class("reprise-track-list"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mapped_column_title_uses_the_subtle_foreground_alpha() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        fn find_label(widget: &gtk4::Widget, text: &str) -> Option<gtk4::Label> {
            if let Ok(label) = widget.clone().downcast::<gtk4::Label>() {
                if label.label() == text {
                    return Some(label);
                }
            }
            let mut child = widget.first_child();
            while let Some(current) = child {
                if let Some(found) = find_label(&current, text) {
                    return Some(found);
                }
                child = current.next_sibling();
            }
            None
        }

        gtk4::init().unwrap();
        let view = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);
        view.append_column(&gtk4::ColumnViewColumn::new(
            Some("Title"),
            None::<gtk4::ListItemFactory>,
        ));
        let unrelated = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);
        unrelated.append_column(&gtk4::ColumnViewColumn::new(
            Some("Other"),
            None::<gtk4::ListItemFactory>,
        ));
        let secondary = gtk4::Label::new(Some("Secondary reference"));
        secondary.add_css_class("reprise-text-secondary");
        crate::ui::style::install();
        super::mark(&view);
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.append(&view);
        root.append(&unrelated);
        root.append(&secondary);
        let window = gtk4::Window::new();
        window.set_child(Some(&root));
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        let title = find_label(view.upcast_ref(), "Title").expect("mapped column title label");
        let other =
            find_label(unrelated.upcast_ref(), "Other").expect("unrelated column title label");
        assert_eq!(title.color(), secondary.color());
        assert_ne!(title.color(), other.color());

        window.close();
    }
}
