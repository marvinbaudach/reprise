//! Album cover grid for the visual Library view — GtkGridView with
//! recycling, client-side sort/filter, hover overlays, now-playing
//! marker, and context menu.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::queries::{self, AlbumSummary};
use rusqlite::Connection;

use crate::ui::album_card::{self, AlbumCardShared};
use crate::ui::album_card_actions;
use crate::ui::album_card_css;
use crate::ui::album_context_menu::{self, AlbumMenuShared};
use crate::ui::album_header::{self, AlbumSortKey};
use crate::ui::cover_loader::CoverLoader;
use crate::ui::strings;

type OnActivate = Rc<dyn Fn(AlbumSummary)>;

pub(in crate::ui) struct AlbumView {
    root: gtk4::Box, // vertical: header + stack(grid | empty | search-empty)
    store: gtk4::gio::ListStore,
    filter_model: gtk4::FilterListModel,
    grid_view: gtk4::GridView,
    count_label: gtk4::Label,
    search_empty: adw::StatusPage,
    stack: gtk4::Stack,
    card_shared: Rc<AlbumCardShared>,
    menu_shared: Rc<AlbumMenuShared>,
    conn: Rc<RefCell<Connection>>,
    on_activate: Rc<RefCell<Option<OnActivate>>>,
    on_artist_activate: Rc<RefCell<Option<Rc<dyn Fn(String)>>>>,
}

