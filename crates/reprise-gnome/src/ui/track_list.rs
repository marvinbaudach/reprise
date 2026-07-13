//! The sortable, searchable track list: a `GtkColumnView` backed by
//! `track_list_model::TrackListModel`, a lazy `gio::ListModel` that fetches
//! `WINDOW_SIZE`-row SQL windows from `queries::query_track_window` on demand
//! — SQL stays the single source of truth for ordering and filtering, GTK
//! never re-sorts the model itself, and the whole result set is never held
//! in memory at once.
//!
//! ## Row data: `glib::BoxedAnyObject`, not a GObject subclass
//!
//! `models::Track` is a plain Rust struct. Returning it from
//! `gio::ListModel::item()` only requires *something* that is
//! `IsA<glib::Object>`; a full `glib::Object` subclass with GObject
//! properties would be needed if the bound widgets had to react to
//! property-level `notify::` signals (e.g. for in-place editing) or if the
//! object needed to cross an FFI/property-binding boundary. Neither applies
//! here — the factory callbacks just read a `Track` once per bind — so
//! `glib::BoxedAnyObject::new(track)` is the simplest correct approach and
//! there is no separate `track_object.rs` module (a bespoke wrapper type
//! would add boilerplate without behavior). `TrackListModel` (see
//! `track_list_model.rs`) is the one place that constructs these boxes.
//!
//! ## Context menu + multi-select (Stage 3 Task 5)
//!
//! The `ColumnView`'s selection model is `gtk::MultiSelection`, not `gtk::
//! NoSelection` (every earlier stage's choice, when nothing needed
//! selection state at all) — the context menu acts on every selected row,
//! not just the one under the pointer. The `Shared::selection` handle built
//! alongside it is what every context-menu action reads its target
//! positions from. The menu itself — the secondary-click `GestureClick`
//! wiring, the `gio::Menu`/`PopoverMenu`, the `"tracklist"` `gio::
//! SimpleAction` group, and the `REPRISE_SMOKE_MENU_ACTION` dev hook — lives
//! in the sibling module `ui::track_list_context_menu` (split out the same
//! way `player_controller.rs` split into `mpris_mirror.rs`/`playback_
//! faults.rs`, Stage 3 Task 1), which reaches back into this module's
//! `Shared`/`reload`/`show_toast` via `pub(super)`. This module still owns
//! `Shared` itself and calls that sibling's `wire_context_menu_gesture` from
//! each column's `connect_setup` and its `wire_context_menu_actions`/`arm_
//! smoke_menu_action` from `TrackList::new`. The pure-ish *logic* the menu's
//! actions invoke — mapping selected positions to track ids, and the
//! playlist/queue mutations themselves — lives in `ui::track_actions`
//! instead, so it's testable without a display; see that module's doc
//! comment for the full position→id/remove-by-position design.
//!
//! ## Sorting: per-column `CustomSorter` as a click signal only
//!
//! `GtkColumnView` headers only become clickable/toggle-sortable once a
//! column has a non-null `sorter`. This module gives every column a
//! `gtk::CustomSorter` whose compare function always returns `Equal` — it
//! never actually reorders `TrackListModel` — purely so GTK renders the sort
//! indicator and emits sort-order changes on click. The real ordering is
//! decided by SQL: clicking a header changes `ColumnView`'s aggregate
//! `ColumnViewSorter` (`primary-sort-column`/`primary-sort-order`), which
//! this module observes, maps back to a whitelisted `queries` sort field via
//! `ColumnViewColumn::id()`, and uses to re-run the model's query via
//! `TrackListModel::set_query`.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use rusqlite::Connection;

use crate::ui::browse_bar::BrowseBar;
use crate::ui::column_layout::{self, ColumnId, ColumnLayout, ColumnRegistry};
use crate::ui::cover_download_worker::CoverDownloadRuntime;
use crate::ui::cover_loader::CoverLoader;
use crate::ui::import_errors_view::ImportErrorsView;
use crate::ui::toasts;
use crate::ui::track_list_activation::{current_queue_ids, wire_activate};
use crate::ui::track_list_columns::{apply_empty_state, build_status_page, empty_state_for};
use crate::ui::track_list_context_menu;
use crate::ui::track_list_dnd_smoke;
use crate::ui::track_list_model::TrackListModel;
use crate::ui::track_list_row_interaction;
use crate::ui::track_list_smoke::{
    arm_smoke_activate, arm_smoke_filter, arm_smoke_sort_column, arm_smoke_source,
};
use crate::ui::track_list_sort::{
    resolve_sort_on_switch, wire_sort_clicks, SortState, PLAYLIST_ORDER_SORT_FIELD,
};
use reprise_core::models::Track;
use reprise_core::queries::BrowseFilter;
use reprise_core::view_source::ViewSource;

