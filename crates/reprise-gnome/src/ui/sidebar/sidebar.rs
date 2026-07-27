//! The navigation sidebar (design mockup 7a): a `gtk::ListBox` (the
//! "navigation-sidebar" GNOME style class) grouped into LIBRARY (Music,
//! Queue — both with a track-count label), PLAYLISTS (`library::playlists::
//! list`, each with its track count, plus grouped create/import actions), and
//! SMART (`library::playlists::list_smart`, no counts — the mockup doesn't
//! show any), persistent connected-device state, then one bottom-pinned region
//! that shows either active progress cards or the complete Issues surface
//! (heading plus Import errors / Missing files). Progress temporarily replaces
//! that surface instead of pushing it upward (FB-8).
//!
//! ## Row identity: a plain `Vec`, not GObject data
//!
//! Rows are plain `gtk::ListBoxRow`s built directly (no `ListView`/factory —
//! at this scale, a handful of static/rebuildable rows per the task brief, a
//! `SignalListItemFactory` would be pure overhead). Rather than stash each
//! row's `ViewSource` as GObject qdata (`ObjectExt::set_data`, `unsafe` in
//! this glib version), `Shared::rows` keeps a plain `Vec<(ListBoxRow,
//! ViewSource, String)>` (row, source, display title) built fresh by every
//! `rebuild`; row identity is compared via `PartialEq` on the `ListBoxRow`
//! wrapper itself (glib object wrappers compare the underlying GObject
//! pointer), which is exactly what `connect_row_selected`/`connect_row_
//! activated` hand back.
//!
//! ## Rebuild-on-refresh, not incremental updates
//!
//! `rebuild` tears down every row and rebuilds the whole list from a fresh
//! set of queries every time counts might have changed (after a scan, after
//! playlist CRUD — see `refresh`/`create_playlist_and_stay`). This is
//! simpler than diffing the previous row set against new data and is cheap
//! enough at this scale (a handful of playlists/smart lists). The previously
//! selected source is re-selected afterwards (see `rebuild`'s `force_select`
//! parameter) so a routine counts refresh never silently changes what's on
//! screen.
//!
//! ## Reentrancy
//!
//! `rebuild` tears down every row and re-selects the logical current source.
//! If a future caller feeds that selection back into another rebuild,
//! `wire_row_selected`'s dedup-by-value check (comparing the
//! newly selected row's `ViewSource` against `shared.current_source`'s
//! already-stored value, not row identity — a fresh `rebuild` always
//! produces a *different* `ListBoxRow` GObject even for "the same" source)
//! is what stops that from looping forever. Every `RefCell` borrow in this
//! module is also scoped to end before any call that could re-enter (the
//! pattern documented project-wide, e.g. `player_controller.rs`'s "Queue
//! borrow discipline" section), so no such reentrant chain ever overlaps two
//! borrows of the same `RefCell` either.
//!
//! The playlist row's drop target/drop-handling logic lives in the sibling
//! `ui::sidebar_dnd` module (split out to keep this file under 800 lines,
//! mirroring `track_list.rs`/`track_list_dnd.rs`) — hence `Shared`/`conn`/
//! `rebuild`/`show_toast`/`on_tracks_added` being `pub(in crate::ui)`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use rusqlite::Connection;

use super::sidebar_activity_slot::SidebarActivitySlot;
use super::sidebar_boundary_navigation::wire_collection_boundary_navigation;
#[cfg(test)]
use super::sidebar_issues_section::{
    bottom_region_placement, issues_surface_for_progress, IssuesSurface,
};
use super::sidebar_navigation_scroller::build_navigation_scroller;
use super::sidebar_root::build_root;
#[cfg(test)]
use super::sidebar_root::{sidebar_root_order, SidebarRootChild};
use reprise_core::view_source::ViewSource;

/// One row's identity: the built widget, the `ViewSource` selecting it
/// switches to, and its display title (handed to `Shared::on_select` so
/// `window.rs` can set the headerbar title without re-deriving it).
pub(in crate::ui) type RowEntry = (gtk4::ListBoxRow, ViewSource, String);

