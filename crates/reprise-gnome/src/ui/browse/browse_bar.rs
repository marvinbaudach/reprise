//! Cascading Genre/Artist/Album controls for the Library source.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::queries::{self, BrowseFacet, BrowseFilter, BrowseValue};
use reprise_core::view_source::ViewSource;
use rusqlite::Connection;

use super::browse_chooser::{
    browse_popup_min_height, build_chooser, chooser_row, load_values, wire_chooser, FACET_PAGE,
    VALUE_PAGE,
};
use crate::ui::browse_filter_strings as filter_strings;
use crate::ui::track_list::Shared;

const SMOKE_ENV: &str = "REPRISE_SMOKE_BROWSE";
const CHIP_CSS_CLASS: &str = "reprise-filter-chip";
const POPOVER_CSS_CLASS: &str = "reprise-filter-popover";
type OnChanged = Rc<dyn Fn(BrowseFilter)>;
type OnVoid = Rc<dyn Fn()>;
const FACETS: [BrowseFacet; 3] = [BrowseFacet::Genre, BrowseFacet::Artist, BrowseFacet::Album];

#[derive(Debug, Clone, PartialEq, Eq)]
struct FilterChip {
    facet: BrowseFacet,
    label: String,
    accessible_remove_label: String,
}

/// Chip and value-popover rules; installed app-wide by [`super::style`].
pub(in crate::ui) fn css() -> String {
    use super::style::tokens::{CHIP_BG_ALPHA, CHIP_BG_HOVER_ALPHA};
    format!(
        ".{CHIP_CSS_CLASS} {{ border-radius: 9999px; padding: 2px 8px; \
         background-color: alpha(@accent_bg_color, {CHIP_BG_ALPHA}); color: @accent_color; }} \
         .{CHIP_CSS_CLASS}:hover {{ background-color: alpha(@accent_bg_color, {CHIP_BG_HOVER_ALPHA}); }} \
         .{POPOVER_CSS_CLASS} contents {{ min-width: 300px; min-height: {}px; }}",
        browse_popup_min_height(0)
    )
}

pub(super) fn apply_selection(
    current: &BrowseFilter,
    facet: BrowseFacet,
    value: Option<String>,
) -> BrowseFilter {
    match facet {
        BrowseFacet::Genre => BrowseFilter {
            genre: value,
            artist: None,
            album: None,
        },
        BrowseFacet::Artist => BrowseFilter {
            genre: current.genre.clone(),
            artist: value,
            album: None,
        },
        BrowseFacet::Album => BrowseFilter {
            genre: current.genre.clone(),
            artist: current.artist.clone(),
            album: value,
        },
    }
}

fn filter_value(filter: &BrowseFilter, facet: BrowseFacet) -> Option<&str> {
    match facet {
        BrowseFacet::Genre => filter.genre.as_deref(),
        BrowseFacet::Artist => filter.artist.as_deref(),
        BrowseFacet::Album => filter.album.as_deref(),
    }
}

fn facet_label(facet: BrowseFacet) -> String {
    let message = match facet {
        BrowseFacet::Genre => filter_strings::BROWSE_GENRE,
        BrowseFacet::Artist => filter_strings::BROWSE_ARTIST,
        BrowseFacet::Album => filter_strings::BROWSE_ALBUM,
    };
    filter_strings::text(message)
}

fn displayed_value(facet: BrowseFacet, value: &str) -> String {
    if !value.is_empty() {
        return value.to_string();
    }
    let message = match facet {
        BrowseFacet::Genre => filter_strings::UNKNOWN_GENRE,
        BrowseFacet::Artist => filter_strings::UNKNOWN_ARTIST,
        BrowseFacet::Album => filter_strings::UNKNOWN_ALBUM,
    };
    filter_strings::text(message)
}

fn filter_chips(filter: &BrowseFilter) -> Vec<FilterChip> {
    FACETS
        .into_iter()
        .filter_map(|facet| {
            let value = displayed_value(facet, filter_value(filter, facet)?);
            let facet_name = facet_label(facet);
            Some(FilterChip {
                facet,
                label: filter_strings::chip_label(&facet_name, &value),
                accessible_remove_label: filter_strings::remove_filter_label(&facet_name, &value),
            })
        })
        .collect()
}

