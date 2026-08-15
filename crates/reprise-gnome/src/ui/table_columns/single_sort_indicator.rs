//! Exactly one visible sort indicator, for any GtkColumnView.
//!
//! GTK's ColumnViewSorter keeps a multi-column sort stack and renders a
//! directional arrow for every column on it, while every table in this app
//! reads only `primary_sort_column`. The secondary arrows therefore claim an
//! order nobody establishes. GTK also updates its own ascending/descending
//! classes one frame after a new primary column is selected, briefly leaving
//! both the old and the new arrow visible.
//!
//! An app-owned class on exactly the primary column's indicator, plus one CSS
//! rule that hides every indicator without it, solves both: the inactive
//! arrows never paint, their width stays reserved, and headers do not shift.
//! GTK's sorter is left untouched.

use gtk4::prelude::*;

const SINGLE_SORT_CLASS: &str = "reprise-single-sort";
pub(in crate::ui) const PRIMARY_SORT_INDICATOR_CLASS: &str = "reprise-primary-sort-indicator";

pub(in crate::ui) fn css() -> String {
    format!(
        ".{SINGLE_SORT_CLASS} sort-indicator:not(.{PRIMARY_SORT_INDICATOR_CLASS}) {{ \
           opacity: 0; }}"
    )
}

pub(in crate::ui) fn mark(view: &gtk4::ColumnView) {
    view.add_css_class(SINGLE_SORT_CLASS);
    view.connect_map(sync_primary_sort_indicator);
    let view_weak = view.downgrade();
    view.columns().connect_items_changed(move |_, _, _, _| {
        if let Some(view) = view_weak.upgrade() {
            sync_primary_sort_indicator(&view);
        }
    });
    if let Some(sorter) = view.sorter().and_downcast::<gtk4::ColumnViewSorter>() {
        let view_weak = view.downgrade();
        sorter.connect_primary_sort_column_notify(move |_| {
            if let Some(view) = view_weak.upgrade() {
                sync_primary_sort_indicator(&view);
            }
        });
    }
}

fn find_sort_indicator(widget: &gtk4::Widget) -> Option<gtk4::Widget> {
    if widget.css_name() == "sort-indicator" {
        return Some(widget.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(indicator) = find_sort_indicator(&current) {
            return Some(indicator);
        }
        child = current.next_sibling();
    }
    None
}

/// Marks exactly the indicator belonging to the `ColumnViewSorter`'s current
/// primary column. The internal header children and `view.columns()` have the
/// same order, including hidden columns.
pub(in crate::ui) fn sync_primary_sort_indicator(view: &gtk4::ColumnView) {
    let primary = view
        .sorter()
        .and_downcast::<gtk4::ColumnViewSorter>()
        .and_then(|sorter| sorter.primary_sort_column());
    let Some(header) = view.first_child() else {
        return;
    };
    let columns = view.columns();
    let mut title = header.first_child();
    let mut index = 0;
    while let Some(current) = title {
        let next = current.next_sibling();
        if let Some(indicator) = find_sort_indicator(&current) {
            let is_primary = columns
                .item(index)
                .and_downcast::<gtk4::ColumnViewColumn>()
                .is_some_and(|column| primary.as_ref() == Some(&column));
            if is_primary {
                indicator.add_css_class(PRIMARY_SORT_INDICATOR_CLASS);
            } else {
                indicator.remove_css_class(PRIMARY_SORT_INDICATOR_CLASS);
            }
        }
        title = next;
        index += 1;
    }
}

#[cfg(test)]
pub(in crate::ui) fn count_primary_indicators(widget: &gtk4::Widget) -> usize {
    let own = usize::from(
        widget.css_name() == "sort-indicator" && widget.has_css_class(PRIMARY_SORT_INDICATOR_CLASS),
    );
    let mut total = own;
    let mut child = widget.first_child();
    while let Some(current) = child {
        total += count_primary_indicators(&current);
        child = current.next_sibling();
    }
    total
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;

    #[test]
    fn single_sort_css_hides_every_non_primary_indicator() {
        let css = super::css();

        assert!(css
            .contains(".reprise-single-sort sort-indicator:not(.reprise-primary-sort-indicator)"));
        assert!(css.contains("opacity: 0"));
        assert!(!css.contains("sort-indicator.unsorted"));
        assert!(!css.contains("reprise-track-list"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn marking_a_column_view_scopes_the_single_sort_rule_to_it() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let view = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);
        let unrelated = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);

        super::mark(&view);

        assert!(view.has_css_class("reprise-single-sort"));
        assert!(!unrelated.has_css_class("reprise-single-sort"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn the_shared_helper_leaves_one_indicator_after_two_sorts() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let view = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);
        let title = gtk4::ColumnViewColumn::new(Some("Title"), None::<gtk4::ListItemFactory>);
        title.set_sorter(Some(&gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal)));
        let artist = gtk4::ColumnViewColumn::new(Some("Artist"), None::<gtk4::ListItemFactory>);
        artist.set_sorter(Some(&gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal)));
        view.append_column(&title);
        view.append_column(&artist);
        let store = gtk4::gio::ListStore::new::<gtk4::glib::Object>();
        let sorted = gtk4::SortListModel::new(Some(store), view.sorter());
        view.set_model(Some(&gtk4::NoSelection::new(Some(sorted))));
        let window = gtk4::Window::builder()
            .default_width(500)
            .default_height(160)
            .child(&view)
            .build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        super::mark(&view);
        view.sort_by_column(Some(&title), gtk4::SortType::Ascending);
        view.sort_by_column(Some(&artist), gtk4::SortType::Ascending);
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert_eq!(super::count_primary_indicators(view.upcast_ref()), 1);
        window.close();
    }
}
