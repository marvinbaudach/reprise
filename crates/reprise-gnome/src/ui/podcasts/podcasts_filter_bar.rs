use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::db::Db;
use reprise_core::podcasts::{self, PodcastKind};

// `active` lives in `podcasts_presentation` (also read by the empty-state
// classification), not duplicated here.
use super::podcasts_presentation::{active, LibrarySummary, PodcastFilter};
use crate::ui::filter_bar_layout::{self, FilterBarLayout};
use crate::ui::strings;
#[cfg(test)]
use crate::ui::style::buttons;
use reprise_view::search_scope::SearchScope;

type OnChanged = Rc<dyn Fn(PodcastFilter)>;
/// SEARCH-8a: fired when the bar itself changes the query — the chip's ×, or a
/// jump that had to relax the search to reach its episode. The shell listens
/// so the header entry stops showing a query the view no longer applies.
type OnQueryChanged = Rc<dyn Fn(&str)>;

pub(super) struct PodcastsFilterBar {
    root: gtk4::Box,
    layout: FilterBarLayout,
    conn: Rc<Db>,
    filter: RefCell<PodcastFilter>,
    committed_query: RefCell<String>,
    chips: gtk4::Box,
    add_filter: gtk4::MenuButton,
    popover_box: gtk4::Box,
    popover: gtk4::Popover,
    result: gtk4::Label,
    clear_all: gtk4::Button,
    clear_selection: gtk4::Button,
    base_result: RefCell<String>,
    /// Whether the summary line is markup (FIL-2a's accented count) or plain
    /// text, so the selection suffix is appended in the same mode.
    base_is_markup: Cell<bool>,
    on_changed: RefCell<Option<OnChanged>>,
    on_query_changed: RefCell<Option<OnQueryChanged>>,
    // `POD-15`: kept past the constructor because the summary line below the
    // chips names channels on the YouTube page and shows on the RSS one.
    kind: PodcastKind,
}

impl PodcastsFilterBar {
    pub(super) fn new(conn: Rc<Db>, kind: PodcastKind) -> Rc<Self> {
        let stored = podcasts::config::load_filter(&conn).unwrap_or_default();
        let filter = PodcastFilter::from_facets(&podcasts::config::PodcastFilterConfig {
            source: None,
            ..stored
        });
        let layout = FilterBarLayout::new();
        let root = layout.root().clone();

        let chips = filter_bar_layout::facet_row();
        layout.fill_facets(&chips);
        let popover_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        popover_box.set_margin_top(8);
        popover_box.set_margin_bottom(8);
        popover_box.set_margin_start(8);
        popover_box.set_margin_end(8);
        let popover = gtk4::Popover::new();
        popover.set_child(Some(&popover_box));
        let add_filter = gtk4::MenuButton::builder()
            .label(format!("+ {}", strings::text(strings::PODCAST_ADD_FILTER)))
            .popover(&popover)
            .build();
        add_filter.add_css_class("pill");
        filter_bar_layout::style_add_filter(&add_filter);
        layout.fill_add_filter(&add_filter);
        let result = filter_bar_layout::count_label();
        layout.fill_count(&result);
        let clear_all =
            filter_bar_layout::clear_all_button(&strings::text(strings::PODCAST_CLEAR_ALL));
        clear_all.set_visible(false);
        layout.fill_clear_all(&clear_all);
        let clear_selection =
            gtk4::Button::with_label(&strings::text(strings::PODCAST_CLEAR_SELECTION));
        clear_selection.add_css_class("flat");
        clear_selection.set_visible(false);
        clear_selection.set_action_name(Some("podcasts.clear-selection"));
        layout.fill_trailing_action(&clear_selection);

        let bar = Rc::new(Self {
            root,
            layout,
            conn,
            filter: RefCell::new(filter),
            committed_query: RefCell::new(String::new()),
            chips,
            add_filter,
            popover_box,
            popover,
            result,
            clear_all,
            clear_selection,
            base_result: RefCell::new(String::new()),
            base_is_markup: Cell::new(false),
            on_changed: RefCell::new(None),
            on_query_changed: RefCell::new(None),
            kind,
        });
        {
            let weak = Rc::downgrade(&bar);
            bar.clear_all.connect_clicked(move |_| {
                if let Some(bar) = weak.upgrade() {
                    bar.clear_all();
                }
            });
        }
        bar.rebuild();
        bar
    }

