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

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use rusqlite::Connection;

use crate::format::format_duration;
use crate::library::stats;
use crate::models::Track;
use crate::queries;
use crate::ui::import_errors_view::ImportErrorsView;
use crate::ui::rating::RatingWidget;
use crate::ui::strings;
use crate::ui::track_list_context_menu;
use crate::ui::track_list_dnd;
use crate::ui::track_list_dnd_smoke;
use crate::ui::track_list_model::TrackListModel;
use crate::view_source::ViewSource;

const STACK_PAGE_EMPTY: &str = "empty";
const STACK_PAGE_LIST: &str = "list";
/// Stage 3 Task 8: the ImportErrors source's dedicated path/reason/time panel
/// (`ui::import_errors_view::ImportErrorsView`) — a third `gtk::Stack` page,
/// shown instead of `STACK_PAGE_LIST` only while `ViewSource::ImportErrors`
/// is selected and has rows (see `apply_empty_state`'s `List` arm).
const STACK_PAGE_IMPORT_ERRORS: &str = "import_errors";

/// Dev/verification hook (permanent, like `REPRISE_SCAN_DIR` and
/// `REPRISE_SMOKE_QUIT`): when set, the first row is activated
/// programmatically — through the exact same `on_activate` path a
/// double-click takes — once the initial load has run and the main loop is
/// idle. Combined with `REPRISE_SCAN_DIR` (populate), `REPRISE_AUDIO_SINK=
/// fakesink` (no audio device) and `REPRISE_SMOKE_QUIT` (exit), this enables
/// the full headless play-a-track E2E:
///
/// `REPRISE_SCAN_DIR=… REPRISE_SMOKE_ACTIVATE=1 REPRISE_AUDIO_SINK=fakesink
///  REPRISE_SMOKE_QUIT=1 xvfb-run -a cargo run`
const SMOKE_ACTIVATE_ENV_VAR: &str = "REPRISE_SMOKE_ACTIVATE";

/// Dev/verification hook (permanent, like `REPRISE_SMOKE_ACTIVATE`): when
/// set to a non-empty string, that string is applied as the search filter —
/// through `set_filter_and_reload`, the exact same filter-apply step
/// `TrackList::set_filter` (the typed-search path) ends in, just invoked
/// directly instead of via `window.rs`'s 200ms keystroke-debounce timer,
/// since there's no keystroke to debounce here — once the initial load has
/// run and the main loop is idle. Combined with `REPRISE_SCAN_DIR`
/// (populate) and `REPRISE_SMOKE_QUIT` (exit), this drives the `NoResults`
/// empty state and the filtered "N of M tracks" status line headlessly:
///
/// `REPRISE_SCAN_DIR=… REPRISE_SMOKE_FILTER=nomatch REPRISE_SMOKE_QUIT=1
///  xvfb-run -a cargo run`
const SMOKE_FILTER_ENV_VAR: &str = "REPRISE_SMOKE_FILTER";

/// Dev/verification hook (permanent, like the other `REPRISE_SMOKE_*` hooks
/// above): when set, switches the track list to the named `ViewSource` once
/// the initial load has run and the main loop is idle, through the exact
/// same `TrackList::set_source` path the future sidebar (Task 4) will use.
/// Accepted values: `library`, `missing`, `queue`, `import_errors`, or
/// `playlist:<id>`/`smart:<id>` (Task 4 wires the sidebar UI for the latter
/// two; the query layer and this hook already support them). Logs `"view
/// source set"` plus the resulting row count, so a headless run can assert
/// both the switch and the row count it produced.
///
/// Usage: `REPRISE_SCAN_DIR=… REPRISE_SMOKE_SOURCE=missing REPRISE_SMOKE_QUIT=1
///  xvfb-run -a cargo run`.
const SMOKE_SOURCE_ENV_VAR: &str = "REPRISE_SMOKE_SOURCE";

/// Dev/verification hook (permanent, like the other `REPRISE_SMOKE_*` hooks
/// above; added for the Task 5 Fix Round 1 "remove from playlist targets the
/// wrong row" data-loss fix): when set to `"title"` or `"artist"`,
/// programmatically calls `GtkColumnView::sort_by_column` on that column —
/// the exact same call a real column-header click triggers (see the initial
/// `column_view.sort_by_column(Some(&artist_column), …)` call in `TrackList::
/// new`) — so a headless E2E run can put the track list into a sort other
/// than a playlist source's own forced `"playlist_order"` default, and then
/// exercise `REPRISE_SMOKE_MENU_ACTION=remove-from-playlist` (`ui::track_
/// list_context_menu`) against the resulting divergent view: this is the
/// only way to drive "remove from a *sorted* playlist view" headlessly,
/// since there is no supported way to synthesize a real pointer click on a
/// column header. Registered to run *after* `REPRISE_SMOKE_SOURCE` (see the
/// arming order in `TrackList::new`), so a `playlist:<name>` switch's own
/// forced default sort has already applied before this overrides it —
/// matching what a real user does (open a playlist, then click a header).
///
/// Usage: `REPRISE_SCAN_DIR=… REPRISE_SMOKE_SOURCE=playlist:P
///  REPRISE_SMOKE_SORT_COLUMN=title
///  REPRISE_SMOKE_MENU_ACTION=remove-from-playlist REPRISE_SMOKE_QUIT=1
///  xvfb-run -a cargo run`.
const SMOKE_SORT_COLUMN_ENV_VAR: &str = "REPRISE_SMOKE_SORT_COLUMN";

/// Icon shown on the empty-library placeholder (nothing has been scanned
/// in yet).
const ICON_EMPTY_LIBRARY: &str = "folder-music-symbolic";
/// Icon shown when a search filter matched zero rows — distinct from the
/// empty-library icon so the two states also read differently at a glance.
const ICON_NO_RESULTS: &str = "system-search-symbolic";
/// Icon shown for the neutral "nothing here" state (`Missing`/`ImportErrors`
/// sources with no rows and no active filter) — distinct from both of the
/// above: this isn't "no music has been scanned in" nor "your search
/// matched nothing", just "this particular view has no members right now".
const ICON_NOTHING_HERE: &str = "dialog-information-symbolic";

#[derive(Debug, Clone, PartialEq, Eq)]
struct SortState {
    field: String,
    dir: String,
}

