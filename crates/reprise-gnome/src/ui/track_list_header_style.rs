//! Scoped visual hierarchy for the sortable track-table headers.
//!
//! GTK owns the `ColumnView` header labels, so the app cannot mark them
//! directly. A root class keeps this selector away from song cells and every
//! other column view; the rule itself is installed app-wide by
//! [`super::style`].

use gtk4::prelude::*;

const TRACK_LIST_CLASS: &str = "reprise-track-list";

/// Quieter column-title rule, scoped to [`TRACK_LIST_CLASS`] roots.
pub(super) fn css() -> String {
    use super::style::tokens::HEADER_TEXT_ALPHA;
    format!(
        ".{TRACK_LIST_CLASS} > header label {{ color: alpha(currentColor, {HEADER_TEXT_ALPHA}); }}"
    )
}

/// Marks a column view as the track table so the scoped header rule applies.
pub(super) fn mark(view: &gtk4::ColumnView) {
    view.add_css_class(TRACK_LIST_CLASS);
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;

    #[test]
    fn header_style_is_subtle_and_scoped_away_from_song_cells() {
        let css = super::css();

        assert!(css.contains(".reprise-track-list > header label"));
        assert!(css.contains("alpha(currentColor, 0.78)"));
        assert!(!css.contains("reprise-track-cell"));
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
        crate::ui::style::install();
        super::mark(&view);
        let window = gtk4::Window::new();
        window.set_child(Some(&view));
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        let title = find_label(view.upcast_ref(), "Title").expect("mapped column title label");
        assert!((title.color().alpha() - 0.78).abs() < 0.01);

        window.close();
    }
}
