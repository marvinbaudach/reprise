//! Album cover grid for the visual Library view — GtkGridView with
//! recycling, client-side sort/filter, hover overlays, now-playing
//! marker, and context menu.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::playback::PlaybackState;
use reprise_core::queries::AlbumSummary;
use reprise_core::view_source::ViewSource;
use rusqlite::Connection;

use crate::ui::album_card::{self, AlbumActivateSlot, AlbumCardShared, ArtistActivateSlot};
use crate::ui::album_card_state::{AlbumCardIdentityRegistry, RevealBindingRegistry};
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
            identity_generation: Rc::new(Cell::new(0)),
            identities: Rc::new(RefCell::new(AlbumCardIdentityRegistry::default())),
            playback_state: Rc::new(Cell::new(PlaybackState::Stopped)),
            reveal_generation: Rc::new(Cell::new(0)),
            pending_reveal: Rc::new(RefCell::new(None)),
            reveal_bindings: Rc::new(RefCell::new(RevealBindingRegistry::default())),
            now_playing_album: Rc::new(RefCell::new(None)),
            on_play: Rc::new(RefCell::new(None)),
            on_primary: Rc::new(RefCell::new(None)),
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
            // One click opens an album: the CELL machinery emits `activate`
            // (routed below), which is reliable where a `GestureClick` on
            // the plain card `Box` was not — the cell claims press
            // sequences before a card gesture's `released` can fire.
            .single_click_activate(true)
            .build();
        grid_view.add_css_class("library-grid");

        // The single activation path for pointer AND keyboard: a single
        // click (via `single_click_activate` above) and Enter on the
        // focused card (GtkListBase binds Return/KP_Enter) both emit
        // `activate`, routed here to `on_activate`.
        {
            let on_activate = on_activate.clone();
            let filter_model = filter_model.clone();
            grid_view.connect_activate(move |_grid, position| {
                let Some(obj) = filter_model.item(position) else {
                    return;
                };
                let Some(boxed) = obj.downcast_ref::<glib::BoxedAnyObject>() else {
                    return;
                };
                let album = boxed.borrow::<AlbumSummary>().clone();
                let callback = on_activate.borrow().clone();
                if let Some(cb) = callback {
                    cb(album);
                }
            });
        }

        let menu_shared = Rc::new(AlbumMenuShared {
            target_album: RefCell::new(None),
            on_play: card_shared.on_play.clone(),
            on_play_next: Rc::new(RefCell::new(None)),
            on_queue: Rc::new(RefCell::new(None)),
            on_artist: on_artist_activate.clone(),
            on_edit_tags: Rc::new(RefCell::new(None)),
        });
        album_view_actions::install_context_menu(&grid_view, &card_shared.identities, &menu_shared);

        // Ctrl+Enter is the only album-specific key binding. Plain Enter and
        // arrows remain native GridView behavior; Space propagates to the
        // window's global play/pause shortcut.
        let album_keys = gtk4::EventControllerKey::new();
        {
            let grid = grid_view.downgrade();
            let identities = card_shared.identities.clone();
            let on_play = card_shared.on_play.clone();
            album_keys.connect_key_pressed(move |_, key, _, modifiers| {
                album_view_actions::route_album_key(key, modifiers, || {
                    let Some(grid) = grid.upgrade() else {
                        return false;
                    };
                    let Some(album) = album_view_actions::focused_album(&grid, &identities) else {
                        return false;
                    };
                    let callback = on_play.borrow().clone();
                    let Some(callback) = callback else {
                        return false;
                    };
                    callback(&album);
                    true
                })
            });
        }
        grid_view.add_controller(album_keys);

        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&grid_view)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .hexpand(true)
            .build();

        // Pointer → keyboard handoff: a primary click on EMPTY grid space
        // hands the grid keyboard focus, so arrows / Enter / Menu key work
        // right away. `pressed` in the CAPTURE phase on the scrolled window:
        // `GtkListBase`'s internal gesture claims the sequence even on empty
        // space (retroactively denying other gestures), so a bubble or
        // capture `released` never fires here. Card hits are explicitly
        // skipped — grabbing focus mid-press scrolls the grid to its focused
        // cell, which cancels the card's own click gesture (a real caught
        // regression: cards stopped opening).
        {
            let empty_click = gtk4::GestureClick::new();
            empty_click.set_propagation_phase(gtk4::PropagationPhase::Capture);
            let grid = grid_view.downgrade();
            empty_click.connect_pressed(move |gesture, _n, x, y| {
                let Some(scrolled) = gesture.widget() else {
                    return;
                };
                let mut hit = scrolled.pick(x, y, gtk4::PickFlags::DEFAULT);
                while let Some(widget) = hit {
                    if widget.has_css_class(crate::ui::album_card_css::CARD_CLASS) {
                        return;
                    }
                    if widget == scrolled {
                        break;
                    }
                    hit = widget.parent();
                }
                let Some(grid) = grid.upgrade() else { return };
                if grid.grab_focus() {
                    tracing::debug!("album grid: empty-space click focused the grid");
                } else {
                    tracing::debug!("album grid: empty-space click focus refused");
                }
            });
            scrolled.add_controller(empty_click);
        }

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
    pub(in crate::ui) fn set_on_play(
        &self,
        callback: impl Fn(Vec<i64>, usize, ViewSource) + 'static,
    ) {
        self.actions.set_on_play(callback);
    }

    pub(in crate::ui) fn set_on_primary(
        &self,
        callback: impl Fn(Vec<i64>, usize, ViewSource, AlbumSummary) + 'static,
    ) {
        self.actions.set_on_primary(callback);
    }

    pub(in crate::ui) fn set_on_play_next(&self, callback: impl Fn(Vec<i64>) + 'static) {
        self.actions.set_on_play_next(callback);
    }

    /// Wires the queue-album callback (append to queue).
    pub(in crate::ui) fn set_on_queue(&self, callback: impl Fn(Vec<i64>) + 'static) {
        self.actions.set_on_queue(callback);
    }

    pub(in crate::ui) fn set_on_edit_tags(&self, callback: impl Fn(Vec<i64>) + 'static) {
        self.actions.set_on_edit_tags(callback);
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

    pub(in crate::ui) fn playback_state_callback(&self) -> Rc<dyn Fn(PlaybackState)> {
        self.state.playback_state_callback()
    }

    pub(in crate::ui) fn reveal_callback(
        &self,
    ) -> crate::ui::window::album_grid_reveal::AlbumRevealCallback {
        let state = self.state.clone();
        let grid = self.grid_view.downgrade();
        Rc::new(move |album, artist| {
            grid.upgrade()
                .is_some_and(|grid| state.reveal_album(&grid, album, artist))
        })
    }

    pub(in crate::ui) fn restore_focus_callback(
        &self,
    ) -> crate::ui::window::album_grid_reveal::AlbumRevealCallback {
        let state = self.state.clone();
        let grid = self.grid_view.downgrade();
        Rc::new(move |album, artist| {
            grid.upgrade()
                .is_some_and(|grid| state.restore_album_focus(&grid, album, artist))
        })
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
    use std::time::Duration;

    use super::*;

    fn wait_for_layout(milliseconds: u64) {
        let main_loop = gtk4::glib::MainLoop::new(None, false);
        let quit = main_loop.clone();
        gtk4::glib::timeout_add_local_once(Duration::from_millis(milliseconds), move || {
            quit.quit();
        });
        main_loop.run();
    }

    fn descendants_with_class(root: &gtk4::Widget, class: &str) -> Vec<gtk4::Widget> {
        let mut matches = Vec::new();
        let mut pending = vec![root.clone()];
        while let Some(widget) = pending.pop() {
            if widget.has_css_class(class) {
                matches.push(widget.clone());
            }
            let mut child = widget.first_child();
            while let Some(current) = child {
                pending.push(current.clone());
                child = current.next_sibling();
            }
        }
        matches
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn keyboard_activate_on_grid_opens_album() {
        gtk4::init().unwrap();
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (path,title,artist,album,added_at) VALUES
             ('/one.flac','One','Artist A','First',0);",
        )
        .unwrap();
        let conn = Rc::new(RefCell::new(conn));
        let loader =
            crate::ui::cover_loader::CoverLoader::new(crate::ui::cover_download_worker::setup());
        let view = AlbumView::new(&conn, loader);

        let activated: Rc<RefCell<Option<AlbumSummary>>> = Rc::new(RefCell::new(None));
        {
            let activated = activated.clone();
            view.set_on_activate(move |album| {
                *activated.borrow_mut() = Some(album);
            });
        }

        // `activate` is the signal GridView's built-in Enter binding emits
        // for the focused cell — emitting it directly exercises the same
        // handler the keyboard path runs.
        view.grid_widget().emit_by_name::<()>("activate", &[&0u32]);

        let opened = activated.borrow();
        assert_eq!(opened.as_ref().map(|a| a.album.as_str()), Some("First"));
    }

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

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn grid_5_reveal_scrolls_to_playing_album() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let settings = gtk4::Settings::default().unwrap();
        let animations_before = settings.is_gtk_enable_animations();
        settings.set_gtk_enable_animations(false);

        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (path,title,artist,album,added_at) VALUES
             ('/one.flac','One','Artist A','First',0),
             ('/two.flac','Two','Artist B','Playing',0),
             ('/three.flac','Three','Artist C','Last',0);",
        )
        .unwrap();
        let conn = Rc::new(RefCell::new(conn));
        let loader =
            crate::ui::cover_loader::CoverLoader::new(crate::ui::cover_download_worker::setup());
        let view = AlbumView::new(&conn, loader);
        let window = gtk4::Window::builder()
            .default_width(500)
            .default_height(600)
            .child(view.widget())
            .build();
        window.present();
        wait_for_layout(50);

        view.now_playing_callback()(Some(("Playing".into(), "Artist B".into())));
        view.filter_callback()("no visible album");
        assert_eq!(view.state.filtered_count(), 0);

        assert!(view.reveal_callback()("Playing", "Artist B"));
        wait_for_layout(50);
        assert_eq!(
            view.state.filtered_count(),
            3,
            "reveal clears the album filter"
        );

        let focused = view
            .grid_widget()
            .focus_child()
            .expect("GtkGridView focused the revealed item");
        let title = descendants_with_class(&focused, crate::ui::album_card_css::TITLE_CLASS)
            .into_iter()
            .next()
            .and_downcast::<gtk4::Label>()
            .expect("revealed title");
        assert_eq!(title.text(), "Playing");

        let reveal_frame =
            descendants_with_class(&focused, crate::ui::album_card_css::REVEAL_FRAME_CLASS)
                .into_iter()
                .next()
                .expect("cover-only reveal frame");
        assert!(reveal_frame.has_css_class(crate::ui::album_card_css::REVEAL_PULSE_STATIC_CLASS));
        let playing_layer =
            descendants_with_class(&focused, crate::ui::album_card_css::PLAYING_LAYER_CLASS)
                .into_iter()
                .next()
                .expect("persistent playing layer");
        assert!(playing_layer.is_visible());

        wait_for_layout(crate::ui::album_card_css::REVEAL_DURATION_MS + 50);
        assert!(!reveal_frame.has_css_class(crate::ui::album_card_css::REVEAL_PULSE_STATIC_CLASS));
        assert_eq!(view.grid_widget().focus_child(), Some(focused));
        assert!(playing_layer.is_visible());

        window.close();
        settings.set_gtk_enable_animations(animations_before);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn grid_6_restore_focus_targets_departed_album_without_reveal_pulse() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (path,title,artist,album,added_at) VALUES
             ('/one.flac','One','Artist A','First',0),
             ('/two.flac','Two','Artist B','Return Here',0),
             ('/three.flac','Three','Artist C','Last',0);",
        )
        .unwrap();
        let conn = Rc::new(RefCell::new(conn));
        let loader =
            crate::ui::cover_loader::CoverLoader::new(crate::ui::cover_download_worker::setup());
        let view = AlbumView::new(&conn, loader);
        let window = gtk4::Window::builder()
            .default_width(500)
            .default_height(600)
            .child(view.widget())
            .build();
        window.present();
        wait_for_layout(50);

        assert!(view.restore_focus_callback()("Return Here", "Artist B"));
        wait_for_layout(50);

        let focused = view
            .grid_widget()
            .focus_child()
            .expect("GtkGridView focused the restored album");
        let title = descendants_with_class(&focused, crate::ui::album_card_css::TITLE_CLASS)
            .into_iter()
            .next()
            .and_downcast::<gtk4::Label>()
            .expect("restored title");
        assert_eq!(title.text(), "Return Here");
        let reveal_frame =
            descendants_with_class(&focused, crate::ui::album_card_css::REVEAL_FRAME_CLASS)
                .into_iter()
                .next()
                .expect("cover-only reveal frame");
        assert!(!reveal_frame.has_css_class(crate::ui::album_card_css::REVEAL_PULSE_CLASS));
        assert!(!reveal_frame.has_css_class(crate::ui::album_card_css::REVEAL_PULSE_STATIC_CLASS));

        window.close();
    }
}