/// Which page of the track-list `Stack` should be visible, and (for the
/// empty variants) which copy the shared `StatusPage` should carry. A plain
/// enum decided by a pure function (`empty_state_for`) so the selection
/// logic is unit-testable without a running GTK application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmptyState {
    /// The library itself has no tracks yet (no filter active either).
    EmptyLibrary,
    /// A neutral "nothing here" state (Stage 3 Task 3): the `Missing`/
    /// `ImportErrors` sources have no rows and no filter is active — unlike
    /// `EmptyLibrary`, this isn't about the library having no music at all,
    /// just this particular view having no members right now.
    NothingHere,
    /// The current source has rows, but the active search filter matched
    /// none.
    NoResults,
    /// At least one row to show — the populated list page.
    List,
}

/// Pure decision of which empty state (or the populated list) applies for a
/// given result-row count, whether a search filter is currently active, and
/// which `ViewSource` is showing. Kept side-effect free and separate from
/// `reload`/`apply_empty_state` so it can be unit tested directly instead of
/// only through a live GTK stack. `source` only matters for the
/// zero-rows/no-filter case: `Missing`/`ImportErrors` get the neutral
/// `NothingHere` copy there instead of `EmptyLibrary`'s "no music yet"
/// (which would be a confusing thing to say about, e.g., a "no files are
/// currently missing" state — that's good news, not an invitation to scan a
/// folder).
fn empty_state_for(row_count: usize, has_filter: bool, source: &ViewSource) -> EmptyState {
    match (row_count, has_filter) {
        (0, true) => EmptyState::NoResults,
        (0, false) => match source {
            ViewSource::Missing | ViewSource::ImportErrors => EmptyState::NothingHere,
            _ => EmptyState::EmptyLibrary,
        },
        _ => EmptyState::List,
    }
}

/// Default sort: artist ascending, matching the secondary-order convention
/// already baked into `queries::SORT_WHITELIST` for the "artist" field.
impl Default for SortState {
    fn default() -> Self {
        Self {
            field: "artist".to_string(),
            dir: "asc".to_string(),
        }
    }
}

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
/// `window.rs` needs all three.
type OnReload = Box<dyn Fn(&ViewSource, usize, &str)>;

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
    column_view: gtk4::ColumnView,
    /// The same UI-owned connection `TrackList::new` was given, kept here
    /// too (alongside the clone `TrackListModel` holds internally) so the
    /// rating column's click handler can write through `library::stats`
    /// without reaching into the model's private state.
    pub(super) conn: Rc<RefCell<Connection>>,
    stack: gtk4::Stack,
    /// The single empty-state placeholder widget. Its title/description/icon
    /// are mutated in place by `apply_empty_state` rather than swapping in a
    /// third stack page — see that function's doc comment.
    empty_page: adw::StatusPage,
    sort: RefCell<SortState>,
    filter: RefCell<String>,
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
    queue_ids_provider: Box<dyn Fn() -> Vec<i64>>,
    /// Shared by `wire_activate` (user activation) and the smoke-activate
    /// hook so both take the identical code path.
    on_activate: OnActivate,
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
    import_errors_view: ImportErrorsView,
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
    /// Invoked after the ImportErrors panel's own Retry/Dismiss actions
    /// mutate `import_errors` — injected via `TrackList::set_on_import_
    /// errors_mutated`, wired by `window.rs` to `Sidebar::refresh` (the
    /// Import-errors badge count just changed).
    pub(super) on_import_errors_mutated: RefCell<Option<Rc<dyn Fn()>>>,
}

/// Handle to the built track list widget. Owns the shared, reference-counted
/// state that the sort-header and search-debounce callbacks close over.
pub struct TrackList {
    shared: Rc<Shared>,
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
        on_reload: impl Fn(&ViewSource, usize, &str) + 'static,
        queue_ids_provider: impl Fn() -> Vec<i64> + 'static,
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

        // Built here, before any column is appended — unlike every stage
        // before Task 5, which built columns first: each column's `connect_
        // setup` now also wires its cell's context-menu gesture, which needs
        // `&shared` (see `wire_context_menu_gesture`). Nothing else in
        // `Shared` depends on the columns existing first, so this reorder
        // has no other consequence.
        let shared = Rc::new(Shared {
            model,
            selection: selection.clone(),
            column_view: column_view.clone(),
            conn,
            stack,
            empty_page,
            sort: RefCell::new(SortState::default()),
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
            on_import_errors_mutated: RefCell::new(None),
        });

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

        let title_column = append_column(
            &column_view,
            &shared,
            "title",
            strings::COLUMN_TITLE,
            0.0,
            false,
            |t| t.title.clone(),
        );
        let artist_column = append_column(
            &column_view,
            &shared,
            "artist",
            strings::COLUMN_ARTIST,
            0.0,
            false,
            |t| t.artist.clone(),
        );
        append_column(
            &column_view,
            &shared,
            "album",
            strings::COLUMN_ALBUM,
            0.0,
            false,
            |t| t.album.clone(),
        );
        append_column(
            &column_view,
            &shared,
            "year",
            strings::COLUMN_YEAR,
            0.0,
            false,
            |t| t.year.map(|y| y.to_string()).unwrap_or_default(),
        );
        append_column(
            &column_view,
            &shared,
            "duration_ms",
            strings::COLUMN_LENGTH,
            1.0,
            true,
            |t| format_duration(t.duration_ms),
        );

        // Built after `shared` exists (unlike the other columns above): its
        // click handler needs `shared.conn`/`shared.model` to persist a
        // rating write and refresh the model's cached row — see
        // `append_rating_column`'s doc comment. Appended last, so it still
        // lands as the rightmost column, matching the visual order the
        // other five columns were just added in.
        append_rating_column(&column_view, &shared);

        wire_sort_clicks(&column_view, &shared);

        // Sets the initial sort indicator (artist ascending) on the column
        // header. `SortState::default()` is already `artist`/`asc`, so the
        // `primary-sort-column`/`primary-sort-order` notify signals this
        // triggers land in `on_sorter_changed`, compute the same
        // (field, dir) pair already stored in `shared.sort`, and the dedup
        // guard there (`if *shared.sort.borrow() == new_sort { return; }`)
        // short-circuits before it would call `reload` — so this call fires
        // zero SQL queries. The one and only initial load below still runs
        // exactly once.
        column_view.sort_by_column(Some(&artist_column), gtk4::SortType::Ascending);