pub(super) const STACK_PAGE_EMPTY: &str = "empty";
pub(super) const STACK_PAGE_LIST: &str = "list";
/// Stage 3 Task 8: the ImportErrors source's dedicated path/reason/time panel
/// (`ui::import_errors_view::ImportErrorsView`) — a third `gtk::Stack` page,
/// shown instead of `STACK_PAGE_LIST` only while `ViewSource::ImportErrors`
/// is selected and has rows (see `apply_empty_state`'s `List` arm).
pub(super) const STACK_PAGE_IMPORT_ERRORS: &str = "import_errors";

/// Callback invoked on row activation (double-click/Enter on a row, or the
/// `REPRISE_SMOKE_ACTIVATE` hook). Provided by `window::build`, which routes
/// it to the player — the track list itself stays free of any playback
/// knowledge. Alongside the activated row's `Track` (for logging/fallback,
/// see the `None` player branch in `window::build`), it also carries the
/// full queue this activation should start: `ids` is every track id in the
/// activated row's current sort/filter view (via `queue_ids_for_activation`)
/// and `start_index` is the activated row's position within that list —
/// together, exactly `PlayerController::play_from_view`'s parameters.
pub type OnActivate = Box<dyn Fn(&Track, Vec<i64>, usize)>;

/// Callback invoked at the end of every `reload()` — see the `Shared::
/// on_reload` doc comment for what each parameter carries and why
/// `window.rs` needs all four.
type OnReload = Box<dyn Fn(&ViewSource, usize, &str, &BrowseFilter)>;

/// Context-menu "Play" action callback — see the `Shared::on_play_selected`
/// doc comment.
type OnPlaySelected = Rc<dyn Fn(Vec<i64>, usize)>;
/// Context-menu "Add to queue" action callback — see the `Shared::on_queue_
/// selected` doc comment.
type OnQueueSelected = Rc<dyn Fn(Vec<i64>)>;
/// Queue drag-reorder callback — see the `Shared::on_queue_reorder` doc
/// comment. Returns whether the move actually happened (`false` for a
/// degraded no-op, e.g. no player wired — see `Shared::on_queue_reorder`'s
/// doc comment), which `ui::track_list_dnd`'s drop handler propagates as its
/// own result rather than reporting success just because a callback was
/// present (Stage 3 Task 6 review finding #3).
type OnQueueReorder = Rc<dyn Fn(usize, usize) -> bool>;
/// Sidebar drag-and-drop "add to playlist" callback — see the `Shared::on_
/// sidebar_playlist_drop` doc comment.
type OnSidebarPlaylistDrop = Rc<dyn Fn(i64, &str, &[i64]) -> bool>;
/// "Remove from library" callback — see the `Shared::on_library_mutated` doc
/// comment. Takes the ids actually deleted (Stage-3 close-out).
type OnLibraryMutated = Rc<dyn Fn(&[i64])>;
/// Successful tag-edit callback. Paths let the player invalidate only the
/// currently displayed cover while the window refreshes sidebar metadata.
type OnTagsMutated = Rc<dyn Fn(&[PathBuf])>;

