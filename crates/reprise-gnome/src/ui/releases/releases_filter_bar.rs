use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::artist_news::{
    persisted_releases_filter, ReleaseTypeSelection, ReleaseWindow, ReleasesFilter,
    RELEASES_FILTER_HIDDEN_KEY, RELEASES_FILTER_TYPE_KEY, RELEASES_FILTER_WINDOW_KEY,
};
use reprise_core::db::Db;

use crate::ui::filter_bar_layout::{self, FilterBarLayout};
use crate::ui::strings;
use reprise_view::search_scope::SearchScope;

type OnChanged = Rc<dyn Fn(ReleasesFilter)>;
/// SEARCH-8a: fired when the bar itself changes the query — the chip's ×
/// or "Clear all" — so the header entry stops showing a query the view no
/// longer applies.
type OnQueryChanged = Rc<dyn Fn(&str)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TypeChip {
    Album,
    Ep,
    Single,
}

fn toggle_type(
    selection: ReleaseTypeSelection,
    chip: TypeChip,
    active: bool,
) -> ReleaseTypeSelection {
    match chip {
        TypeChip::Album => ReleaseTypeSelection {
            album: active,
            ..selection
        },
        TypeChip::Ep => ReleaseTypeSelection {
            ep: active,
            ..selection
        },
        TypeChip::Single => ReleaseTypeSelection {
            single: active,
            ..selection
        },
    }
}

fn persist_filter(db: &Db, filter: &ReleasesFilter) -> Result<(), rusqlite::Error> {
    reprise_core::library::settings::set_setting(
        db,
        RELEASES_FILTER_TYPE_KEY,
        &filter.release_types.setting_value(),
    )?;
    reprise_core::library::settings::set_setting(
        db,
        RELEASES_FILTER_WINDOW_KEY,
        filter.window.setting_value(),
    )?;
    reprise_core::library::settings::set_bool(db, RELEASES_FILTER_HIDDEN_KEY, filter.hidden)
}

pub(super) struct ReleasesFilterBar {
    root: gtk4::Box,
    layout: FilterBarLayout,
    conn: Rc<Db>,
    filter: RefCell<ReleasesFilter>,
    chips: gtk4::Box,
    add_filter: gtk4::MenuButton,
    add_filter_box: gtk4::Box,
    result_label: gtk4::Label,
    clear_all: gtk4::Button,
    counts: Cell<(usize, usize)>,
    /// SEARCH-8a: this view's query. Deliberately *beside* `ReleasesFilter`
    /// rather than inside it: that type is persisted, while a query must not
    /// be restored on the next launch.
    query: RefCell<String>,
    on_changed: RefCell<Option<OnChanged>>,
    on_query_changed: RefCell<Option<OnQueryChanged>>,
}

impl ReleasesFilterBar {
    pub(super) fn new(conn: Rc<Db>) -> Rc<Self> {
        let filter = persisted_releases_filter(&conn).unwrap_or_default();
        let layout = FilterBarLayout::new();
        let root = layout.root().clone();

        let chips = filter_bar_layout::facet_row();
        layout.fill_facets(&chips);

        let add_filter_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        add_filter_box.set_margin_top(8);
        add_filter_box.set_margin_bottom(8);
        add_filter_box.set_margin_start(8);
        add_filter_box.set_margin_end(8);
        let add_filter_popover = gtk4::Popover::builder().child(&add_filter_box).build();
        let add_filter = gtk4::MenuButton::builder()
            .label(strings::text(strings::RELEASES_ADD_FILTER))
            .popover(&add_filter_popover)
            .build();
        add_filter.add_css_class("pill");
        filter_bar_layout::style_add_filter(&add_filter);
        layout.fill_add_filter(&add_filter);

        let result_label = filter_bar_layout::count_label();
        layout.fill_count(&result_label);
        let clear_all =
            filter_bar_layout::clear_all_button(&strings::text(strings::RELEASES_CLEAR_ALL));
        layout.fill_clear_all(&clear_all);

        let bar = Rc::new(Self {
            root,
            layout,
            conn,
            filter: RefCell::new(filter),
            chips,
            add_filter,
            add_filter_box,
            result_label,
            clear_all,
            counts: Cell::new((0, 0)),
            query: RefCell::new(String::new()),
            on_changed: RefCell::new(None),
            on_query_changed: RefCell::new(None),
        });
        wire_clear_all(&bar);
        bar.rebuild();
        bar
    }