        wire_activate(&column_view, &shared);
        track_list_context_menu::wire_context_menu_actions(&column_view, &shared);

        reload(&shared);
        arm_smoke_activate(&shared);
        arm_smoke_filter(&shared);
        arm_smoke_source(&shared);
        arm_smoke_sort_column(&column_view, &title_column, &artist_column);
        track_list_context_menu::arm_smoke_menu_action(&shared);
        track_list_dnd_smoke::arm_smoke_dnd(&shared);

        Self { shared }
    }

    /// The root widget to embed as the window body (a `gtk::Stack` that
    /// switches between the empty placeholder and the populated list).
    pub fn widget(&self) -> &gtk4::Stack {
        &self.shared.stack
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
        reload(&self.shared);
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
        Some(overlay) => overlay.add_toast(adw::Toast::new(text)),
        None => {
            tracing::warn!(text, "toast overlay is gone; degrading to log-only");
        }
    }
}

/// Builds the shared empty-state placeholder, initially carrying the
/// empty-library copy (the state `TrackList::new`'s first `reload()` will
/// normally confirm, since there's no library yet on first launch).
/// `apply_empty_state` swaps its title/description/icon in place for the
/// no-results case rather than building a second widget.
fn build_status_page() -> adw::StatusPage {
    adw::StatusPage::builder()
        .icon_name(ICON_EMPTY_LIBRARY)
        .title(strings::EMPTY_LIBRARY_TITLE)
        .description(strings::EMPTY_LIBRARY_DESCRIPTION)
        .vexpand(true)
        .build()
}

/// Builds one `ColumnViewColumn` bound to a `SignalListItemFactory` that
/// renders a single `gtk::Label` per cell. `sort_id` is a whitelisted
/// `queries` sort field name, stashed on the column via `set_id` so header
/// clicks can be mapped back to it. `right_align` additionally marks the
/// label with the "numeric" style class (tabular figures, GNOME convention
/// for right-aligned numeric columns such as file sizes/durations). Returns
/// the built column so `TrackList::new` can set the initial sort indicator
/// on the artist column. `shared`/`column_view` are threaded through to
/// `wire_context_menu_gesture` (Stage 3 Task 5) so a secondary click on this
/// column's cells opens the row context menu — see that function's doc
/// comment.
fn append_column(
    column_view: &gtk4::ColumnView,
    shared: &Rc<Shared>,
    sort_id: &'static str,
    title: &str,
    xalign: f32,
    right_align: bool,
    render: impl Fn(&Track) -> String + 'static,
) -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();

    let shared = shared.clone();
    let column_view_for_setup = column_view.clone();
    factory.connect_setup(move |_, obj| {
        let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
            tracing::warn!("track list column setup: object is not a ListItem");
            return;
        };
        let label = gtk4::Label::new(None);
        label.set_xalign(xalign);
        label.set_halign(if right_align {
            gtk4::Align::End
        } else {
            gtk4::Align::Start
        });
        if right_align {
            label.add_css_class("numeric");
        }
        track_list_context_menu::wire_context_menu_gesture(
            &label,
            item,
            &shared,
            &column_view_for_setup,
        );
        // Stage 3 Task 6: drag-source (fill a playlist / reorder) and
        // drop-target (reorder) — see `ui::track_list_dnd`'s doc comment.
        track_list_dnd::wire_row_dnd(&label, item, &shared);
        item.set_child(Some(&label));
    });

    factory.connect_bind(move |_, obj| {
        let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
            tracing::warn!("track list column bind: object is not a ListItem");
            return;
        };
        let Some(label) = item.child().and_then(|w| w.downcast::<gtk4::Label>().ok()) else {
            tracing::warn!("track list column bind: list item child is not a Label");
            return;
        };
        let Some(boxed) = item
            .item()
            .and_then(|o| o.downcast::<glib::BoxedAnyObject>().ok())
        else {
            tracing::warn!("track list column bind: item is not a BoxedAnyObject<Track>");
            return;
        };
        let track = boxed.borrow::<Track>();
        label.set_text(&render(&track));
    });

    let column = gtk4::ColumnViewColumn::builder()
        .title(title)
        .factory(&factory)
        .resizable(true)
        .build();
    column.set_id(Some(sort_id));

    // Dummy sorter: makes the header clickable/toggleable without ever
    // reordering the model itself (SQL is the sort source of truth — see
    // module doc comment).
    let never_sorts = gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal);
    column.set_sorter(Some(&never_sorts));

    column_view.append_column(&column);
    column
}