#[cfg(test)]
fn chip_labels(search: &str, filter: &BrowseFilter) -> Vec<String> {
    let mut labels = Vec::new();
    if !search.trim().is_empty() {
        labels.push(filter_strings::search_chip_label(search.trim()));
    }
    labels.extend(filter_chips(filter).into_iter().map(|chip| chip.label));
    labels
}

fn available_facets(filter: &BrowseFilter) -> Vec<BrowseFacet> {
    FACETS
        .into_iter()
        .filter(|facet| filter_value(filter, *facet).is_none())
        .collect()
}

fn remove_filter(filter: &BrowseFilter, facet: BrowseFacet) -> BrowseFilter {
    apply_selection(filter, facet, None)
}

fn value_matches_search(value: &str, search: &str) -> bool {
    value.to_lowercase().contains(&search.trim().to_lowercase())
}

fn restored_filter(filter: &BrowseFilter) -> BrowseFilter {
    filter.clone()
}

pub struct BrowseBar {
    root: gtk4::Box,
    search: RefCell<String>,
    track_source: Cell<bool>,
    is_library: Cell<bool>,
    preference_visible: Cell<bool>,
    section_label: gtk4::Label,
    chips: gtk4::FlowBox,
    pub(super) add_filter: gtk4::MenuButton,
    chooser_stack: gtk4::Stack,
    pub(super) facet_list: gtk4::ListBox,
    pub(super) chooser_back: gtk4::Button,
    pub(super) value_search: gtk4::SearchEntry,
    pub(super) value_list: gtk4::ListBox,
    result_label: gtk4::Label,
    clear_all: gtk4::Button,
    pub(super) chooser_facets: RefCell<Vec<BrowseFacet>>,
    pub(super) chooser_facet: Cell<Option<BrowseFacet>>,
    chooser_values: RefCell<Vec<BrowseValue>>,
    pub(super) visible_values: RefCell<Vec<String>>,
    filter: RefCell<BrowseFilter>,
    result_count: Cell<Option<(usize, usize)>>,
    conn: Rc<RefCell<Connection>>,
    on_changed: RefCell<Option<OnChanged>>,
    on_search_cleared: RefCell<Option<OnVoid>>,
    on_clear_all: RefCell<Option<OnVoid>>,
}

