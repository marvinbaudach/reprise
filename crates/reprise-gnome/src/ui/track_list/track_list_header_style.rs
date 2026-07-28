//! Scoped visual hierarchy for the sortable track-table headers.
//!
//! GTK owns the `ColumnView` header labels, so the app cannot mark them
//! directly. A root class keeps this selector away from song cells and every
//! other column view; the rule itself is installed app-wide by
//! [`super::style`].

use gtk4::prelude::*;

const TRACK_LIST_CLASS: &str = "reprise-track-list";
const PRIMARY_SORT_INDICATOR_CLASS: &str = "reprise-primary-sort-indicator";

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
/// `sort-indicator` arrow in each header for that. GTK updates its own
/// `ascending`/`descending` classes one frame after a new primary column is
/// selected, briefly leaving both the old and new arrows visible. The app-owned
/// [`PRIMARY_SORT_INDICATOR_CLASS`] is synchronized in the same call that
/// changes sorting, so the inactive arrow disappears immediately while its
/// width stays reserved and headers do not shift.
pub(in crate::ui) fn css() -> String {
    format!(
        ".{TRACK_LIST_CLASS} > header label {{ color: @reprise_secondary_fg_color; }}\n\
         .{TRACK_LIST_CLASS} > header {{ \
           border-bottom: 1px solid rgba(255, 255, 255, 0.07); }}\n\
         .{TRACK_LIST_CLASS} sort-indicator:not(.{PRIMARY_SORT_INDICATOR_CLASS}) {{ \
           opacity: 0; }}\n\
         .{TRACK_LIST_CLASS} > listview > row > cell {{ \
           border-left: none; border-right: none; \
           border-bottom: 1px solid rgba(255, 255, 255, 0.045); }}"
    )
}

/// Marks a column view as the track table so the scoped header rule applies.
pub(in crate::ui) fn mark(view: &gtk4::ColumnView) {
    view.add_css_class(TRACK_LIST_CLASS);
    view.connect_map(sync_primary_sort_indicator);
    let view_weak = view.downgrade();
    view.columns().connect_items_changed(move |_, _, _, _| {
        if let Some(view) = view_weak.upgrade() {
            sync_primary_sort_indicator(&view);
        }
    });
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
pub(super) fn sync_primary_sort_indicator(view: &gtk4::ColumnView) {
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
mod tests {
    use gtk4::prelude::*;

    #[test]
    fn header_style_is_subtle_and_scoped_away_from_song_cells() {
        let css = super::css();

        assert!(css.contains(".reprise-track-list > header label"));
        assert!(css.contains("@reprise_secondary_fg_color"));
        assert!(!css.contains("reprise-track-cell"));
        assert!(css.contains("sort-indicator:not(.reprise-primary-sort-indicator)"));
        assert!(css.contains("opacity: 0"));
        assert!(!css.contains("sort-indicator.unsorted"));
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

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn inactive_sort_columns_render_no_arrow() {
        let _main_context = crate::ui::test_main_context::lock_main_context();

        fn collect_sort_indicators(widget: &gtk4::Widget, indicators: &mut Vec<gtk4::Widget>) {
            if widget.css_name() == "sort-indicator" {
                indicators.push(widget.clone());
            }
            let mut child = widget.first_child();
            while let Some(current) = child {
                collect_sort_indicators(&current, indicators);
                child = current.next_sibling();
            }
        }

        fn rendered_alpha(window: &gtk4::Window, widget: &gtk4::Widget) -> usize {
            let paintable = gtk4::WidgetPaintable::new(Some(widget));
            let snapshot = gtk4::Snapshot::new();
            paintable.snapshot(
                &snapshot,
                f64::from(widget.width()),
                f64::from(widget.height()),
            );
            let Some(node) = snapshot.to_node() else {
                return 0;
            };
            let renderer = window
                .native()
                .and_then(|native| native.renderer())
                .expect("the presented window has a renderer");
            let texture = renderer.render_texture(&node, None);
            let stride = texture.width() as usize * 4;
            let mut pixels = vec![0; stride * texture.height() as usize];
            texture.download(&mut pixels, stride);
            pixels.chunks_exact(4).filter(|pixel| pixel[3] != 0).count()
        }

        fn settle_transitions() {
            crate::ui::test_settle::settle_for(std::time::Duration::from_millis(300));
        }

        gtk4::init().unwrap();
        crate::ui::style::install();

        let view = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);
        super::mark(&view);
        let artist = gtk4::ColumnViewColumn::new(Some("Artist"), None::<gtk4::ListItemFactory>);
        artist.set_sorter(Some(&gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal)));
        let rating = gtk4::ColumnViewColumn::new(Some("Rating"), None::<gtk4::ListItemFactory>);
        rating.set_sorter(Some(&gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal)));
        view.append_column(&artist);
        view.append_column(&rating);
        let store = gtk4::gio::ListStore::new::<gtk4::glib::Object>();
        let sort_model = gtk4::SortListModel::new(Some(store), view.sorter());
        let selection = gtk4::NoSelection::new(Some(sort_model));
        view.set_model(Some(&selection));
        super::super::track_list_sort::sort_by_column(&view, &rating, gtk4::SortType::Ascending);

        let window = gtk4::Window::new();
        window.set_default_size(500, 160);
        window.set_child(Some(&view));
        window.present();
        settle_transitions();

        super::super::track_list_sort::sort_by_column(&view, &artist, gtk4::SortType::Ascending);
        while gtk4::glib::MainContext::default().iteration(false) {}

        let mut indicators = Vec::new();
        collect_sort_indicators(view.upcast_ref(), &mut indicators);
        assert_eq!(
            indicators.len(),
            2,
            "one indicator widget per sortable column"
        );
        let rendered: Vec<_> = indicators
            .iter()
            .map(|indicator| (indicator.css_classes(), rendered_alpha(&window, indicator)))
            .collect();
        let painted = rendered.iter().filter(|(_, alpha)| *alpha > 0).count();
        assert_eq!(
            painted, 1,
            "only the primary sort indicator may be visible; got {rendered:?}"
        );

        window.close();
    }
}
