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
const DROPDOWN_CSS_CLASS: &str = "reprise-browse-dropdown";
const POPUP_MIN_HEIGHT: i32 = 200;
type OnChanged = Rc<dyn Fn(BrowseFilter)>;
// Task 1 defines and tests this pure projection before Task 2 wires it into
// GTK. The temporary allowance is removed with that wiring.
#[cfg_attr(not(test), allow(dead_code))]
const FACETS: [BrowseFacet; 3] = [BrowseFacet::Genre, BrowseFacet::Artist, BrowseFacet::Album];

#[cfg_attr(not(test), allow(dead_code))]
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
        ".{DROPDOWN_CSS_CLASS} popover contents {{ min-height: {}px; }}",
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

#[cfg_attr(not(test), allow(dead_code))]
fn filter_value(filter: &BrowseFilter, facet: BrowseFacet) -> Option<&str> {
    match facet {
        BrowseFacet::Genre => filter.genre.as_deref(),
        BrowseFacet::Artist => filter.artist.as_deref(),
        BrowseFacet::Album => filter.album.as_deref(),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn facet_label(facet: BrowseFacet) -> String {
    let message = match facet {
        BrowseFacet::Genre => filter_strings::BROWSE_GENRE,
        BrowseFacet::Artist => filter_strings::BROWSE_ARTIST,
        BrowseFacet::Album => filter_strings::BROWSE_ALBUM,
    };
    filter_strings::text(message)
}

#[cfg_attr(not(test), allow(dead_code))]
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

#[cfg_attr(not(test), allow(dead_code))]
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

#[cfg_attr(not(test), allow(dead_code))]
fn available_facets(filter: &BrowseFilter) -> Vec<BrowseFacet> {
    FACETS
        .into_iter()
        .filter(|facet| filter_value(filter, *facet).is_none())
        .collect()
}

#[cfg_attr(not(test), allow(dead_code))]
fn remove_filter(filter: &BrowseFilter, facet: BrowseFacet) -> BrowseFilter {
    apply_selection(filter, facet, None)
}

#[cfg_attr(not(test), allow(dead_code))]
fn value_matches_search(value: &str, search: &str) -> bool {
    value.to_lowercase().contains(&search.trim().to_lowercase())
}

fn restored_filter(filter: &BrowseFilter) -> BrowseFilter {
    filter.clone()
}

fn browse_search_match_mode() -> gtk4::StringFilterMatchMode {
    gtk4::StringFilterMatchMode::Substring
}

pub struct BrowseBar {
    root: gtk4::Box,
    genre: gtk4::DropDown,
    artist: gtk4::DropDown,
    album: gtk4::DropDown,
    genre_values: RefCell<Vec<Option<String>>>,
    artist_values: RefCell<Vec<Option<String>>>,
    album_values: RefCell<Vec<Option<String>>>,
    filter: RefCell<BrowseFilter>,
    updating: Cell<bool>,
    conn: Rc<RefCell<Connection>>,
    on_changed: RefCell<Option<OnChanged>>,
}

impl BrowseBar {
    pub fn new(conn: Rc<RefCell<Connection>>) -> Rc<Self> {
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        root.set_margin_top(6);
        root.set_margin_bottom(6);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.add_css_class("toolbar");
        install_popup_style(&root);

        let (genre_box, genre) = facet_widget(
            &filter_strings::text(filter_strings::BROWSE_GENRE),
            &filter_strings::text(filter_strings::ALL_GENRES),
        );
        let (artist_box, artist) = facet_widget(
            &filter_strings::text(filter_strings::BROWSE_ARTIST),
            &filter_strings::text(filter_strings::ALL_ARTISTS),
        );
        let (album_box, album) = facet_widget(
            &filter_strings::text(filter_strings::BROWSE_ALBUM),
            &filter_strings::text(filter_strings::ALL_ALBUMS),
        );
        root.append(&genre_box);
        root.append(&artist_box);
        root.append(&album_box);

        let bar = Rc::new(Self {
            root,
            genre,
            artist,
            album,
            genre_values: RefCell::new(vec![None]),
            artist_values: RefCell::new(vec![None]),
            album_values: RefCell::new(vec![None]),
            filter: RefCell::new(BrowseFilter::default()),
            updating: Cell::new(false),
            conn,
            on_changed: RefCell::new(None),
        });
        wire_dropdown(&bar, BrowseFacet::Genre);
        wire_dropdown(&bar, BrowseFacet::Artist);
        wire_dropdown(&bar, BrowseFacet::Album);
        bar.refresh();
        bar
    }

    pub fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub fn filter(&self) -> BrowseFilter {
        self.filter.borrow().clone()
    }

    pub(super) fn restore_filter(&self, filter: &BrowseFilter) {
        let filter = restored_filter(filter);
        *self.filter.borrow_mut() = filter;
        self.refresh();
    }

    pub fn set_on_changed(&self, callback: impl Fn(BrowseFilter) + 'static) {
        *self.on_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub fn set_library_visible(&self, visible: bool) {
        self.root.set_visible(visible);
        tracing::info!(visible, "browse bar visibility updated");
    }

    pub fn refresh(&self) {
        let filter = self.filter();
        let (genres, artists, albums) = {
            let conn = self.conn.borrow();
            (
                load_values(&conn, BrowseFacet::Genre, &filter),
                load_values(&conn, BrowseFacet::Artist, &filter),
                load_values(&conn, BrowseFacet::Album, &filter),
            )
        };
        self.updating.set(true);
        replace_options(
            &self.genre,
            &self.genre_values,
            &filter_strings::text(filter_strings::ALL_GENRES),
            &filter_strings::text(filter_strings::UNKNOWN_GENRE),
            genres,
            filter.genre.as_deref(),
        );
        replace_options(
            &self.artist,
            &self.artist_values,
            &filter_strings::text(filter_strings::ALL_ARTISTS),
            &filter_strings::text(filter_strings::UNKNOWN_ARTIST),
            artists,
            filter.artist.as_deref(),
        );
        replace_options(
            &self.album,
            &self.album_values,
            &filter_strings::text(filter_strings::ALL_ALBUMS),
            &filter_strings::text(filter_strings::UNKNOWN_ALBUM),
            albums,
            filter.album.as_deref(),
        );
        self.updating.set(false);
    }

    fn selected(self: &Rc<Self>, facet: BrowseFacet) {
        if self.updating.get() {
            return;
        }
        let (dropdown, values) = match facet {
            BrowseFacet::Genre => (&self.genre, &self.genre_values),
            BrowseFacet::Artist => (&self.artist, &self.artist_values),
            BrowseFacet::Album => (&self.album, &self.album_values),
        };
        let value = values
            .borrow()
            .get(dropdown.selected() as usize)
            .cloned()
            .flatten();
        let current = self.filter();
        let next = apply_selection(&current, facet, value);
        if next == current {
            return;
        }
        *self.filter.borrow_mut() = next.clone();
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

    fn select_raw(&self, facet: BrowseFacet, value: &str) -> bool {
        let (dropdown, values) = match facet {
            BrowseFacet::Genre => (&self.genre, &self.genre_values),
            BrowseFacet::Artist => (&self.artist, &self.artist_values),
            BrowseFacet::Album => (&self.album, &self.album_values),
        };
        let Some(position) = values
            .borrow()
            .iter()
            .position(|candidate| candidate.as_deref() == Some(value))
        else {
            return false;
        };
        dropdown.set_selected(position as u32);
        true
    }
}

fn facet_widget(label: &str, all_label: &str) -> (gtk4::Box, gtk4::DropDown) {
    let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    container.set_hexpand(true);
    let label = gtk4::Label::new(Some(label));
    label.add_css_class("dim-label");
    let dropdown = gtk4::DropDown::from_strings(&[all_label]);
    dropdown.add_css_class(DROPDOWN_CSS_CLASS);
    let expression = gtk4::StringObject::this_expression("string");
    dropdown.set_expression(Some(expression));
    dropdown.set_enable_search(true);
    dropdown.set_search_match_mode(browse_search_match_mode());
    dropdown.set_hexpand(true);
    container.append(&label);
    container.append(&dropdown);
    (container, dropdown)
}

fn wire_dropdown(bar: &Rc<BrowseBar>, facet: BrowseFacet) {
    let dropdown = match facet {
        BrowseFacet::Genre => bar.genre.clone(),
        BrowseFacet::Artist => bar.artist.clone(),
        BrowseFacet::Album => bar.album.clone(),
    };
    let weak = Rc::downgrade(bar);
    dropdown.connect_selected_notify(move |_| {
        if let Some(bar) = weak.upgrade() {
            bar.selected(facet);
        }
    });
}

fn load_values(conn: &Connection, facet: BrowseFacet, filter: &BrowseFilter) -> Vec<BrowseValue> {
    queries::query_browse_values(conn, facet, filter).unwrap_or_else(|error| {
        tracing::warn!(%error, ?facet, "could not load browse facet values");
        Vec::new()
    })
}

#[allow(clippy::too_many_arguments)]
fn replace_options(
    dropdown: &gtk4::DropDown,
    stored_values: &RefCell<Vec<Option<String>>>,
    all_label: &str,
    unknown_label: &str,
    mut values: Vec<BrowseValue>,
    selected: Option<&str>,
) {
    if let Some(selected) = selected {
        if !values.iter().any(|value| value.value == selected) {
            values.push(BrowseValue {
                value: selected.to_string(),
                count: 0,
            });
        }
    }
    let mut raw = vec![None];
    let mut labels = vec![all_label.to_string()];
    for value in values {
        let display = if value.value.is_empty() {
            unknown_label
        } else {
            &value.value
        };
        labels.push(format!("{display} ({})", value.count));
        raw.push(Some(value.value));
    }
    let position = selected
        .and_then(|selected| {
            raw.iter()
                .position(|value| value.as_deref() == Some(selected))
        })
        .unwrap_or(0);
    let label_refs: Vec<_> = labels.iter().map(String::as_str).collect();
    dropdown.set_model(Some(&gtk4::StringList::new(&label_refs)));
    dropdown.set_selected(position as u32);
    *stored_values.borrow_mut() = raw;
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
        tracing::info!(?browse, ?ids, "browse smoke completed");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn browse_search_matches_substrings() {
        assert_eq!(
            browse_search_match_mode(),
            gtk4::StringFilterMatchMode::Substring
        );
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
}