/// Callback invoked whenever the logically selected source changes — see
/// `Shared::on_select`'s doc comment for the full contract.
type OnSelect = Rc<dyn Fn(ViewSource, String)>;
pub(in crate::ui) type OnRemoveMissing = Rc<dyn Fn(&[i64])>;
use super::sidebar_dnd::{OnConversionDrop, OnQueueDrop};
use super::sidebar_row_wiring::{wire_focus_leave_resync, wire_row_activated, wire_row_selected};

/// `pub(in crate::ui)` (visible to `crate::ui` and its descendants, e.g. `ui::
/// sidebar_dnd` — see this file's `## Playlist drop target lives in sidebar_
/// dnd` module-doc section) rather than fully private, same reasoning/shape
/// as `track_list.rs`/`track_list_dnd.rs`. Only the fields that module
/// actually needs are `pub(in crate::ui)` individually below.
pub(in crate::ui) struct Shared {
    pub(in crate::ui) conn: Rc<RefCell<Connection>>,
    pub(in crate::ui) listbox: gtk4::ListBox,
    /// The non-scrolling issue-source list (Import errors / Missing files),
    /// pinned at the very bottom of the Issues section below its shared
    /// activity slot (design mockup 14a; QA #6). A single `ListBox` can't
    /// bottom-pin a subset of the main navigation rows, so this is its own
    /// list, with selection mirrored against `listbox` (`wire_row_selected`
    /// clears the sibling on select). Hidden entirely when there are no issue
    /// sources; the separate Issues heading mirrors that visibility.
    pub(in crate::ui) issues_listbox: gtk4::ListBox,
    /// Supplies the current queue's length for the "Queue" row's counter.
    /// Wired once at construction (mirrors `TrackList`'s `queue_ids_
    /// provider`) to a closure over the `PlayerController`.
    pub(in crate::ui) queue_len_provider: Box<dyn Fn() -> usize>,
    /// Which source is logically selected right now — kept in sync by the
    /// `row-selected` handler and used by a routine `rebuild` (`force_select
    /// = None`) to re-select the same source's (rebuilt) row afterwards.
    pub(in crate::ui) current_source: RefCell<ViewSource>,
    /// Every row built by the most recent `rebuild`, for row-identity lookup
    /// (see the module doc's `## Row identity` section). Rebuilt from
    /// scratch on every `rebuild` call.
    pub(in crate::ui) rows: RefCell<Vec<RowEntry>>,
    /// The "New playlist" action row, so `wire_row_activated` can tell it
    /// apart from a normal navigation row (identity compare) — it's
    /// `selectable(false)` so it never appears in `rows`/`row-selected`, only
    /// `row-activated`.
    pub(in crate::ui) new_playlist_row: RefCell<Option<gtk4::ListBoxRow>>,
    /// The adjacent "Import playlist…" action row. Kept separately so row
    /// activation can invoke the file-dialog callback wired by `window.rs`.
    pub(in crate::ui) import_playlist_row: RefCell<Option<gtk4::ListBoxRow>>,
    /// Invoked whenever the logically selected source *changes* (real user
    /// click or an explicit forced selection) — never
    /// for a same-source reselect (see `wire_row_selected`'s dedup-by-value
    /// check, not a time-windowed suppress flag: `rebuild` tears down and
    /// rebuilds every row on every refresh, so "the same row" is a new
    /// `ListBoxRow` GObject even when it denotes the same `ViewSource` — a
    /// reselect after a routine counts refresh always re-fires `row-
    /// selected` on that new object, and only a source-equality check, not
    /// widget identity, can tell that apart from an actual change). `Rc<dyn
    /// Fn>`, not `Box`, per the project's `RefCell` callback-storage
    /// convention (cheap to clone out before calling — see the module doc's
    /// `## Reentrancy` section).
    pub(in crate::ui) on_select: RefCell<Option<OnSelect>>,
    /// Invoked whenever a navigation row is *activated* (click/Enter) —
    /// including re-activating the row that's already selected, which fires
    /// `row-activated` but **not** `row-selected` again (see `wire_row_
    /// selected`'s dedup-by-value check). `on_select` alone can't cover that
    /// case: a user who backed out to the sidebar in collapsed mode without
    /// picking a *different* source needs tapping the same row again to
    /// bring the content page back, and `on_select` only fires on an actual
    /// source change. `window.rs` wires this to the same "show content if
    /// collapsed" logic it feeds `on_select`'s callback (see `Sidebar::set_
    /// on_show_content`'s doc comment) — never to a source switch or reload,
    /// so re-tapping the current row is free of any query cost.
    pub(in crate::ui) on_show_content: RefCell<Option<Rc<dyn Fn()>>>,
    /// Opens the M3U import picker from the Playlist section. The dialog
    /// implementation remains in `playlist_io`; the sidebar only owns the
    /// action's placement and activation.
    pub(in crate::ui) on_import_playlist: RefCell<Option<Rc<dyn Fn()>>>,
    /// Invoked after a drag-and-drop drop onto a playlist row successfully
    /// adds tracks (Stage 3 Task 6) — `window.rs` wires this to `TrackList::
    /// reload` so the track list picks up the new rows immediately in the
    /// (uncommon but real) case where the currently-viewed playlist is the
    /// very one just dropped onto. This sidebar already refreshes its own
    /// counts directly (`rebuild`, called from the drop handler itself) —
    /// this callback is the *other* direction (sidebar mutation -> track
    /// list refresh), the mirror image of `track_list.rs`'s `on_playlist_
    /// mutated` (track list mutation -> sidebar refresh).
    pub(in crate::ui) on_tracks_added: RefCell<Option<Rc<dyn Fn()>>>,
    /// Routes the sidebar's Missing-files bulk action into the track list's
    /// shared tombstone/Undo service. The callback receives the exact live
    /// missing ids and is cloned out before invocation for reentrancy safety.
    pub(in crate::ui) on_remove_missing: RefCell<Option<OnRemoveMissing>>,
    /// Invoked with the dragged track ids when they're dropped onto the
    /// Queue nav row (the drag-and-drop analogue of the context menu's "Add
    /// to queue" — the Queue must be fillable by drag exactly like a
    /// playlist row is). `window.rs` wires this to `PlayerController::
    /// append_to_queue`; returns whether anything was actually appended
    /// (`false` when no player is available), mirroring `track_list.rs`'s
    /// `on_queue_reorder` degraded-no-op convention. No sidebar `rebuild`/
    /// track-list reload runs from the drop handler itself: `append_to_
    /// queue` already funnels through `PlayerController::notify_queue_
    /// changed`, whose `window.rs` wiring refreshes this sidebar's Queue
    /// count *and* reloads the Queue view if visible (trigger inventory
    /// item 6 in `Sidebar::refresh`'s doc comment).
    pub(in crate::ui) on_queue_drop: RefCell<Option<OnQueueDrop>>,
    /// Enqueues a dragged selection as one instrumental batch when dropped on
    /// the gated Conversions row.
    pub(in crate::ui) on_conversion_drop: RefCell<Option<OnConversionDrop>>,
    /// The window, for the "New playlist" dialog and `ui::sidebar_export`'s
    /// export dialog plus playlist-delete confirmation — hence `pub(in crate::ui)`,
    /// mirroring `conn`/`on_tracks_added`
    /// above. `WeakRef` so the sidebar can never keep the window alive past
    /// its natural lifetime (same shape as `TrackList::toast_overlay`).
    pub(in crate::ui) window: glib::WeakRef<adw::ApplicationWindow>,
    /// Injected post-construction once `window.rs` builds it (same seam
    /// shape as `TrackList::toast_overlay`) — surfaces a failed playlist
    /// creation as a toast rather than only a log line.
    pub(in crate::ui) toast_overlay: glib::WeakRef<adw::ToastOverlay>,
    /// Counts every `rebuild` call (routine refresh or forced selection
    /// alike), logged alongside each call's `reason` — see `rebuild`'s
    /// tracing line and `Sidebar::refresh`'s doc comment for the trigger
    /// inventory this exists to make visible in headless E2E logs (Stage 3
    /// Task 4 review finding #2: the sidebar was rebuilding on every search
    /// keystroke/sort click before that trigger was narrowed).
    pub(in crate::ui) refresh_count: Cell<u64>,
}

