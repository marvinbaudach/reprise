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
//! playlist CRUD — see `refresh`/`create_playlist_and_select`). This is
//! simpler than diffing the previous row set against new data and is cheap
//! enough at this scale (a handful of playlists/smart lists). The previously
//! selected source is re-selected afterwards (see `rebuild`'s `force_select`
//! parameter) so a routine counts refresh never silently changes what's on
//! screen.
//!
//! ## Reentrancy: selecting a row can, deep in the call stack, rebuild this
//! same sidebar
//!
//! Selecting a row invokes `Shared::on_select`, which (via `window.rs`)
//! calls `TrackList::set_source` → `reload()` → `TrackList`'s `on_reload`
//! hook → `Sidebar::refresh()` → `rebuild()` — all synchronously, before the
//! original `row-selected` signal handler returns. `rebuild` clearing the
//! `ListBox` deselects the just-selected row (a `None` emission, ignored by
//! `wire_row_selected`'s early return) and then re-selects the same logical
//! source on a freshly built row — a *different* `ListBoxRow` GObject, so
//! `row-selected` fires again for it. Without care this would loop forever:
//! each notify triggers another `rebuild`, which re-selects and re-notifies.
//! `wire_row_selected` breaks the cycle by comparing the newly selected
//! row's `ViewSource` against `shared.current_source`'s already-stored value
//! — equal means "nothing actually changed", so the recursion bottoms out
//! after exactly one nested `rebuild` every time, whether the underlying GTK
//! selection signal happens to fire synchronously or gets deferred to a
//! later main-loop turn. Every `RefCell` borrow in this module is also
//! scoped to end before any call that could re-enter (the pattern
//! documented project-wide, e.g. `player_controller.rs`'s "Queue borrow
//! discipline" section), so this reentrant chain never overlaps two borrows
//! of the same `RefCell` either.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use rusqlite::Connection;

use crate::format::format_thousands;
use crate::library::playlists;
use crate::queries;
use crate::ui::strings;
use crate::view_source::ViewSource;

/// `AdwAlertDialog` response id for the "New playlist" dialog's Cancel
/// action. Not user-facing text (see `add_response`'s separate `label`
/// argument for the button's actual copy) — an internal identifier, so it
/// lives here rather than in `strings.rs`.
const RESPONSE_CANCEL: &str = "cancel";
/// `AdwAlertDialog` response id for the "New playlist" dialog's Create
/// action — see `RESPONSE_CANCEL`'s doc comment.
const RESPONSE_CREATE: &str = "create";

/// One row's identity: the built widget, the `ViewSource` selecting it
/// switches to, and its display title (handed to `Shared::on_select` so
/// `window.rs` can set the headerbar title without re-deriving it).
type RowEntry = (gtk4::ListBoxRow, ViewSource, String);

/// Callback invoked whenever the logically selected source changes — see
/// `Shared::on_select`'s doc comment for the full contract.
type OnSelect = Rc<dyn Fn(ViewSource, String)>;

struct Shared {
    conn: Rc<RefCell<Connection>>,
    listbox: gtk4::ListBox,
    /// Supplies the current queue's length for the "Queue" row's counter.
    /// Wired once at construction (mirrors `TrackList`'s `queue_ids_
    /// provider`) to a closure over the `PlayerController`.
    queue_len_provider: Box<dyn Fn() -> usize>,
    /// Which source is logically selected right now — kept in sync by the
    /// `row-selected` handler and used by a routine `rebuild` (`force_select
    /// = None`) to re-select the same source's (rebuilt) row afterwards.
    current_source: RefCell<ViewSource>,
    /// Every row built by the most recent `rebuild`, for row-identity lookup
    /// (see the module doc's `## Row identity` section). Rebuilt from
    /// scratch on every `rebuild` call.
    rows: RefCell<Vec<RowEntry>>,
    /// The "New playlist" action row, so `wire_row_activated` can tell it
    /// apart from a normal navigation row (identity compare) — it's
    /// `selectable(false)` so it never appears in `rows`/`row-selected`, only
    /// `row-activated`.
    new_playlist_row: RefCell<Option<gtk4::ListBoxRow>>,
    /// Invoked whenever the logically selected source *changes* (real user
    /// click, or the programmatic post-create-playlist selection) — never
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
    /// The window, for the "New playlist" `AlertDialog`'s parent. `WeakRef`
    /// so the sidebar can never keep the window alive past its natural
    /// lifetime (same shape as `TrackList::toast_overlay`).
    window: glib::WeakRef<adw::ApplicationWindow>,
    /// Injected post-construction once `window.rs` builds it (same seam
    /// shape as `TrackList::toast_overlay`) — surfaces a failed playlist
    /// creation as a toast rather than only a log line.
    toast_overlay: glib::WeakRef<adw::ToastOverlay>,
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
            window: window.downgrade(),
            toast_overlay: glib::WeakRef::new(),
        });

        wire_row_selected(&shared);
        wire_row_activated(&shared);

        rebuild(&shared, Some(ViewSource::default()));

        Self { shared, root }
    }

    /// The root widget to embed as the `AdwNavigationSplitView`'s sidebar
    /// page content.
    pub fn widget(&self) -> &gtk4::ScrolledWindow {
        &self.root
    }

    /// Sets the callback invoked whenever the selected source changes (user
    /// click, or the programmatic select-after-create following "New
    /// playlist"). `window.rs` wires this once, after `TrackList` exists, to
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

    /// Injects the window's toast overlay, once it exists (built after the
    /// sidebar — same post-construction seam as `TrackList::set_toast_
    /// overlay`).
    pub fn set_toast_overlay(&self, overlay: &adw::ToastOverlay) {
        self.shared.toast_overlay.set(Some(overlay));
    }

    /// Re-runs every count/list query and rebuilds the row set, preserving
    /// whichever source is currently selected. Called from `TrackList`'s
    /// `on_reload` hook (every reload: initial load, search, sort, source
    /// switch, and the explicit reload after a scan completes) — see the
    /// module doc's `## Reentrancy` section for why a reload triggered by
    /// this very sidebar's own selection is safe to feed back into it.
    pub fn refresh(&self) {
        rebuild(&self.shared, None);
    }
}