/// Builds the interactive `Rating` column: each cell is a `RatingWidget`
/// (`ui::rating`) instead of a `gtk::Label` — the one column whose factory
/// writes back to the database on user interaction rather than only
/// rendering a `Track` field. Requires a fully-built `shared` (its
/// `conn`/`model` are used by the click handler), which is why
/// `TrackList::new` calls this after constructing `Shared`, unlike the
/// other five columns built by `append_column` beforehand.
fn append_rating_column(
    column_view: &gtk4::ColumnView,
    shared: &Rc<Shared>,
) -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();

    {
        let shared = shared.clone();
        let column_view = column_view.clone();
        factory.connect_setup(move |_, obj| {
            let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
                tracing::warn!("rating column setup: object is not a ListItem");
                return;
            };
            let rating_widget = RatingWidget::new();
            // Secondary-click (button 3) context menu (Stage 3 Task 5) —
            // the rating column's own stars only ever respond to primary-
            // button clicks (`gtk::Button`'s default), so this can never
            // steal a rating click. See `wire_context_menu_gesture`'s doc
            // comment.
            track_list_context_menu::wire_context_menu_gesture(
                &rating_widget,
                item,
                &shared,
                &column_view,
            );
            // Stage 3 Task 6: same drag-source/drop-target wiring as the
            // five text columns — see `ui::track_list_dnd`'s doc comment.
            track_list_dnd::wire_row_dnd(&rating_widget, item, &shared);
            item.set_child(Some(&rating_widget));
        });
    }

    {
        let shared = shared.clone();
        factory.connect_bind(move |_, obj| {
            let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
                tracing::warn!("rating column bind: object is not a ListItem");
                return;
            };
            let Some(rating_widget) = item.child().and_then(|w| w.downcast::<RatingWidget>().ok())
            else {
                tracing::warn!("rating column bind: list item child is not a RatingWidget");
                return;
            };
            let Some(boxed) = item
                .item()
                .and_then(|o| o.downcast::<glib::BoxedAnyObject>().ok())
            else {
                tracing::warn!("rating column bind: item is not a BoxedAnyObject<Track>");
                return;
            };
            let track = boxed.borrow::<Track>();
            // Programmatic display update only — `RatingWidget::set_rating`
            // never invokes the `on_changed` callback, so this can never
            // recurse into `on_rating_changed` below (see the module doc
            // comment on `ui::rating`).
            rating_widget.set_rating(track.rating);

            let track_id = track.id;
            let title = track.title.clone();
            let position = item.position();
            let shared = shared.clone();
            rating_widget.set_on_changed(move |new_rating| {
                on_rating_changed(&shared, track_id, &title, position, new_rating);
            });
        });
    }

    // Recycling guard: on unbind (the row is about to be rebound to a
    // different `Track`, or the widget dropped off-screen entirely), clear
    // the callback rather than leaving it pointed at the just-vacated
    // `(track_id, position)` pair. `connect_bind` always installs a fresh
    // one before the widget can be interacted with again, so this mainly
    // closes off a race that's already vanishingly unlikely — but a no-op
    // closure costs nothing and removes the possibility outright.
    factory.connect_unbind(move |_, obj| {
        let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(rating_widget) = item.child().and_then(|w| w.downcast::<RatingWidget>().ok())
        else {
            return;
        };
        rating_widget.set_on_changed(|_| {});
    });

    let column = gtk4::ColumnViewColumn::builder()
        .title(strings::RATING)
        .factory(&factory)
        .resizable(true)
        .build();
    column.set_id(Some("rating"));

    // Dummy sorter: makes the header clickable/toggleable without ever
    // reordering the model itself (SQL is the sort source of truth — see
    // module doc comment).
    let never_sorts = gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal);
    column.set_sorter(Some(&never_sorts));

    column_view.append_column(&column);
    column
}

/// Persists a rating change via `library::stats::set_rating` and, on
/// success, invalidates the model's cached copy of the affected row (see
/// `TrackListModel::invalidate_window_at`). A write failure is logged and,
/// since Stage 3 Task 1 (backlog item a), also surfaced as a toast: the
/// displayed rating already reflects the click (`RatingWidget::set_rating`
/// ran first), so without a toast the user couldn't tell the write didn't
/// persist until scrolling away and back; never crashes or wedges the UI
/// either way (fault tolerance).
fn on_rating_changed(
    shared: &Rc<Shared>,
    track_id: i64,
    title: &str,
    position: u32,
    new_rating: i32,
) {
    tracing::debug!(track_id, position, new_rating, "rating changed");
    let result = {
        let conn = shared.conn.borrow();
        stats::set_rating(&conn, track_id, new_rating)
    };
    match result {
        Ok(()) => shared.model.invalidate_window_at(position),
        Err(error) => {
            tracing::error!(%error, track_id, new_rating, "failed to persist rating change");
            show_toast(shared, &strings::rating_save_failed_toast(title));
        }
    }
}

/// Observes the `ColumnView`'s aggregate sorter for header clicks and maps
/// them back to a whitelisted sort field + direction, then reloads.
fn wire_sort_clicks(column_view: &gtk4::ColumnView, shared: &Rc<Shared>) {
    let Some(sorter) = column_view.sorter() else {
        tracing::warn!("track list: ColumnView has no aggregate sorter; header clicks won't sort");
        return;
    };
    let Some(cv_sorter) = sorter.downcast_ref::<gtk4::ColumnViewSorter>() else {
        tracing::warn!(
            "track list: ColumnView sorter is not a ColumnViewSorter; header clicks won't sort"
        );
        return;
    };

    {
        let shared = shared.clone();
        cv_sorter.connect_primary_sort_column_notify(move |s| on_sorter_changed(&shared, s));
    }
    {
        let shared = shared.clone();
        cv_sorter.connect_primary_sort_order_notify(move |s| on_sorter_changed(&shared, s));
    }
}

fn on_sorter_changed(shared: &Rc<Shared>, sorter: &gtk4::ColumnViewSorter) {
    let Some(column) = sorter.primary_sort_column() else {
        return;
    };
    let Some(id) = column.id() else {
        tracing::warn!("track list: sorted column has no id; ignoring click");
        return;
    };
    let dir = match sorter.primary_sort_order() {
        gtk4::SortType::Descending => "desc",
        _ => "asc",
    };
    let new_sort = SortState {
        field: id.to_string(),
        dir: dir.to_string(),
    };

    // A single header click can fire both `primary-sort-column-notify` and
    // `primary-sort-order-notify` (e.g. switching to a new column changes
    // both which column is primary and its initial direction). Both land
    // here; only reload once the (field, dir) pair has actually changed so
    // one click can't trigger two identical SQL queries.
    if *shared.sort.borrow() == new_sort {
        return;
    }

    *shared.sort.borrow_mut() = new_sort;
    reload(shared);
}

/// Row activation (double-click or Enter on a focused row): resolve the
/// row's `Track` via `TrackListModel::track_at`, build its queue via
/// `queue_ids_for_activation`, and hand both to the `on_activate` callback
/// (which `window::build` routes to the player).
fn wire_activate(column_view: &gtk4::ColumnView, shared: &Rc<Shared>) {
    let shared = shared.clone();
    column_view.connect_activate(move |_view, position| {
        let Some(track) = shared.model.track_at(position) else {
            tracing::warn!(position, "track list activate: no item at position");
            return;
        };
        tracing::info!(path = %track.path, "activate track");
        let (ids, start_index) = queue_ids_for_activation(&shared, position, track.id);
        (shared.on_activate)(&track, ids, start_index);
    });
}