impl BrowseBar {
    pub fn new(conn: Rc<RefCell<Connection>>) -> Rc<Self> {
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        root.set_margin_top(6);
        root.set_margin_bottom(6);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.add_css_class("toolbar");

        let section_label = gtk4::Label::new(Some(&filter_strings::text(filter_strings::FILTERS)));
        section_label.add_css_class("dim-label");
        section_label.add_css_class("caption-heading");

        let chips = gtk4::FlowBox::builder()
            .selection_mode(gtk4::SelectionMode::None)
            .column_spacing(6)
            .row_spacing(4)
            .max_children_per_line(20)
            .hexpand(true)
            .halign(gtk4::Align::Fill)
            .build();

        let popover = gtk4::Popover::new();
        popover.add_css_class(POPOVER_CSS_CLASS);
        let (chooser_stack, facet_list, chooser_back, value_search, value_list) = build_chooser();
        popover.set_child(Some(&chooser_stack));

        let add_label = gtk4::Label::new(Some(&format!(
            "+ {}",
            filter_strings::text(filter_strings::ADD_FILTER)
        )));
        let add_filter = gtk4::MenuButton::new();
        add_filter.set_child(Some(&add_label));
        add_filter.set_popover(Some(&popover));
        add_filter.add_css_class("pill");
        add_filter.update_property(&[gtk4::accessible::Property::Label(&filter_strings::text(
            filter_strings::ADD_FILTER,
        ))]);
        chips.append(&add_filter);

        let result_label = gtk4::Label::new(None);
        result_label.add_css_class("dim-label");
        result_label.add_css_class("caption");
        result_label.set_visible(false);

        let clear_all = gtk4::Button::with_label(&format!(
            "{} ×",
            filter_strings::text(filter_strings::CLEAR_ALL)
        ));
        clear_all.add_css_class("flat");
        clear_all.add_css_class(CHIP_CSS_CLASS);
        clear_all.set_visible(false);

        root.append(&section_label);
        root.append(&chips);
        root.append(&result_label);
        root.append(&clear_all);

        let bar = Rc::new(Self {
            root,
            search: RefCell::new(String::new()),
            track_source: Cell::new(true),
            is_library: Cell::new(true),
            preference_visible: Cell::new(true),
            section_label,
            chips,
            add_filter,
            chooser_stack,
            facet_list,
            chooser_back,
            value_search,
            value_list,
            result_label,
            clear_all,
            chooser_facets: RefCell::new(Vec::new()),
            chooser_facet: Cell::new(None),
            chooser_values: RefCell::new(Vec::new()),
            visible_values: RefCell::new(Vec::new()),
            filter: RefCell::new(BrowseFilter::default()),
            result_count: Cell::new(None),
            conn,
            on_changed: RefCell::new(None),
            on_search_cleared: RefCell::new(None),
            on_clear_all: RefCell::new(None),
        });
        {
            let weak = Rc::downgrade(&bar);
            bar.clear_all.connect_clicked(move |_| {
                let Some(bar) = weak.upgrade() else {
                    return;
                };
                let callback = bar.on_clear_all.borrow().clone();
                if let Some(callback) = callback {
                    callback();
                }
            });
        }
        wire_chooser(&bar);
        bar.sync_visibility();
        bar.refresh();
        bar
    }

    pub fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub fn filter(&self) -> BrowseFilter {
        self.filter.borrow().clone()
    }

    pub(in crate::ui) fn restore_filter(self: &Rc<Self>, filter: &BrowseFilter) {
        let filter = restored_filter(filter);
        *self.filter.borrow_mut() = filter;
        self.refresh();
    }

    pub fn set_on_changed(&self, callback: impl Fn(BrowseFilter) + 'static) {
        *self.on_changed.borrow_mut() = Some(Rc::new(callback));
    }

    #[allow(dead_code)] // Wired to the window-level reset in Task 4.
    pub fn set_on_search_cleared(&self, callback: impl Fn() + 'static) {
        *self.on_search_cleared.borrow_mut() = Some(Rc::new(callback));
    }

    #[allow(dead_code)] // Wired to the window-level reset in Task 4.
    pub fn set_on_clear_all(&self, callback: impl Fn() + 'static) {
        *self.on_clear_all.borrow_mut() = Some(Rc::new(callback));
    }

    #[allow(dead_code)] // Reload and session restoration adopt this API in Task 3.
    pub fn set_source_context(&self, source: &ViewSource) {
        self.track_source
            .set(super::filter_restriction::is_track_source(source));
        self.is_library.set(matches!(source, ViewSource::Library));
        self.sync_visibility();
    }

    pub fn set_library_visible(&self, visible: bool) {
        self.track_source.set(visible);
        self.is_library.set(visible);
        self.sync_visibility();
    }

    #[allow(dead_code)] // Reload adopts this API in Task 3.
    pub fn set_search(self: &Rc<Self>, text: &str) {
        *self.search.borrow_mut() = text.to_string();
        self.refresh();
        self.sync_visibility();
    }

    pub fn set_preference_visible(&self, visible: bool) {
        self.preference_visible.set(visible);
        self.sync_visibility();
    }

    fn sync_visibility(&self) {
        let restricted =
            super::filter_restriction::is_restricted(&self.search.borrow(), &self.filter.borrow());
        let visible = super::filter_restriction::row_visible(
            self.track_source.get(),
            restricted,
            self.preference_visible.get(),
        );
        self.root.set_visible(visible);
        self.section_label.set_visible(restricted);
        self.clear_all.set_visible(restricted);
        tracing::info!(visible, restricted, "filter row visibility updated");
    }

