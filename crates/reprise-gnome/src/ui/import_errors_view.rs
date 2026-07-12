//! A dedicated three-column (path/reason/time) view for the `import_errors`
//! table (Stage 3 Task 8's ImportErrors source). These rows aren't `Track`s
//! — see `view_source.rs`'s `ImportErrors` doc comment for why this source
//! has always been the one exception to `track_list.rs`'s "one list, many
//! sources" `ColumnView` — so this is a separate, small widget rather than a
//! seventh shape the shared `TrackListModel` would need to understand.
//! `track_list.rs` embeds this as a third `gtk::Stack` page, alongside the
//! existing empty/list pages, and switches to it only while `ViewSource::
//! ImportErrors` is selected and has rows (see that module's `reload`).
//!
//! ## Plain rows, not a `ColumnView`/factory
//!
//! Same reasoning as `ui::sidebar`'s module doc: at the scale a scan's
//! import-error list actually reaches (a handful to a few dozen unreadable
//! files), a `SignalListItemFactory` would be pure overhead. Rows are built
//! fresh on every [`ImportErrorsView::refresh`], mirroring `ui::sidebar::
//! rebuild`'s own tear-down-and-rebuild approach for the same reason.
//!
//! ## Retry: a synchronous single-file scan, not a background worker thread
//!
//! Unlike a full "Scan folder…" (which can walk an entire library and so
//! always runs on its own thread against its own connection — see `ui::
//! window::spawn_scan`), "Retry" re-scans exactly one already-known path.
//! `library::scanner::scan_folder` happily accepts a *file* path (`walkdir`
//! just visits that one entry), so this runs synchronously, on the UI
//! thread, against the same shared `Rc<RefCell<Connection>>` every other
//! UI-thread database access already uses — one `lofty` tag read plus one
//! small transaction is cheap enough not to be worth a worker thread and the
//! channel-marshalling that would come with it.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use rusqlite::Connection;

use crate::ui::strings;
use crate::ui::toasts;
use reprise_core::format::format_unix_timestamp;
use reprise_core::library::scanner;
use reprise_core::queries::{self, ImportErrorRow};

struct Shared {
    conn: Rc<RefCell<Connection>>,
    listbox: gtk4::ListBox,
    /// Invoked after a successful Retry or Dismiss (regardless of whether
    /// the retry itself cleared the error) — `track_list.rs` wires this to
    /// also refresh the sidebar's Import-errors badge, since the count
    /// backing it just changed.
    on_mutated: RefCell<Option<Rc<dyn Fn()>>>,
    /// Injected post-construction via `ImportErrorsView::set_toast_overlay`
    /// (`track_list.rs`'s `TrackList::set_toast_overlay` forwards to it) —
    /// same seam shape as `track_list.rs`/`sidebar.rs`'s own toast overlay:
    /// surfaces a failed Retry as a toast rather than only a log line, since
    /// (unlike Dismiss, which cannot fail in a user-visible way) a Retry
    /// failing to even run leaves the user staring at an unchanged row with
    /// no other feedback.
    toast_overlay: glib::WeakRef<adw::ToastOverlay>,
}

/// Handle to the built import-errors panel widget.
pub struct ImportErrorsView {
    shared: Rc<Shared>,
    root: gtk4::ScrolledWindow,
}

impl ImportErrorsView {
    pub fn new(conn: Rc<RefCell<Connection>>) -> Self {
        let listbox = gtk4::ListBox::new();
        listbox.set_selection_mode(gtk4::SelectionMode::None);
        listbox.add_css_class("boxed-list");
        listbox.set_margin_start(12);
        listbox.set_margin_end(12);
        listbox.set_margin_top(6);
        listbox.set_margin_bottom(12);
        listbox.set_valign(gtk4::Align::Start);

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.append(&build_header_row());
        content.append(&listbox);

        let root = gtk4::ScrolledWindow::builder()
            .child(&content)
            .vexpand(true)
            .hexpand(true)
            .build();

        let shared = Rc::new(Shared {
            conn,
            listbox,
            on_mutated: RefCell::new(None),
            toast_overlay: glib::WeakRef::new(),
        });

        Self { shared, root }
    }

    /// The root widget to embed as a `gtk::Stack` page.
    pub fn widget(&self) -> &gtk4::ScrolledWindow {
        &self.root
    }