    pub(super) fn widget(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    pub(super) fn filter(&self) -> ReleasesFilter {
        self.filter.borrow().clone()
    }

    pub(super) fn set_on_changed(&self, callback: impl Fn(ReleasesFilter) + 'static) {
        *self.on_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn set_on_query_changed(&self, callback: impl Fn(&str) + 'static) {
        *self.on_query_changed.borrow_mut() = Some(Rc::new(callback));
    }

    /// FIL-1d: matched against release title and artist.
    pub(super) fn query(&self) -> String {
        self.query.borrow().clone()
    }

    /// SEARCH-8a: this view's query, handed in by the shell.
    pub(super) fn set_query(self: &Rc<Self>, query: &str) {
        if *self.query.borrow() == query.trim() {
            return;
        }
        self.query.replace(query.trim().to_owned());
        self.rebuild();
        self.notify_changed();
    }

    fn clear_query(self: &Rc<Self>) {
        if self.query.borrow().is_empty() {
            return;
        }
        self.query.replace(String::new());
        // The borrow ends on this line. Left inside the `if let` condition it
        // would live for the whole body, and a callback that touched this
        // `RefCell` would panic instead of misbehaving visibly.
        let callback = self.on_query_changed.borrow().clone();
        if let Some(callback) = callback {
            callback("");
        }
    }

    pub(super) fn set_counts(self: &Rc<Self>, shown: usize, total: usize) {
        self.counts.set((shown, total));
        self.rebuild();
    }

    /// NR-25/FIL-2a: takes the filter row back to its default and clears this
    /// section's transient search query.
    ///
    /// Back to *default*, not to the widest scope: since the default is itself
    /// a filter — five years, no singles — a "clear" that landed on the widest
    /// would be a one-way door. It would be permanently offered, and the way
    /// back would exist only chip by chip.
    pub(super) fn clear_all(self: &Rc<Self>) {
        self.clear_query();
        self.apply_filter(ReleasesFilter::default());
    }

    /// Opens the catalog as far as it goes — every type, every year.
    ///
    /// This is what the zero-result step and the shell's cross-section
    /// "clear filters" need: at zero results under the default filter,
    /// returning to that same default would change nothing on screen.
    pub(super) fn show_widest(self: &Rc<Self>) {
        self.clear_query();
        self.apply_filter(ReleasesFilter::widest(false));
    }

    fn apply_filter(self: &Rc<Self>, filter: ReleasesFilter) {
        if let Err(error) = persist_filter(&self.conn, &filter) {
            tracing::warn!(%error, "could not persist Releases filter");
            return;
        }
        self.filter.replace(filter);
        self.rebuild();
        self.notify_changed();
    }

    fn notify_changed(&self) {
        let callback = self.on_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(self.filter());
        }
    }

    fn rebuild(self: &Rc<Self>) {
        while let Some(child) = self.chips.first_child() {
            self.chips.remove(&child);
        }
        let filter = self.filter();
        let query = self.query();
        let weak = Rc::downgrade(self);
        self.layout
            .replace_scoped_search(SearchScope::Releases, &query, move || {
                let Some(bar) = weak.upgrade() else {
                    return;
                };
                bar.clear_query();
                bar.rebuild();
                bar.notify_changed();
            });
        for (chip, selected, label) in [
            (
                TypeChip::Album,
                filter.release_types.album,
                strings::RELEASES_ALBUM,
            ),
            (TypeChip::Ep, filter.release_types.ep, strings::RELEASES_EP),
            (
                TypeChip::Single,
                filter.release_types.single,
                strings::RELEASES_SINGLE,
            ),
        ] {
            if selected {
                self.append_type_chip(chip, label);
            }
        }
        self.append_window_chip(filter.window);
        if filter.hidden {
            self.append_hidden_chip();
        }
        self.chips.set_visible(self.chips.first_child().is_some());
        self.rebuild_add_filter(&filter);

        // Measured against the default, not the widest: the default view is
        // the quiet one, so it offers no "Clear all" and no accent — while
        // still naming its total, which is the only sign that five years and
        // no singles are a choice someone made.
        let dirty = filter != ReleasesFilter::default() || !query.is_empty();
        self.clear_all.set_visible(dirty);
        let (shown, total) = self.counts.get();
        let text;
        let presentation = if dirty && shown != total {
            text = strings::release_count_line_markup(shown, total);
            filter_bar_layout::CountPresentation::RestrictedMarkup(&text)
        } else {
            text = release_count_presentation(shown, total);
            filter_bar_layout::CountPresentation::Plain(&text)
        };
        filter_bar_layout::present_count(&self.result_label, presentation);
    }

    fn append_type_chip(self: &Rc<Self>, chip: TypeChip, label: &str) {
        let button = gtk4::ToggleButton::with_label(&strings::text(label));
        button.add_css_class("pill");
        button.set_active(true);
        let weak = Rc::downgrade(self);
        button.connect_toggled(move |button| {
            let Some(bar) = weak.upgrade() else {
                return;
            };
            let current = bar.filter();
            bar.apply_filter(ReleasesFilter {
                release_types: toggle_type(current.release_types, chip, button.is_active()),
                ..current
            });
        });
        self.chips.append(&button);
    }

    fn append_window_chip(self: &Rc<Self>, selected: ReleaseWindow) {
        let menu = gtk4::MenuButton::new();
        menu.set_label(&strings::text(window_label(selected)));
        menu.add_css_class("pill");
        let list = gtk4::ListBox::new();
        list.add_css_class("boxed-list");
        let values = [
            ReleaseWindow::OneYear,
            ReleaseWindow::FiveYears,
            ReleaseWindow::TenYears,
            ReleaseWindow::All,
        ];
        for value in values {
            list.append(&chooser_row(&strings::text(window_label(value))));
        }
        let popover = gtk4::Popover::new();
        popover.set_child(Some(&padded(&list)));
        menu.set_popover(Some(&popover));
        let weak = Rc::downgrade(self);
        list.connect_row_activated(move |_, row| {
            let Some(bar) = weak.upgrade() else {
                return;
            };
            let Some(window) = values.get(row.index() as usize).copied() else {
                return;
            };
            bar.apply_filter(ReleasesFilter {
                window,
                ..bar.filter()
            });
        });
        self.chips.append(&menu);
    }

    fn append_hidden_chip(self: &Rc<Self>) {
        let button = gtk4::ToggleButton::with_label(&strings::text(strings::RELEASES_HIDDEN));
        button.add_css_class("pill");
        button.set_active(true);
        let weak = Rc::downgrade(self);
        button.connect_toggled(move |button| {
            let Some(bar) = weak.upgrade() else {
                return;
            };
            bar.apply_filter(ReleasesFilter {
                hidden: button.is_active(),
                ..bar.filter()
            });
        });
        self.chips.append(&button);
    }

    fn rebuild_add_filter(self: &Rc<Self>, filter: &ReleasesFilter) {
        while let Some(child) = self.add_filter_box.first_child() {
            self.add_filter_box.remove(&child);
        }
        let mut choices = 0_usize;
        for (chip, selected, label) in [
            (
                TypeChip::Album,
                filter.release_types.album,
                strings::RELEASES_ALBUM,
            ),
            (TypeChip::Ep, filter.release_types.ep, strings::RELEASES_EP),
            (
                TypeChip::Single,
                filter.release_types.single,
                strings::RELEASES_SINGLE,
            ),
        ] {
            if selected {
                continue;
            }
            choices += 1;
            let button = gtk4::Button::with_label(&strings::text(label));
            button.add_css_class("flat");
            button.set_halign(gtk4::Align::Fill);
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                let Some(bar) = weak.upgrade() else {
                    return;
                };
                let current = bar.filter();
                bar.add_filter.popdown();
                bar.apply_filter(ReleasesFilter {
                    release_types: toggle_type(current.release_types, chip, true),
                    ..current
                });
            });
            self.add_filter_box.append(&button);
        }
        if !filter.hidden {
            choices += 1;
            let button = gtk4::Button::with_label(&strings::text(strings::RELEASES_HIDDEN));
            button.add_css_class("flat");
            button.set_halign(gtk4::Align::Fill);
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                let Some(bar) = weak.upgrade() else {
                    return;
                };
                bar.add_filter.popdown();
                bar.apply_filter(ReleasesFilter {
                    hidden: true,
                    ..bar.filter()
                });
            });
            self.add_filter_box.append(&button);
        }
        self.add_filter.set_sensitive(choices > 0);
    }
}