    pub fn set_result_count(&self, filtered: usize, total: usize) {
        self.result_count.set(Some((filtered, total)));
        let (markup, accented) = filter_strings::result_count_markup(filtered, total);
        self.result_label.set_markup(&markup);
        if accented {
            self.result_label.add_css_class("accent");
        } else {
            self.result_label.remove_css_class("accent");
        }
        self.result_label.set_visible(true);
    }

    pub(in crate::ui) fn result_count(&self) -> Option<(usize, usize)> {
        self.result_count.get()
    }

    pub fn hide_result_count(&self) {
        self.result_count.set(None);
        self.result_label.set_visible(false);
    }

    pub fn refresh(self: &Rc<Self>) {
        let filter = self.filter();
        self.rebuild_chips(&filter);
        self.rebuild_facet_page(&filter);
    }

    pub(super) fn apply_filter(self: &Rc<Self>, next: BrowseFilter) {
        let current = self.filter();
        if next == current {
            return;
        }
        *self.filter.borrow_mut() = next.clone();
        self.add_filter.popdown();
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            let Some(bar) = weak.upgrade() else {
                return;
            };
            bar.refresh();
            bar.sync_visibility();
            let callback = bar.on_changed.borrow().clone();
            if let Some(callback) = callback {
                callback(next);
            }
        });
    }

    fn rebuild_chips(self: &Rc<Self>, filter: &BrowseFilter) {
        self.chips.remove_all();
        let query = self.search.borrow().trim().to_string();
        if !query.is_empty() {
            let button = gtk4::Button::with_label(&format!(
                "{}  ×",
                filter_strings::search_chip_label(&query)
            ));
            button.add_css_class("flat");
            button.add_css_class(CHIP_CSS_CLASS);
            button.update_property(&[gtk4::accessible::Property::Label(
                &filter_strings::remove_search_label(&query),
            )]);
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                let Some(bar) = weak.upgrade() else {
                    return;
                };
                let callback = bar.on_search_cleared.borrow().clone();
                if let Some(callback) = callback {
                    callback();
                }
            });
            self.chips.append(&button);
        }
        for chip in filter_chips(filter) {
            let button = gtk4::Button::with_label(&format!("{}  ×", chip.label));
            button.add_css_class("flat");
            button.add_css_class(CHIP_CSS_CLASS);
            button.update_property(&[gtk4::accessible::Property::Label(
                &chip.accessible_remove_label,
            )]);
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                let Some(bar) = weak.upgrade() else {
                    return;
                };
                let next = remove_filter(&bar.filter(), chip.facet);
                bar.apply_filter(next);
            });
            self.chips.append(&button);
        }
        if self.is_library.get() {
            self.chips.append(&self.add_filter);
        }
        self.add_filter
            .set_sensitive(!available_facets(filter).is_empty());
    }

    pub(super) fn rebuild_facet_page(&self, filter: &BrowseFilter) {
        self.facet_list.remove_all();
        let facets = available_facets(filter);
        if facets.is_empty() {
            self.facet_list.append(&chooser_row(
                &filter_strings::text(filter_strings::NO_FILTERS_AVAILABLE),
                None,
            ));
        } else {
            for facet in &facets {
                self.facet_list
                    .append(&chooser_row(&facet_label(*facet), None));
            }
        }
        *self.chooser_facets.borrow_mut() = facets;
        self.chooser_stack.set_visible_child_name(FACET_PAGE);
    }

    pub(super) fn show_values(&self, facet: BrowseFacet) {
        let filter = self.filter();
        let values = {
            let conn = self.conn.borrow();
            load_values(&conn, facet, &filter)
        };
        self.chooser_facet.set(Some(facet));
        *self.chooser_values.borrow_mut() = values;
        self.value_search.set_text("");
        self.rebuild_value_rows();
        self.chooser_stack.set_visible_child_name(VALUE_PAGE);
        self.value_search.grab_focus();
    }

    pub(super) fn rebuild_value_rows(&self) {
        self.value_list.remove_all();
        let Some(facet) = self.chooser_facet.get() else {
            return;
        };
        let search = self.value_search.text();
        let mut visible = Vec::new();
        let values = self.chooser_values.borrow().clone();
        for value in values {
            let display = displayed_value(facet, &value.value);
            if !value_matches_search(&display, &search) {
                continue;
            }
            self.value_list.append(&chooser_row(
                &display,
                Some(&reprise_core::format::format_thousands(value.count)),
            ));
            visible.push(value.value);
        }
        *self.visible_values.borrow_mut() = visible;
    }

    fn select_raw(self: &Rc<Self>, facet: BrowseFacet, value: &str) -> bool {
        let filter = self.filter();
        let found = {
            let conn = self.conn.borrow();
            load_values(&conn, facet, &filter)
                .iter()
                .any(|candidate| candidate.value == value)
        };
        if !found {
            return false;
        }
        self.apply_filter(apply_selection(&filter, facet, Some(value.to_string())));
        true
    }
}