impl AlbumView {
    pub(in crate::ui) fn new(
        conn: Rc<RefCell<Connection>>,
        cover_loader: Rc<CoverLoader>,
    ) -> Self {
        // ── Model chain: ListStore → SortListModel → FilterListModel ──
        let store = gtk4::gio::ListStore::new::<glib::BoxedAnyObject>();
        let initial_sort = {
            let c = conn.borrow();
            album_header::current_sort_key(&c)
        };
        let sorter = album_header::build_sorter(initial_sort);
        let sort_model = gtk4::SortListModel::new(Some(store.clone()), Some(sorter));
        let filter = gtk4::CustomFilter::new(|_| true);
        let filter_model = gtk4::FilterListModel::new(Some(sort_model.clone()), Some(filter));

        // ── Shared state for card factory ──────────────────────────────
        let on_activate: Rc<RefCell<Option<OnActivate>>> = Rc::new(RefCell::new(None));
        let on_artist_activate: Rc<RefCell<Option<Rc<dyn Fn(String)>>>> =
            Rc::new(RefCell::new(None));

        let card_shared = Rc::new(AlbumCardShared {
            conn: conn.clone(),
            cover_loader,
            generation: Rc::new(Cell::new(0)),
            now_playing_album: Rc::new(RefCell::new(None)),
            playback_paused: Rc::new(Cell::new(false)),
            on_activate: on_activate.clone(),
            on_play: Rc::new(RefCell::new(None)),
            on_queue: Rc::new(RefCell::new(None)),
            on_artist_activate: on_artist_activate.clone(),
        });

        // ── Card factory ───────────────────────────────────────────────
        let factory = album_card::build_factory(&card_shared);

        // ── GridView ───────────────────────────────────────────────────
        let grid_view = gtk4::GridView::builder()
            .model(&gtk4::NoSelection::new(Some(filter_model.clone())))
            .factory(&factory)
            .min_columns(1)
            .max_columns(200)
            .build();
        grid_view.add_css_class("library-grid");

        // Context menu action group on the grid view.
        let menu_shared = Rc::new(AlbumMenuShared {
            conn: conn.clone(),
            target_album: RefCell::new(None),
            on_play: card_shared.on_play.clone(),
            on_queue: card_shared.on_queue.clone(),
            on_shuffle: Rc::new(RefCell::new(None)),
            on_toast: Rc::new(RefCell::new(None)),
        });
        let action_group = album_context_menu::wire_actions(&menu_shared);
        grid_view.insert_action_group(album_context_menu::ACTION_GROUP_NAME, Some(&action_group));

        // Right-click gesture on the grid.
        let right_click = gtk4::GestureClick::builder()
            .button(3) // secondary button
            .build();
        {
            let menu_shared = menu_shared.clone();
            let grid_weak = grid_view.downgrade();
            let filter_model_ref = filter_model.clone();
            right_click.connect_released(move |gesture, _n, x, y| {
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                let Some(grid) = grid_weak.upgrade() else {
                    return;
                };
                // Walk up from picked widget to find the album card.
                if let Some(widget) = grid.pick(x, y, gtk4::PickFlags::DEFAULT) {
                    let mut w: Option<gtk4::Widget> = Some(widget);
                    while let Some(ref current) = w {
                        if current.has_css_class(album_card_css::CARD_CLASS) {
                            break;
                        }
                        w = current.parent();
                    }
                    if let Some(card) = w {
                        // Match album by tooltip text (album title).
                        let tooltip = card.tooltip_text().unwrap_or_default();
                        let album_title = tooltip.split(" · ").next().unwrap_or("");
                        let n = filter_model_ref.n_items();
                        for i in 0..n {
                            if let Some(obj) = filter_model_ref.item(i) {
                                if let Some(boxed) = obj.downcast_ref::<glib::BoxedAnyObject>() {
                                    let album: std::cell::Ref<AlbumSummary> = boxed.borrow();
                                    if album.album == album_title {
                                        let album_clone = album.clone();
                                        drop(album);
                                        album_context_menu::show(
                                            &card,
                                            &menu_shared,
                                            album_clone,
                                            x,
                                            y,
                                        );
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            });
        }
        grid_view.add_controller(right_click);

        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&grid_view)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .hexpand(true)
            .build();

        // ── Empty states ───────────────────────────────────────────────
        let empty = adw::StatusPage::builder()
            .icon_name("folder-music-symbolic")
            .title(strings::text(strings::ALBUMS_EMPTY_TITLE))
            .description(strings::text(strings::ALBUMS_EMPTY_DESCRIPTION))
            .build();
        let search_empty = adw::StatusPage::builder()
            .icon_name("edit-find-symbolic")
            .title("")
            .build();

        let stack = gtk4::Stack::new();
        stack.add_named(&scrolled, Some("grid"));
        stack.add_named(&empty, Some("empty"));
        stack.add_named(&search_empty, Some("search-empty"));

        // ── Header ─────────────────────────────────────────────────────
        let sort_model_for_header = sort_model.clone();
        let (header, count_label, _dropdown) = album_header::build_header(
            conn.clone(),
            move |new_key: AlbumSortKey| {
                let new_sorter = album_header::build_sorter(new_key);
                sort_model_for_header.set_sorter(Some(&new_sorter));
            },
        );

        // ── Root layout ────────────────────────────────────────────────
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.append(&header);
        root.append(&stack);

        let view = Self {
            root,
            store,
            filter_model,
            grid_view,
            count_label,
            search_empty,
            stack,
            card_shared,
            menu_shared,
            conn,
            on_activate,
            on_artist_activate,
        };
        view.refresh();
        view
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(in crate::ui) fn set_on_activate(&self, callback: impl Fn(AlbumSummary) + 'static) {
        *self.on_activate.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_artist_activate(&self, callback: impl Fn(String) + 'static) {
        *self.on_artist_activate.borrow_mut() = Some(Rc::new(callback));
    }

    /// Wires the play-album callback (queue replace + play).
    pub(in crate::ui) fn set_on_play(&self, callback: impl Fn(Vec<i64>, usize) + 'static) {
        let conn = self.conn.clone();
        let cb = Rc::new(callback);
        let play_cb: Rc<dyn Fn(&AlbumSummary)> = Rc::new(move |album: &AlbumSummary| {
            let ids = {
                let c = conn.borrow();
                album_card_actions::album_track_ids(&c, album)
            };
            if !ids.is_empty() {
                cb(ids, 0);
            }
        });
        *self.card_shared.on_play.borrow_mut() = Some(play_cb.clone());
        *self.menu_shared.on_play.borrow_mut() = Some(play_cb);
    }

    /// Wires the queue-album callback (append to queue).
    pub(in crate::ui) fn set_on_queue(&self, callback: impl Fn(Vec<i64>) + 'static) {
        let conn = self.conn.clone();
        let cb = Rc::new(callback);
        let queue_cb: Rc<dyn Fn(&AlbumSummary)> = Rc::new(move |album: &AlbumSummary| {
            let ids = {
                let c = conn.borrow();
                album_card_actions::album_track_ids(&c, album)
            };
            if !ids.is_empty() {
                cb(ids);
            }
        });
        *self.card_shared.on_queue.borrow_mut() = Some(queue_cb.clone());
        *self.menu_shared.on_queue.borrow_mut() = Some(queue_cb);
    }

    /// Wires the shuffle-album callback (queue replace + shuffled play).
    pub(in crate::ui) fn set_on_shuffle(&self, callback: impl Fn(Vec<i64>, usize) + 'static) {
        let conn = self.conn.clone();
        let cb = Rc::new(callback);
        let shuffle_cb: Rc<dyn Fn(&AlbumSummary)> = Rc::new(move |album: &AlbumSummary| {
            let mut ids = {
                let c = conn.borrow();
                album_card_actions::album_track_ids(&c, album)
            };
            if !ids.is_empty() {
                album_card_actions::shuffle_ids(&mut ids);
                cb(ids, 0);
            }
        });
        *self.menu_shared.on_shuffle.borrow_mut() = Some(shuffle_cb);
    }

    /// Sets the search filter text. Empty = show all.
    pub(in crate::ui) fn set_filter(&self, text: &str) {
        let text = text.trim().to_lowercase();
        if text.is_empty() {
            self.filter_model
                .set_filter(Some(&gtk4::CustomFilter::new(|_| true)));
            if self.store.n_items() > 0 {
                self.stack.set_visible_child_name("grid");
            }
        } else {
            let text_clone = text.clone();
            self.filter_model
                .set_filter(Some(&gtk4::CustomFilter::new(move |obj| {
                    let Some(boxed) = obj.downcast_ref::<glib::BoxedAnyObject>() else {
                        return false;
                    };
                    let album: std::cell::Ref<AlbumSummary> = boxed.borrow();
                    album.album.to_lowercase().contains(&text_clone)
                        || album.album_artist.to_lowercase().contains(&text_clone)
                })));
            if self.filter_model.n_items() == 0 {
                self.search_empty.set_title(
                    &strings::text(strings::ALBUM_SEARCH_EMPTY).replace("{}", &text),
                );
                self.stack.set_visible_child_name("search-empty");
            } else {
                self.stack.set_visible_child_name("grid");
            }
        }
        album_header::update_count(&self.count_label, self.filter_model.n_items());
    }

    /// Updates the now-playing album identity. Pass `None` when playback stops.
    pub(in crate::ui) fn set_now_playing_album(&self, album: Option<(String, String)>) {
        let old = self.card_shared.now_playing_album.borrow().clone();
        *self.card_shared.now_playing_album.borrow_mut() = album.clone();

        // Trigger rebind for old and new now-playing positions.
        if let Some((old_album, old_artist)) = old {
            self.rebind_album(&old_album, &old_artist);
        }
        if let Some((new_album, new_artist)) = album {
            self.rebind_album(&new_album, &new_artist);
        }
    }

    /// Sets the playback paused state (freezes EQ bars via CSS class).
    pub(in crate::ui) fn set_playback_paused(&self, paused: bool) {
        self.card_shared.playback_paused.set(paused);
        if paused {
            self.grid_view.add_css_class("playback-paused");
        } else {
            self.grid_view.remove_css_class("playback-paused");
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
            album_header::update_count(&self.count_label, 0);
            return;
        }
        let generation = self.card_shared.generation.get().wrapping_add(1);
        self.card_shared.generation.set(generation);

        // Rebuild the store.
        self.store.remove_all();
        for album in albums {
            self.store.append(&glib::BoxedAnyObject::new(album));
        }
        self.stack.set_visible_child_name("grid");
        album_header::update_count(&self.count_label, self.filter_model.n_items());
    }

    /// Returns a `'static` closure that applies a search filter to this view.
    /// Used to wire the search entry in `window.rs` without needing Rc<AlbumView>.
    pub(in crate::ui) fn filter_callback(&self) -> Rc<dyn Fn(&str)> {
        let store = self.store.clone();
        let filter_model = self.filter_model.clone();
        let stack = self.stack.clone();
        let count_label = self.count_label.downgrade();
        let search_empty = self.search_empty.clone();
        Rc::new(move |raw_text: &str| {
            let text = raw_text.trim().to_lowercase();
            if text.is_empty() {
                filter_model.set_filter(Some(&gtk4::CustomFilter::new(|_| true)));
                if store.n_items() > 0 {
                    stack.set_visible_child_name("grid");
                }
            } else {
                let t = text.clone();
                filter_model.set_filter(Some(&gtk4::CustomFilter::new(move |obj| {
                    let Some(boxed) = obj.downcast_ref::<glib::BoxedAnyObject>() else {
                        return false;
                    };
                    let album: std::cell::Ref<AlbumSummary> = boxed.borrow();
                    album.album.to_lowercase().contains(&t)
                        || album.album_artist.to_lowercase().contains(&t)
                })));
                if filter_model.n_items() == 0 {
                    search_empty.set_title(
                        &strings::text(strings::ALBUM_SEARCH_EMPTY).replace("{}", &text),
                    );
                    stack.set_visible_child_name("search-empty");
                } else {
                    stack.set_visible_child_name("grid");
                }
            }
            if let Some(label) = count_label.upgrade() {
                album_header::update_count(&label, filter_model.n_items());
            }
        })
    }

    /// Returns a `'static` closure that updates the now-playing album identity
    /// for this view, triggering card rebinds so EQ markers appear/disappear.
    /// Used by `window.rs` to wire the `PlayerController` callback without
    /// needing an `Rc<AlbumView>`.
    pub(in crate::ui) fn now_playing_callback(
        &self,
    ) -> Rc<dyn Fn(Option<(String, String)>)> {
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

    pub(in crate::ui) fn refresh_callback(&self) -> Rc<dyn Fn()> {
        let root = self.root.downgrade();
        let store = self.store.clone();
        let stack = self.stack.clone();
        let count_label = self.count_label.downgrade();
        let card_shared = self.card_shared.clone();
        let conn = self.conn.clone();
        let filter_model = self.filter_model.clone();
        Rc::new(move || {
            let Some(_root) = root.upgrade() else {
                return;
            };
            let albums = {
                let conn = conn.borrow();
                queries::query_albums(&conn)
            };
            let albums = match albums {
                Ok(a) => a,
                Err(error) => {
                    tracing::warn!(%error, "could not refresh Albums view");
                    stack.set_visible_child_name("empty");
                    return;
                }
            };
            if albums.is_empty() {
                store.remove_all();
                stack.set_visible_child_name("empty");
                if let Some(label) = count_label.upgrade() {
                    album_header::update_count(&label, 0);
                }
                return;
            }
            let generation = card_shared.generation.get().wrapping_add(1);
            card_shared.generation.set(generation);
            store.remove_all();
            for album in albums {
                store.append(&glib::BoxedAnyObject::new(album));
            }
            stack.set_visible_child_name("grid");
            if let Some(label) = count_label.upgrade() {
                album_header::update_count(&label, filter_model.n_items());
            }
        })
    }

    /// Forces a rebind for the card displaying the given album (if visible).
    fn rebind_album(&self, album: &str, album_artist: &str) {
        let n = self.store.n_items();
        for i in 0..n {
            if let Some(obj) = self.store.item(i) {
                if let Some(boxed) = obj.downcast_ref::<glib::BoxedAnyObject>() {
                    let a: std::cell::Ref<AlbumSummary> = boxed.borrow();
                    if a.album.eq_ignore_ascii_case(album)
                        && a.album_artist.eq_ignore_ascii_case(album_artist)
                    {
                        drop(a);
                        // Splice: remove + re-insert triggers items_changed → rebind.
                        let item = self.store.item(i).unwrap();
                        self.store.remove(i);
                        self.store.insert(i, &item);
                        break;
                    }
                }
            }
        }
    }

    #[cfg(test)]
    fn album_count(&self) -> u32 {
        self.store.n_items()
    }
}

/// Removes and re-inserts an album item in `store` so the card factory rebinds
/// it. Used by `now_playing_callback` to trigger EQ marker updates without a
/// full store rebuild.
fn rebind_in_store(store: &gtk4::gio::ListStore, album: &str, album_artist: &str) {
    let n = store.n_items();
    for i in 0..n {
        if let Some(obj) = store.item(i) {
            if let Some(boxed) = obj.downcast_ref::<glib::BoxedAnyObject>() {
                let a: std::cell::Ref<AlbumSummary> = boxed.borrow();
                if a.album.eq_ignore_ascii_case(album)
                    && a.album_artist.eq_ignore_ascii_case(album_artist)
                {
                    drop(a);
                    let item = store.item(i).unwrap();
                    store.remove(i);
                    store.insert(i, &item);
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn album_grid_loads_from_query_and_supports_filter() {
        gtk4::init().unwrap();
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (path,title,artist,album,added_at) VALUES
             ('/one.flac','One','Artist A','First',0),
             ('/two.flac','Two','Artist B','Second',0);",
        )
        .unwrap();
        let conn = Rc::new(RefCell::new(conn));
        let loader =
            crate::ui::cover_loader::CoverLoader::new(crate::ui::cover_download_worker::setup());
        let view = AlbumView::new(conn, loader);

        assert_eq!(view.album_count(), 2);

        view.set_filter("first");
        assert_eq!(view.filter_model.n_items(), 1);

        view.set_filter("");
        assert_eq!(view.filter_model.n_items(), 2);
    }
}