/// The unaccented count text. At the widest scope shown and total are the
/// same number, and "629 of 629 gaps" says less than "629 gaps".
fn release_count_presentation(shown: usize, total: usize) -> String {
    if shown == total {
        strings::release_total_line(total)
    } else {
        strings::release_count_line(shown, total)
    }
}

fn window_label(window: ReleaseWindow) -> &'static str {
    match window {
        ReleaseWindow::OneYear => strings::RELEASES_WINDOW_ONE_YEAR,
        ReleaseWindow::FiveYears => strings::RELEASES_WINDOW_FIVE_YEARS,
        ReleaseWindow::TenYears => strings::RELEASES_WINDOW_TEN_YEARS,
        ReleaseWindow::All => strings::RELEASES_WINDOW_ALL,
    }
}

fn chooser_row(label: &str) -> gtk4::ListBoxRow {
    let label = gtk4::Label::builder()
        .label(label)
        .xalign(0.0)
        .margin_top(7)
        .margin_bottom(7)
        .margin_start(10)
        .margin_end(10)
        .build();
    gtk4::ListBoxRow::builder().child(&label).build()
}

fn padded(child: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    page.set_margin_top(8);
    page.set_margin_bottom(8);
    page.set_margin_start(8);
    page.set_margin_end(8);
    page.append(child);
    page
}