pub(in crate::ui) fn arm_smoke(shared: &Rc<Shared>) {
    let Ok(value) = std::env::var(SMOKE_ENV) else {
        return;
    };
    let selections: VecDeque<_> = value
        .split('|')
        .filter_map(|part| {
            let (name, value) = part.split_once(':')?;
            let facet = match name {
                "genre" => BrowseFacet::Genre,
                "artist" => BrowseFacet::Artist,
                "album" => BrowseFacet::Album,
                _ => return None,
            };
            Some((facet, value.to_string()))
        })
        .collect();
    let shared_weak = Rc::downgrade(shared);
    glib::idle_add_local_once(move || {
        schedule_smoke_step(shared_weak, Rc::new(RefCell::new(selections)));
    });
}

fn schedule_smoke_step(
    shared_weak: std::rc::Weak<Shared>,
    selections: Rc<RefCell<VecDeque<(BrowseFacet, String)>>>,
) {
    glib::timeout_add_local_once(Duration::from_millis(25), move || {
        let Some(shared) = shared_weak.upgrade() else {
            return;
        };
        let selection = selections.borrow_mut().pop_front();
        if let Some((facet, value)) = selection {
            if !shared.browse_bar.select_raw(facet, &value) {
                tracing::warn!(?facet, %value, "browse smoke value not found");
            }
            schedule_smoke_step(Rc::downgrade(&shared), selections);
            return;
        }
        let browse = shared.browse_filter.borrow().clone();
        let sort = shared.sort.borrow().clone();
        let filter = shared.filter.borrow().clone();
        let ids = {
            let conn = shared.conn.borrow();
            queries::query_track_ids_browsed(
                &conn,
                &reprise_core::view_source::ViewSource::Library,
                &sort.field,
                &sort.dir,
                &filter,
                &browse,
                &[],
            )
        };
        let chips: Vec<_> = filter_chips(&browse)
            .into_iter()
            .map(|chip| chip.label)
            .collect();
        let result_count = shared.browse_bar.result_count();
        tracing::info!(
            ?browse,
            ?chips,
            ?result_count,
            ?ids,
            "browse smoke completed"
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // UX FIL-1a: chip order is search first, then the facet cascade.
    #[test]
    fn fil_1a_search_appears_as_chip_before_facet_chips() {
        let browse = BrowseFilter {
            genre: Some("Rock".into()),
            artist: None,
            album: None,
        };
        let labels = chip_labels("falling", &browse);
        assert_eq!(
            labels,
            vec![
                "⌕ “falling” in any field".to_string(),
                "Genre: Rock".to_string()
            ]
        );
        assert!(chip_labels("  ", &BrowseFilter::default()).is_empty());
    }

    fn full_filter() -> BrowseFilter {
        BrowseFilter {
            genre: Some("Rock".into()),
            artist: Some("A".into()),
            album: Some("Stage".into()),
        }
    }

    #[test]
    fn genre_selection_resets_artist_and_album() {
        assert_eq!(
            apply_selection(&full_filter(), BrowseFacet::Genre, Some("Jazz".into())),
            BrowseFilter {
                genre: Some("Jazz".into()),
                artist: None,
                album: None,
            }
        );
    }

    #[test]
    fn artist_selection_keeps_genre_and_resets_album() {
        assert_eq!(
            apply_selection(&full_filter(), BrowseFacet::Artist, Some("B".into())),
            BrowseFilter {
                genre: Some("Rock".into()),
                artist: Some("B".into()),
                album: None,
            }
        );
    }

    #[test]
    fn restored_filter_preserves_empty_unknown_values() {
        let filter = BrowseFilter {
            genre: Some(String::new()),
            artist: Some(String::new()),
            album: Some(String::new()),
        };
        assert_eq!(restored_filter(&filter), filter);
    }

    #[test]
    fn browse_popup_minimum_height_does_not_collapse_with_zero_results() {
        assert_eq!(browse_popup_min_height(0), browse_popup_min_height(5));
    }

    #[test]
    fn filter_chips_follow_cascade_order_and_render_unknown_values() {
        let filter = BrowseFilter {
            genre: Some(String::new()),
            artist: Some("Brand of Sacrifice".into()),
            album: Some(String::new()),
        };

        let chips = filter_chips(&filter);
        let projection: Vec<_> = chips
            .iter()
            .map(|chip| (chip.facet, chip.label.as_str()))
            .collect();
        assert_eq!(
            projection,
            vec![
                (BrowseFacet::Genre, "Genre: Unknown genre"),
                (BrowseFacet::Artist, "Artist: Brand of Sacrifice"),
                (BrowseFacet::Album, "Album: Unknown album"),
            ]
        );
        assert_eq!(
            chips[1].accessible_remove_label,
            "Remove Artist filter: Brand of Sacrifice"
        );
    }

    #[test]
    fn available_facets_omit_filters_that_are_already_active() {
        let filter = BrowseFilter {
            genre: Some("Metal".into()),
            artist: None,
            album: Some("Lifeblood".into()),
        };

        assert_eq!(available_facets(&filter), vec![BrowseFacet::Artist]);
    }

    #[test]
    fn removing_a_parent_filter_clears_dependent_filters() {
        assert_eq!(
            remove_filter(&full_filter(), BrowseFacet::Genre),
            BrowseFilter::default()
        );
        assert_eq!(
            remove_filter(&full_filter(), BrowseFacet::Artist),
            BrowseFilter {
                genre: Some("Rock".into()),
                artist: None,
                album: None,
            }
        );
        assert_eq!(
            remove_filter(&full_filter(), BrowseFacet::Album),
            BrowseFilter {
                genre: Some("Rock".into()),
                artist: Some("A".into()),
                album: None,
            }
        );
    }

    #[test]
    fn the_single_value_search_is_case_insensitive_and_matches_substrings() {
        assert!(value_matches_search("Brand of Sacrifice", "SACRI"));
        assert!(value_matches_search("Brand of Sacrifice", ""));
        assert!(!value_matches_search("Brand of Sacrifice", "Chelsea"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn widget_projects_removable_chips_without_a_redundant_reset_button() {
        if gtk4::init().is_err() {
            return;
        }
        let conn = Connection::open_in_memory().unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let bar = BrowseBar::new(Rc::new(RefCell::new(conn)));

        bar.restore_filter(&full_filter());
        assert_eq!(bar.chips.observe_children().n_items(), 4);
        assert_eq!(bar.root.observe_children().n_items(), 4);
        assert_eq!(bar.root.last_child(), Some(bar.clear_all.clone().upcast()));

        let genre_chip = bar.chips.child_at_index(0).unwrap().child().unwrap();
        genre_chip
            .downcast::<gtk4::Button>()
            .unwrap()
            .emit_clicked();
        let context = glib::MainContext::default();
        while context.pending() {
            context.iteration(false);
        }
        assert_eq!(bar.filter(), BrowseFilter::default());
        assert_eq!(bar.chips.observe_children().n_items(), 1);
        assert!(!bar.add_filter.has_css_class("flat"));
        assert_eq!(
            bar.add_filter
                .child()
                .and_then(|child| child.downcast::<gtk4::Label>().ok())
                .map(|label| label.text().to_string()),
            Some("+ Add filter".into())
        );
    }
}
