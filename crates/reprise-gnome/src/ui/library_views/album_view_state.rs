//! State transitions for the Album grid, kept separate from widget composition.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::playback::PlaybackState;
use reprise_core::queries::{self, AlbumSummary};
use rusqlite::Connection;

use crate::ui::album_card::AlbumCardShared;
use crate::ui::album_card_state::{album_index, PendingAlbumReveal};
use crate::ui::album_header;
use crate::ui::strings;

pub(in crate::ui) type NowPlayingAlbumCallback = Rc<dyn Fn(Option<(String, String)>)>;

pub(in crate::ui) struct AlbumViewStateParts {
    root: glib::WeakRef<gtk4::Box>,
    store: gtk4::gio::ListStore,
    filter_model: gtk4::FilterListModel,
    count_label: glib::WeakRef<gtk4::Label>,
    search_empty: adw::StatusPage,
    stack: gtk4::Stack,
}

impl AlbumViewStateParts {
    pub(in crate::ui) fn new(
        root: &gtk4::Box,
        store: &gtk4::gio::ListStore,
        filter_model: &gtk4::FilterListModel,
        count_label: &gtk4::Label,
        search_empty: &adw::StatusPage,
        stack: &gtk4::Stack,
    ) -> Self {
        Self {
            root: root.downgrade(),
            store: store.clone(),
            filter_model: filter_model.clone(),
            count_label: count_label.downgrade(),
            search_empty: search_empty.clone(),
            stack: stack.clone(),
        }
    }
}

#[derive(Clone)]
pub(in crate::ui) struct AlbumViewState {
    root: glib::WeakRef<gtk4::Box>,
    store: gtk4::gio::ListStore,
    filter_model: gtk4::FilterListModel,
    count_label: glib::WeakRef<gtk4::Label>,
    search_empty: adw::StatusPage,
    stack: gtk4::Stack,
    card_shared: Rc<AlbumCardShared>,
    conn: Rc<RefCell<Connection>>,
}

impl AlbumViewState {
    pub(in crate::ui) fn new(
        parts: AlbumViewStateParts,
        card_shared: &Rc<AlbumCardShared>,
        conn: &Rc<RefCell<Connection>>,
    ) -> Self {
        let AlbumViewStateParts {
            root,
            store,
            filter_model,
            count_label,
            search_empty,
            stack,
        } = parts;
        Self {
            root,
            store,
            filter_model,
            count_label,
            search_empty,
            stack,
            card_shared: card_shared.clone(),
            conn: conn.clone(),
        }
    }

    pub(in crate::ui) fn refresh(&self) {
        let albums = {
            let conn = self.conn.borrow();
            queries::query_albums(&conn)
        };
        let albums = match albums {
            Ok(albums) => albums,
            Err(error) => {
                tracing::warn!(%error, "could not load Albums view");
                self.stack.set_visible_child_name("empty");
                return;
            }
        };
        if albums.is_empty() {
            self.store.remove_all();
            self.stack.set_visible_child_name("empty");
            self.update_count(0);
            return;
        }

        let generation = self.card_shared.generation.get().wrapping_add(1);
        self.card_shared.generation.set(generation);
        self.store.remove_all();
        for album in albums {
            self.store.append(&glib::BoxedAnyObject::new(album));
        }
        self.stack.set_visible_child_name("grid");
        self.update_count(self.filter_model.n_items());
    }

    pub(in crate::ui) fn filter_callback(&self) -> Rc<dyn Fn(&str)> {
        let state = self.clone();
        Rc::new(move |raw_text: &str| state.apply_filter(raw_text))
    }

    pub(in crate::ui) fn now_playing_callback(&self) -> NowPlayingAlbumCallback {
        let now_playing = self.card_shared.now_playing_album.clone();
        let store = self.store.clone();
        Rc::new(move |album: Option<(String, String)>| {
            let old = now_playing.borrow().clone();
            *now_playing.borrow_mut() = album.clone();
            if let Some((old_album, old_artist)) = old {
                rebind_in_store(&store, &old_album, &old_artist);
            }
            if let Some((new_album, new_artist)) = album {
                rebind_in_store(&store, &new_album, &new_artist);
            }
        })
    }

    pub(in crate::ui) fn playback_state_callback(&self) -> Rc<dyn Fn(PlaybackState)> {
        let playback_state = self.card_shared.playback_state.clone();
        let now_playing_album = self.card_shared.now_playing_album.clone();
        let store = self.store.clone();
        Rc::new(move |state| {
            playback_state.set(state);
            let current_album = now_playing_album.borrow().clone();
            if let Some((album, artist)) = current_album {
                rebind_in_store(&store, &album, &artist);
            }
        })
    }

    pub(in crate::ui) fn refresh_callback(&self) -> Rc<dyn Fn()> {
        let state = self.clone();
        Rc::new(move || {
            if state.root.upgrade().is_some() {
                state.refresh();
            }
        })
    }

    pub(in crate::ui) fn reveal_album(
        &self,
        grid: &gtk4::GridView,
        title: &str,
        artist: &str,
    ) -> bool {
        self.apply_filter("");
        let Some(index) = self.filtered_album_index(title, artist) else {
            return false;
        };

        let generation = self.card_shared.reveal_generation.get().wrapping_add(1);
        self.card_shared.reveal_generation.set(generation);
        *self.card_shared.pending_reveal.borrow_mut() = Some(PendingAlbumReveal {
            album: title.to_owned(),
            artist: artist.to_owned(),
            generation,
        });
        rebind_in_store(&self.store, title, artist);

        focus_album_at(grid, index);
        true
    }