/// Shows `text` as an `adw::Toast`, degrading to a warn log if no overlay is
/// wired or it's gone — mirrors `track_list.rs`/`player_controller.rs`'s
/// `show_toast` (same seam, same degrade behavior).
fn show_toast(shared: &Shared, text: &str) {
    match shared.toast_overlay.upgrade() {
        Some(overlay) => overlay.add_toast(adw::Toast::new(text)),
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
///   change what's on screen or trigger a redundant reload.
fn rebuild(shared: &Rc<Shared>, force_select: Option<ViewSource>) {
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

    append_header(&shared.listbox, strings::SIDEBAR_SECTION_LIBRARY);
    add_row(
        shared,
        ViewSource::Library,
        strings::SIDEBAR_MUSIC,
        Some(music_count),
    );
    add_row(
        shared,
        ViewSource::Queue,
        strings::SIDEBAR_QUEUE,
        Some(queue_count),
    );

    append_header(&shared.listbox, strings::SIDEBAR_SECTION_PLAYLISTS);
    for playlist in &playlist_rows {
        add_row(
            shared,
            ViewSource::Playlist(playlist.id),
            &playlist.name,
            Some(playlist.track_count),
        );
    }
    let new_playlist_row = append_new_playlist_row(&shared.listbox);
    *shared.new_playlist_row.borrow_mut() = Some(new_playlist_row);

    append_header(&shared.listbox, strings::SIDEBAR_SECTION_SMART);
    for smart in &smart_rows {
        add_row(shared, ViewSource::Smart(smart.id), &smart.name, None);
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
                strings::SIDEBAR_IMPORT_ERRORS,
                Some(import_error_count),
            );
        }
        if missing_count > 0 {
            add_row(
                shared,
                ViewSource::Missing,
                strings::SIDEBAR_MISSING_FILES,
                Some(missing_count),
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
    let select_source = force_select.unwrap_or_else(|| shared.current_source.borrow().clone());
    if let Some(row) = find_row(shared, &select_source) {
        shared.listbox.select_row(Some(&row));
    }
}

/// Looks up the row currently backing `source` in `shared.rows` (rebuilt on
/// every `rebuild` call, so this only ever searches the *current* row set).
fn find_row(shared: &Rc<Shared>, source: &ViewSource) -> Option<gtk4::ListBoxRow> {
    shared
        .rows
        .borrow()
        .iter()
        .find(|(_, s, _)| s == source)
        .map(|(row, _, _)| row.clone())
}

/// Appends a non-selectable, non-activatable section header row (LIBRARY /
/// PLAYLISTS / SMART) — dim, small caps text per the mockup.
fn append_header(listbox: &gtk4::ListBox, text: &str) {
    let label = gtk4::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class("caption-heading");
    label.add_css_class("dim-label");
    label.set_margin_start(12);
    label.set_margin_end(12);
    label.set_margin_top(12);
    label.set_margin_bottom(2);

    let row = gtk4::ListBoxRow::builder()
        .child(&label)
        .selectable(false)
        .activatable(false)
        .build();
    listbox.append(&row);
}

/// Builds one navigation row (title + optional right-aligned count) and
/// registers it in `shared.rows` against `source`.
fn add_row(shared: &Rc<Shared>, source: ViewSource, title: &str, count: Option<i64>) {
    let row = build_nav_row(title, count);
    shared.listbox.append(&row);
    shared
        .rows
        .borrow_mut()
        .push((row, source, title.to_string()));
}

/// Builds the widget tree for one navigation row: a title label (start-
/// aligned, ellipsized, hexpand) and, if `count` is `Some`, a dim right-
/// aligned counter/badge label.
fn build_nav_row(title: &str, count: Option<i64>) -> gtk4::ListBoxRow {
    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    hbox.set_margin_start(12);
    hbox.set_margin_end(12);
    hbox.set_margin_top(6);
    hbox.set_margin_bottom(6);

    let title_label = gtk4::Label::new(Some(title));
    title_label.set_xalign(0.0);
    title_label.set_hexpand(true);
    title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    hbox.append(&title_label);

    if let Some(count) = count {
        let count_label = gtk4::Label::new(Some(&format_thousands(count)));
        count_label.add_css_class("dim-label");
        count_label.add_css_class("numeric");
        hbox.append(&count_label);
    }

    gtk4::ListBoxRow::builder().child(&hbox).build()
}

/// Appends the "New playlist" action row: not selectable (it never
/// participates in source-selection), but activatable (a click/Enter fires
/// `row-activated`, caught by `wire_row_activated`).
fn append_new_playlist_row(listbox: &gtk4::ListBox) -> gtk4::ListBoxRow {
    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    hbox.set_margin_start(12);
    hbox.set_margin_end(12);
    hbox.set_margin_top(6);
    hbox.set_margin_bottom(6);

    let icon = gtk4::Image::from_icon_name("list-add-symbolic");
    hbox.append(&icon);

    let label = gtk4::Label::new(Some(strings::SIDEBAR_NEW_PLAYLIST));
    label.set_xalign(0.0);
    label.add_css_class("dim-label");
    hbox.append(&label);

    let row = gtk4::ListBoxRow::builder()
        .child(&hbox)
        .selectable(false)
        .activatable(true)
        .build();
    listbox.append(&row);
    row
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

/// Wires the `ListBox`'s `row-activated` signal, used only by the "New
/// playlist" row (the one non-selectable-but-activatable row): every other
/// row is selectable, so a click there fires `row-selected` instead (handled
/// by `wire_row_selected`) and this handler no-ops for it.
fn wire_row_activated(shared: &Rc<Shared>) {
    let listbox = shared.listbox.clone();
    let shared = shared.clone();
    listbox.connect_row_activated(move |_, row| {
        let is_new_playlist_row = shared.new_playlist_row.borrow().as_ref() == Some(row);
        if is_new_playlist_row {
            show_new_playlist_dialog(&shared);
        }
    });
}

/// Shows the "New playlist" `AdwAlertDialog`: a heading, an entry (Create
/// disabled until non-blank), and Cancel/Create responses. On Create,
/// creates the playlist and switches straight to it (`create_playlist_and_
/// select`).
fn show_new_playlist_dialog(shared: &Rc<Shared>) {
    let Some(window) = shared.window.upgrade() else {
        tracing::warn!("sidebar: window is gone; cannot show new-playlist dialog");
        return;
    };

    let entry = gtk4::Entry::builder()
        .placeholder_text(strings::NEW_PLAYLIST_ENTRY_PLACEHOLDER)
        .activates_default(true)
        .build();

    let dialog = adw::AlertDialog::builder()
        .heading(strings::NEW_PLAYLIST_DIALOG_HEADING)
        .default_response(RESPONSE_CREATE)
        .close_response(RESPONSE_CANCEL)
        .extra_child(&entry)
        .build();
    dialog.add_response(RESPONSE_CANCEL, strings::CANCEL);
    dialog.add_response(RESPONSE_CREATE, strings::CREATE);
    dialog.set_response_appearance(RESPONSE_CREATE, adw::ResponseAppearance::Suggested);
    // Backend accepts an empty/whitespace-only name (`playlists::create`'s
    // doc comment: "backend is dumb; UI validates") — this is the UI-side
    // validation that comment refers to.
    dialog.set_response_enabled(RESPONSE_CREATE, false);

    entry.connect_changed({
        let dialog = dialog.clone();
        move |entry| {
            let has_name = !entry.text().trim().is_empty();
            dialog.set_response_enabled(RESPONSE_CREATE, has_name);
        }
    });

    let shared = shared.clone();
    dialog.choose(Some(&window), gio::Cancellable::NONE, move |response| {
        if response.as_str() == RESPONSE_CREATE {
            let name = entry.text().to_string();
            create_playlist_and_select(&shared, name.trim());
        }
    });
}

/// Creates a playlist named `name` and, on success, rebuilds the sidebar and
/// switches straight to it (`rebuild`'s `force_select` path, which — unlike
/// a routine refresh — lets the selection notify fire normally, so `window
/// .rs` switches the track list and headerbar title to it too). A creation
/// failure (e.g. a locked/corrupt database) is logged and surfaced as a
/// toast rather than silently dropped.
fn create_playlist_and_select(shared: &Rc<Shared>, name: &str) {
    let created = {
        let conn = shared.conn.borrow();
        playlists::create(&conn, name)
    };
    match created {
        Ok(id) => {
            tracing::info!(id, name, "playlist created");
            rebuild(shared, Some(ViewSource::Playlist(id)));
        }
        Err(error) => {
            tracing::error!(%error, name, "failed to create playlist");
            show_toast(shared, &strings::playlist_create_failed_toast(name));
        }
    }
}
