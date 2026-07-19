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
use crate::ui::album_glow::AlbumGlow;
use crate::ui::album_header::{self, AlbumSortKey};
use crate::ui::album_view_actions::{self, AlbumViewActions};
use crate::ui::album_view_memory::{self, SavedAlbumViewState};
use crate::ui::album_view_state::{AlbumViewState, AlbumViewStateParts};
use crate::ui::cover_loader::CoverLoader;
use crate::ui::current_track_selection::NowPlayingAlbum;
use crate::ui::discovery_hint::AlbumDiscovery;
use crate::ui::strings;

pub(in crate::ui) type NowPlayingAlbumCallback = Rc<dyn Fn(Option<NowPlayingAlbum>)>;

pub(in crate::ui) struct AlbumView {
    root: gtk4::Box, // vertical: header + stack(grid | empty | search-empty)
    grid_view: gtk4::GridView,
    selection: gtk4::SingleSelection,
    filter_model: gtk4::FilterListModel,
    saved_view_state: Rc<RefCell<Option<SavedAlbumViewState>>>,
    state: AlbumViewState,
    actions: AlbumViewActions,
    on_activate: AlbumActivateSlot,
    on_artist_activate: ArtistActivateSlot,
    discovery: AlbumDiscovery,
    glow: AlbumGlow,
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
        let saved_view_state = Rc::new(RefCell::new(None));

        // ── Shared state for card factory ──────────────────────────────
        let on_activate: AlbumActivateSlot = Rc::new(RefCell::new(None));
        let on_artist_activate: ArtistActivateSlot = Rc::new(RefCell::new(None));

        let cover_download_enabled = reprise_core::modules::is_enabled(
            &conn.borrow(),
            &reprise_core::modules::COVER_DOWNLOAD_MODULE,
        )
        .unwrap_or(false);
        let discovery = AlbumDiscovery::new(conn, cover_download_enabled);
        let glow = AlbumGlow::new(cover_loader.clone());
        let card_shared = Rc::new(AlbumCardShared {
            cover_loader,
            fallback_evidence: discovery.evidence(),
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
        let selection = gtk4::SingleSelection::builder()
            .model(&filter_model)
            .autoselect(false)
            .can_unselect(true)
            .build();
        let grid_view = gtk4::GridView::builder()
            .model(&selection)
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
            // input-parity: ACC-8 keyboard=native-grid-focus
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

        // Keep the grid page filling the Library viewport even after its
        // native GridView is wrapped by the Glass scroll-range adapter. The
        // surrounding Overlay cannot reliably infer expansion through a
        // hidden Stack page during the Tracks → Albums transition.
        let stack = gtk4::Stack::builder().vexpand(true).build();
        stack.add_named(&scrolled, Some("grid"));
        stack.add_named(&empty, Some("empty"));
        stack.add_named(&search_empty, Some("search-empty"));

        // ── Header ─────────────────────────────────────────────────────
        let sort_model_for_header = sort_model.clone();
        let grid_for_sort = grid_view.clone();
        let selection_for_sort = selection.clone();
        let filter_for_sort = filter_model.clone();
        let (header, count_label, _dropdown) =
            album_header::build_header(conn, move |new_key: AlbumSortKey| {
                let saved = album_view_memory::capture(
                    &grid_for_sort,
                    &selection_for_sort,
                    &filter_for_sort,
                );
                let new_sorter = album_header::build_sorter(new_key);
                sort_model_for_header.set_sorter(Some(&new_sorter));
                album_view_memory::restore(
                    &grid_for_sort,
                    &selection_for_sort,
                    &filter_for_sort,
                    &saved,
                );
            });

        // ── Root layout ────────────────────────────────────────────────
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.add_css_class("album-view-content");
        content.append(&header);
        content.append(discovery.widget());
        content.append(&stack);

        let ambient = gtk4::Overlay::new();
        ambient.set_child(Some(glow.picture()));
        ambient.add_overlay(&content);
        ambient.set_measure_overlay(&content, true);
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.append(&ambient);

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
            selection,
            filter_model,
            saved_view_state,
            state,
            actions,
            on_activate,
            on_artist_activate,
            discovery,
            glow,
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

    pub(in crate::ui) fn set_on_hint_settings(
        &self,
        callback: impl Fn(&'static [&'static str]) + 'static,
    ) {
        self.discovery.set_on_open_plugins(callback);
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
        let identity = self.state.now_playing_callback();
        let glow = self.glow.clone();
        Rc::new(move |album| {
            identity(
                album
                    .as_ref()
                    .map(|album| (album.album.clone(), album.artist.clone())),
            );
            glow.set_track_path(album.as_ref().map(|album| album.track_path.as_str()));
        })
    }

    pub(in crate::ui) fn playback_state_callback(&self) -> Rc<dyn Fn(PlaybackState)> {
        self.state.playback_state_callback()
    }

    pub(in crate::ui) fn remember_view_state_callback(&self) -> Rc<dyn Fn()> {
        let grid = self.grid_view.downgrade();
        let selection = self.selection.clone();
        let model = self.filter_model.clone();
        let memory = self.saved_view_state.clone();
        Rc::new(move || {
            let Some(grid) = grid.upgrade() else { return };
            *memory.borrow_mut() = Some(album_view_memory::capture(&grid, &selection, &model));
        })
    }

    pub(in crate::ui) fn restore_view_state_callback(&self) -> Rc<dyn Fn()> {
        let grid = self.grid_view.downgrade();
        let selection = self.selection.clone();
        let model = self.filter_model.clone();
        let memory = self.saved_view_state.clone();
        Rc::new(move || {
            let Some(saved) = memory.borrow().clone() else {
                return;
            };
            let Some(grid) = grid.upgrade() else { return };
            album_view_memory::restore(&grid, &selection, &model, &saved);
        })
    }

    pub(in crate::ui) fn reveal_playing_context_callback(&self) -> Rc<dyn Fn() -> bool> {
        let grid = self.grid_view.downgrade();
        let model = self.filter_model.clone();
        let now_playing = self.state.now_playing_identity_cell();
        Rc::new(move || {
            let Some((title, artist)) = now_playing.borrow().clone() else {
                return false;
            };
            let Some(grid) = grid.upgrade() else {
                return false;
            };
            album_view_memory::reveal(
                &grid,
                &model,
                &crate::ui::album_view_memory::AlbumIdentity { title, artist },
            )
        })
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
#[path = "album_view_tests.rs"]
mod tests;