    /// Restores keyboard focus after Back without clearing the user's search
    /// or replaying GRID-5's one-second reveal highlight.
    pub(in crate::ui) fn restore_album_focus(
        &self,
        grid: &gtk4::GridView,
        title: &str,
        artist: &str,
    ) -> bool {
        let Some(index) = self.filtered_album_index(title, artist) else {
            return false;
        };
        focus_album_at(grid, index);
        true
    }

    fn filtered_album_index(&self, title: &str, artist: &str) -> Option<u32> {
        let albums = (0..self.filter_model.n_items())
            .filter_map(|index| {
                let object = self.filter_model.item(index)?;
                let boxed = object.downcast_ref::<glib::BoxedAnyObject>()?;
                let album = boxed.borrow::<AlbumSummary>().clone();
                Some(album)
            })
            .collect::<Vec<_>>();
        album_index(&albums, title, artist)
    }

    #[cfg(test)]
    pub(in crate::ui) fn album_count(&self) -> u32 {
        self.store.n_items()
    }

    #[cfg(test)]
    pub(in crate::ui) fn filtered_count(&self) -> u32 {
        self.filter_model.n_items()
    }

    fn apply_filter(&self, raw_text: &str) {
        let text = raw_text.trim().to_lowercase();
        if text.is_empty() {
            self.filter_model
                .set_filter(Some(&gtk4::CustomFilter::new(|_| true)));
            if self.store.n_items() > 0 {
                self.stack.set_visible_child_name("grid");
            }
        } else {
            let filter_text = text.clone();
            self.filter_model
                .set_filter(Some(&gtk4::CustomFilter::new(move |object| {
                    let Some(boxed) = object.downcast_ref::<glib::BoxedAnyObject>() else {
                        return false;
                    };
                    let album: std::cell::Ref<AlbumSummary> = boxed.borrow();
                    matches_filter(&album, &filter_text)
                })));
            if self.filter_model.n_items() == 0 {
                self.search_empty
                    .set_title(&strings::text(strings::ALBUM_SEARCH_EMPTY).replace("{}", &text));
                self.stack.set_visible_child_name("search-empty");
            } else {
                self.stack.set_visible_child_name("grid");
            }
        }
        self.update_count(self.filter_model.n_items());
    }

    fn update_count(&self, count: u32) {
        if let Some(label) = self.count_label.upgrade() {
            album_header::update_count(&label, count);
        }
    }
}

fn focus_album_at(grid: &gtk4::GridView, index: u32) {
    // `ListScrollFlags::FOCUS` alone moves focus only WITHIN the grid — it does
    // not pull the grid into the window's focus chain. Both callers arrive from
    // elsewhere (the player surfaces for reveal, the navigation stack for Back),
    // so the grid holds no focus at that moment and the flag silently does
    // nothing: the grid scrolls, but `focus_child()` stays unset and the
    // keyboard still drives whatever the user came from. Grabbing focus first
    // makes the grid the focus widget, then the flag lands it on `index`.
    grid.grab_focus();
    let scroll = gtk4::ScrollInfo::new();
    scroll.set_enable_vertical(true);
    grid.scroll_to(index, gtk4::ListScrollFlags::FOCUS, Some(scroll));
}

pub(in crate::ui) fn matches_filter(album: &AlbumSummary, raw_filter: &str) -> bool {
    let normalized = raw_filter.trim().to_lowercase();
    normalized.is_empty()
        || album.album.to_lowercase().contains(&normalized)
        || album.album_artist.to_lowercase().contains(&normalized)
}

pub(in crate::ui) fn identity_matches(album: &AlbumSummary, title: &str, artist: &str) -> bool {
    album.album.eq_ignore_ascii_case(title) && album.album_artist.eq_ignore_ascii_case(artist)
}

fn rebind_in_store(store: &gtk4::gio::ListStore, album: &str, album_artist: &str) {
    for index in 0..store.n_items() {
        let Some(object) = store.item(index) else {
            continue;
        };
        let Some(boxed) = object.downcast_ref::<glib::BoxedAnyObject>() else {
            continue;
        };
        let summary: std::cell::Ref<AlbumSummary> = boxed.borrow();
        if identity_matches(&summary, album, album_artist) {
            drop(summary);
            store.remove(index);
            store.insert(index, &object);
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use reprise_core::queries::AlbumSummary;

    fn album(title: &str, artist: &str) -> AlbumSummary {
        AlbumSummary {
            album: title.to_string(),
            album_artist: artist.to_string(),
            representative_path: String::new(),
            track_count: 0,
            year: None,
            total_duration_ms: 0,
            max_added_at: 0,
            total_play_count: 0,
        }
    }

    #[test]
    fn filter_matches_trimmed_case_insensitive_title_or_artist() {
        let album = album("Blue Train", "John Coltrane");
        assert!(super::matches_filter(&album, " blue "));
        assert!(super::matches_filter(&album, "COLTRANE"));
        assert!(!super::matches_filter(&album, "Miles"));
    }

    #[test]
    fn identity_match_is_case_insensitive_for_both_parts() {
        let album = album("Blue Train", "John Coltrane");
        assert!(super::identity_matches(
            &album,
            "blue train",
            "JOHN COLTRANE"
        ));
        assert!(!super::identity_matches(
            &album,
            "Blue Train",
            "Miles Davis"
        ));
    }
}
