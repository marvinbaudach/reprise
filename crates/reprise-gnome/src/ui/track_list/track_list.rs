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
//! `Shared`/`reload`/`show_toast` via `pub(in crate::ui)`. This module still owns
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
use crate::ui::column_layout::ColumnRegistry;
use crate::ui::cover_download_worker::CoverDownloadRuntime;
use crate::ui::cover_loader::CoverLoader;
use crate::ui::import_errors_view::ImportErrorsView;
use crate::ui::toasts;
use crate::ui::track_list_model::TrackListModel;
pub(in crate::ui) use crate::ui::track_list_reload::{
    reload, set_filter_and_reload, set_source_and_reload,
};
use crate::ui::track_list_sort::{SortState, PLAYLIST_ORDER_SORT_FIELD};
use reprise_core::models::Track;
use reprise_core::queries::BrowseFilter;
use reprise_core::view_source::ViewSource;

pub(in crate::ui) const STACK_PAGE_EMPTY: &str = "empty";
pub(in crate::ui) const STACK_PAGE_LIST: &str = "list";
/// Stage 3 Task 8: the ImportErrors source's dedicated path/reason/time panel
/// (`ui::import_errors_view::ImportErrorsView`) — a third `gtk::Stack` page,
/// shown instead of `STACK_PAGE_LIST` only while `ViewSource::ImportErrors`
/// is selected and has rows (see `apply_empty_state`'s `List` arm).
pub(in crate::ui) const STACK_PAGE_IMPORT_ERRORS: &str = "import_errors";

/// Callback invoked on row activation (double-click/Enter on a row, or the
/// `REPRISE_SMOKE_ACTIVATE` hook). Provided by `window::build`, which routes
/// it to the player — the track list itself stays free of any playback
/// knowledge. Alongside the activated row's `Track` (for logging/fallback,
/// see the `None` player branch in `window::build`), it also carries the
/// full queue this activation should start: `ids` is every track id in the
/// activated row's current sort/filter view (via `queue_ids_for_activation`)
/// and `start_index` is the activated row's position within that list —
/// together, exactly `PlayerController::play_from_view`'s parameters.
pub type OnActivate = Box<dyn Fn(&Track, Vec<i64>, usize, ViewSource)>;

/// Callback invoked at the end of every `reload()` — see the `Shared::
/// on_reload` doc comment for what each parameter carries and why
/// `window.rs` needs all four.
type OnReload = Box<dyn Fn(&ViewSource, usize, &str, &BrowseFilter)>;

/// Context-menu "Play" action callback — see the `Shared::on_play_selected`
/// doc comment.
type OnPlaySelected = Rc<dyn Fn(Vec<i64>, usize, ViewSource)>;
/// Context-menu "Add to queue" action callback — see the `Shared::on_queue_
/// selected` doc comment.
type OnQueueSelected = Rc<dyn Fn(Vec<i64>)>;
type OnQueueActivate = Rc<dyn Fn(usize)>;
type OnQueueRemove = Rc<dyn Fn(&[usize]) -> usize>;
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
/// Sidebar drag-and-drop "add to queue" callback — see the `Shared::on_
/// sidebar_queue_drop` doc comment.
type OnSidebarQueueDrop = Rc<dyn Fn(&[i64]) -> bool>;
/// "Remove from library" callback — see the `Shared::on_library_mutated` doc
/// comment. Takes the ids actually deleted (Stage-3 close-out).
type OnLibraryMutated = Rc<dyn Fn(&[i64])>;
/// Successful tag-edit callback. Paths let the player invalidate only the
/// currently displayed cover while the window refreshes sidebar metadata.
type OnTagsMutated = Rc<dyn Fn(&[PathBuf])>;
type OnSelectionChanged = Rc<dyn Fn(crate::ui::info_panel_state::PanelContext)>;