/// `pub(super)` (visible to `crate::ui` and its descendants, e.g. `ui::
/// track_list_context_menu` — see that module's doc comment) rather than
/// fully private: Stage 3 Task 5 splits the context-menu logic out into a
/// sibling module exactly the way `player_controller.rs` split its MPRIS
/// mirror and fault-tolerance logic into `mpris_mirror.rs`/`playback_
/// faults.rs` (Stage 3 Task 1) — same reasoning, same visibility shape. Only
/// the fields that module actually needs are marked `pub(super)`
/// individually below; everything else stays private to this file.
pub(super) struct Shared {
    pub(super) model: TrackListModel,
    /// The `ColumnView`'s selection model (Stage 3 Task 5) — every context-
    /// menu action reads its target row positions from here (`selection()`/
    /// `is_selected()`/`select_range()`), and `wire_context_menu_gesture`'s
    /// GNOME-convention reselect-if-not-selected step writes to it. Kept as
    /// its own field (not re-derived by downcasting `column_view.model()`
    /// on every use) since `TrackList::new` already builds the concrete
    /// `gtk::MultiSelection` directly.
    pub(super) selection: gtk4::MultiSelection,
    /// The `ColumnView` widget itself (Stage 3 Task 9): kept so `TrackList::
    /// focus_track_list` can move keyboard focus onto it directly, rather
    /// than relying on `widget()`'s outer `gtk::Stack` to delegate focus to
    /// the right descendant on its own — see that method's doc comment for
    /// why the Escape shortcut (`ui::shortcuts`) needs a precise handle
    /// rather than "whatever's focusable in the current stack page."
    pub(super) column_view: gtk4::ColumnView,
    /// The same UI-owned connection `TrackList::new` was given, kept here
    /// too (alongside the clone `TrackListModel` holds internally) so the
    /// rating column's click handler can write through `library::stats`
    /// without reaching into the model's private state.
    pub(super) conn: Rc<RefCell<Connection>>,
    /// Shared list-cell cover cache, retained so successful tag writes can
    /// invalidate entries keyed by the same path before rows are rebound.
    pub(super) cover_loader: Rc<CoverLoader>,
    pub(super) browse_bar: Rc<BrowseBar>,
    pub(super) browse_filter: RefCell<BrowseFilter>,
    pub(super) stack: gtk4::Stack,
    /// The single empty-state placeholder widget. Its title/description/icon
    /// are mutated in place by `apply_empty_state` rather than swapping in a
    /// third stack page — see that function's doc comment.
    pub(super) empty_page: adw::StatusPage,
    pub(super) sort: RefCell<SortState>,
    pub(super) restoring_view: Cell<bool>,
    pub(super) filter: RefCell<String>,
    /// Which of the six sources (Stage 3 Task 3) the list is currently
    /// showing — defaults to `ViewSource::Library`. Set via `TrackList::
    /// set_source` (and the `REPRISE_SMOKE_SOURCE` hook); read by `reload`
    /// and `queue_ids_for_activation`.
    pub(super) source: RefCell<ViewSource>,
    /// Supplies the current queue's track ids, in play order, when `source`
    /// is `ViewSource::Queue` — see `queries::query_track_window`'s doc
    /// comment for why that source needs an explicit id list. Wired once at
    /// construction (`TrackList::new`'s `queue_ids_provider` parameter) to a
    /// closure over the `PlayerController`, which already exists by the
    /// time `TrackList::new` runs (see `window::build`) — unlike `toast_
    /// overlay`/`on_activate`, no post-construction injection dance is
    /// needed here. A `Box<dyn Fn() -> Vec<i64>>`, not a `WeakRef`-style
    /// seam: the closure itself only holds whatever `window::build` gives
    /// it (typically a clone of `Option<Rc<PlayerController>>`), so there's
    /// no ownership cycle back to `TrackList` to worry about.
    pub(super) queue_ids_provider: Box<dyn Fn() -> Vec<i64>>,
    /// Shared by `wire_activate` (user activation) and the smoke-activate
    /// hook so both take the identical code path.
    pub(super) on_activate: OnActivate,
    /// Invoked at the end of every `reload()` — initial load, search-filter
    /// changes, sort-header clicks, source switches, and the explicit
    /// `TrackList::reload()` call `window.rs` makes after a scan completes —
    /// with the source just queried, the row count it produced, and the
    /// filter string that reload just ran against. `window.rs` uses this
    /// single hook to keep `status_bar::StatusBar` in sync: library-wide
    /// totals (via the filter string) when `source` is `Library`, a simple
    /// "{n} tracks" line (via the row count) otherwise — see `status_bar::
    /// StatusBar::refresh_for_source_count`. This is the seam chosen over
    /// exposing `TrackList::source()`/`filter()` getters: `reload` already
    /// has all three values in local variables at the one call site that
    /// invokes this hook.
    on_reload: OnReload,
    /// Stage 3 Task 1 (a): the window's toast overlay, injected post-
    /// construction via `TrackList::set_toast_overlay` — same seam shape as
    /// `PlayerController::toast_overlay` (see that module's `## Toast +
    /// track-list-reload seam` doc section): built in `window::build` after
    /// `TrackList::new`, so it can't be a constructor parameter. `WeakRef`,
    /// not a strong reference, so `TrackList` can never keep the window
    /// alive past its natural lifetime.
    toast_overlay: glib::WeakRef<adw::ToastOverlay>,
    /// The main window, injected post-construction via `TrackList::set_
    /// window` — same seam shape as `toast_overlay` above. Needed as the
    /// parent for the context menu's "New playlist…" `AdwAlertDialog`
    /// (`show_new_playlist_dialog` below — mirrors `ui::sidebar`'s own
    /// dialog of the same shape). `WeakRef`, not a strong reference, for the
    /// same reason as `toast_overlay`.
    pub(super) window: glib::WeakRef<adw::ApplicationWindow>,
    /// Context-menu "Play" action callback (Stage 3 Task 5), injected via
    /// `TrackList::set_on_play_selected` — wraps `PlayerController::
    /// play_from_view` without this module depending on that type directly
    /// (same decoupling-via-closure seam as `on_activate`/`queue_ids_
    /// provider`). `RefCell<Option<Rc<dyn Fn>>>`, not a plain field set at
    /// construction, since the player controller is built by `window.rs`
    /// independently of `TrackList` and wired in afterwards.
    pub(super) on_play_selected: RefCell<Option<OnPlaySelected>>,
    /// Context-menu "Add to queue" action callback, injected via
    /// `TrackList::set_on_queue_selected` — wraps `PlayerController::
    /// append_to_queue`. Same seam shape as `on_play_selected`.
    pub(super) on_queue_selected: RefCell<Option<OnQueueSelected>>,
    /// Invoked after any context-menu action that mutates a playlist's
    /// membership (add to an existing playlist, add to a brand new one, or
    /// remove) — injected via `TrackList::set_on_playlist_mutated`, wired by
    /// `window.rs` to `Sidebar::refresh` (a new trigger alongside the three
    /// already listed in that method's doc comment: scan completion,
    /// playlist CRUD from the sidebar itself, and missing-marking). Sidebar
    /// track counts must stay in sync with playlist changes made from this
    /// menu, exactly as they already do for changes made from the sidebar's
    /// own "New playlist" dialog.
    pub(super) on_playlist_mutated: RefCell<Option<Rc<dyn Fn()>>>,
    /// Queue drag-reorder callback (Stage 3 Task 6), injected via
    /// `TrackList::set_on_queue_reorder` — wraps `PlayerController::
    /// move_queue_item`. Same seam shape as `on_play_selected`/`on_queue_
    /// selected`: `window.rs` wires this once the controller exists, and
    /// `ui::track_list_dnd`'s queue-reorder drop handler is the only caller
    /// (see that module's doc comment for the full drag/drop design). Returns
    /// whether the move actually happened — `window.rs`'s wiring returns
    /// `false` when no player is available at all, exactly like `Queue::
    /// move_item`'s own no-op cases (Stage 3 Task 6 review finding #3), so a
    /// degraded environment reports failure rather than a false "moved".
    pub(super) on_queue_reorder: RefCell<Option<OnQueueReorder>>,
    /// Sidebar drag-and-drop "add to playlist" callback (Stage 3 Task 6
    /// review finding #1), injected via `TrackList::set_on_sidebar_playlist_
    /// drop` — wraps `Sidebar::handle_playlist_drop`, the same function the
    /// sidebar's own `gtk::DropTarget` calls for a real pointer drag. This
    /// seam exists purely so `ui::track_list_dnd`'s `REPRISE_SMOKE_DND=
    /// addplaylist:<name>` hook can drive the *exact* sidebar drop-handling
    /// sequence (DB write, sidebar rebuild + toast, `on_tracks_added` ->
    /// this track list's own reload) instead of calling `library::
    /// playlists::add_tracks` directly and only proving the database write —
    /// `ui::track_list_dnd` has no other way to reach `Sidebar`'s private
    /// state, exactly the same "no direct handle across widgets" reason
    /// `on_playlist_mutated`/`on_queue_reorder` are callbacks rather than
    /// direct calls. Takes `(playlist_id, playlist_name, ids)` and returns
    /// whether anything was actually added.
    pub(super) on_sidebar_playlist_drop: RefCell<Option<OnSidebarPlaylistDrop>>,
    /// Stage 3 Task 8: the ImportErrors source's dedicated panel — see
    /// `STACK_PAGE_IMPORT_ERRORS`'s doc comment. Built once, alongside every
    /// other widget, and refreshed (not rebuilt) on every `reload()` while
    /// this source is selected.
    pub(super) import_errors_view: ImportErrorsView,
    /// "Rescan library" (Missing-source context menu item, Stage 3 Task 8):
    /// injected via `TrackList::set_on_rescan_library` — wraps `ui::window`'s
    /// scan flow against the persisted library root without this module
    /// depending on the scan machinery/settings table directly (same
    /// decoupling-via-closure seam as `on_play_selected`/`on_queue_
    /// selected`).
    pub(super) on_rescan_library: RefCell<Option<Rc<dyn Fn()>>>,
    /// "Remove from library" (Missing-source context menu item, Stage 3 Task
    /// 8): injected via `TrackList::set_on_library_mutated` — `window.rs`
    /// wires this to `Sidebar::refresh` (the Missing badge count can only
    /// ever shrink from this action) AND `PlayerController::purge_queue_ids`
    /// (Stage-3 close-out: a hard-deleted track must also be purged from the
    /// playback queue). Takes the ids `queries::remove_missing_tracks`
    /// actually deleted — not just a bare notification — so the queue purge
    /// knows exactly which ids to remove.
    pub(super) on_library_mutated: RefCell<Option<OnLibraryMutated>>,
    /// Invoked after successful file-tag writes and DB reconciliation.
    /// Kept separate from `on_library_mutated`: editing tags must never purge
    /// otherwise valid tracks from the playback queue.
    pub(super) on_tags_mutated: RefCell<Option<OnTagsMutated>>,
    /// Invoked after the ImportErrors panel's own Retry/Dismiss actions
    /// mutate `import_errors` — injected via `TrackList::set_on_import_
    /// errors_mutated`, wired by `window.rs` to `Sidebar::refresh` (the
    /// Import-errors badge count just changed).
    pub(super) on_import_errors_mutated: RefCell<Option<Rc<dyn Fn()>>>,
}