/// Handle to the built sidebar widget: scrolling navigation, then the
/// bottom-pinned region (either Issues or active scan progress).
pub struct Sidebar {
    pub(in crate::ui) shared: Rc<Shared>,
    root: gtk4::Box,
    pub(super) activity_slot: SidebarActivitySlot,
}

impl Sidebar {
    /// Builds the sidebar and performs its initial row build, selecting
    /// `ViewSource::default()` (`Library`) — matching `TrackList::new`'s own
    /// default initial source, so the two start in agreement without a
    /// round trip through `on_select` (not yet wired at this point; see
    /// `set_on_select`'s doc comment).
    pub fn new(
        conn: Rc<RefCell<Connection>>,
        window: &adw::ApplicationWindow,
        queue_len_provider: impl Fn() -> usize + 'static,
    ) -> Self {
        let listbox = gtk4::ListBox::new();
        listbox.add_css_class("navigation-sidebar");
        listbox.set_selection_mode(gtk4::SelectionMode::Single);

        let issues_listbox = gtk4::ListBox::new();
        configure_issues_listbox(&issues_listbox);
        // Natural height, anchored at the bottom, hidden until `rebuild` finds
        // issues to show.
        issues_listbox.set_visible(false);

        let scrolled = build_navigation_scroller(&listbox);

        let activity_slot = SidebarActivitySlot::new();
        let root = build_root(&scrolled, &activity_slot, &issues_listbox);

        let shared = Rc::new(Shared {
            conn,
            listbox: listbox.clone(),
            issues_listbox: issues_listbox.clone(),
            queue_len_provider: Box::new(queue_len_provider),
            current_source: RefCell::new(ViewSource::default()),
            rows: RefCell::new(Vec::new()),
            new_playlist_row: RefCell::new(None),
            import_playlist_row: RefCell::new(None),
            on_select: RefCell::new(None),
            on_show_content: RefCell::new(None),
            on_import_playlist: RefCell::new(None),
            on_tracks_added: RefCell::new(None),
            on_remove_missing: RefCell::new(None),
            on_queue_drop: RefCell::new(None),
            on_conversion_drop: RefCell::new(None),
            window: window.downgrade(),
            toast_overlay: glib::WeakRef::new(),
            refresh_count: Cell::new(0),
        });

        wire_row_selected(&shared);
        wire_row_activated(&shared);
        wire_focus_leave_resync(&shared);
        wire_collection_boundary_navigation(&shared);

        rebuild(&shared, Some(ViewSource::default()), "initial build");

        Self {
            shared,
            root,
            activity_slot,
        }
    }