    /// Sets the callback invoked after a Retry/Dismiss action mutates the
    /// `import_errors` table — see `Shared::on_mutated`'s doc comment.
    pub fn set_on_mutated(&self, callback: impl Fn() + 'static) {
        *self.shared.on_mutated.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the window's toast overlay — see `Shared::toast_overlay`'s
    /// doc comment. `track_list.rs`'s `TrackList::set_toast_overlay` forwards
    /// to this.
    pub fn set_toast_overlay(&self, overlay: &adw::ToastOverlay) {
        self.shared.toast_overlay.set(Some(overlay));
    }

    /// Re-queries `import_errors` and rebuilds every row. Returns the row
    /// count just loaded — `track_list.rs`'s `reload()` uses this exactly
    /// like every other source's row count to decide between this page and
    /// the shared "nothing here" empty state.
    pub fn refresh(&self) -> usize {
        refresh(&self.shared)
    }
}

/// Builds the column-header row (Path / Reason / Time) shown once, above the
/// per-error rows — a plain label row, not part of the `ListBox` itself (so
/// it never gets torn down/rebuilt by `refresh`'s `remove_all`).
fn build_header_row() -> gtk4::Box {
    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    hbox.set_margin_start(20);
    hbox.set_margin_end(20);
    hbox.set_margin_top(6);
    hbox.set_margin_bottom(2);

    let path_header = gtk4::Label::new(Some(&strings::text(strings::IMPORT_ERROR_COLUMN_PATH)));
    path_header.set_xalign(0.0);
    path_header.set_hexpand(true);
    path_header.add_css_class("caption-heading");
    path_header.add_css_class("dim-label");
    hbox.append(&path_header);

    let reason_header = gtk4::Label::new(Some(&strings::text(strings::IMPORT_ERROR_COLUMN_REASON)));
    reason_header.set_xalign(0.0);
    reason_header.set_width_chars(24);
    reason_header.add_css_class("caption-heading");
    reason_header.add_css_class("dim-label");
    hbox.append(&reason_header);

    let time_header = gtk4::Label::new(Some(&strings::text(strings::IMPORT_ERROR_COLUMN_TIME)));
    time_header.add_css_class("caption-heading");
    time_header.add_css_class("dim-label");
    hbox.append(&time_header);

    hbox
}

/// Mirrors `track_list.rs`/`sidebar.rs`'s own `show_toast` — same seam, same
/// degrade-to-log behavior when no overlay is wired or it's gone.
fn show_toast(shared: &Shared, text: &str) {
    match shared.toast_overlay.upgrade() {
        Some(overlay) => toasts::show(&overlay, text),
        None => tracing::warn!(text, "import errors panel: toast overlay is gone; log-only"),
    }
}

fn refresh(shared: &Rc<Shared>) -> usize {
    let rows = {
        let conn = shared.conn.borrow();
        queries::query_import_errors(&conn).unwrap_or_else(|error| {
            tracing::error!(%error, "import errors panel: failed to load rows");
            Vec::new()
        })
    };

    shared.listbox.remove_all();
    let count = rows.len();
    for row in &rows {
        shared.listbox.append(&build_row(shared, row));
    }
    count
}

/// Builds one row: path (ellipsized, tooltip carries the full text) / reason
/// / time, followed by Retry and Dismiss buttons.
fn build_row(shared: &Rc<Shared>, row: &ImportErrorRow) -> gtk4::ListBoxRow {
    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    hbox.set_margin_start(8);
    hbox.set_margin_end(8);
    hbox.set_margin_top(8);
    hbox.set_margin_bottom(8);

    let path_label = gtk4::Label::new(Some(&row.path));
    path_label.set_xalign(0.0);
    path_label.set_hexpand(true);
    path_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    path_label.set_tooltip_text(Some(&row.path));
    hbox.append(&path_label);

    let reason_label = gtk4::Label::new(Some(&row.reason));
    reason_label.set_xalign(0.0);
    reason_label.add_css_class("dim-label");
    reason_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    reason_label.set_width_chars(24);
    reason_label.set_tooltip_text(Some(&row.reason));
    hbox.append(&reason_label);

    let time_label = gtk4::Label::new(Some(&format_unix_timestamp(row.occurred_at)));
    time_label.add_css_class("dim-label");
    time_label.add_css_class("numeric");
    hbox.append(&time_label);

    let retry_button = gtk4::Button::with_label(&strings::text(strings::IMPORT_ERROR_RETRY));
    {
        let shared = shared.clone();
        let path = row.path.clone();
        retry_button.connect_clicked(move |_| handle_retry(&shared, &path));
    }
    hbox.append(&retry_button);

    let dismiss_button = gtk4::Button::with_label(&strings::text(strings::IMPORT_ERROR_DISMISS));
    {
        let shared = shared.clone();
        let id = row.id;
        dismiss_button.connect_clicked(move |_| handle_dismiss(&shared, id));
    }
    hbox.append(&dismiss_button);

    gtk4::ListBoxRow::builder()
        .child(&hbox)
        .activatable(false)
        .selectable(false)
        .build()
}

/// Clone-out-then-call `on_mutated` (hoisted per this project's `RefCell`
/// callback discipline — see `ui::track_list`'s module doc comment), then
/// always refresh this panel's own rows regardless of whether a callback was
/// wired, so the row that was just acted on disappears/updates immediately.
fn notify_mutated_and_refresh(shared: &Rc<Shared>) {
    let callback = shared.on_mutated.borrow().clone();
    match callback {
        Some(callback) => callback(),
        None => tracing::warn!("import errors panel: mutated but no on_mutated callback is wired"),
    }
    refresh(shared);
}

/// "Retry" (see the module doc's `## Retry` section): re-scans exactly
/// `path`. On success `scan_folder` itself clears the `import_errors` row if
/// the file is now readable (or refreshes `reason`/`occurred_at` if it still
/// isn't) — this function's job is just to run the scan and then refresh.
fn handle_retry(shared: &Rc<Shared>, path: &str) {
    let result = {
        let mut conn = shared.conn.borrow_mut();
        scanner::scan_folder(&mut conn, Path::new(path))
    };
    match result {
        Ok(report) => {
            tracing::info!(path, ?report, "import errors panel: retry rescan complete");
        }
        Err(error) => {
            tracing::error!(%error, path, "import errors panel: retry rescan failed to run");
            show_toast(shared, &strings::import_error_retry_failed_toast());
        }
    }
    notify_mutated_and_refresh(shared);
}

/// "Dismiss": deletes the `import_errors` row itself — never a file, never
/// any `tracks` row (there isn't one for an import failure).
fn handle_dismiss(shared: &Rc<Shared>, id: i64) {
    let result = {
        let conn = shared.conn.borrow();
        queries::delete_import_error(&conn, id)
    };
    if let Err(error) = result {
        tracing::error!(%error, id, "import errors panel: dismiss failed");
    }
    notify_mutated_and_refresh(shared);
}
