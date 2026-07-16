//! Album cover grid for the visual Library view — GtkGridView with
//! recycling, client-side sort/filter, hover overlays, now-playing
//! marker, and context menu.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::queries::AlbumSummary;
use rusqlite::Connection;

use crate::ui::album_card::{self, AlbumActivateSlot, AlbumCardShared, ArtistActivateSlot};
use crate::ui::album_context_menu::AlbumMenuShared;
use crate::ui::album_header::{self, AlbumSortKey};
use crate::ui::album_view_actions::{self, AlbumViewActions};
use crate::ui::album_view_state::{AlbumViewState, AlbumViewStateParts, NowPlayingAlbumCallback};
use crate::ui::cover_loader::CoverLoader;
use crate::ui::strings;

pub(in crate::ui) struct AlbumView {
    root: gtk4::Box, // vertical: header + stack(grid | empty | search-empty)
    grid_view: gtk4::GridView,
    state: AlbumViewState,
    actions: AlbumViewActions,
    on_activate: AlbumActivateSlot,
    on_artist_activate: ArtistActivateSlot,
}

impl AlbumView {
    pub(in crate::ui) fn new(
        conn: &Rc<RefCell<Connection>>,
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
        let on_activate: AlbumActivateSlot = Rc::new(RefCell::new(None));
        let on_artist_activate: ArtistActivateSlot = Rc::new(RefCell::new(None));

        let card_shared = Rc::new(AlbumCardShared {
            cover_loader,
            generation: Rc::new(Cell::new(0)),
            now_playing_album: Rc::new(RefCell::new(None)),
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

        let menu_shared = Rc::new(AlbumMenuShared {
            conn: conn.clone(),
            target_album: RefCell::new(None),
            on_play: card_shared.on_play.clone(),
            on_queue: card_shared.on_queue.clone(),
            on_shuffle: Rc::new(RefCell::new(None)),
            on_toast: Rc::new(RefCell::new(None)),
        });
        album_view_actions::install_context_menu(&grid_view, &filter_model, &menu_shared);

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
        let (header, count_label, _dropdown) =
            album_header::build_header(conn, move |new_key: AlbumSortKey| {
                let new_sorter = album_header::build_sorter(new_key);
                sort_model_for_header.set_sorter(Some(&new_sorter));
            });

        // ── Root layout ────────────────────────────────────────────────
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.append(&header);
        root.append(&stack);

        let state = AlbumViewState::new(
            AlbumViewStateParts::new(
                &root,
                &store,
                &filter_model,
                &count_label,
                &search_empty,
                &stack,
            ),
            &card_shared,
            conn,
        );
        let actions = AlbumViewActions::new(conn, &card_shared, &menu_shared);

        let view = Self {
            root,
            grid_view,
            state,
            actions,
            on_activate,
            on_artist_activate,
        };
        view.refresh();
        view
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    /// The `GridView` that holds album cards — exposed so `window.rs` can grab
    /// a weak reference for the playback-state callback (the `AlbumView` itself
    /// is not `Rc`-wrapped, but the GTK widget is ref-counted).
    pub(in crate::ui) fn grid_widget(&self) -> &gtk4::GridView {
        &self.grid_view
    }

    pub(in crate::ui) fn set_on_activate(&self, callback: impl Fn(AlbumSummary) + 'static) {
        *self.on_activate.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_artist_activate(&self, callback: impl Fn(String) + 'static) {
        *self.on_artist_activate.borrow_mut() = Some(Rc::new(callback));
    }

    /// Wires the play-album callback (queue replace + play).
    pub(in crate::ui) fn set_on_play(&self, callback: impl Fn(Vec<i64>, usize) + 'static) {
        self.actions.set_on_play(callback);
    }

    /// Wires the queue-album callback (append to queue).
    pub(in crate::ui) fn set_on_queue(&self, callback: impl Fn(Vec<i64>) + 'static) {
        self.actions.set_on_queue(callback);
    }

    /// Wires the toast overlay so the album context menu can surface
    /// "Added N tracks to Playlist" toasts. Must be called after the window's
    /// `adw::ToastOverlay` exists — same post-construction injection pattern
    /// as `PlayerController::set_toast_overlay`.
    pub(in crate::ui) fn set_toast_overlay(&self, overlay: &adw::ToastOverlay) {
        self.actions.set_toast_overlay(overlay);
    }

    /// Wires the shuffle-album callback (queue replace + shuffled play).
    pub(in crate::ui) fn set_on_shuffle(&self, callback: impl Fn(Vec<i64>, usize) + 'static) {
        self.actions.set_on_shuffle(callback);
    }

    pub(in crate::ui) fn refresh(&self) {
        self.state.refresh();
    }

    /// Returns a `'static` closure that applies a search filter to this view.
    /// Used to wire the search entry in `window.rs` without needing `Rc<AlbumView>`.
    pub(in crate::ui) fn filter_callback(&self) -> Rc<dyn Fn(&str)> {
        self.state.filter_callback()
    }

    /// Returns a `'static` closure that updates the now-playing album identity
    /// for this view, triggering card rebinds so EQ markers appear/disappear.
    /// Used by `window.rs` to wire the `PlayerController` callback without
    /// needing an `Rc<AlbumView>`.
    pub(in crate::ui) fn now_playing_callback(&self) -> NowPlayingAlbumCallback {
        self.state.now_playing_callback()
    }

    pub(in crate::ui) fn refresh_callback(&self) -> Rc<dyn Fn()> {
        self.state.refresh_callback()
    }

    #[cfg(test)]
    fn album_count(&self) -> u32 {
        self.state.album_count()
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
        let view = AlbumView::new(&conn, loader);

        assert_eq!(view.album_count(), 2);

        let filter = view.filter_callback();
        filter("first");
        assert_eq!(view.state.filtered_count(), 1);

        filter("");
        assert_eq!(view.state.filtered_count(), 2);
    }
}