/// `pub(in crate::ui)` (visible to `crate::ui` and its descendants, e.g. `ui::
/// track_list_context_menu` — see that module's doc comment) rather than
/// fully private: Stage 3 Task 5 splits the context-menu logic out into a
/// sibling module exactly the way `player_controller.rs` split its MPRIS
/// mirror and fault-tolerance logic into `mpris_mirror.rs`/`playback_
/// faults.rs` (Stage 3 Task 1) — same reasoning, same visibility shape. Only
/// the fields that module actually needs are marked `pub(in crate::ui)`
/// individually below; everything else stays private to this file.
pub(in crate::ui) struct Shared {
    pub(in crate::ui) model: TrackListModel,
    /// The `ColumnView`'s selection model (Stage 3 Task 5) — every context-
    /// menu action reads its target row positions from here (`selection()`/
    /// `is_selected()`/`select_range()`), and `wire_context_menu_gesture`'s
    /// GNOME-convention reselect-if-not-selected step writes to it. Kept as
    /// its own field (not re-derived by downcasting `column_view.model()`
    /// on every use) since `TrackList::new` already builds the concrete
    /// `gtk::MultiSelection` directly.
    pub(in crate::ui) selection: gtk4::MultiSelection,
    /// The `ColumnView` widget itself (Stage 3 Task 9): kept so `TrackList::
    /// focus_track_list` can move keyboard focus onto it directly, rather
    /// than relying on `widget()`'s outer `gtk::Stack` to delegate focus to
    /// the right descendant on its own — see that method's doc comment for
    /// why the Escape shortcut (`ui::shortcuts`) needs a precise handle
    /// rather than "whatever's focusable in the current stack page."
    pub(in crate::ui) column_view: gtk4::ColumnView,
    /// Track id of the currently-playing row (the now-playing marker), or
    /// `None` when nothing is playing. Every column's `connect_bind` reads
    /// this to toggle the `.now-playing` marker class on its cell, so a row
    /// scrolled into view while it is the playing track is marked with no
    /// extra bookkeeping. `current_track_selection.rs` updates it on track
    /// change / stop and invalidates just the old and new rows, so the marker
    /// moves without rebuilding the list. A `Cell` (not `RefCell`) because the
    /// payload is a `Copy` `Option<i64>` read on every bind.
    pub(in crate::ui) playing_track_id: Cell<Option<i64>>,
    /// One-shot marker armed by `activate_track` with the id the user just
    /// started from the table (double-click/Enter/queue activation), telling
    /// the next now-playing follow (`current_track_selection::
    /// select_current_track`) to select the row but skip the viewport
    /// centering — the row is already on screen under the pointer, so a
    /// center would visibly yank the table. Consumed (`take`) on every
    /// follow regardless of id so a stale marker from an activation that
    /// never reached playback can't suppress a later auto-advance scroll.
    /// A `Cell`: `Copy` payload, single-threaded UI access, same rationale
    /// as `playing_track_id`.
    pub(in crate::ui) suppress_follow_scroll: Cell<Option<i64>>,
    /// NAV-5: per-source scroll/selection memory for this session. Written
    /// by `view_state_memory::remember_on_leave` when a source switch leaves
    /// a view, read by `view_state_memory::restore_on_attach` after the
    /// switched-to source reloaded. Never persisted (NAV-5 precision: view
    /// state must not survive an app restart).
    pub(in crate::ui) view_state_memory:
        RefCell<std::collections::HashMap<ViewSource, super::view_state_memory::SavedViewState>>,
    /// The same UI-owned connection `TrackList::new` was given, kept here
    /// too (alongside the clone `TrackListModel` holds internally) so the
    /// rating column's click handler can write through `library::stats`
    /// without reaching into the model's private state.
    pub(in crate::ui) conn: Rc<RefCell<Connection>>,
    /// Shared list-cell cover cache, retained so successful tag writes can
    /// invalidate entries keyed by the same path before rows are rebound.
    pub(in crate::ui) cover_loader: Rc<CoverLoader>,
    pub(in crate::ui) browse_bar: Rc<BrowseBar>,
    pub(in crate::ui) browse_filter: RefCell<BrowseFilter>,
    pub(in crate::ui) stack: gtk4::Stack,
    /// The single empty-state placeholder widget. Its title/description/icon
    /// are mutated in place by `apply_empty_state` rather than swapping in a
    /// third stack page — see that function's doc comment.
    pub(in crate::ui) empty_page: adw::StatusPage,
    pub(in crate::ui) sort: RefCell<SortState>,
    pub(in crate::ui) restoring_view: Cell<bool>,
    pub(in crate::ui) filter: RefCell<String>,
    /// Which of the six sources (Stage 3 Task 3) the list is currently
    /// showing — defaults to `ViewSource::Library`. Set via `TrackList::
    /// set_source` (and the `REPRISE_SMOKE_SOURCE` hook); read by `reload`
    /// and `queue_ids_for_activation`.
    pub(in crate::ui) source: RefCell<ViewSource>,
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
    pub(in crate::ui) queue_ids_provider: Box<dyn Fn() -> Vec<i64>>,
    /// Shared by `wire_activate` (user activation) and the smoke-activate
    /// hook so both take the identical code path.
    pub(in crate::ui) on_activate: OnActivate,
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
    pub(in crate::ui) on_reload: OnReload,
    /// Stage 3 Task 1 (a): the window's toast overlay, injected post-
    /// construction via `TrackList::set_toast_overlay` — same seam shape as
    /// `PlayerController::toast_overlay` (see that module's `## Toast +
    /// track-list-reload seam` doc section): built in `window::build` after
    /// `TrackList::new`, so it can't be a constructor parameter. `WeakRef`,
    /// not a strong reference, so `TrackList` can never keep the window
    /// alive past its natural lifetime.
    pub(in crate::ui) toast_overlay: glib::WeakRef<adw::ToastOverlay>,
    /// The main window, injected post-construction via `TrackList::set_
    /// window` — same seam shape as `toast_overlay` above. Needed as the
    /// parent for the context menu's "New playlist…" `AdwAlertDialog`
    /// (`show_new_playlist_dialog` below — mirrors `ui::sidebar`'s own
    /// dialog of the same shape). `WeakRef`, not a strong reference, for the
    /// same reason as `toast_overlay`.
    pub(in crate::ui) window: glib::WeakRef<adw::ApplicationWindow>,
    /// Context-menu "Play" action callback (Stage 3 Task 5), injected via
    /// `TrackList::set_on_play_selected` — wraps `PlayerController::
    /// play_from_view` without this module depending on that type directly
    /// (same decoupling-via-closure seam as `on_activate`/`queue_ids_
    /// provider`). `RefCell<Option<Rc<dyn Fn>>>`, not a plain field set at
    /// construction, since the player controller is built by `window.rs`
    /// independently of `TrackList` and wired in afterwards.
    pub(in crate::ui) on_play_selected: RefCell<Option<OnPlaySelected>>,
    /// Context-menu "Add to queue" action callback, injected via
    /// `TrackList::set_on_queue_selected` — wraps `PlayerController::
    /// append_to_queue`. Same seam shape as `on_play_selected`.
    pub(in crate::ui) on_queue_selected: RefCell<Option<OnQueueSelected>>,
    pub(in crate::ui) on_queue_activate: RefCell<Option<OnQueueActivate>>,
    pub(in crate::ui) on_queue_remove: RefCell<Option<OnQueueRemove>>,
    /// Invoked after any context-menu action that mutates a playlist's
    /// membership (add to an existing playlist, add to a brand new one, or
    /// remove) — injected via `TrackList::set_on_playlist_mutated`, wired by
    /// `window.rs` to `Sidebar::refresh` (a new trigger alongside the three
    /// already listed in that method's doc comment: scan completion,
    /// playlist CRUD from the sidebar itself, and missing-marking). Sidebar
    /// track counts must stay in sync with playlist changes made from this
    /// menu, exactly as they already do for changes made from the sidebar's
    /// own "New playlist" dialog.
    pub(in crate::ui) on_playlist_mutated: RefCell<Option<Rc<dyn Fn()>>>,
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
    pub(in crate::ui) on_queue_reorder: RefCell<Option<OnQueueReorder>>,
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
    pub(in crate::ui) on_sidebar_playlist_drop: RefCell<Option<OnSidebarPlaylistDrop>>,
    /// Sidebar drag-and-drop "add to queue" callback — the Queue-row twin of
    /// `on_sidebar_playlist_drop` above, existing for the identical reason
    /// (`ui::track_list_dnd_smoke`'s `REPRISE_SMOKE_DND=addqueue` hook needs
    /// to drive the *exact* sequence a real drop onto the Queue row runs, and
    /// this module has no direct handle on `Sidebar`). `window.rs` wires it
    /// to `Sidebar::handle_queue_drop`; returns whether anything was appended.
    pub(in crate::ui) on_sidebar_queue_drop: RefCell<Option<OnSidebarQueueDrop>>,
    /// Stage 3 Task 8: the ImportErrors source's dedicated panel — see
    /// `STACK_PAGE_IMPORT_ERRORS`'s doc comment. Built once, alongside every
    /// other widget, and refreshed (not rebuilt) on every `reload()` while
    /// this source is selected.
    pub(in crate::ui) import_errors_view: ImportErrorsView,
    /// "Rescan library" (Missing-source context menu item, Stage 3 Task 8):
    /// injected via `TrackList::set_on_rescan_library` — wraps `ui::window`'s
    /// scan flow against the persisted library root without this module
    /// depending on the scan machinery/settings table directly (same
    /// decoupling-via-closure seam as `on_play_selected`/`on_queue_
    /// selected`).
    pub(in crate::ui) on_rescan_library: RefCell<Option<Rc<dyn Fn()>>>,
    /// "Remove from library" (Missing-source context menu item, Stage 3 Task
    /// 8): injected via `TrackList::set_on_library_mutated` — `window.rs`
    /// wires this to `Sidebar::refresh` (the Missing badge count can only
    /// ever shrink from this action) AND `PlayerController::purge_queue_ids`
    /// (Stage-3 close-out: a hard-deleted track must also be purged from the
    /// playback queue). Takes the ids `queries::remove_missing_tracks`
    /// actually deleted — not just a bare notification — so the queue purge
    /// knows exactly which ids to remove.
    pub(in crate::ui) on_library_mutated: RefCell<Option<OnLibraryMutated>>,
    /// Invoked after successful file-tag writes and DB reconciliation.
    /// Kept separate from `on_library_mutated`: editing tags must never purge
    /// otherwise valid tracks from the playback queue.
    pub(in crate::ui) on_tags_mutated: RefCell<Option<OnTagsMutated>>,
    /// Invoked after the ImportErrors panel's own Retry/Dismiss actions
    /// mutate `import_errors` — injected via `TrackList::set_on_import_
    /// errors_mutated`, wired by `window.rs` to `Sidebar::refresh` (the
    /// Import-errors badge count just changed).
    pub(in crate::ui) on_import_errors_mutated: RefCell<Option<Rc<dyn Fn()>>>,
    pub(in crate::ui) on_selection_changed: RefCell<Option<OnSelectionChanged>>,
    /// The player controller, injected post-construction via `TrackList::set_
    /// player` — used by tag-edit flow to refresh now-playing metadata after
    /// successful tag edits. `Weak`, not a strong reference, to avoid
    /// circular ownership with the player controller.
    pub(in crate::ui) player:
        RefCell<std::rc::Weak<crate::ui::player_controller::PlayerController>>,
}

