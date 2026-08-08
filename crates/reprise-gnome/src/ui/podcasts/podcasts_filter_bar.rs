use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::db::Db;
use reprise_core::podcasts::{self, PodcastKind};

// `active` lives in `podcasts_presentation` (also read by the empty-state
// classification), not duplicated here.
use super::podcasts_presentation::{active, LibrarySummary, PodcastFilter};
use crate::ui::browse::browse_bar::CHIP_CSS_CLASS;
use crate::ui::search_chip;
use crate::ui::strings;
use crate::ui::style::buttons;
use reprise_view::search_scope::SearchScope;

const FILTER_BAR_MIN_HEIGHT: i32 = 34;
type OnChanged = Rc<dyn Fn(PodcastFilter)>;
/// SEARCH-8a: fired when the bar itself changes the query — the chip's ×, or a
/// jump that had to relax the search to reach its episode. The shell listens
/// so the header entry stops showing a query the view no longer applies.
type OnQueryChanged = Rc<dyn Fn(&str)>;

pub(in crate::ui) fn add_button(kind: PodcastKind) -> gtk4::Button {
    let add = gtk4::Button::builder()
        .label(strings::text(match kind {
            PodcastKind::Rss => strings::PODCAST_ADD,
            PodcastKind::Youtube => strings::YOUTUBE_ADD,
        }))
        .build();
    buttons::arm(&add, buttons::ADD_ACTION_CLASS);
    add.set_action_name(Some("podcasts.open-add"));
    add
}

pub(super) struct PodcastsFilterBar {
    root: gtk4::Box,
    conn: Rc<Db>,
    filter: RefCell<PodcastFilter>,
    chips: gtk4::Box,
    popover_box: gtk4::Box,
    popover: gtk4::Popover,
    result: gtk4::Label,
    clear_selection: gtk4::Button,
    base_result: RefCell<String>,
    /// Whether the summary line is markup (FIL-2's accented count) or plain
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
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        root.set_margin_top(6);
        root.set_margin_bottom(6);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.set_size_request(-1, FILTER_BAR_MIN_HEIGHT);
        root.add_css_class("toolbar");

        let add = add_button(kind);
        root.append(&add);

        let chips = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        chips.set_hexpand(true);
        root.append(&chips);
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
        chips.append(&add_filter);
        let result = gtk4::Label::new(None);
        result.add_css_class("dim-label");
        result.add_css_class("caption");
        root.append(&result);
        let clear_selection =
            gtk4::Button::with_label(&strings::text(strings::PODCAST_CLEAR_SELECTION));
        clear_selection.add_css_class("flat");
        clear_selection.set_visible(false);
        clear_selection.set_action_name(Some("podcasts.clear-selection"));
        root.append(&clear_selection);

        let bar = Rc::new(Self {
            root,
            conn,
            filter: RefCell::new(filter),
            chips,
            popover_box,
            popover,
            result,
            clear_selection,
            base_result: RefCell::new(String::new()),
            base_is_markup: Cell::new(false),
            on_changed: RefCell::new(None),
            on_query_changed: RefCell::new(None),
            kind,
        });
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

    pub(super) fn set_context(
        self: &Rc<Self>,
        shown: usize,
        summary: LibrarySummary,
        selected_count: usize,
    ) {
        let filter = self.filter();
        let base_result = if active(&filter) {
            // FIL-2: a filtered list counts in its own unit with the shown
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
        if self.base_is_markup.get() {
            self.result.set_markup(&text);
            self.result.add_css_class("accent");
        } else {
            self.result.remove_css_class("accent");
            self.result.set_text(&text);
        }
        self.clear_selection.set_visible(selected_count > 0);
    }

    /// FIL-2 / SEARCH-8a: "Clear all" drops this view's query together with
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
            if child.is::<gtk4::MenuButton>() {
                break;
            }
            self.chips.remove(&child);
        }
        let filter = self.filter();
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
        if active(&filter) {
            let clear = gtk4::Button::with_label(&format!(
                "{}  ×",
                strings::text(strings::PODCAST_CLEAR_ALL)
            ));
            clear.add_css_class(CHIP_CSS_CLASS);
            clear.add_css_class("flat");
            let weak = Rc::downgrade(self);
            clear.connect_clicked(move |_| {
                if let Some(bar) = weak.upgrade() {
                    bar.clear_all();
                }
            });
            self.chips.prepend(&clear);
        }
        // FIL-1a/FIL-1d: prepended after everything else, so the search chip
        // ends up first in the row — ahead of the facet chips and of the
        // "Clear all" pill, the way the Library filter row already reads.
        if filter.has_query() {
            let weak = Rc::downgrade(self);
            let chip = search_chip::build(self.scope(), &filter.query, move || {
                if let Some(bar) = weak.upgrade() {
                    let cleared = bar.filter().with_query("");
                    bar.apply(cleared);
                }
            });
            self.chips.prepend(&chip);
        }
        self.rebuild_popover();
    }

    fn prepend_chip(
        self: &Rc<Self>,
        label: &str,
        remove: impl Fn(PodcastFilter) -> PodcastFilter + 'static,
    ) {
        let button = gtk4::Button::with_label(&format!("{label}  ×"));
        button.add_css_class(CHIP_CSS_CLASS);
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
        assert_ne!(buttons::ADD_ACTION_CLASS, CHIP_CSS_CLASS);
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
}