/// Handle to the built track list widget. Owns the shared, reference-counted
/// state that the sort-header and search-debounce callbacks close over.
pub struct TrackList {
    pub(super) shared: Rc<Shared>,
    root: gtk4::Box,
    pub(super) column_registry: ColumnRegistry,
}

impl TrackList {
    /// Builds the track list and performs the initial load (unfiltered,
    /// default sort, `ViewSource::Library`). `conn` is the shared UI-owned
    /// database connection; `on_activate` receives the `Track` of every
    /// activated row; `on_reload` is called after every reload (see the
    /// `Shared::on_reload` doc comment); `queue_ids_provider` supplies the
    /// current queue's ids whenever `source` is switched to `ViewSource::
    /// Queue` (see the `Shared::queue_ids_provider` doc comment).
    pub fn new(
        conn: Rc<RefCell<Connection>>,
        on_activate: OnActivate,
        on_reload: impl Fn(&ViewSource, usize, &str, &BrowseFilter) + 'static,
        queue_ids_provider: impl Fn() -> Vec<i64> + 'static,
        cover_download: CoverDownloadRuntime,
    ) -> Self {
        let model = TrackListModel::new(conn.clone());
        // `gtk::MultiSelection`, not `gtk::NoSelection` (Stage 3 Task 5):
        // the context menu acts on every selected row, not just the one
        // under the pointer — see the module doc's `## Context menu +
        // multi-select` section.
        let selection = gtk4::MultiSelection::new(Some(model.clone()));

        let column_view = gtk4::ColumnView::builder()
            .model(&selection)
            .show_row_separators(true)
            .show_column_separators(true)
            .build();
        track_list_row_interaction::install_reorder_indicator_style(&column_view);

        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&column_view)
            .vexpand(true)
            .hexpand(true)
            .build();