/// Builds the `(ids, start_index)` pair `OnActivate` carries: every track id
/// in the activated row's *current* source/sort/filter view, via
/// `queries::query_track_ids` — deliberately not `TrackListModel::
/// track_at`/`query_track_window`, which are windowed and capped at
/// `MAX_WINDOW_LIMIT` (500, sized for one `ColumnView` page) rather than a
/// whole playback queue (`QUEUE_LIMIT`, 10,000). `shared.source`/`shared.
/// sort`/`shared.filter` are read here rather than reaching into
/// `TrackListModel`'s private state (see the module doc comment on why the
/// model's `imp()` state isn't exposed) — `Shared` is the one place both the
/// model's query and this activation path already agree on the current
/// source/sort/filter, so it's the natural seam for a second query using
/// the same state. When `source` is `ViewSource::Queue`, `queue_ids` is
/// fetched fresh from `current_queue_ids` (same as `reload`) so re-
/// activating a row while already viewing the queue re-queues that exact
/// list, starting at the clicked position.
///
/// `position` doubles as `start_index` into `ids`: activation always uses
/// the unfiltered-by-cap ordering, so the row the user clicked is always the
/// same index in this ids list as it is in the `ColumnView` — as long as the
/// query wasn't truncated by `QUEUE_LIMIT` before reaching that row, which
/// `is_queue_capped` can't fully rule out but is exceedingly unlikely (a
/// 10,000+ track library with the activated row past the cap). On a query
/// failure, degrades to a single-track queue (`[activated_id]`, index 0) so
/// the click still plays something instead of silently doing nothing.
fn queue_ids_for_activation(
    shared: &Rc<Shared>,
    position: u32,
    activated_id: i64,
) -> (Vec<i64>, usize) {
    let sort = shared.sort.borrow().clone();
    let filter = shared.filter.borrow().clone();
    let source = shared.source.borrow().clone();

    let queue_ids = if matches!(source, ViewSource::Queue) {
        current_queue_ids(shared)
    } else {
        Vec::new()
    };

    let ids = {
        let conn = shared.conn.borrow();
        queries::query_track_ids(&conn, &source, &sort.field, &sort.dir, &filter, &queue_ids)
    };

    match ids {
        Ok(ids) => {
            if queries::is_queue_capped(ids.len()) {
                tracing::warn!(
                    limit = queries::QUEUE_LIMIT,
                    "queue capped at {} tracks",
                    queries::QUEUE_LIMIT
                );
            }
            (ids, position as usize)
        }
        Err(error) => {
            tracing::error!(
                %error,
                "failed to build queue ids for activation; falling back to a single-track queue"
            );
            (vec![activated_id], 0)
        }
    }
}

/// Fetches the current queue's ids (in play order) via `shared.queue_ids_
/// provider`, for `reload`/`queue_ids_for_activation` to pass through to the
/// `queries` layer when `source` is `ViewSource::Queue`. Every call site
/// already checks `source` first, so this is only ever invoked when a fresh
/// snapshot is actually needed.
fn current_queue_ids(shared: &Rc<Shared>) -> Vec<i64> {
    (shared.queue_ids_provider)()
}

/// Arms the `REPRISE_SMOKE_ACTIVATE` hook (see `SMOKE_ACTIVATE_ENV_VAR`):
/// one idle callback, deferred so it runs once the main loop is up rather
/// than in the middle of window construction, that pushes the first row
/// through the same `on_activate` path as a real double-click.
fn arm_smoke_activate(shared: &Rc<Shared>) {
    if std::env::var(SMOKE_ACTIVATE_ENV_VAR).is_err() {
        return;
    }
    tracing::info!("{SMOKE_ACTIVATE_ENV_VAR} set: arming first-row activation");
    let shared = shared.clone();
    glib::idle_add_local_once(move || {
        let Some(track) = shared.model.track_at(0) else {
            tracing::warn!("{SMOKE_ACTIVATE_ENV_VAR}: track list is empty; nothing to activate");
            return;
        };
        tracing::info!(path = %track.path, "{SMOKE_ACTIVATE_ENV_VAR}: activating first row");
        let (ids, start_index) = queue_ids_for_activation(&shared, 0, track.id);
        (shared.on_activate)(&track, ids, start_index);
    });
}

/// Sets `shared.filter` and reloads — the one place that mutates the filter
/// before reloading, shared by `TrackList::set_filter` (the typed-search
/// path, reached via `window.rs`'s debounce timer) and the
/// `REPRISE_SMOKE_FILTER` dev hook (`arm_smoke_filter`), so both apply a new
/// filter through the identical code path.
fn set_filter_and_reload(shared: &Rc<Shared>, text: &str) {
    *shared.filter.borrow_mut() = text.to_string();
    reload(shared);
}

/// Mirrors `queries.rs`'s `"playlist_order"` `SORT_WHITELIST` sentinel (see
/// that module's `Playlist(id)` doc section) — the one sort field this
/// module ever sets on a source switch rather than a column-header click.
const PLAYLIST_ORDER_SORT_FIELD: &str = "playlist_order";

/// Pure decision of what `shared.sort` should become when the track list
/// switches to `source`, *before* the switch's reload runs — factored out of
/// `set_source_and_reload` so it can be unit tested without building a live
/// `TrackList` (constructing one requires an initialized GTK display, unlike
/// this plain function).
///
/// `Some(sort)` names the exact default a newly-selected source forces
/// regardless of whatever sort was previously active: today only `Playlist`
/// does this, defaulting to `pt.position` order via the `"playlist_order"`
/// sentinel (see `queries.rs`'s module doc) rather than the general
/// `SortState::default()` (artist/asc) every other source starts from.
///
/// `None` means the new source has no forced default of its own — the
/// caller (`set_source_and_reload`) then only needs to make sure a
/// *previous* source's forced default doesn't linger: the `"playlist_order"`
/// sentinel only resolves to valid SQL inside a query that joins
/// `playlist_tracks` (i.e. only for `ViewSource::Playlist`), so leaving a
/// playlist for any other source must reset it back to `SortState::
/// default()` rather than silently keep sorting by a column expression the
/// new source's query doesn't select.
fn default_sort_for_source(source: &ViewSource) -> Option<SortState> {
    match source {
        ViewSource::Playlist(_) => Some(SortState {
            field: PLAYLIST_ORDER_SORT_FIELD.to_string(),
            dir: "asc".to_string(),
        }),
        ViewSource::Library | ViewSource::Smart(_) | ViewSource::Queue | ViewSource::Missing => {
            None
        }
        ViewSource::ImportErrors => None,
    }
}