/// Handle to the built track list widget. Owns the shared, reference-counted
/// state that the sort-header and search-debounce callbacks close over.
pub struct TrackList {
    pub(in crate::ui) shared: Rc<Shared>,
    pub(in crate::ui) root: gtk4::Box,
    pub(in crate::ui) column_registry: ColumnRegistry,
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
        super::track_list_builder::build(
            conn,
            on_activate,
            on_reload,
            queue_ids_provider,
            cover_download,
        )
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

    pub fn reload_queue_if_visible(&self) {
        if matches!(*self.shared.source.borrow(), ViewSource::Queue) {
            reload(&self.shared);
        }
    }

    pub(in crate::ui) fn set_browse_visible(&self, visible: bool) {
        self.shared.browse_bar.set_preference_visible(visible);
    }

    pub(in crate::ui) fn toast(&self, message: &str) {
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
    pub fn set_on_play_selected(&self, callback: impl Fn(Vec<i64>, usize, ViewSource) + 'static) {
        *self.shared.on_play_selected.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the context menu's "Add to queue" action callback — see the
    /// `Shared::on_queue_selected` doc comment. `window.rs` wires this to
    /// `PlayerController::append_to_queue`.
    pub fn set_on_queue_selected(&self, callback: impl Fn(Vec<i64>) + 'static) {
        *self.shared.on_queue_selected.borrow_mut() = Some(Rc::new(callback));
    }

    pub fn set_on_queue_activate(&self, callback: impl Fn(usize) + 'static) {
        *self.shared.on_queue_activate.borrow_mut() = Some(Rc::new(callback));
    }

    pub fn set_on_queue_remove(&self, callback: impl Fn(&[usize]) -> usize + 'static) {
        *self.shared.on_queue_remove.borrow_mut() = Some(Rc::new(callback));
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

    /// Injects the sidebar "add to queue" drag-and-drop callback — see the
    /// `Shared::on_sidebar_queue_drop` doc comment. `window.rs` wires this to
    /// `Sidebar::handle_queue_drop`.
    pub fn set_on_sidebar_queue_drop(&self, callback: impl Fn(&[i64]) -> bool + 'static) {
        *self.shared.on_sidebar_queue_drop.borrow_mut() = Some(Rc::new(callback));
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

    /// Sets a widget as the child of the empty-library status page, so it
    /// appears below the icon/title/description during a first scan.
    /// Called from `window.rs` after the `EmptyScanIndicator` is created,
    /// to embed its container widget in the status page.
    pub fn set_empty_scan_widget(&self, widget: &impl IsA<gtk4::Widget>) {
        self.shared.empty_page.set_child(Some(widget));
    }

    /// Injects the player controller — injected post-construction via
    /// `TrackList::set_player`, used by the tag-edit flow to refresh
    /// now-playing metadata after successful tag edits.
    pub fn set_player(&self, player: &Rc<crate::ui::player_controller::PlayerController>) {
        *self.shared.player.borrow_mut() = Rc::downgrade(player);
    }
}

/// Clone-out-then-call `on_import_errors_mutated` (hoisted per this
/// project's `RefCell` callback discipline), then `reload` — the panel's own
/// `refresh()` already updated its rows before this callback fired (see
/// `import_errors_view.rs`'s `notify_mutated_and_refresh`), but only `reload`
/// re-derives this `TrackList`'s stack-page decision (e.g. switching to the
/// "nothing here" empty page once the last error is dismissed).
pub(in crate::ui) fn notify_import_errors_mutated_and_reload(shared: &Rc<Shared>) {
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
pub(in crate::ui) fn playlist_reorder_allowed(shared: &Shared) -> bool {
    matches!(*shared.source.borrow(), ViewSource::Playlist(_))
        && shared.sort.borrow().field == PLAYLIST_ORDER_SORT_FIELD
        && shared.filter.borrow().trim().is_empty()
}

/// Shows `text` as an `adw::Toast`, degrading to a warn log if no overlay is
/// wired or it's gone — mirrors `player_controller.rs`'s `show_toast` (same
/// seam, same degrade behavior), not shared code: the two owning types are
/// otherwise unrelated and this is a two-line `WeakRef::upgrade` match.
pub(in crate::ui) fn show_toast(shared: &Shared, text: &str) {
    match shared.toast_overlay.upgrade() {
        Some(overlay) => toasts::show(&overlay, text),
        None => {
            tracing::warn!(text, "toast overlay is gone; degrading to log-only");
        }
    }
}