    /// The root widget to embed as the main `AdwOverlaySplitView`'s sidebar
    /// page content.
    pub fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    /// Sets the callback invoked whenever the selected source changes.
    /// `window.rs` wires this once, after `TrackList` exists, to
    /// switch its source and update the headerbar title.
    ///
    /// This can't run the callback for the sidebar's own initial selection
    /// (made during `Sidebar::new`, before this method has ever been
    /// called): `TrackList::new` already defaults to `ViewSource::Library`
    /// itself, so no round trip is needed for the track list to agree; only
    /// the headerbar title needs a nudge, which `window.rs` gives directly
    /// (it already knows the initial source is `Library`) rather than
    /// through this seam.
    pub fn set_on_select(&self, callback: impl Fn(ViewSource, String) + 'static) {
        *self.shared.on_select.borrow_mut() = Some(Rc::new(callback));
    }

    /// Sets the callback invoked whenever a navigation row is *activated*,
    /// whether or not that changes the selected source — see `Shared::on_
    /// show_content`'s doc comment for why this is a separate seam from
    /// `set_on_select`. `window.rs` wires both to the same "bring the
    /// content page forward if the split view is collapsed" closure.
    pub fn set_on_show_content(&self, callback: impl Fn() + 'static) {
        *self.shared.on_show_content.borrow_mut() = Some(Rc::new(callback));
    }

    /// Sets the callback invoked by the Playlist section's import action.
    pub fn set_on_import_playlist(&self, callback: impl Fn() + 'static) {
        *self.shared.on_import_playlist.borrow_mut() = Some(Rc::new(callback));
    }

    /// Sets the callback invoked after a drag-and-drop drop onto a playlist
    /// row successfully adds tracks (Stage 3 Task 6) — see `Shared::on_
    /// tracks_added`'s doc comment. `window.rs` wires this to `TrackList::
    /// reload`.
    pub fn set_on_tracks_added(&self, callback: impl Fn() + 'static) {
        *self.shared.on_tracks_added.borrow_mut() = Some(Rc::new(callback));
    }