/// Pure decision of what the active sort should become when the track list
/// switches from a state currently sorted by `current` to `target` —
/// factored out of `set_source_and_reload` (like `default_sort_for_source`,
/// which it builds on) so the *whole* switch matrix is unit-testable,
/// including the previously-untested leaving-a-playlist reset arm:
///
/// - `target` forces a default of its own (today: `Playlist` →
///   `"playlist_order"`) → that default wins, whatever was active.
/// - `target` forces nothing, but `current` is the `"playlist_order"`
///   sentinel → reset to `SortState::default()`: the sentinel only
///   resolves to valid SQL inside a query that joins `playlist_tracks`,
///   so it must never leak into any other source's query.
/// - otherwise → `current` is kept as-is; a column-header click's sort
///   deliberately survives source switches (matching pre-Stage-3 behavior
///   for Library/Missing/… hops).
fn resolve_sort_on_switch(current: &SortState, target: &ViewSource) -> SortState {
    match default_sort_for_source(target) {
        Some(sort) => sort,
        None if current.field == PLAYLIST_ORDER_SORT_FIELD => SortState::default(),
        None => current.clone(),
    }
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
fn set_source_and_reload(shared: &Rc<Shared>, source: ViewSource) {
    // Hoisted so the `sort` borrow ends before the `borrow_mut` below.
    let new_sort = resolve_sort_on_switch(&shared.sort.borrow(), &source);
    *shared.sort.borrow_mut() = new_sort;
    *shared.source.borrow_mut() = source;
    reload(shared);
}

/// Arms the `REPRISE_SMOKE_FILTER` hook (see `SMOKE_FILTER_ENV_VAR`): one
/// idle callback, deferred so it runs once the main loop is up (matching
/// `arm_smoke_activate`), that applies the env var's value as the search
/// filter via `set_filter_and_reload`.
fn arm_smoke_filter(shared: &Rc<Shared>) {
    let Ok(text) = std::env::var(SMOKE_FILTER_ENV_VAR) else {
        return;
    };
    tracing::info!(filter = %text, "{SMOKE_FILTER_ENV_VAR} set: arming programmatic filter");
    let shared = shared.clone();
    glib::idle_add_local_once(move || {
        set_filter_and_reload(&shared, &text);
    });
}

/// Parses `REPRISE_SMOKE_SOURCE`'s value into a `ViewSource`. `None` for an
/// unrecognized value (caller logs and ignores) rather than silently
/// defaulting to `Library` — a typo in the env var should be visible, not
/// swallowed. Accepts `playlist:<id>`/`smart:<id>` too (Task 4's sidebar is
/// the eventual primary way to reach those, but the query layer and this
/// hook already support them).
fn parse_smoke_source(value: &str) -> Option<ViewSource> {
    match value {
        "library" => Some(ViewSource::Library),
        "missing" => Some(ViewSource::Missing),
        "queue" => Some(ViewSource::Queue),
        "import_errors" => Some(ViewSource::ImportErrors),
        _ => value
            .strip_prefix("playlist:")
            .and_then(|id| id.parse::<i64>().ok())
            .map(ViewSource::Playlist)
            .or_else(|| {
                value
                    .strip_prefix("smart:")
                    .and_then(|id| id.parse::<i64>().ok())
                    .map(ViewSource::Smart)
            }),
    }
}

/// Fallback for `REPRISE_SMOKE_SOURCE=playlist:<name>` (Stage 3 Task 4):
/// playlist ids aren't stable across the scratch databases headless E2E runs
/// seed fresh each time, so once `parse_smoke_source` fails to parse the text
/// after `playlist:` as an id, this looks the playlist up by exact name via
/// `library::playlists::list` instead. Only tried for the `playlist:` prefix
/// — smart playlist ids ARE stable (the three seeds are created once, at
/// migration, never re-created by a test), so `smart:<id>` never needs a
/// name-based fallback. Returns `None` (caller warns and ignores) if the
/// prefix doesn't match, the lookup query fails, or no playlist has that
/// exact name. Names aren't required to be unique (`playlists::create`
/// doesn't enforce it) — if more than one playlist shares `name`, this logs
/// a warning and still picks the first one by `playlists::list`'s `ORDER BY
/// position ASC` (good enough for a headless-only smoke hook, but flagged so
/// duplicate names don't resolve silently and ambiguously).
fn resolve_smoke_source_playlist_by_name(shared: &Rc<Shared>, value: &str) -> Option<ViewSource> {
    let name = value.strip_prefix("playlist:")?;
    let conn = shared.conn.borrow();
    let playlists = crate::library::playlists::list(&conn)
        .inspect_err(|error| {
            tracing::error!(%error, name, "failed to list playlists for smoke-source name lookup");
        })
        .ok()?;
    let mut matches = playlists.into_iter().filter(|p| p.name == name);
    let first = matches.next()?;
    let remaining = matches.count();
    if remaining > 0 {
        tracing::warn!(
            name,
            match_count = remaining + 1,
            "multiple playlists share this name; picking the first by position"
        );
    }
    Some(ViewSource::Playlist(first.id))
}

/// Arms the `REPRISE_SMOKE_SOURCE` hook (see `SMOKE_SOURCE_ENV_VAR`): one
/// idle callback, deferred so it runs once the main loop is up (matching
/// `arm_smoke_activate`/`arm_smoke_filter`), that switches the track list to
/// the parsed `ViewSource` via `set_source_and_reload` and logs the
/// resulting row count. Registered last in `TrackList::new` (after `arm_
/// smoke_activate`), so if both hooks are set together (e.g. verifying
/// `source=queue` after an activation), the queue is already populated by
/// the time this callback runs — GLib dispatches same-priority idle
/// callbacks in the order they were registered.
///
/// Values `parse_smoke_source` can't parse directly (today: only
/// `playlist:<name>`, since ids aren't stable across scratch DBs — see
/// `resolve_smoke_source_playlist_by_name`) fall back to a by-name playlist
/// lookup before giving up.
fn arm_smoke_source(shared: &Rc<Shared>) {
    let Ok(text) = std::env::var(SMOKE_SOURCE_ENV_VAR) else {
        return;
    };
    let shared = shared.clone();
    glib::idle_add_local_once(move || {
        let source = parse_smoke_source(&text)
            .or_else(|| resolve_smoke_source_playlist_by_name(&shared, &text));
        let Some(source) = source else {
            tracing::warn!(
                value = %text,
                "{SMOKE_SOURCE_ENV_VAR} set to an unrecognized value; ignoring"
            );
            return;
        };
        tracing::info!(value = %text, "{SMOKE_SOURCE_ENV_VAR} set: applying programmatic view-source switch");
        set_source_and_reload(&shared, source);
        let label = shared.source.borrow().label();
        // Stage 3 Task 8: the ImportErrors source's rows live in `import_
        // errors_view`, not `shared.model` (which is always empty for this
        // source — see `reload`'s own branch) — mirror that here so this
        // log line reports the real row count instead of a stale 0.
        let rows = if matches!(*shared.source.borrow(), ViewSource::ImportErrors) {
            shared.import_errors_view.refresh() as u32
        } else {
            shared.model.n_items()
        };
        tracing::info!(source = %label, rows, "view source set to {label} ({rows} rows)");
    });
}

/// Arms the `REPRISE_SMOKE_SORT_COLUMN` hook (see `SMOKE_SORT_COLUMN_ENV_
/// VAR`): one idle callback that calls `GtkColumnView::sort_by_column` on
/// the matching column, exactly like a real column-header click. Registered
/// after `arm_smoke_source` (see the arming order in `TrackList::new`) so a
/// prior `REPRISE_SMOKE_SOURCE=playlist:<name>` switch's own forced default
/// sort has already landed before this overrides it. Only `"title"`/
/// `"artist"` are recognized today — the two columns `TrackList::new` already
/// keeps a handle to (`artist_column` for the initial-sort call above); an
/// unrecognized value is logged and ignored rather than silently doing
/// nothing.
fn arm_smoke_sort_column(
    column_view: &gtk4::ColumnView,
    title_column: &gtk4::ColumnViewColumn,
    artist_column: &gtk4::ColumnViewColumn,
) {
    let Ok(field) = std::env::var(SMOKE_SORT_COLUMN_ENV_VAR) else {
        return;
    };
    let column = match field.as_str() {
        "title" => title_column.clone(),
        "artist" => artist_column.clone(),
        _ => {
            tracing::warn!(
                field,
                "{SMOKE_SORT_COLUMN_ENV_VAR} set to an unrecognized column id; ignoring"
            );
            return;
        }
    };
    let column_view = column_view.clone();
    glib::idle_add_local_once(move || {
        tracing::info!(
            field,
            "{SMOKE_SORT_COLUMN_ENV_VAR} set: applying programmatic column sort"
        );
        column_view.sort_by_column(Some(&column), gtk4::SortType::Ascending);
    });
}

/// Re-runs the query against the current source/sort/filter state via
/// `TrackListModel::set_query`. Switches the stack to whichever page
/// `empty_state_for` selects for the resulting row count, filter state, and
/// source.
pub(super) fn reload(shared: &Rc<Shared>) {
    let sort = shared.sort.borrow().clone();
    let filter = shared.filter.borrow().clone();
    let source = shared.source.borrow().clone();
    let has_filter = !filter.trim().is_empty();

    let queue_ids = if matches!(source, ViewSource::Queue) {
        current_queue_ids(shared)
    } else {
        Vec::new()
    };

    shared
        .model
        .set_query(&source, &sort.field, &sort.dir, &filter, &queue_ids);

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
        source = %source.label(),
        "query matched {count} tracks"
    );

    (shared.on_reload)(&source, count, &filter);
}

