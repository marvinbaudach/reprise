//! Cascading Genre/Artist/Album controls for the Library source.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::queries::{self, BrowseFacet, BrowseFilter, BrowseValue};
use rusqlite::Connection;

use crate::ui::strings;
use crate::ui::track_list::Shared;

const SMOKE_ENV: &str = "REPRISE_SMOKE_BROWSE";
type OnChanged = Rc<dyn Fn(BrowseFilter)>;

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

        let (genre_box, genre) = facet_widget(strings::BROWSE_GENRE, strings::ALL_GENRES);
        let (artist_box, artist) = facet_widget(strings::BROWSE_ARTIST, strings::ALL_ARTISTS);
        let (album_box, album) = facet_widget(strings::BROWSE_ALBUM, strings::ALL_ALBUMS);
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
            strings::ALL_GENRES,
            strings::UNKNOWN_GENRE,
            genres,
            filter.genre.as_deref(),
        );
        replace_options(
            &self.artist,
            &self.artist_values,
            strings::ALL_ARTISTS,
            strings::UNKNOWN_ARTIST,
            artists,
            filter.artist.as_deref(),
        );
        replace_options(
            &self.album,
            &self.album_values,
            strings::ALL_ALBUMS,
            strings::UNKNOWN_ALBUM,
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
    dropdown.set_enable_search(true);
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
}