    /// Routes Missing-files bulk cleanup through the shared tombstone/Undo
    /// service owned by the track list.
    pub fn set_on_remove_missing(&self, callback: impl Fn(&[i64]) + 'static) {
        *self.shared.on_remove_missing.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the window's toast overlay, once it exists (built after the
    /// sidebar — same post-construction seam as `TrackList::set_toast_
    /// overlay`).
    pub fn set_toast_overlay(&self, overlay: &adw::ToastOverlay) {
        self.shared.toast_overlay.set(Some(overlay));
    }

    /// Re-runs every count/list query and rebuilds the row set, preserving
    /// whichever source is currently selected. `reason` is a short,
    /// human-readable label logged alongside the rebuild counter (see
    /// `rebuild`'s tracing line) so headless E2E runs can confirm exactly
    /// which triggers actually fired.
    ///
    /// ## Trigger inventory (Stage 3 Task 4 review finding #2)
    ///
    /// This must be called ONLY when something the sidebar displays (counts,
    /// playlists, smart lists, badges) can actually have changed — never on
    /// a routine `TrackList` reload (search-filter debounce, sort-header
    /// click, plain source switch), which is why this is *not* wired into
    /// `TrackList`'s generic `on_reload` hook. The known call sites:
    ///
    /// 1. **Scan completion** — `window.rs`'s `spawn_scan` success arm, after
    ///    `track_list.reload()`: a scan can add tracks/playlists and clear
    ///    import-error/missing counts.
    /// 2. **Playlist CRUD** — `create_playlist_and_stay` in this file calls
    ///    `rebuild` directly while preserving the current source.
    /// 3. **Missing-marking** — `window.rs`'s `player.set_track_list_reload`
    ///    closure (the seam `playback_faults.rs`'s `handle_unplayable_track`
    ///    calls through `PlayerController::reload_track_list` after a
    ///    successful `mark_track_missing`) — the only thing that can flip the
    ///    Missing badge outside of a scan.
    /// 4. **Context-menu playlist mutation** (Stage 3 Task 5) —
    ///    `window.rs`'s `track_list.set_on_playlist_mutated` closure, called
    ///    by `track_list.rs`'s `notify_playlist_mutated` after "Add to
    ///    playlist" (existing or newly created), and "Remove from playlist"
    ///    each succeed — playlist track counts (and, for a new playlist, the
    ///    playlist row itself) can change from that menu exactly as they can
    ///    from this sidebar's own "New playlist" dialog.
    /// 5. **Issue view opened** — `view_session::record_issue_viewed` writes
    ///    the relevant timestamp, then refreshes so its new-since-viewed
    ///    badge clears immediately.
    /// 6. **Queue length mutation** — `queue_transport::wire_sidebar_count`
    ///    refreshes after a queue is replaced, appended, restored, or purged.
    ///
    /// See the module doc's `## Reentrancy` section for why a rebuild
    /// triggered by this very sidebar's own selection is still safe to feed
    /// back into it.
    pub fn refresh(&self, reason: &str) {
        rebuild(&self.shared, None, reason);
    }

    /// Rebuilds counts and selects `source` through the normal row-selected
    /// callback, keeping sidebar highlight, track list, title, and adaptive
    /// navigation synchronized. Used after importing a populated playlist.
    pub fn refresh_and_select(&self, source: ViewSource, reason: &str) {
        rebuild(&self.shared, Some(source), reason);
    }

    pub(in crate::ui) fn restore_source(&self, requested: ViewSource) -> (ViewSource, String) {
        crate::ui::sidebar_session::restore_source(&self.shared, requested)
    }

    /// Shows connected devices below the navigation rows and routes card
    /// activation through the existing source-selection callback.
    pub fn bind_device_sync(
        &self,
        runtime: &Rc<crate::ui::device_sync_runtime::DeviceSyncRuntime>,
    ) {
        let window = self.shared.window.clone();
        let runtime_for_open = runtime.clone();
        let section = super::sidebar_device_card::bind(
            runtime,
            Rc::new(move |serial, _| {
                let Some(window) = window.upgrade() else {
                    return;
                };
                if crate::ui::device_sync::device_sync_dialog::present(
                    &window,
                    &serial,
                    &runtime_for_open,
                )
                .is_none()
                {
                    tracing::warn!(device_id = serial, "could not open Android sync dialog");
                }
            }),
        );
        self.activity_slot.set_device_section(&section);
    }
}

fn configure_issues_listbox(listbox: &gtk4::ListBox) {
    listbox.add_css_class("navigation-sidebar");
    listbox.set_selection_mode(gtk4::SelectionMode::Single);
    // Selection remains exclusive with the main navigation list, so this
    // collection often has no selected row for GTK to use as its tab entry.
    // Keep the container itself in the focus chain; arrow navigation then
    // moves to its first selectable issue row.
    // a11y-semantics: role=list name=issues state=focusable action=arrow-navigation
    listbox.set_focusable(true);
}

pub(in crate::ui) fn remember_issue_focus_entry(listbox: &gtk4::ListBox, row: &gtk4::ListBoxRow) {
    if listbox.focus_child().is_none() {
        listbox.set_focus_child(Some(row));
    }
}

/// Re-runs the count/list queries and rebuilds every row. `force_select`
/// decides what happens to selection afterwards:
///
/// - `Some(source)`: select that source's row (if found) and let the normal
///   notify path run — used for the initial build (`Sidebar::new`, `on_
///   select` not wired yet, so this is a no-op notify) and after creating a
///   playlist (switch straight to it).
/// - `None`: silently re-select whatever `shared.current_source` already
///   was, suppressing `on_select` — a routine counts refresh must never
///   change what's on screen or trigger a redundant reload. If that source's
///   row no longer exists (e.g. a smart list emptied out, or the Missing/
///   Import-errors row disappeared because its count hit zero), falls back
///   to selecting `Library` instead of leaving nothing selected — see the
///   fallback logic below.
///
/// `reason` is a short human-readable label for the `"sidebar refresh #N
/// (reason)"` debug log (`shared.refresh_count`) — see `Sidebar::refresh`'s
/// doc comment for the full trigger inventory this makes verifiable in
/// headless E2E output.
pub(in crate::ui) fn rebuild(shared: &Rc<Shared>, force_select: Option<ViewSource>, reason: &str) {
    crate::ui::sidebar_rebuild::rebuild(shared, force_select, reason);
}

/// Selects `row` in whichever of the two nav lists actually contains it (the
/// main scrolling list or the bottom-pinned issues list), so selection-follow
/// works regardless of which list a source lives in. Its `row-selected`
/// handler then clears the sibling list, keeping a single visible selection.
pub(in crate::ui) fn select_row_in_its_listbox(row: &gtk4::ListBoxRow) {
    if let Some(listbox) = row
        .parent()
        .and_then(|p| p.downcast::<gtk4::ListBox>().ok())
    {
        listbox.select_row(Some(row));
    }
}

/// Pure decision behind the vanished-source fallback (Stage 3 Task 4 review
/// finding #3): given the source `rebuild` would like to (re)select and
/// whether a row for it still exists, decides what to actually select.
/// Returns `(source_to_select, fell_back)`, where `fell_back` is `true` when
/// `requested` no longer has a row and `Library` was substituted instead.
/// Kept free of `Shared`/GTK so it's unit-testable without a live `ListBox`
/// (see the `resolve_select_source_tests` module at the end of this file —
/// grouped there, not right below this function, per `clippy::items_after_
/// test_module`).
pub(in crate::ui) fn resolve_select_source(
    requested: ViewSource,
    row_exists: bool,
) -> (ViewSource, bool) {
    if row_exists {
        (requested, false)
    } else {
        (ViewSource::Library, true)
    }
}

/// Looks up the row currently backing `source` in `shared.rows` (rebuilt on
/// every `rebuild` call, so this only ever searches the *current* row set).
pub(in crate::ui) fn find_row(
    shared: &Rc<Shared>,
    source: &ViewSource,
) -> Option<gtk4::ListBoxRow> {
    shared
        .rows
        .borrow()
        .iter()
        .find(|(_, s, _)| s == source)
        .map(|(row, _, _)| row.clone())
}

#[cfg(test)]
#[path = "sidebar_tests.rs"]
mod resolve_select_source_tests;

#[cfg(test)]
#[path = "sidebar_layout_tests.rs"]
mod sidebar_layout_tests;