/// Applies an `EmptyState` decision to the widget tree. For the two empty
/// variants this mutates the single shared `StatusPage`'s title,
/// description, and icon in place before switching the stack to it, rather
/// than maintaining a third stack page — the empty page's layout role
/// (centered icon + title + description, `vexpand`) never changes, only its
/// copy does, so swapping three properties on one widget is simpler than
/// building and switching between two near-identical `StatusPage`s.
fn apply_empty_state(shared: &Rc<Shared>, state: EmptyState) {
    match state {
        EmptyState::List => {
            // Stage 3 Task 8: the ImportErrors source's populated page is the
            // dedicated panel, not the shared `ColumnView` page — every other
            // source keeps using `STACK_PAGE_LIST` exactly as before.
            let page = if matches!(*shared.source.borrow(), ViewSource::ImportErrors) {
                STACK_PAGE_IMPORT_ERRORS
            } else {
                STACK_PAGE_LIST
            };
            shared.stack.set_visible_child_name(page);
        }
        EmptyState::EmptyLibrary => {
            shared.empty_page.set_icon_name(Some(ICON_EMPTY_LIBRARY));
            shared.empty_page.set_title(strings::EMPTY_LIBRARY_TITLE);
            shared
                .empty_page
                .set_description(Some(strings::EMPTY_LIBRARY_DESCRIPTION));
            shared.stack.set_visible_child_name(STACK_PAGE_EMPTY);
        }
        EmptyState::NoResults => {
            shared.empty_page.set_icon_name(Some(ICON_NO_RESULTS));
            shared.empty_page.set_title(strings::NO_RESULTS_TITLE);
            shared
                .empty_page
                .set_description(Some(strings::NO_RESULTS_DESCRIPTION));
            shared.stack.set_visible_child_name(STACK_PAGE_EMPTY);
        }
        EmptyState::NothingHere => {
            shared.empty_page.set_icon_name(Some(ICON_NOTHING_HERE));
            shared.empty_page.set_title(strings::NOTHING_HERE_TITLE);
            shared
                .empty_page
                .set_description(Some(strings::NOTHING_HERE_DESCRIPTION));
            shared.stack.set_visible_child_name(STACK_PAGE_EMPTY);
        }
    }
    // Debug level (not info): this fires on every reload, including every
    // keystroke-debounced search, so it would be noisy at the default log
    // level — but it's exactly what a headless run needs to assert which
    // empty state (if any) is currently shown.
    tracing::debug!(?state, "track list empty-state page selected");
}

#[cfg(test)]
mod empty_state_tests {
    use super::*;

