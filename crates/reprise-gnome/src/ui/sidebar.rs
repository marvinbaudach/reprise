//! The navigation sidebar (design mockup 7a): a `gtk::ListBox` (the
//! "navigation-sidebar" GNOME style class) grouped into LIBRARY (Music,
//! Queue — both with a track-count label), PLAYLISTS (`library::playlists::
//! list`, each with its track count, plus a "New playlist" action row), and
//! SMART (`library::playlists::list_smart`, no counts — the mockup doesn't
//! show any), followed by the "problem sources" — Import errors / Missing
//! files — each shown only while its count is non-zero.
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
//! `rebuild`/`show_toast`/`on_tracks_added` being `pub(super)`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use rusqlite::Connection;

use crate::ui::dialogs;
use crate::ui::sidebar_dnd;
use crate::ui::sidebar_export;
use crate::ui::sidebar_playlist_creation;
use crate::ui::sidebar_presentation::{self, NavIcon};
use crate::ui::strings;
use crate::ui::toasts;
use reprise_core::library::playlists;
use reprise_core::queries;
use reprise_core::view_source::ViewSource;

/// One row's identity: the built widget, the `ViewSource` selecting it
/// switches to, and its display title (handed to `Shared::on_select` so
/// `window.rs` can set the headerbar title without re-deriving it).
type RowEntry = (gtk4::ListBoxRow, ViewSource, String);

/// Callback invoked whenever the logically selected source changes — see
/// `Shared::on_select`'s doc comment for the full contract.
type OnSelect = Rc<dyn Fn(ViewSource, String)>;

/// `pub(super)` (visible to `crate::ui` and its descendants, e.g. `ui::
/// sidebar_dnd` — see this file's `## Playlist drop target lives in sidebar_
/// dnd` module-doc section) rather than fully private, same reasoning/shape
/// as `track_list.rs`/`track_list_dnd.rs`. Only the fields that module
/// actually needs are `pub(super)` individually below.
pub(super) struct Shared {
    pub(super) conn: Rc<RefCell<Connection>>,
    pub(super) listbox: gtk4::ListBox,
    /// Supplies the current queue's length for the "Queue" row's counter.
    /// Wired once at construction (mirrors `TrackList`'s `queue_ids_
    /// provider`) to a closure over the `PlayerController`.
    queue_len_provider: Box<dyn Fn() -> usize>,
    /// Which source is logically selected right now — kept in sync by the
    /// `row-selected` handler and used by a routine `rebuild` (`force_select
    /// = None`) to re-select the same source's (rebuilt) row afterwards.
    pub(super) current_source: RefCell<ViewSource>,
    /// Every row built by the most recent `rebuild`, for row-identity lookup
    /// (see the module doc's `## Row identity` section). Rebuilt from
    /// scratch on every `rebuild` call.
    pub(super) rows: RefCell<Vec<RowEntry>>,
    /// The "New playlist" action row, so `wire_row_activated` can tell it
    /// apart from a normal navigation row (identity compare) — it's
    /// `selectable(false)` so it never appears in `rows`/`row-selected`, only
    /// `row-activated`.
    new_playlist_row: RefCell<Option<gtk4::ListBoxRow>>,
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
    on_select: RefCell<Option<OnSelect>>,
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
    on_show_content: RefCell<Option<Rc<dyn Fn()>>>,
    /// Invoked after a drag-and-drop drop onto a playlist row successfully
    /// adds tracks (Stage 3 Task 6) — `window.rs` wires this to `TrackList::
    /// reload` so the track list picks up the new rows immediately in the
    /// (uncommon but real) case where the currently-viewed playlist is the
    /// very one just dropped onto. This sidebar already refreshes its own
    /// counts directly (`rebuild`, called from the drop handler itself) —
    /// this callback is the *other* direction (sidebar mutation -> track
    /// list refresh), the mirror image of `track_list.rs`'s `on_playlist_
    /// mutated` (track list mutation -> sidebar refresh).
    pub(super) on_tracks_added: RefCell<Option<Rc<dyn Fn()>>>,
    /// The window, for the "New playlist" `AlertDialog`'s parent, and (Stage
    /// 3 Task 7) `ui::sidebar_export`'s "Export playlist…" `gtk::FileDialog`
    /// parent — hence `pub(super)`, mirroring `conn`/`on_tracks_added`
    /// above. `WeakRef` so the sidebar can never keep the window alive past
    /// its natural lifetime (same shape as `TrackList::toast_overlay`).
    pub(super) window: glib::WeakRef<adw::ApplicationWindow>,
    /// Injected post-construction once `window.rs` builds it (same seam
    /// shape as `TrackList::toast_overlay`) — surfaces a failed playlist
    /// creation as a toast rather than only a log line.
    toast_overlay: glib::WeakRef<adw::ToastOverlay>,
    /// Counts every `rebuild` call (routine refresh or forced selection
    /// alike), logged alongside each call's `reason` — see `rebuild`'s
    /// tracing line and `Sidebar::refresh`'s doc comment for the trigger
    /// inventory this exists to make visible in headless E2E logs (Stage 3
    /// Task 4 review finding #2: the sidebar was rebuilding on every search
    /// keystroke/sort click before that trigger was narrowed).
    refresh_count: Cell<u64>,
}