        let empty_page = build_status_page();

        // Stage 3 Task 8: built before the stack so its widget can be added
        // as a third page alongside empty/list — see `STACK_PAGE_IMPORT_
        // ERRORS`'s doc comment.
        let import_errors_view = ImportErrorsView::new(conn.clone());

        let stack = gtk4::Stack::new();
        stack.add_named(&empty_page, Some(STACK_PAGE_EMPTY));
        stack.add_named(&scrolled, Some(STACK_PAGE_LIST));
        stack.add_named(import_errors_view.widget(), Some(STACK_PAGE_IMPORT_ERRORS));
        stack.set_visible_child_name(STACK_PAGE_EMPTY);

        let browse_bar = BrowseBar::new(conn.clone());
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.append(browse_bar.widget());
        root.append(&stack);

        // Built here, before any column is appended — unlike every stage
        // before Task 5, which built columns first: each column's `connect_
        // setup` now also wires its cell's context-menu gesture, which needs
        // `&shared` (see `wire_context_menu_gesture`). Nothing else in
        // `Shared` depends on the columns existing first, so this reorder
        // has no other consequence.
        let cover_loader = CoverLoader::new(cover_download);
        let shared = Rc::new(Shared {
            model,
            selection: selection.clone(),
            column_view: column_view.clone(),
            conn,
            cover_loader: cover_loader.clone(),
            browse_bar: browse_bar.clone(),
            browse_filter: RefCell::new(BrowseFilter::default()),
            stack,
            empty_page,
            sort: RefCell::new(SortState::default()),
            restoring_view: Cell::new(false),
            filter: RefCell::new(String::new()),
            source: RefCell::new(ViewSource::default()),
            queue_ids_provider: Box::new(queue_ids_provider),
            on_activate,
            on_reload: Box::new(on_reload),
            toast_overlay: glib::WeakRef::new(),
            window: glib::WeakRef::new(),
            on_play_selected: RefCell::new(None),
            on_queue_selected: RefCell::new(None),
            on_playlist_mutated: RefCell::new(None),
            on_queue_reorder: RefCell::new(None),
            on_sidebar_playlist_drop: RefCell::new(None),
            import_errors_view,
            on_rescan_library: RefCell::new(None),
            on_library_mutated: RefCell::new(None),
            on_tags_mutated: RefCell::new(None),
            on_import_errors_mutated: RefCell::new(None),
        });

        {
            let shared_weak = Rc::downgrade(&shared);
            browse_bar.set_on_changed(move |filter| {
                let Some(shared) = shared_weak.upgrade() else {
                    return;
                };
                *shared.browse_filter.borrow_mut() = filter;
                reload(&shared);
            });
        }

        // Stage 3 Task 8: the panel's own Retry/Dismiss actions must both
        // refresh this `TrackList`'s stack-page/count decision (via `reload`,
        // which also re-refreshes the panel — cheap, and consistent with
        // this module's general tolerance for a redundant-but-harmless extra
        // refresh) and let `window.rs` know the Import-errors badge count may
        // have changed. `Weak`: this callback lives as long as the panel
        // widget itself (owned by `shared`), so an `Rc` here would be a
        // self-referential cycle keeping `shared` alive forever.
        {
            let shared_weak = Rc::downgrade(&shared);
            shared
                .import_errors_view
                .set_on_mutated(move || match shared_weak.upgrade() {
                    Some(shared) => notify_import_errors_mutated_and_reload(&shared),
                    None => tracing::warn!(
                        "import errors panel: mutated callback fired after track list was dropped"
                    ),
                });
        }

        let built_columns = column_layout::build_columns(&column_view, &shared, &cover_loader);
        let title_column = built_columns.title;
        let artist_column = built_columns.artist;
        let column_registry = built_columns.registry;
        let initial_sort_column = if column_registry.is_visible(ColumnId::Artist) {
            artist_column.clone()
        } else {
            *shared.sort.borrow_mut() = SortState {
                field: "title".into(),
                dir: "asc".into(),
            };
            title_column.clone()
        };

        wire_sort_clicks(&column_view, &shared);

