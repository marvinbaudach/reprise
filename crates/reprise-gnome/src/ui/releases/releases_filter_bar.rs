use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::artist_news::{
    persisted_releases_filter, ReleaseTypeSelection, ReleaseWindow, ReleasesFilter,
    RELEASES_FILTER_HIDDEN_KEY, RELEASES_FILTER_TYPE_KEY, RELEASES_FILTER_WINDOW_KEY,
};
use reprise_core::db::Db;

use crate::ui::browse::browse_bar::CHIP_CSS_CLASS;
use crate::ui::search_chip;
use crate::ui::strings;
use reprise_view::search_scope::SearchScope;

const FILTER_BAR_MIN_HEIGHT: i32 = 34;

type OnChanged = Rc<dyn Fn(ReleasesFilter)>;
/// SEARCH-8: fired when the bar itself changes the query — the chip's ×
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
    conn: Rc<Db>,
    filter: RefCell<ReleasesFilter>,
    chips: gtk4::FlowBox,
    result_label: gtk4::Label,
    clear_all: gtk4::Button,
    counts: Cell<(usize, usize)>,
    /// SEARCH-8: this section's query. Deliberately *beside* `ReleasesFilter`
    /// rather than inside it: that type is persisted, while a query must not
    /// be restored on the next launch.
    query: RefCell<String>,
    on_changed: RefCell<Option<OnChanged>>,
    on_query_changed: RefCell<Option<OnQueryChanged>>,
}

impl ReleasesFilterBar {
    pub(super) fn new(conn: Rc<Db>) -> Rc<Self> {
        let filter = persisted_releases_filter(&conn).unwrap_or_default();
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        root.set_margin_top(6);
        root.set_margin_bottom(6);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.set_size_request(-1, FILTER_BAR_MIN_HEIGHT);
        root.add_css_class("toolbar");

        let section_label = gtk4::Label::new(Some(&strings::text(strings::RELEASES_FILTER)));
        section_label.add_css_class("dim-label");
        section_label.add_css_class("caption-heading");
        root.append(&section_label);

        let chips = gtk4::FlowBox::builder()
            .selection_mode(gtk4::SelectionMode::None)
            .column_spacing(6)
            .row_spacing(4)
            .hexpand(true)
            .max_children_per_line(20)
            .build();
        root.append(&chips);

        let result_label = gtk4::Label::new(None);
        result_label.add_css_class("dim-label");
        result_label.add_css_class("caption");
        root.append(&result_label);
        let clear_all = gtk4::Button::with_label(&strings::text(strings::RELEASES_CLEAR_ALL));
        clear_all.add_css_class("flat");
        clear_all.add_css_class(CHIP_CSS_CLASS);
        root.append(&clear_all);

        let bar = Rc::new(Self {
            root,
            conn,
            filter: RefCell::new(filter),
            chips,
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

    /// SEARCH-8: this section's query, handed in by the shell.
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

    /// NR-25/FIL-2: takes the filter row back to its default and clears this
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
        self.chips.remove_all();
        let filter = self.filter();
        let query = self.query();
        if !query.is_empty() {
            self.append_search_chip(&query);
        }
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
            self.append_type_chip(chip, selected, label);
        }
        self.append_window_chip(filter.window);
        self.append_hidden_chip(filter.hidden);

        // Measured against the default, not the widest: the default view is
        // the quiet one, so it offers no "Clear all" and no accent — while
        // still naming its total, which is the only sign that five years and
        // no singles are a choice someone made.
        let dirty = filter != ReleasesFilter::default() || !query.is_empty();
        self.clear_all.set_visible(dirty);
        let (shown, total) = self.counts.get();
        if dirty && shown != total {
            self.result_label
                .set_markup(&strings::release_count_line_markup(shown, total));
            self.result_label.add_css_class("accent");
        } else {
            self.result_label.remove_css_class("accent");
            self.result_label
                .set_text(&release_count_presentation(shown, total));
        }
    }

    fn append_search_chip(self: &Rc<Self>, query: &str) {
        let weak = Rc::downgrade(self);
        let chip = search_chip::build(SearchScope::Releases, query, move || {
            let Some(bar) = weak.upgrade() else {
                return;
            };
            bar.clear_query();
            bar.rebuild();
            bar.notify_changed();
        });
        self.chips.append(&chip);
    }

    fn append_type_chip(self: &Rc<Self>, chip: TypeChip, selected: bool, label: &str) {
        let button = gtk4::ToggleButton::with_label(&strings::text(label));
        button.add_css_class("pill");
        button.set_active(selected);
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

    fn append_hidden_chip(self: &Rc<Self>, selected: bool) {
        let button = gtk4::ToggleButton::with_label(&strings::text(strings::RELEASES_HIDDEN));
        button.add_css_class("pill");
        button.set_active(selected);
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
        assert_eq!(bar.root.height_request(), FILTER_BAR_MIN_HEIGHT);
        assert!(bar.chips.child_at_index(0).is_some());
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