    #[test]
    fn empty_library_when_no_rows_and_no_filter_for_library_source() {
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Library),
            EmptyState::EmptyLibrary
        );
    }

    #[test]
    fn no_results_when_no_rows_and_filter_active_regardless_of_source() {
        assert_eq!(
            empty_state_for(0, true, &ViewSource::Library),
            EmptyState::NoResults
        );
        assert_eq!(
            empty_state_for(0, true, &ViewSource::Missing),
            EmptyState::NoResults
        );
        assert_eq!(
            empty_state_for(0, true, &ViewSource::ImportErrors),
            EmptyState::NoResults
        );
    }

    #[test]
    fn list_when_rows_present_regardless_of_filter_or_source() {
        assert_eq!(
            empty_state_for(3, false, &ViewSource::Library),
            EmptyState::List
        );
        assert_eq!(
            empty_state_for(3, true, &ViewSource::Missing),
            EmptyState::List
        );
    }

    /// Stage 3 Task 3: `Missing`/`ImportErrors` get the neutral "nothing
    /// here" copy for the zero-rows/no-filter case, not `EmptyLibrary`'s
    /// "no music yet" (which would read oddly for "no files are currently
    /// missing").
    #[test]
    fn nothing_here_for_missing_and_import_errors_with_no_rows_and_no_filter() {
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Missing),
            EmptyState::NothingHere
        );
        assert_eq!(
            empty_state_for(0, false, &ViewSource::ImportErrors),
            EmptyState::NothingHere
        );
    }

    /// Non-Library, non-Missing/ImportErrors sources (Playlist, Smart,
    /// Queue) still get `EmptyLibrary`'s copy for now — a dedicated "this
    /// playlist has no tracks yet" message is left to a later stage (Task 4
    /// builds the sidebar that would make such a distinction meaningful).
    #[test]
    fn playlist_smart_and_queue_fall_back_to_empty_library_copy() {
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Playlist(1)),
            EmptyState::EmptyLibrary
        );
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Smart(1)),
            EmptyState::EmptyLibrary
        );
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Queue),
            EmptyState::EmptyLibrary
        );
    }
}

#[cfg(test)]
mod default_sort_for_source_tests {
    use super::*;

    /// CRITICAL fix (review round 1): a `Playlist` source must always
    /// resolve to the `"playlist_order"` sentinel/asc, regardless of the id
    /// it carries — this is what `set_source_and_reload` now applies to
    /// `shared.sort` *before* reloading, which is the missing wiring that
    /// let switching to a playlist silently reload with whatever sort
    /// (usually artist/asc) was already active instead of `pt.position`.
    #[test]
    fn playlist_always_defaults_to_playlist_order_ascending() {
        for id in [1, 2, 42] {
            assert_eq!(
                default_sort_for_source(&ViewSource::Playlist(id)),
                Some(SortState {
                    field: "playlist_order".to_string(),
                    dir: "asc".to_string(),
                })
            );
        }
    }

    /// Every non-Playlist source has no forced default of its own — the
    /// caller (`set_source_and_reload`, via `resolve_sort_on_switch`) is
    /// responsible for resetting away from a lingering playlist sentinel in
    /// this case, not this function.
    #[test]
    fn non_playlist_sources_have_no_forced_default() {
        assert_eq!(default_sort_for_source(&ViewSource::Library), None);
        assert_eq!(default_sort_for_source(&ViewSource::Smart(1)), None);
        assert_eq!(default_sort_for_source(&ViewSource::Queue), None);
        assert_eq!(default_sort_for_source(&ViewSource::Missing), None);
        assert_eq!(default_sort_for_source(&ViewSource::ImportErrors), None);
    }
}

/// The full source-switch sort matrix for `resolve_sort_on_switch` — the
/// exact logic `set_source_and_reload` applies to `shared.sort` before
/// every reload, including the previously-untested leaving-a-playlist
/// reset arm (Stage 3 re-review follow-up).
#[cfg(test)]
mod resolve_sort_on_switch_tests {
    use super::*;

    fn playlist_order_sort() -> SortState {
        SortState {
            field: PLAYLIST_ORDER_SORT_FIELD.to_string(),
            dir: "asc".to_string(),
        }
    }

    fn header_click_sort() -> SortState {
        SortState {
            field: "title".to_string(),
            dir: "desc".to_string(),
        }
    }

    #[test]
    fn library_to_playlist_forces_playlist_order() {
        assert_eq!(
            resolve_sort_on_switch(&SortState::default(), &ViewSource::Playlist(1)),
            playlist_order_sort()
        );
    }

    #[test]
    fn playlist_to_playlist_keeps_forcing_playlist_order() {
        // Switching between two playlists: the sentinel is re-applied (and
        // any header-click override from the first playlist is dropped —
        // the second playlist starts in its own default order).
        assert_eq!(
            resolve_sort_on_switch(&playlist_order_sort(), &ViewSource::Playlist(2)),
            playlist_order_sort()
        );
        assert_eq!(
            resolve_sort_on_switch(&header_click_sort(), &ViewSource::Playlist(2)),
            playlist_order_sort()
        );
    }

    /// The reset arm this module exists to pin down: leaving a playlist
    /// while the `"playlist_order"` sentinel is active must fall back to
    /// the general default — the sentinel only resolves to valid SQL in a
    /// query that joins `playlist_tracks`.
    #[test]
    fn playlist_to_library_resets_sentinel_to_default() {
        assert_eq!(
            resolve_sort_on_switch(&playlist_order_sort(), &ViewSource::Library),
            SortState::default()
        );
    }

    #[test]
    fn playlist_sentinel_resets_for_every_non_playlist_target() {
        for target in [
            ViewSource::Library,
            ViewSource::Smart(1),
            ViewSource::Queue,
            ViewSource::Missing,
            ViewSource::ImportErrors,
        ] {
            assert_eq!(
                resolve_sort_on_switch(&playlist_order_sort(), &target),
                SortState::default(),
                "sentinel must not leak into {target:?}"
            );
        }
    }

    #[test]
    fn library_to_missing_keeps_current_sort() {
        assert_eq!(
            resolve_sort_on_switch(&SortState::default(), &ViewSource::Missing),
            SortState::default()
        );
    }

    /// A column-header click inside a playlist overrides the sentinel; on
    /// leaving the playlist that (non-sentinel) sort survives — only the
    /// sentinel itself is reset, matching pre-Stage-3 behavior where a
    /// header-click sort persisted across Library/Missing/… hops.
    #[test]
    fn header_click_override_survives_leaving_a_playlist() {
        assert_eq!(
            resolve_sort_on_switch(&header_click_sort(), &ViewSource::Library),
            header_click_sort()
        );
        assert_eq!(
            resolve_sort_on_switch(&header_click_sort(), &ViewSource::Queue),
            header_click_sort()
        );
    }
}