        // Sets the initial sort indicator (artist ascending, or title when
        // an imported layout hides artist). `shared.sort` already matches
        // that choice, so the
        // `primary-sort-column`/`primary-sort-order` notify signals this
        // triggers land in `on_sorter_changed`, compute the same
        // (field, dir) pair already stored in `shared.sort`, and the dedup
        // guard there (`if *shared.sort.borrow() == new_sort { return; }`)
        // short-circuits before it would call `reload` — so this call fires
        // zero SQL queries. The one and only initial load below still runs
        // exactly once.
        column_view.sort_by_column(Some(&initial_sort_column), gtk4::SortType::Ascending);

        wire_activate(&column_view, &shared);
        track_list_context_menu::wire_context_menu_actions(&column_view, &shared);

        reload(&shared);
        arm_smoke_activate(&shared);
        arm_smoke_filter(&shared);
        arm_smoke_source(&shared);
        arm_smoke_sort_column(&column_view, &title_column, &artist_column);
        track_list_context_menu::arm_smoke_menu_action(&shared);
        crate::ui::tag_edit_flow::arm_smoke(&shared);
        crate::ui::delete_tracks::arm_smoke(&shared);
        crate::ui::browse_bar::arm_smoke(&shared);
        track_list_dnd_smoke::arm_smoke_dnd(&shared);