    pub(super) fn widget(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    pub(super) fn filter(&self) -> PodcastFilter {
        self.filter.borrow().clone()
    }

    /// `POD-9`: the rendered header line — the library summary or, once a
    /// filter narrows the view, the "shown of total" count `set_context`
    /// writes into `result`. Exists purely so a test can prove the header
    /// text `podcasts_view::render` computes actually reaches this widget,
    /// rather than only re-proving the pure projection/formatting `set_context`
    /// itself delegates to.
    pub(super) fn result_text(&self) -> String {
        self.result.text().to_string()
    }

    pub(super) fn set_on_changed(&self, callback: impl Fn(PodcastFilter) + 'static) {
        *self.on_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn set_on_query_changed(&self, callback: impl Fn(&str) + 'static) {
        *self.on_query_changed.borrow_mut() = Some(Rc::new(callback));
    }

    /// SEARCH-8a: the view's query, handed in by the shell as the user
    /// types. A no-op for an unchanged query so a re-entry into the section
    /// does not re-render the whole list.
    pub(super) fn set_query(self: &Rc<Self>, query: &str) {
        let current = self.filter();
        if current.query == query.trim() {
            return;
        }
        self.apply_internal(current.with_query(query), false);
    }

    pub(super) fn set_committed_query(self: &Rc<Self>, query: &str) {
        if *self.committed_query.borrow() == query {
            return;
        }
        self.committed_query.replace(query.to_owned());
        self.rebuild();
    }

    fn committed_query(&self) -> String {
        self.committed_query.borrow().clone()
    }

    pub(super) fn set_context(
        self: &Rc<Self>,
        shown: usize,
        summary: LibrarySummary,
        selected_count: usize,
    ) {
        let filter = self.filter();
        let base_result = if active(&filter) {
            // FIL-2a: a filtered list counts in its own unit with the shown
            // number accented.
            self.base_is_markup.set(true);
            match self.kind {
                PodcastKind::Rss => strings::podcast_filtered_count_markup(shown, summary.episodes),
                PodcastKind::Youtube => {
                    strings::youtube_filtered_count_markup(shown, summary.episodes)
                }
            }
        } else {
            self.base_is_markup.set(false);
            // `G2` (design 6a): "4 shows · 41 episodes · 7 new" replaces the
            // bare episode count once nothing is filtered. The filtered
            // branch above is unchanged — "shown of total" is what matters
            // once a filter has narrowed the view. `POD-15`: the YouTube page
            // subscribes to channels, so it counts channels here.
            match self.kind {
                PodcastKind::Rss => {
                    strings::podcast_library_summary(summary.shows, summary.episodes, summary.new)
                }
                PodcastKind::Youtube => {
                    strings::youtube_library_summary(summary.shows, summary.episodes, summary.new)
                }
            }
        };
        self.base_result.replace(base_result);
        self.set_selection_count(selected_count);
        self.rebuild();
    }

    pub(super) fn set_selection_count(&self, selected_count: usize) {
        let text =
            strings::podcast_summary_with_selection(&self.base_result.borrow(), selected_count);
        let presentation = if self.base_is_markup.get() {
            filter_bar_layout::CountPresentation::RestrictedMarkup(&text)
        } else {
            filter_bar_layout::CountPresentation::Plain(&text)
        };
        filter_bar_layout::present_count(&self.result, presentation);
        self.clear_selection.set_visible(selected_count > 0);
    }

    /// FIL-2a / SEARCH-8a: "Clear all" drops this view's query together with
    /// its facets, and nothing outside this section.
    pub(super) fn clear_all(self: &Rc<Self>) {
        self.apply(PodcastFilter::default());
    }

    pub(super) fn apply_filter(self: &Rc<Self>, filter: PodcastFilter) {
        self.apply(filter);
    }

    fn apply(self: &Rc<Self>, filter: PodcastFilter) {
        self.apply_internal(filter, true);
    }

    /// `announce_query`: whether a query change originated here (chip ×,
    /// "Clear all", a jump that relaxed the search) and therefore has to be
    /// mirrored back into the header entry. A query arriving *from* the entry
    /// must not be echoed, or the two would ping-pong.
    fn apply_internal(self: &Rc<Self>, filter: PodcastFilter, announce_query: bool) {
        let previous = self.filter.replace(filter.clone());
        // SEARCH-8a: only the facets are persisted, and only when they
        // actually changed — every keystroke in the header search comes
        // through here, and none of them is a settings write.
        //
        // The applied filter is deliberately not gated on that write
        // succeeding. It used to be, and that made a transient `SQLITE_BUSY`
        // eat the keystroke that hit it: the view kept rendering the previous
        // filter while the entry showed the new query, with nothing but a log
        // line to say so. A filter the user can see and remove is the honest
        // failure mode; the next successful write re-syncs the disk.
        if previous.facets() != filter.facets() {
            if let Err(error) = podcasts::config::save_filter(&self.conn, &filter.facets()) {
                tracing::warn!(%error, "could not persist podcast filters");
            }
        }
        let previous_query = previous.query;
        self.popover.popdown();
        // Drop the receipt in the same turn as the query — see the note in
        // `concerts_filter_bar::clear_query`. `rebuild` below reads the
        // committed query, and the sink that would empty it only answers after
        // a round trip through the header entry. An empty query has no chip
        // under either surface, so clearing it here cannot disagree with the
        // sink.
        if filter.query.trim().is_empty() {
            self.committed_query.replace(String::new());
        }
        self.rebuild();
        if announce_query && previous_query != filter.query {
            let callback = self.on_query_changed.borrow().clone();
            if let Some(callback) = callback {
                callback(&filter.query);
            }
        }
        let callback = self.on_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(filter);
        }
    }

    fn scope(&self) -> SearchScope {
        match self.kind {
            PodcastKind::Rss => SearchScope::Podcasts,
            PodcastKind::Youtube => SearchScope::Youtube,
        }
    }

    fn rebuild(self: &Rc<Self>) {
        while let Some(child) = self.chips.first_child() {
            self.chips.remove(&child);
        }
        let filter = self.filter();
        let committed_query = self.committed_query();
        if filter.unplayed_only {
            self.prepend_chip(&strings::text(strings::PODCAST_FILTER_UNPLAYED), |filter| {
                PodcastFilter {
                    unplayed_only: false,
                    ..filter
                }
            });
        }
        if filter.downloaded_only {
            self.prepend_chip(
                &strings::text(strings::PODCAST_FILTER_DOWNLOADED),
                |filter| PodcastFilter {
                    downloaded_only: false,
                    ..filter
                },
            );
        }
        // FIL-1a/FIL-1d: the dedicated search slot keeps the search chip first.
        let weak = Rc::downgrade(self);
        self.layout
            .replace_scoped_search(self.scope(), &committed_query, move || {
                if let Some(bar) = weak.upgrade() {
                    let cleared = bar.filter().with_query("");
                    bar.apply(cleared);
                }
            });
        self.chips.set_visible(self.chips.first_child().is_some());
        self.clear_all.set_visible(active(&filter));
        self.rebuild_popover();
    }

    fn prepend_chip(
        self: &Rc<Self>,
        label: &str,
        remove: impl Fn(PodcastFilter) -> PodcastFilter + 'static,
    ) {
        let button = gtk4::Button::with_label(&format!("{label}  ×"));
        button.add_css_class(filter_bar_layout::CHIP_CSS_CLASS);
        button.add_css_class("flat");
        button.set_size_request(-1, 20);
        let weak = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            if let Some(bar) = weak.upgrade() {
                bar.apply(remove(bar.filter()));
            }
        });
        self.chips.prepend(&button);
    }

    fn rebuild_popover(self: &Rc<Self>) {
        while let Some(child) = self.popover_box.first_child() {
            self.popover_box.remove(&child);
        }
        self.add_value_button(&strings::text(strings::PODCAST_FILTER_UNPLAYED), |filter| {
            PodcastFilter {
                unplayed_only: true,
                ..filter
            }
        });
        self.add_value_button(
            &strings::text(strings::PODCAST_FILTER_DOWNLOADED),
            |filter| PodcastFilter {
                downloaded_only: true,
                ..filter
            },
        );
    }

    fn add_value_button(
        self: &Rc<Self>,
        label: &str,
        apply: impl Fn(PodcastFilter) -> PodcastFilter + 'static,
    ) {
        let button = gtk4::Button::with_label(label);
        button.add_css_class("flat");
        button.set_halign(gtk4::Align::Fill);
        let weak = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            if let Some(bar) = weak.upgrade() {
                bar.apply(apply(bar.filter()));
            }
        });
        self.popover_box.append(&button);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UX SEARCH-8a: typing in the header is not a settings write. The query
    /// leaves the persisted facets untouched, and — the point of the
    /// separation — applying it is not gated on a write succeeding, so a
    /// busy database can never eat a keystroke.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_8a_a_query_neither_persists_nor_depends_on_persistence() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let bar = PodcastsFilterBar::new(conn.clone(), PodcastKind::Rss);
        bar.apply_filter(PodcastFilter {
            unplayed_only: true,
            ..PodcastFilter::default()
        });

        bar.set_query("wer");

        assert_eq!(bar.filter().query, "wer", "the query is applied in-session");
        let stored = podcasts::config::load_filter(&conn).unwrap();
        assert!(
            stored.unplayed_only,
            "the facet the user picked is persisted"
        );
        assert_eq!(
            PodcastFilter::from_facets(&stored),
            PodcastFilter {
                unplayed_only: true,
                ..PodcastFilter::default()
            },
            "nothing the query touched reached the database"
        );

        // Removing the query is the same trade in the other direction.
        bar.set_query("");
        assert_eq!(bar.filter().query, "");
        assert!(podcasts::config::load_filter(&conn).unwrap().unplayed_only);
    }

    #[test]
    fn src_2_add_action_is_tinted_button_not_chip() {
        assert_eq!(buttons::ADD_ACTION_CLASS, "reprise-btn-add");
        assert_ne!(buttons::ADD_ACTION_CLASS, filter_bar_layout::CHIP_CSS_CLASS);
        assert!(!buttons::ADD_ACTION_CLASS.contains("chip"));
    }

    #[test]
    fn active_filter_detection_tracks_unplayed_and_downloaded_independently() {
        assert!(!active(&PodcastFilter::default()));
        assert!(active(&PodcastFilter {
            unplayed_only: true,
            ..PodcastFilter::default()
        }));
        assert!(active(&PodcastFilter {
            downloaded_only: true,
            ..PodcastFilter::default()
        }));
        assert!(active(&PodcastFilter {
            unplayed_only: true,
            downloaded_only: true,
            ..PodcastFilter::default()
        }));
        assert!(!active(&PodcastFilter {
            source: Some(PodcastKind::Youtube),
            ..PodcastFilter::default()
        }));
    }

    /// UX SEARCH-4a: the receipt goes away with the click that removes it.
    ///
    /// The commit sink is the authority on which query gets a chip, but it
    /// answers only after a round trip through the header entry. The bar
    /// rebuilds its chip row synchronously in the same turn as the click — so
    /// without clearing the stored receipt here, that rebuild reads the old
    /// value and paints the very chip the user just dismissed. No main loop is
    /// pumped in this test on purpose: the frame the user sees is this one.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_4a_podcasts_clear_path_removes_query_and_chip() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let bar =
            PodcastsFilterBar::new(Rc::new(crate::test_db::open().unwrap()), PodcastKind::Rss);
        bar.set_query("falling");
        bar.set_committed_query("falling");
        assert!(
            bar.layout
                .populated_slot_order()
                .contains(&crate::ui::filter_bar_layout::FilterBarSlot::Search),
            "the committed query is showing before the click"
        );

        bar.layout
            .slot_child(crate::ui::filter_bar_layout::FilterBarSlot::Search)
            .and_downcast::<gtk4::Button>()
            .expect("Podcasts search chip")
            .emit_clicked();

        bar.layout.assert_search_cleared(&bar.filter().query);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_2a_podcasts_and_youtube_fill_the_same_ordered_slots() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        for kind in [PodcastKind::Rss, PodcastKind::Youtube] {
            let bar = PodcastsFilterBar::new(Rc::new(crate::test_db::open().unwrap()), kind);
            assert_eq!(
                bar.root.height_request(),
                filter_bar_layout::FILTER_BAR_MIN_HEIGHT
            );
            bar.set_query("falling");
            bar.set_committed_query("falling");
            bar.set_context(
                3,
                LibrarySummary {
                    shows: 4,
                    episodes: 44,
                    new: 2,
                },
                1,
            );

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
                &bar.result
            ));
            assert!(bar.layout.slot_contains(
                crate::ui::filter_bar_layout::FilterBarSlot::ClearAll,
                &bar.clear_all
            ));
            assert!(bar.layout.slot_contains(
                crate::ui::filter_bar_layout::FilterBarSlot::TrailingAction,
                &bar.clear_selection
            ));
            let first = bar
                .layout
                .slot_child(crate::ui::filter_bar_layout::FilterBarSlot::Search)
                .expect("search chip");
            assert!(first
                .downcast::<gtk4::Button>()
                .ok()
                .and_then(|button| button.label())
                .is_some_and(|label| label.starts_with('⌕')));
            assert!(bar.clear_all.is_visible());
            assert!(bar.clear_selection.is_visible());
        }
    }
}