/// Handle to the built sidebar widget (a `ScrolledWindow` wrapping the
/// navigation `ListBox`).
pub struct Sidebar {
    shared: Rc<Shared>,
    root: gtk4::ScrolledWindow,
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

        let root = gtk4::ScrolledWindow::builder()
            .child(&listbox)
            .vexpand(true)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .build();

        let shared = Rc::new(Shared {
            conn,
            listbox: listbox.clone(),
            queue_len_provider: Box::new(queue_len_provider),
            current_source: RefCell::new(ViewSource::default()),
            rows: RefCell::new(Vec::new()),
            new_playlist_row: RefCell::new(None),
            on_select: RefCell::new(None),
            on_show_content: RefCell::new(None),
            on_tracks_added: RefCell::new(None),
            window: window.downgrade(),
            toast_overlay: glib::WeakRef::new(),
            refresh_count: Cell::new(0),
        });

        wire_row_selected(&shared);
        wire_row_activated(&shared);

        rebuild(&shared, Some(ViewSource::default()), "initial build");

        Self { shared, root }
    }

    /// The root widget to embed as the `AdwNavigationSplitView`'s sidebar
    /// page content.
    pub fn widget(&self) -> &gtk4::ScrolledWindow {
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

    /// Sets the callback invoked after a drag-and-drop drop onto a playlist
    /// row successfully adds tracks (Stage 3 Task 6) — see `Shared::on_
    /// tracks_added`'s doc comment. `window.rs` wires this to `TrackList::
    /// reload`.
    pub fn set_on_tracks_added(&self, callback: impl Fn() + 'static) {
        *self.shared.on_tracks_added.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the window's toast overlay, once it exists (built after the
    /// sidebar — same post-construction seam as `TrackList::set_toast_
    /// overlay`).
    pub fn set_toast_overlay(&self, overlay: &adw::ToastOverlay) {
        self.shared.toast_overlay.set(Some(overlay));
    }

    /// Drives the same drop-handling sequence `sidebar_dnd::wire_playlist_
    /// drop_target`'s real `connect_drop` closure runs (see `sidebar_dnd::
    /// handle_playlist_drop`'s doc comment) for callers that can't
    /// synthesize a pointer drag. `window.rs` wires this to `TrackList::
    /// set_on_sidebar_playlist_drop`, which `ui::track_list_dnd_smoke`'s
    /// `REPRISE_SMOKE_DND=addplaylist:<name>` hook calls (Stage 3 Task 6
    /// review finding #1). Returns whether anything was actually added.
    pub fn handle_playlist_drop(&self, playlist_id: i64, playlist_name: &str, ids: &[i64]) -> bool {
        sidebar_dnd::handle_playlist_drop(&self.shared, playlist_id, playlist_name, ids)
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
    /// 5. **Queue length mutation** — `queue_transport::wire_sidebar_count`
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

    pub(super) fn restore_source(&self, requested: ViewSource) -> (ViewSource, String) {
        crate::ui::sidebar_session::restore_source(&self.shared, requested)
    }
}

/// Shows `text` as an `adw::Toast`, degrading to a warn log if no overlay is
/// wired or it's gone — mirrors `track_list.rs`/`player_controller.rs`'s
/// `show_toast` (same seam, same degrade behavior).
pub(super) fn show_toast(shared: &Shared, text: &str) {
    match shared.toast_overlay.upgrade() {
        Some(overlay) => toasts::show(&overlay, text),
        None => {
            tracing::warn!(text, "toast overlay is gone; degrading to log-only");
        }
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
pub(super) fn rebuild(shared: &Rc<Shared>, force_select: Option<ViewSource>, reason: &str) {
    let refresh_number = shared.refresh_count.get() + 1;
    shared.refresh_count.set(refresh_number);
    tracing::debug!(
        refresh_number,
        reason,
        "sidebar refresh #{refresh_number} ({reason})"
    );

    // One connection borrow, dropped before any row/selection work below —
    // no GTK/notify call happens while this is alive.
    let (music_count, missing_count, import_error_count, playlist_rows, smart_rows) = {
        let conn = shared.conn.borrow();
        let music_count =
            queries::query_track_count(&conn, &ViewSource::Library, "", &[]).unwrap_or(0);
        let missing_count =
            queries::query_track_count(&conn, &ViewSource::Missing, "", &[]).unwrap_or(0);
        let import_error_count = queries::query_import_error_count(&conn).unwrap_or_else(|error| {
            tracing::error!(%error, "failed to count import errors for sidebar badge");
            0
        });
        let playlist_rows = playlists::list(&conn).unwrap_or_else(|error| {
            tracing::error!(%error, "failed to list playlists for sidebar");
            Vec::new()
        });
        let smart_rows = playlists::list_smart(&conn).unwrap_or_else(|error| {
            tracing::error!(%error, "failed to list smart playlists for sidebar");
            Vec::new()
        });
        (
            music_count,
            missing_count,
            import_error_count,
            playlist_rows,
            smart_rows,
        )
    };
    let queue_count = (shared.queue_len_provider)() as i64;
    let playlist_count = playlist_rows.len();

    shared.listbox.remove_all();
    shared.rows.borrow_mut().clear();
    *shared.new_playlist_row.borrow_mut() = None;

    sidebar_presentation::append_header(
        &shared.listbox,
        &strings::text(strings::SIDEBAR_SECTION_LIBRARY),
    );
    add_row(
        shared,
        ViewSource::Library,
        &strings::text(strings::SIDEBAR_MUSIC),
        Some(music_count),
        NavIcon::Library,
    );
    add_row(
        shared,
        ViewSource::Queue,
        &strings::text(strings::SIDEBAR_QUEUE),
        Some(queue_count),
        NavIcon::Queue,
    );

    sidebar_presentation::append_header(
        &shared.listbox,
        &strings::text(strings::SIDEBAR_SECTION_PLAYLISTS),
    );
    for playlist in &playlist_rows {
        add_row(
            shared,
            ViewSource::Playlist(playlist.id),
            &playlist.name,
            Some(playlist.track_count),
            NavIcon::Playlist,
        );
    }
    let new_playlist_row = sidebar_presentation::append_new_playlist_row(&shared.listbox);
    *shared.new_playlist_row.borrow_mut() = Some(new_playlist_row);

    sidebar_presentation::append_header(
        &shared.listbox,
        &strings::text(strings::SIDEBAR_SECTION_SMART),
    );
    for smart in &smart_rows {
        add_row(
            shared,
            ViewSource::Smart(smart.id),
            &smart.name,
            None,
            sidebar_presentation::smart_icon(&smart.sort_field),
        );
    }

    // Problem sources: no section header in the mockup (unlike LIBRARY/
    // PLAYLISTS/SMART above) — just a separator, and only when at least one
    // of the two has anything to show.
    if import_error_count > 0 || missing_count > 0 {
        let separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
        shared.listbox.append(&separator);
        if import_error_count > 0 {
            add_row(
                shared,
                ViewSource::ImportErrors,
                &strings::text(strings::SIDEBAR_IMPORT_ERRORS),
                Some(import_error_count),
                NavIcon::ImportErrors,
            );
        }
        if missing_count > 0 {
            add_row(
                shared,
                ViewSource::Missing,
                &strings::text(strings::SIDEBAR_MISSING_FILES),
                Some(missing_count),
                NavIcon::Missing,
            );
        }
    }

    tracing::debug!(
        playlists = playlist_count,
        missing = missing_count,
        import_errors = import_error_count,
        "sidebar built: {playlist_count} playlists, missing={missing_count}, import_errors={import_error_count}"
    );

    // Either way this just re-selects a (possibly brand new) `ListBoxRow`
    // object — `wire_row_selected`'s dedup-by-value check (comparing the
    // `ViewSource`, not row identity) is what decides whether that actually
    // notifies `on_select`, not anything done here.
    let requested_source = force_select.unwrap_or_else(|| shared.current_source.borrow().clone());
    let requested_row = find_row(shared, &requested_source);
    let (select_source, fell_back) =
        resolve_select_source(requested_source.clone(), requested_row.is_some());
    if fell_back {
        // The previously (or forced-)selected source's row is gone — e.g. a
        // smart list/playlist that vanished, or a problem-source row whose
        // count just dropped to zero. Leaving nothing selected would strand
        // the user on a source `TrackList` still thinks is current (Stage 3
        // Task 4 review finding #3). `resolve_select_source` already decided
        // Library is the fallback; `requested_source` (not `Library`, since
        // it's the very source that just failed to resolve) compares
        // unequal to `shared.current_source`'s stored value, so `wire_row_
        // selected`'s dedup-by-value guard does NOT suppress the reselect
        // below: it notifies `on_select` like any real switch, which is
        // exactly what's needed to also move `TrackList` off the vanished
        // source.
        tracing::debug!(
            vanished_source = %requested_source.label(),
            "selected source vanished; falling back to Library"
        );
    }
    // Library's row always exists (added unconditionally above), so this is
    // only ever `None` in the non-fallback branch (`requested_row` reused,
    // no second `find_row` scan needed for the common case).
    let row_to_select = if fell_back {
        find_row(shared, &select_source)
    } else {
        requested_row
    };
    if let Some(row) = row_to_select {
        shared.listbox.select_row(Some(&row));
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
pub(super) fn resolve_select_source(requested: ViewSource, row_exists: bool) -> (ViewSource, bool) {
    if row_exists {
        (requested, false)
    } else {
        (ViewSource::Library, true)
    }
}

/// Looks up the row currently backing `source` in `shared.rows` (rebuilt on
/// every `rebuild` call, so this only ever searches the *current* row set).
pub(super) fn find_row(shared: &Rc<Shared>, source: &ViewSource) -> Option<gtk4::ListBoxRow> {
    shared
        .rows
        .borrow()
        .iter()
        .find(|(_, s, _)| s == source)
        .map(|(row, _, _)| row.clone())
}

/// Builds one navigation row (title + optional right-aligned count) and
/// registers it in `shared.rows` against `source`. Playlist rows additionally
/// get a drag-and-drop drop target (Stage 3 Task 6) — see `sidebar_dnd::
/// wire_playlist_drop_target`'s doc comment — and a right-click "Export
/// playlist…" context menu (Stage 3 Task 7) — see `sidebar_export::
/// wire_playlist_context_menu`'s doc comment.
fn add_row(
    shared: &Rc<Shared>,
    source: ViewSource,
    title: &str,
    count: Option<i64>,
    icon: NavIcon,
) {
    let row = sidebar_presentation::build_nav_row(title, count, icon);
    if let ViewSource::Playlist(playlist_id) = source {
        sidebar_dnd::wire_playlist_drop_target(shared, &row, playlist_id, title);
        sidebar_export::wire_playlist_context_menu(shared, &row, playlist_id, title);
    }
    shared.listbox.append(&row);
    shared
        .rows
        .borrow_mut()
        .push((row, source, title.to_string()));
}

/// Wires the `ListBox`'s `row-selected` signal: a navigation row becoming
/// selected updates `shared.current_source` and notifies `on_select` — but
/// only when the newly selected row's `ViewSource` actually *differs* from
/// `shared.current_source`'s current value. This is a value comparison, not
/// a time-windowed suppress flag, because `rebuild` tears down and rebuilds
/// every row on every refresh: a routine refresh's silent re-selection is
/// still selecting a brand new `ListBoxRow` GObject (row identity always
/// changes), so only comparing the *logical* source can tell "nothing
/// actually changed" apart from a real switch. `row` is `None` for a
/// deselection, including the one `rebuild`'s `remove_all` causes when it
/// clears out the previously selected row — see the module doc's
/// `## Reentrancy` section.
fn wire_row_selected(shared: &Rc<Shared>) {
    let listbox = shared.listbox.clone();
    let shared = shared.clone();
    listbox.connect_row_selected(move |_, row| {
        let Some(row) = row else {
            return;
        };
        let matched = shared
            .rows
            .borrow()
            .iter()
            .find(|(r, _, _)| r == row)
            .map(|(_, source, title)| (source.clone(), title.clone()));
        let Some((source, title)) = matched else {
            // Selecting the "New playlist" row (or a header) can't happen —
            // both are `selectable(false)` — so this would only fire for a
            // genuine bug in row bookkeeping; warn rather than panic.
            tracing::warn!("sidebar: selected row not found in row map; ignoring");
            return;
        };
        if *shared.current_source.borrow() == source {
            // Same logical source as before (a routine refresh's silent
            // re-select, or re-selecting the row that's already active) —
            // nothing to notify.
            return;
        }
        tracing::debug!(source = %source.label(), "sidebar: row selected");
        *shared.current_source.borrow_mut() = source.clone();
        // Hoisted clone-out before calling, per this project's `RefCell`
        // callback discipline (see the module doc's `## Reentrancy`
        // section): `on_select` can synchronously trigger a `rebuild` that
        // touches every field on `shared`, including this same `RefCell`.
        let callback = shared.on_select.borrow().clone();
        if let Some(callback) = callback {
            callback(source, title);
        }
    });
}

/// Wires the `ListBox`'s `row-activated` signal. Every navigation row is
/// both selectable *and* activatable (GTK's default), so a click on one
/// fires this alongside `row-selected` — but `row-selected` only notifies on
/// an actual source change (see `wire_row_selected`'s dedup-by-value check),
/// so re-activating the row that's already selected (re-tapping it after
/// backing out to the sidebar in collapsed mode, or pressing Enter on it)
/// fires `row-activated` alone. Stage 3 Task 4 review finding #1: that case
/// needs to bring the content page forward too, so every navigation row
/// (found in `shared.rows`) invokes `on_show_content` here unconditionally —
/// cheap and idempotent (`window.rs`'s callback only flips `show-content`
/// when the split view is collapsed), so firing it redundantly alongside a
/// real `on_select`-driven switch is harmless. The "New playlist" row (non-
/// selectable, so it never appears in `shared.rows`) is handled separately:
/// it opens the dialog instead.
fn wire_row_activated(shared: &Rc<Shared>) {
    let listbox = shared.listbox.clone();
    let shared = shared.clone();
    listbox.connect_row_activated(move |_, row| {
        let is_new_playlist_row = shared.new_playlist_row.borrow().as_ref() == Some(row);
        if is_new_playlist_row {
            show_new_playlist_dialog(&shared);
            return;
        }
        let is_nav_row = shared.rows.borrow().iter().any(|(r, _, _)| r == row);
        if is_nav_row {
            // Hoisted clone-out before calling, per this project's `RefCell`
            // callback discipline (same reasoning as `wire_row_selected`'s
            // `on_select` clone-out just above).
            let callback = shared.on_show_content.borrow().clone();
            if let Some(callback) = callback {
                callback();
            }
        }
    });
}

/// Shows the "New playlist" `AdwAlertDialog`: a heading, an entry (Create
/// disabled until non-blank), and Cancel/Create responses. On Create, it
/// creates the playlist while keeping the current source visible.
fn show_new_playlist_dialog(shared: &Rc<Shared>) {
    let Some(window) = shared.window.upgrade() else {
        tracing::warn!("sidebar: window is gone; cannot show new-playlist dialog");
        return;
    };

    let shared = shared.clone();
    dialogs::prompt_name(
        &window,
        &strings::text(strings::NEW_PLAYLIST_DIALOG_HEADING),
        &strings::text(strings::NEW_PLAYLIST_ENTRY_PLACEHOLDER),
        &strings::text(strings::CREATE),
        move |name| create_playlist_and_stay(&shared, &name),
    );
}

/// Creates a playlist named `name` and refreshes the sidebar without leaving
/// the current source. A creation failure is logged and surfaced as a toast.
fn create_playlist_and_stay(shared: &Rc<Shared>, name: &str) {
    let created = {
        let conn = shared.conn.borrow();
        playlists::create(&conn, name)
    };
    match created {
        Ok(id) => {
            tracing::info!(id, name, "playlist created");
            rebuild(
                shared,
                sidebar_playlist_creation::refresh_target_after_empty_creation(),
                "playlist created",
            );
        }
        Err(error) => {
            tracing::error!(%error, name, "failed to create playlist");
            show_toast(shared, &strings::playlist_create_failed_toast(name));
        }
    }
}

#[cfg(test)]
mod resolve_select_source_tests {
    use super::*;

    #[test]
    fn keeps_requested_source_when_its_row_still_exists() {
        let (source, fell_back) = resolve_select_source(ViewSource::Playlist(3), true);
        assert_eq!(source, ViewSource::Playlist(3));
        assert!(!fell_back);
    }

    #[test]
    fn falls_back_to_library_when_requested_row_is_gone() {
        let (source, fell_back) = resolve_select_source(ViewSource::Missing, false);
        assert_eq!(source, ViewSource::Library);
        assert!(fell_back);
    }

    #[test]
    fn falls_back_to_library_when_a_smart_list_vanished() {
        let (source, fell_back) = resolve_select_source(ViewSource::Smart(7), false);
        assert_eq!(source, ViewSource::Library);
        assert!(fell_back);
    }

    #[test]
    fn restored_source_reuses_the_vanished_source_fallback() {
        assert_eq!(
            resolve_select_source(ViewSource::Playlist(99), false).0,
            ViewSource::Library
        );
        assert_eq!(
            resolve_select_source(ViewSource::Queue, true).0,
            ViewSource::Queue
        );
    }
}
