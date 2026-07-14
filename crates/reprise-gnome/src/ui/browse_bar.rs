//! Cascading Genre/Artist/Album controls for the Library source.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::queries::{self, BrowseFacet, BrowseFilter, BrowseValue};
use rusqlite::Connection;

use crate::ui::browse_filter_strings as filter_strings;
use crate::ui::track_list::Shared;

const SMOKE_ENV: &str = "REPRISE_SMOKE_BROWSE";
const CHIP_CSS_CLASS: &str = "reprise-filter-chip";
const POPOVER_CSS_CLASS: &str = "reprise-filter-popover";
const POPUP_MIN_HEIGHT: i32 = 200;
const FACET_PAGE: &str = "facets";
const VALUE_PAGE: &str = "values";
type OnChanged = Rc<dyn Fn(BrowseFilter)>;
const FACETS: [BrowseFacet; 3] = [BrowseFacet::Genre, BrowseFacet::Artist, BrowseFacet::Album];

#[derive(Debug, Clone, PartialEq, Eq)]
struct FilterChip {
    facet: BrowseFacet,
    label: String,
    accessible_remove_label: String,
}

fn browse_popup_min_height(_option_count: usize) -> i32 {
    POPUP_MIN_HEIGHT
}

fn install_popup_style(widget: &impl IsA<gtk4::Widget>) {
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(&format!(
        ".{CHIP_CSS_CLASS} {{ border-radius: 9999px; padding: 2px 8px; \
         background-color: alpha(@accent_bg_color, 0.22); color: @accent_color; }} \
         .{CHIP_CSS_CLASS}:hover {{ background-color: alpha(@accent_bg_color, 0.32); }} \
         .{POPOVER_CSS_CLASS} contents {{ min-width: 300px; min-height: {}px; }}",
        browse_popup_min_height(0)
    ));
    gtk4::style_context_add_provider_for_display(
        &widget.display(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn apply_selection(
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

fn browse_bar_visible(library_visible: bool, preference_visible: bool) -> bool {
    library_visible && preference_visible
}

pub struct BrowseBar {
    root: gtk4::Box,
    library_visible: Cell<bool>,
    preference_visible: Cell<bool>,
    chips: gtk4::FlowBox,
    add_filter: gtk4::MenuButton,
    chooser_stack: gtk4::Stack,
    facet_list: gtk4::ListBox,
    chooser_back: gtk4::Button,
    value_search: gtk4::SearchEntry,
    value_list: gtk4::ListBox,
    result_label: gtk4::Label,
    chooser_facets: RefCell<Vec<BrowseFacet>>,
    chooser_facet: Cell<Option<BrowseFacet>>,
    chooser_values: RefCell<Vec<BrowseValue>>,
    visible_values: RefCell<Vec<String>>,
    filter: RefCell<BrowseFilter>,
    result_count: Cell<Option<(usize, usize)>>,
    conn: Rc<RefCell<Connection>>,
    on_changed: RefCell<Option<OnChanged>>,
}

impl BrowseBar {
    pub fn new(conn: Rc<RefCell<Connection>>) -> Rc<Self> {
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        root.set_margin_top(6);
        root.set_margin_bottom(6);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.add_css_class("toolbar");
        install_popup_style(&root);

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

        root.append(&section_label);
        root.append(&chips);
        root.append(&result_label);

        let bar = Rc::new(Self {
            root,
            library_visible: Cell::new(true),
            preference_visible: Cell::new(true),
            chips,
            add_filter,
            chooser_stack,
            facet_list,
            chooser_back,
            value_search,
            value_list,
            result_label,
            chooser_facets: RefCell::new(Vec::new()),
            chooser_facet: Cell::new(None),
            chooser_values: RefCell::new(Vec::new()),
            visible_values: RefCell::new(Vec::new()),
            filter: RefCell::new(BrowseFilter::default()),
            result_count: Cell::new(None),
            conn,
            on_changed: RefCell::new(None),
        });
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

    pub(super) fn restore_filter(self: &Rc<Self>, filter: &BrowseFilter) {
        let filter = restored_filter(filter);
        *self.filter.borrow_mut() = filter;
        self.refresh();
    }

    pub fn set_on_changed(&self, callback: impl Fn(BrowseFilter) + 'static) {
        *self.on_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub fn set_library_visible(&self, visible: bool) {
        self.library_visible.set(visible);
        self.sync_visibility();
    }

    pub fn set_preference_visible(&self, visible: bool) {
        self.preference_visible.set(visible);
        self.sync_visibility();
    }

    fn sync_visibility(&self) {
        let visible = browse_bar_visible(self.library_visible.get(), self.preference_visible.get());
        self.root.set_visible(visible);
        tracing::info!(visible, "browse bar visibility updated");
    }

    pub fn set_result_count(&self, filtered: usize, total: usize) {
        self.result_count.set(Some((filtered, total)));
        self.result_label
            .set_text(&filter_strings::result_count(filtered, total));
        self.result_label.set_visible(true);
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

    fn apply_filter(self: &Rc<Self>, next: BrowseFilter) {
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
            let callback = bar.on_changed.borrow().clone();
            if let Some(callback) = callback {
                callback(next);
            }
        });
    }

    fn rebuild_chips(self: &Rc<Self>, filter: &BrowseFilter) {
        self.chips.remove_all();
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
        self.chips.append(&self.add_filter);
        self.add_filter
            .set_sensitive(!available_facets(filter).is_empty());
    }

    fn rebuild_facet_page(&self, filter: &BrowseFilter) {
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

    fn show_values(&self, facet: BrowseFacet) {
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

    fn rebuild_value_rows(&self) {
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

fn build_chooser() -> (
    gtk4::Stack,
    gtk4::ListBox,
    gtk4::Button,
    gtk4::SearchEntry,
    gtk4::ListBox,
) {
    let stack = gtk4::Stack::new();
    stack.set_transition_type(gtk4::StackTransitionType::SlideLeftRight);

    let facet_page = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    facet_page.set_margin_top(8);
    facet_page.set_margin_bottom(8);
    facet_page.set_margin_start(8);
    facet_page.set_margin_end(8);
    let heading = gtk4::Label::new(Some(&filter_strings::text(filter_strings::ADD_FILTER)));
    heading.add_css_class("heading");
    heading.set_halign(gtk4::Align::Start);
    let facet_list = gtk4::ListBox::new();
    facet_list.add_css_class("boxed-list");
    facet_page.append(&heading);
    facet_page.append(&facet_list);
    stack.add_named(&facet_page, Some(FACET_PAGE));

    let value_page = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    value_page.set_margin_top(8);
    value_page.set_margin_bottom(8);
    value_page.set_margin_start(8);
    value_page.set_margin_end(8);
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let back = gtk4::Button::from_icon_name("go-previous-symbolic");
    back.add_css_class("flat");
    back.set_tooltip_text(Some(&filter_strings::text(filter_strings::BACK)));
    let search = gtk4::SearchEntry::builder()
        .placeholder_text(filter_strings::text(filter_strings::SEARCH_VALUES))
        .hexpand(true)
        .build();
    header.append(&back);
    header.append(&search);
    let value_list = gtk4::ListBox::new();
    value_list.add_css_class("boxed-list");
    let scrolled = gtk4::ScrolledWindow::builder()
        .child(&value_list)
        .min_content_height(POPUP_MIN_HEIGHT)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .build();
    value_page.append(&header);
    value_page.append(&scrolled);
    stack.add_named(&value_page, Some(VALUE_PAGE));
    stack.set_visible_child_name(FACET_PAGE);

    (stack, facet_list, back, search, value_list)
}

fn chooser_row(title: &str, count: Option<&str>) -> gtk4::ListBoxRow {
    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    content.set_margin_top(7);
    content.set_margin_bottom(7);
    content.set_margin_start(10);
    content.set_margin_end(10);
    let title = gtk4::Label::new(Some(title));
    title.set_halign(gtk4::Align::Start);
    title.set_hexpand(true);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    content.append(&title);
    if let Some(count) = count {
        let count = gtk4::Label::new(Some(count));
        count.add_css_class("dim-label");
        count.add_css_class("caption");
        content.append(&count);
    }
    gtk4::ListBoxRow::builder().child(&content).build()
}

fn wire_chooser(bar: &Rc<BrowseBar>) {
    {
        let weak = Rc::downgrade(bar);
        bar.add_filter.connect_active_notify(move |button| {
            if !button.is_active() {
                return;
            }
            if let Some(bar) = weak.upgrade() {
                let filter = bar.filter();
                bar.rebuild_facet_page(&filter);
            }
        });
    }
    {
        let weak = Rc::downgrade(bar);
        bar.facet_list.connect_row_activated(move |_, row| {
            let Some(bar) = weak.upgrade() else {
                return;
            };
            let facet = bar
                .chooser_facets
                .borrow()
                .get(row.index() as usize)
                .copied();
            if let Some(facet) = facet {
                bar.show_values(facet);
            }
        });
    }
    {
        let weak = Rc::downgrade(bar);
        bar.value_search.connect_search_changed(move |_| {
            if let Some(bar) = weak.upgrade() {
                bar.rebuild_value_rows();
            }
        });
    }
    {
        let weak = Rc::downgrade(bar);
        bar.value_list.connect_row_activated(move |_, row| {
            let Some(bar) = weak.upgrade() else {
                return;
            };
            let value = bar
                .visible_values
                .borrow()
                .get(row.index() as usize)
                .cloned();
            let Some((facet, value)) = bar.chooser_facet.get().zip(value) else {
                return;
            };
            let current = bar.filter();
            bar.apply_filter(apply_selection(&current, facet, Some(value)));
        });
    }
    {
        let weak = Rc::downgrade(bar);
        bar.chooser_back.connect_clicked(move |_| {
            let Some(bar) = weak.upgrade() else {
                return;
            };
            let filter = bar.filter();
            bar.rebuild_facet_page(&filter);
        });
    }
}

fn load_values(conn: &Connection, facet: BrowseFacet, filter: &BrowseFilter) -> Vec<BrowseValue> {
    queries::query_browse_values(conn, facet, filter).unwrap_or_else(|error| {
        tracing::warn!(%error, ?facet, "could not load browse facet values");
        Vec::new()
    })
}

pub(super) fn arm_smoke(shared: &Rc<Shared>) {
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
        let result_count = shared.browse_bar.result_count.get();
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

    #[test]
    fn visibility_requires_library_source_and_user_preference() {
        assert!(browse_bar_visible(true, true));
        assert!(!browse_bar_visible(true, false));
        assert!(!browse_bar_visible(false, true));
        assert!(!browse_bar_visible(false, false));
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
        assert_eq!(bar.root.observe_children().n_items(), 3);
        assert_eq!(
            bar.root.last_child(),
            Some(bar.result_label.clone().upcast())
        );

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