        Self {
            shared,
            root,
            column_registry,
        }
    }

    /// The root widget: Library browse bar above the Stack that switches
    /// between the empty placeholder and the populated list.
    pub fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    /// Moves keyboard focus onto the track list's `ColumnView` (Stage 3 Task
    /// 9): the second stage of the Escape shortcut (`ui::shortcuts`) hands
    /// focus back here once the search entry's text is already clear.
    /// Returns whether GTK actually granted focus (`gtk::Widget::grab_
    /// focus`'s own return value, e.g. `false` if the column view isn't
    /// currently mapped/visible) — the caller logs a `false` rather than
    /// treating it as fatal, matching every other best-effort focus move in
    /// this codebase.
    pub fn focus_track_list(&self) -> bool {
        self.shared.column_view.grab_focus()
    }

    /// Sets the live-search filter and reloads. Called from `window.rs`
    /// after its own debounce timer fires.
    pub fn set_filter(&self, text: &str) {
        set_filter_and_reload(&self.shared, text);
    }

    /// Switches which `ViewSource` the list is showing and reloads (Stage 3
    /// Task 3). Called by `ui::sidebar`'s row-selection callback (Task 4);
    /// the `REPRISE_SMOKE_SOURCE` hook (`arm_smoke_source`) still calls the
    /// private `set_source_and_reload` directly instead (no live `TrackList`
    /// handle to call a public method on at that point).
    pub fn set_source(&self, source: ViewSource) {
        set_source_and_reload(&self.shared, source);
    }

    /// Re-runs the current sort/filter query and refreshes the list without
    /// changing either — used by `window.rs` after a scan completes, so
    /// newly added tracks show up without disturbing an active search.
    pub fn reload(&self) {
        self.shared.browse_bar.refresh();
        reload(&self.shared);
    }

    pub(super) fn apply_column_layout(&self, layout: &ColumnLayout) -> Result<(), rusqlite::Error> {
        let serialized = column_layout::serialize_layout(layout);
        reprise_core::library::settings::set_setting(
            &self.shared.conn.borrow(),
            reprise_core::library::settings::COLUMN_LAYOUT_KEY,
            &serialized,
        )?;
        self.column_registry.apply(layout);
        let sort = self.shared.sort.borrow().clone();
        let current_id = ColumnId::from_sort_field(&sort.field);
        let (column, order) = if current_id.is_some_and(|id| self.column_registry.is_visible(id)) {
            let column = current_id.and_then(|id| self.column_registry.column(id));
            let order = if sort.dir == "desc" {
                gtk4::SortType::Descending
            } else {
                gtk4::SortType::Ascending
            };
            (column, order)
        } else {
            (
                self.column_registry.column(ColumnId::Title),
                gtk4::SortType::Ascending,
            )
        };
        if let Some(column) = column {
            self.shared.column_view.sort_by_column(Some(column), order);
        }
        Ok(())
    }

    pub(super) fn current_column_layout(&self) -> ColumnLayout {
        column_layout::load_layout(&self.shared.conn.borrow())
    }

    pub(super) fn toast(&self, message: &str) {
        show_toast(&self.shared, message);
    }

    /// Injects the window's toast overlay, once it exists — see the
    /// `Shared::toast_overlay` doc comment for why this can't be a
    /// constructor parameter. Stored as a `WeakRef`; `show_toast` degrades
    /// to log-only if the upgrade ever fails.
    pub fn set_toast_overlay(&self, overlay: &adw::ToastOverlay) {
        self.shared.toast_overlay.set(Some(overlay));
        // Stage 3 Task 8: the ImportErrors panel has its own toast overlay
        // seam (for a failed Retry) — forwarded here rather than injected
        // separately, since `window.rs` already calls this one method at the
        // right point in construction.
        self.shared.import_errors_view.set_toast_overlay(overlay);
    }

    /// Injects the main window, once it exists — see the `Shared::window`
    /// doc comment for why this can't be a constructor parameter and what
    /// it's used for (the context menu's "New playlist…" dialog parent).
    pub fn set_window(&self, window: &adw::ApplicationWindow) {
        self.shared.window.set(Some(window));
    }

    /// Injects the context menu's "Play" action callback (Stage 3 Task 5) —
    /// see the `Shared::on_play_selected` doc comment. `window.rs` wires
    /// this to `PlayerController::play_from_view` once the controller
    /// exists.
    pub fn set_on_play_selected(&self, callback: impl Fn(Vec<i64>, usize) + 'static) {
        *self.shared.on_play_selected.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the context menu's "Add to queue" action callback — see the
    /// `Shared::on_queue_selected` doc comment. `window.rs` wires this to
    /// `PlayerController::append_to_queue`.
    pub fn set_on_queue_selected(&self, callback: impl Fn(Vec<i64>) + 'static) {
        *self.shared.on_queue_selected.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the callback invoked after any context-menu action that
    /// mutates a playlist's membership — see the `Shared::on_playlist_
    /// mutated` doc comment. `window.rs` wires this to `Sidebar::refresh`.
    pub fn set_on_playlist_mutated(&self, callback: impl Fn() + 'static) {
        *self.shared.on_playlist_mutated.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the queue drag-reorder callback (Stage 3 Task 6) — see the
    /// `Shared::on_queue_reorder` doc comment. `window.rs` wires this to
    /// `PlayerController::move_queue_item`.
    pub fn set_on_queue_reorder(&self, callback: impl Fn(usize, usize) -> bool + 'static) {
        *self.shared.on_queue_reorder.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the sidebar "add to playlist" drag-and-drop callback (Stage 3
    /// Task 6 review finding #1) — see the `Shared::on_sidebar_playlist_drop`
    /// doc comment. `window.rs` wires this to `Sidebar::handle_playlist_drop`.
    pub fn set_on_sidebar_playlist_drop(
        &self,
        callback: impl Fn(i64, &str, &[i64]) -> bool + 'static,
    ) {
        *self.shared.on_sidebar_playlist_drop.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the "Rescan library" context-menu action callback (Missing
    /// source, Stage 3 Task 8) — see the `Shared::on_rescan_library` doc
    /// comment. `window.rs` wires this to `trigger_rescan_of_library_root`.
    pub fn set_on_rescan_library(&self, callback: impl Fn() + 'static) {
        *self.shared.on_rescan_library.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the callback invoked after "Remove from library" deletes rows
    /// (Missing source, Stage 3 Task 8) — see the `Shared::on_library_
    /// mutated` doc comment. `window.rs` wires this to `Sidebar::refresh`.
    pub fn set_on_library_mutated(&self, callback: impl Fn(&[i64]) + 'static) {
        *self.shared.on_library_mutated.borrow_mut() = Some(Rc::new(callback));
    }

    pub fn set_on_tags_mutated(&self, callback: impl Fn(&[PathBuf]) + 'static) {
        *self.shared.on_tags_mutated.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the callback invoked after the ImportErrors panel's own
    /// Retry/Dismiss actions mutate `import_errors` (Stage 3 Task 8) — see
    /// the `Shared::on_import_errors_mutated` doc comment. `window.rs` wires
    /// this to `Sidebar::refresh`.
    pub fn set_on_import_errors_mutated(&self, callback: impl Fn() + 'static) {
        *self.shared.on_import_errors_mutated.borrow_mut() = Some(Rc::new(callback));
    }
}

/// Clone-out-then-call `on_import_errors_mutated` (hoisted per this
/// project's `RefCell` callback discipline), then `reload` — the panel's own
/// `refresh()` already updated its rows before this callback fired (see
/// `import_errors_view.rs`'s `notify_mutated_and_refresh`), but only `reload`
/// re-derives this `TrackList`'s stack-page decision (e.g. switching to the
/// "nothing here" empty page once the last error is dismissed).
fn notify_import_errors_mutated_and_reload(shared: &Rc<Shared>) {
    reload(shared);
    let callback = shared.on_import_errors_mutated.borrow().clone();
    match callback {
        Some(callback) => callback(),
        None => tracing::warn!(
            "import errors panel: mutated but no on_import_errors_mutated callback is wired"
        ),
    }
}

/// Whether the track list's current state allows a drag-reorder *within* a
/// playlist view (Stage 3 Task 6) — the true-position rule's guard, mirroring
/// `ui::track_actions`'s "Remove from playlist" reasoning one step further:
/// removal can always resolve the true `pt.position` of whatever row is
/// selected (via `Track::playlist_position`), no matter the sort/filter, so it
/// stays correct under any view state. A *reorder* drag has no such
/// escape hatch — dropping a row "between rows 2 and 3" is only a meaningful,
/// unambiguous instruction when the on-screen row order already *is*
/// `pt.position` order (the playlist's own default, the `"playlist_order"`
/// sentinel) with no search filter thinning out which rows are even visible.
/// Under a column-header sort or a live filter, "between the two visible
/// rows" doesn't correspond to any single well-defined target position in the
/// full unsorted/unfiltered list, so this returns `false` and `ui::track_
/// list_dnd`'s reorder-drop handler must treat the drag as a no-op rather
/// than guess. `false` for every non-Playlist source too (Library/Smart/
/// Missing/ImportErrors have no `pt.position` to reorder in the first place;
/// Queue has its own reorder path, gated separately — see that module's doc
/// comment for why Queue never needs this guard at all).
pub(super) fn playlist_reorder_allowed(shared: &Shared) -> bool {
    matches!(*shared.source.borrow(), ViewSource::Playlist(_))
        && shared.sort.borrow().field == PLAYLIST_ORDER_SORT_FIELD
        && shared.filter.borrow().trim().is_empty()
}

/// Shows `text` as an `adw::Toast`, degrading to a warn log if no overlay is
/// wired or it's gone — mirrors `player_controller.rs`'s `show_toast` (same
/// seam, same degrade behavior), not shared code: the two owning types are
/// otherwise unrelated and this is a two-line `WeakRef::upgrade` match.
pub(super) fn show_toast(shared: &Shared, text: &str) {
    match shared.toast_overlay.upgrade() {
        Some(overlay) => toasts::show(&overlay, text),
        None => {
            tracing::warn!(text, "toast overlay is gone; degrading to log-only");
        }
    }
}

/// Sets `shared.filter` and reloads — the one place that mutates the filter
/// before reloading, shared by `TrackList::set_filter` (the typed-search
/// path, reached via `window.rs`'s debounce timer) and the
/// `REPRISE_SMOKE_FILTER` dev hook (`arm_smoke_filter`), so both apply a new
/// filter through the identical code path.
pub(super) fn set_filter_and_reload(shared: &Rc<Shared>, text: &str) {
    *shared.filter.borrow_mut() = text.to_string();
    reload(shared);
}

/// Sets `shared.source` and reloads — the one place that mutates the source
/// before reloading, shared by `TrackList::set_source` and the `REPRISE_
/// SMOKE_SOURCE` dev hook (`arm_smoke_source`), so both switch sources
/// through the identical code path.
///
/// Also resolves `shared.sort` via `resolve_sort_on_switch` (CRITICAL fix,
/// review round 1; see that function for the full matrix): without this,
/// switching to a `Playlist` source reloaded with whatever sort was
/// already active (`SortState::default()`'s artist/asc on first switch),
/// never the playlist's own `pt.position` order — the `"playlist_order"`
/// sentinel existed in `queries.rs`'s whitelist but was only ever
/// exercised by that module's own unit tests, never by the live UI path.
/// A column-header click (`on_sorter_changed`) still overrides this
/// temporarily, exactly as before.
pub(super) fn set_source_and_reload(shared: &Rc<Shared>, source: ViewSource) {
    // Hoisted so the `sort` borrow ends before the `borrow_mut` below.
    let new_sort = resolve_sort_on_switch(&shared.sort.borrow(), &source);
    *shared.sort.borrow_mut() = new_sort;
    *shared.source.borrow_mut() = source;
    shared
        .browse_bar
        .set_library_visible(matches!(*shared.source.borrow(), ViewSource::Library));
    reload(shared);
}

/// Re-runs the query against the current source/sort/filter state via
/// `TrackListModel::set_query`. Switches the stack to whichever page
/// `empty_state_for` selects for the resulting row count, filter state, and
/// source.
pub(super) fn reload(shared: &Rc<Shared>) {
    let sort = shared.sort.borrow().clone();
    let filter = shared.filter.borrow().clone();
    let source = shared.source.borrow().clone();
    let browse = if matches!(source, ViewSource::Library) {
        shared.browse_filter.borrow().clone()
    } else {
        BrowseFilter::default()
    };
    let has_filter = !filter.trim().is_empty() || !browse.is_empty();

    let queue_ids = if matches!(source, ViewSource::Queue) {
        current_queue_ids(shared)
    } else {
        Vec::new()
    };

    shared.model.set_query_browsed(
        &source,
        &sort.field,
        &sort.dir,
        &filter,
        &browse,
        &queue_ids,
    );

    // Stage 3 Task 8: the ImportErrors source's rows live in `import_errors_
    // view`, not `shared.model` (which `queries.rs` always resolves to an
    // empty window/count for this source — see its module doc's `ImportErrors`
    // section) — so its row count comes from refreshing that panel instead.
    let count = if matches!(source, ViewSource::ImportErrors) {
        shared.import_errors_view.refresh()
    } else {
        shared.model.n_items() as usize
    };
    apply_empty_state(shared, empty_state_for(count, has_filter, &source));

    tracing::info!(
        count,
        field = %sort.field,
        dir = %sort.dir,
        filter = %filter,
        ?browse,
        source = %source.label(),
        "query matched {count} tracks"
    );

    (shared.on_reload)(&source, count, &filter, &browse);
}