fn wire_clear_all(bar: &Rc<ReleasesFilterBar>) {
    let weak = Rc::downgrade(bar);
    bar.clear_all.connect_clicked(move |_| {
        if let Some(bar) = weak.upgrade() {
            bar.clear_all();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nr_25_type_toggles_are_independent_and_empty_means_every_type() {
        let selection = ReleaseTypeSelection::default();
        let selection = toggle_type(selection, TypeChip::Album, false);
        let selection = toggle_type(selection, TypeChip::Ep, false);
        assert!(selection.is_empty());
        assert!(selection.includes("Album"));
        assert!(selection.includes("EP"));
        assert!(selection.includes("Single"));
    }

    #[test]
    fn nr_25_widest_scope_count_line_names_shown_and_total() {
        // Nothing is filtered away, so the line states one number.
        assert_eq!(release_count_presentation(19, 19), "19 gaps");
        // The default view filters, and says so without an alarm.
        assert_eq!(release_count_presentation(168, 629), "168 of 629 gaps");
    }

    #[test]
    fn sticky_release_filter_round_trips_every_facet() {
        let conn = crate::test_db::open().unwrap();
        let filter = ReleasesFilter {
            release_types: ReleaseTypeSelection {
                album: true,
                ep: false,
                single: true,
            },
            window: ReleaseWindow::TenYears,
            hidden: true,
        };
        persist_filter(&conn, &filter).unwrap();
        assert_eq!(persisted_releases_filter(&conn).unwrap(), filter);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nr_25_filter_header_is_permanent_and_reserves_its_height() {
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let bar = ReleasesFilterBar::new(conn);
        assert_eq!(
            bar.root.height_request(),
            filter_bar_layout::FILTER_BAR_MIN_HEIGHT
        );
        assert!(bar.chips.first_child().is_some());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_2a_releases_fill_filters_count_and_clear_slots_without_a_caption() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let bar = ReleasesFilterBar::new(conn);
        bar.set_query("falling");
        bar.set_counts(15, 1_664);

        assert!(bar.layout.slot_contains(
            crate::ui::filter_bar_layout::FilterBarSlot::Facets,
            &bar.chips
        ));
        assert!(bar.layout.slot_contains(
            crate::ui::filter_bar_layout::FilterBarSlot::AddFilter,
            &bar.add_filter
        ));
        assert!(bar.layout.slot_contains(
            crate::ui::filter_bar_layout::FilterBarSlot::Count,
            &bar.result_label
        ));
        assert!(bar.layout.slot_contains(
            crate::ui::filter_bar_layout::FilterBarSlot::ClearAll,
            &bar.clear_all
        ));
        let first = bar
            .layout
            .slot_child(crate::ui::filter_bar_layout::FilterBarSlot::Search)
            .expect("the query produces the first chip");
        assert!(first
            .downcast::<gtk4::Button>()
            .ok()
            .and_then(|button| button.label())
            .is_some_and(|label| label.starts_with('⌕')));
        assert!(!descendant_labels(bar.widget())
            .iter()
            .any(|text| text == "FILTER"));
        assert!(bar.clear_all.is_visible());
    }

    fn descendant_labels(widget: &impl IsA<gtk4::Widget>) -> Vec<String> {
        let mut labels = Vec::new();
        let mut child = widget.as_ref().first_child();
        while let Some(current) = child {
            if let Ok(label) = current.clone().downcast::<gtk4::Label>() {
                labels.push(label.text().to_string());
            }
            labels.extend(descendant_labels(&current));
            child = current.next_sibling();
        }
        labels
    }

    /// NR-25: the default view is the quiet one. The row still names its
    /// total, but offers no "Clear all" and accents nothing — the button
    /// appears when the reader has changed something, not because the
    /// default itself filters.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nr_25_the_default_filter_row_offers_no_clear_all() {
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let bar = ReleasesFilterBar::new(conn);
        bar.set_counts(168, 629);

        assert!(
            !bar.clear_all.get_visible(),
            "a default filter row has nothing to clear"
        );

        bar.apply_filter(ReleasesFilter::widest(false));

        assert!(
            bar.clear_all.get_visible(),
            "widening the scope is a change, and a change is undoable"
        );
    }
}
