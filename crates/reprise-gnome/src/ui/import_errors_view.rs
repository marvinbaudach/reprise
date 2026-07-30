//! Grouped import-error triage view built on the shared issue-card kit.

use std::cell::RefCell;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::db::Db;
use reprise_core::library::scanner;
use reprise_core::models::ImportErrorKind;
use reprise_core::queries::{self, ImportErrorEntry};

use crate::ui::issues::{CollapsedList, IssueCard, IssuePill, IssueRow, RowSpec};
use crate::ui::{one_shot_task, strings, toasts};

#[derive(Debug, Clone, PartialEq, Eq)]
struct KindCopy {
    icon: String,
    title: String,
    row_text: String,
}

fn kind_copy(kind: ImportErrorKind) -> KindCopy {
    let (icon, title, row_text) = match kind {
        ImportErrorKind::UnreadableTags => (
            strings::IMPORT_ISSUE_TAGS_ICON,
            strings::IMPORT_ISSUE_TAGS_TITLE,
            strings::IMPORT_ISSUE_TAGS_ROW,
        ),
        ImportErrorKind::PermissionDenied => (
            strings::IMPORT_ISSUE_PERMISSION_ICON,
            strings::IMPORT_ISSUE_PERMISSION_TITLE,
            strings::IMPORT_ISSUE_PERMISSION_ROW,
        ),
        ImportErrorKind::UnsupportedFormat => (
            strings::IMPORT_ISSUE_FORMAT_ICON,
            strings::IMPORT_ISSUE_FORMAT_TITLE,
            strings::IMPORT_ISSUE_FORMAT_ROW,
        ),
        ImportErrorKind::Io => (
            strings::IMPORT_ISSUE_IO_ICON,
            strings::IMPORT_ISSUE_IO_TITLE,
            strings::IMPORT_ISSUE_IO_ROW,
        ),
        ImportErrorKind::Unknown => (
            strings::IMPORT_ISSUE_UNKNOWN_ICON,
            strings::IMPORT_ISSUE_UNKNOWN_TITLE,
            strings::IMPORT_ISSUE_UNKNOWN_ROW,
        ),
    };
    KindCopy {
        icon: strings::issue_text(icon),
        title: strings::issue_text(title),
        row_text: strings::issue_text(row_text),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportRowAction {
    Retry,
    EditTags,
    Dismiss,
    ShowInFiles,
}

fn row_actions(is_hint: bool) -> Vec<ImportRowAction> {
    if is_hint {
        vec![ImportRowAction::EditTags, ImportRowAction::Dismiss]
    } else {
        vec![
            ImportRowAction::Retry,
            ImportRowAction::Dismiss,
            ImportRowAction::ShowInFiles,
        ]
    }
}

type EditHintCallback = Rc<dyn Fn(&str)>;

struct Shared {
    conn: Rc<Db>,
    groups: gtk4::Box,
    dismissed: gtk4::Box,
    on_mutated: RefCell<Option<Rc<dyn Fn()>>>,
    on_edit_hint: RefCell<Option<EditHintCallback>>,
    toast_overlay: glib::WeakRef<adw::ToastOverlay>,
    window: glib::WeakRef<adw::ApplicationWindow>,
}

/// Handle to the dedicated Import-errors stack page.
pub struct ImportErrorsView {
    shared: Rc<Shared>,
    root: gtk4::ScrolledWindow,
}

impl ImportErrorsView {
    pub fn new(conn: Rc<Db>) -> Self {
        let groups = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        let dismissed = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        content.set_margin_start(16);
        content.set_margin_end(16);
        content.set_margin_top(12);
        content.set_margin_bottom(96);

        let (header, retry_all, dismiss_all, export) = build_header();
        content.append(&header);
        content.append(&groups);
        content.append(&dismissed);

        let root = gtk4::ScrolledWindow::builder()
            .child(&content)
            .vexpand(true)
            .hexpand(true)
            .build();
        let shared = Rc::new(Shared {
            conn,
            groups,
            dismissed,
            on_mutated: RefCell::new(None),
            on_edit_hint: RefCell::new(None),
            toast_overlay: glib::WeakRef::new(),
            window: glib::WeakRef::new(),
        });

        {
            let shared = shared.clone();
            retry_all.connect_clicked(move |_| handle_retry_all(&shared));
        }
        {
            let shared = shared.clone();
            dismiss_all.connect_clicked(move |_| handle_dismiss_all(&shared));
        }
        {
            let shared = shared.clone();
            export.connect_clicked(move |_| handle_export(&shared));
        }

        Self { shared, root }
    }

    pub fn widget(&self) -> &gtk4::ScrolledWindow {
        &self.root
    }

    pub fn set_on_mutated(&self, callback: impl Fn() + 'static) {
        *self.shared.on_mutated.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_edit_hint(&self, callback: impl Fn(&str) + 'static) {
        *self.shared.on_edit_hint.borrow_mut() = Some(Rc::new(callback));
    }

    pub fn set_toast_overlay(&self, overlay: &adw::ToastOverlay) {
        self.shared.toast_overlay.set(Some(overlay));
    }

    pub(in crate::ui) fn set_window(&self, window: &adw::ApplicationWindow) {
        self.shared.window.set(Some(window));
    }

    /// Returns active plus dismissed rows so the restore footer remains
    /// reachable while this page is selected after the last dismissal.
    pub fn refresh(&self) -> usize {
        refresh(&self.shared)
    }
}

fn build_header() -> (gtk4::Box, gtk4::Button, gtk4::Button, gtk4::Button) {
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    header.set_halign(gtk4::Align::End);
    let retry_all = header_button(strings::IMPORT_ISSUE_RETRY_ALL, true);
    let dismiss_all = header_button(strings::IMPORT_ISSUE_DISMISS_ALL, false);
    let export = header_button(strings::IMPORT_ISSUE_EXPORT, false);
    header.append(&retry_all);
    header.append(&dismiss_all);
    header.append(&export);
    (header, retry_all, dismiss_all, export)
}

fn header_button(label: &str, accent: bool) -> gtk4::Button {
    let button = gtk4::Button::with_label(&strings::issue_text(label));
    button.add_css_class("pill");
    if accent {
        button.add_css_class("suggested-action");
    } else {
        button.add_css_class("flat");
    }
    button
}

fn refresh(shared: &Rc<Shared>) -> usize {
    remove_children(&shared.groups);
    remove_children(&shared.dismissed);
    let (groups, dismissed_count) = {
        let conn = &shared.conn;
        let groups = queries::query_import_errors_grouped(conn).unwrap_or_else(|error| {
            tracing::error!(%error, "import errors view: failed to load active groups");
            Vec::new()
        });
        let dismissed = queries::count_dismissed_import_errors(conn).unwrap_or_else(|error| {
            tracing::error!(%error, "import errors view: failed to count dismissed rows");
            0
        });
        (groups, dismissed)
    };
    let active_count = groups
        .iter()
        .map(|(_, entries)| entries.len())
        .sum::<usize>();
    for (kind, entries) in groups {
        shared.groups.append(&build_group(shared, kind, entries));
    }
    if dismissed_count > 0 {
        shared
            .dismissed
            .append(&build_dismissed_footer(shared, dismissed_count));
    }
    active_count + dismissed_count as usize
}

fn remove_children(container: &gtk4::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn build_group(
    shared: &Rc<Shared>,
    kind: ImportErrorKind,
    entries: Vec<ImportErrorEntry>,
) -> gtk4::Widget {
    let copy = kind_copy(kind);
    let card = IssueCard::new(
        &copy.icon,
        &copy.title,
        &strings::import_issue_file_count(entries.len()),
        None,
    );
    let entries = Rc::new(entries);
    let total = u32::try_from(entries.len()).unwrap_or(u32::MAX);
    let shared_for_rows = shared.clone();
    CollapsedList::attach_to(
        card.body_listbox(),
        total,
        Rc::new(move |index| {
            let entry = entries[index as usize].clone();
            build_active_row(&shared_for_rows, &entry).upcast()
        }),
    );
    card.widget().clone().upcast()
}

fn build_active_row(shared: &Rc<Shared>, entry: &ImportErrorEntry) -> gtk4::ListBoxRow {
    let copy = kind_copy(entry.kind);
    let path = Path::new(&entry.path);
    let filename = path.file_name().map_or_else(
        || entry.path.clone(),
        |name| name.to_string_lossy().into_owned(),
    );
    let parent = path
        .parent()
        .map_or_else(String::new, |value| value.to_string_lossy().into_owned());
    let mut pills = Vec::new();
    for action in row_actions(entry.is_hint) {
        pills.push(action_pill(shared, entry, action));
    }
    let primary = if entry.is_hint {
        format!(
            "{} · {filename}",
            strings::issue_text(strings::IMPORT_ISSUE_HINT_PREFIX)
        )
    } else {
        filename
    };
    let row = IssueRow::new(RowSpec {
        cover: None,
        primary,
        secondary: parent,
        tertiary: copy.row_text,
        right_idle: strings::import_issue_seen(entry.seen_count),
        pills,
    });
    row.widget()
        .set_tooltip_text(Some(&format!("{}\n{}", entry.path, entry.detail)));
    if entry.is_hint {
        row.widget().add_css_class("import-hint-row");
    }
    row.widget().clone()
}

fn action_pill(
    shared: &Rc<Shared>,
    entry: &ImportErrorEntry,
    action: ImportRowAction,
) -> IssuePill {
    match action {
        ImportRowAction::Retry => {
            let shared = shared.clone();
            let path = entry.path.clone();
            IssuePill::new(
                strings::issue_text(strings::IMPORT_ERROR_RETRY),
                move || {
                    handle_retry(&shared, &path);
                },
            )
        }
        ImportRowAction::EditTags => {
            let shared = shared.clone();
            let path = entry.path.clone();
            IssuePill::new(
                strings::issue_text(strings::IMPORT_ISSUE_EDIT_TAGS),
                move || {
                    let callback = shared.on_edit_hint.borrow().clone();
                    if let Some(callback) = callback {
                        callback(&path);
                    }
                },
            )
        }
        ImportRowAction::Dismiss => {
            let shared = shared.clone();
            let path = entry.path.clone();
            IssuePill::new(
                strings::issue_text(strings::IMPORT_ERROR_DISMISS),
                move || handle_dismiss(&shared, &path),
            )
        }
        ImportRowAction::ShowInFiles => {
            let shared = shared.clone();
            let path = entry.path.clone();
            IssuePill::new(
                strings::issue_text(strings::IMPORT_ISSUE_SHOW_FILES),
                move || show_in_files(&shared, &path),
            )
        }
    }
}

fn build_dismissed_footer(shared: &Rc<Shared>, count: u32) -> gtk4::Widget {
    let box_ = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    let show = gtk4::Button::with_label(&strings::import_issue_dismissed(count));
    show.add_css_class("flat");
    show.add_css_class("pill");
    show.set_halign(gtk4::Align::Center);
    let rows = gtk4::ListBox::new();
    rows.add_css_class("issue-card-list");
    rows.set_visible(false);
    box_.append(&show);
    box_.append(&rows);
    let shared = shared.clone();
    show.connect_clicked(move |button| {
        let visible = !rows.is_visible();
        rows.set_visible(visible);
        let label = if visible {
            strings::issue_text(strings::IMPORT_ISSUE_HIDE_DISMISSED)
        } else {
            strings::import_issue_dismissed(count)
        };
        button.set_label(&label);
        if visible && rows.first_child().is_none() {
            populate_dismissed(&shared, &rows);
        }
    });
    box_.upcast()
}

fn populate_dismissed(shared: &Rc<Shared>, rows: &gtk4::ListBox) {
    let dismissed = {
        let conn = &shared.conn;
        queries::query_dismissed_import_errors(conn).unwrap_or_else(|error| {
            tracing::error!(%error, "import errors view: failed to load dismissed rows");
            Vec::new()
        })
    };
    for entry in dismissed {
        let path = entry.path.clone();
        let shared_for_restore = shared.clone();
        let row = IssueRow::new(RowSpec {
            cover: None,
            primary: Path::new(&entry.path).file_name().map_or_else(
                || entry.path.clone(),
                |name| name.to_string_lossy().into_owned(),
            ),
            secondary: entry.path.clone(),
            tertiary: kind_copy(entry.kind).title,
            right_idle: strings::import_issue_seen(entry.seen_count),
            pills: vec![IssuePill::new(
                strings::issue_text(strings::IMPORT_ISSUE_RESTORE),
                move || handle_restore(&shared_for_restore, &path),
            )],
        });
        row.widget()
            .set_tooltip_text(Some(&format!("{}\n{}", entry.path, entry.detail)));
        rows.append(row.widget());
    }
}

pub(in crate::ui) fn file_stat(path: &str) -> Option<(i64, i64)> {
    let metadata = std::fs::metadata(path).ok()?;
    Some((
        metadata.mtime(),
        i64::try_from(metadata.len()).unwrap_or(i64::MAX),
    ))
}

fn handle_retry(shared: &Rc<Shared>, path: &str) {
    let result = {
        let conn = &shared.conn;
        scanner::scan_folder(conn, Path::new(path))
    };
    if let Err(error) = result {
        tracing::error!(%error, path, "import errors view: retry failed to run");
        show_toast(shared, &strings::import_error_retry_failed_toast());
    }
    notify_mutated(shared);
}

fn handle_restore(shared: &Rc<Shared>, path: &str) {
    let result = {
        let conn = &shared.conn;
        queries::restore_import_error(conn, path)
    };
    match result {
        Ok(()) => handle_retry(shared, path),
        Err(error) => tracing::error!(%error, path, "import errors view: restore failed"),
    }
}

fn handle_dismiss(shared: &Rc<Shared>, path: &str) {
    let Some((mtime, size)) = file_stat(path) else {
        tracing::warn!(
            path,
            "import errors view: dismiss skipped because stat failed"
        );
        show_toast(
            shared,
            &strings::issue_text(strings::IMPORT_ISSUE_DISMISS_FAILED),
        );
        return;
    };
    let result = {
        let conn = &shared.conn;
        queries::dismiss_import_error(conn, path, mtime, size)
    };
    match result {
        Ok(()) => notify_mutated(shared),
        Err(error) => tracing::error!(%error, path, "import errors view: dismiss failed"),
    }
}

fn handle_dismiss_all(shared: &Rc<Shared>) {
    let result = {
        let conn = &shared.conn;
        queries::dismiss_all_import_errors(conn, &file_stat)
    };
    match result {
        Ok(_) => notify_mutated(shared),
        Err(error) => tracing::error!(%error, "import errors view: dismiss all failed"),
    }
}

fn active_paths(shared: &Shared) -> Vec<String> {
    let conn = &shared.conn;
    queries::query_import_errors_grouped(conn).map_or_else(
        |error| {
            tracing::error!(%error, "import errors view: failed to load retry paths");
            Vec::new()
        },
        |groups| {
            groups
                .into_iter()
                .flat_map(|(_, entries)| entries.into_iter().map(|entry| entry.path))
                .collect()
        },
    )
}

fn handle_retry_all(shared: &Rc<Shared>) {
    let paths = active_paths(shared);
    if paths.is_empty() {
        return;
    }
    let db_path = shared.conn.path();
    let Some(db_path) = db_path else {
        show_toast(
            shared,
            &strings::issue_text(strings::IMPORT_ISSUE_RETRY_ALL_FAILED),
        );
        return;
    };
    let receiver = match one_shot_task::spawn("reprise-retry-import-errors", move || {
        let conn = reprise_core::db::Db::open_migrated(Some(&db_path))
            .map_err(|error| error.to_string())?;
        let mut failures = 0usize;
        for path in paths {
            if let Err(error) = scanner::scan_folder(&conn, Path::new(&path)) {
                failures += 1;
                tracing::error!(%error, path, "import errors view: retry-all item failed");
            }
        }
        Ok::<usize, String>(failures)
    }) {
        Ok(receiver) => receiver,
        Err(error) => {
            tracing::error!(%error, "import errors view: could not start retry-all worker");
            show_toast(
                shared,
                &strings::issue_text(strings::IMPORT_ISSUE_RETRY_ALL_FAILED),
            );
            return;
        }
    };
    let shared = Rc::downgrade(shared);
    glib::spawn_future_local(async move {
        let Ok(result) = receiver.recv().await else {
            return;
        };
        let Some(shared) = shared.upgrade() else {
            return;
        };
        match result {
            Ok(0) => {}
            Ok(failures) => tracing::warn!(failures, "import errors view: retry all incomplete"),
            Err(error) => {
                tracing::error!(%error, "import errors view: retry all failed");
                show_toast(
                    &shared,
                    &strings::issue_text(strings::IMPORT_ISSUE_RETRY_ALL_FAILED),
                );
            }
        }
        notify_mutated(&shared);
    });
}

fn export_text(shared: &Shared) -> String {
    let mut paths = active_paths(shared);
    paths.sort_unstable();
    if paths.is_empty() {
        String::new()
    } else {
        format!("{}\n", paths.join("\n"))
    }
}

fn handle_export(shared: &Rc<Shared>) {
    let Some(window) = shared.window.upgrade() else {
        tracing::warn!("import errors view: window is gone; cannot export");
        return;
    };
    let dialog = gtk4::FileDialog::builder()
        .title(strings::issue_text(strings::IMPORT_ISSUE_EXPORT_TITLE))
        .modal(true)
        .initial_name("reprise-import-errors.txt")
        .build();
    let shared = shared.clone();
    glib::spawn_future_local(async move {
        let file = match dialog.save_future(Some(&window)).await {
            Ok(file) => file,
            Err(error)
                if error.matches(gtk4::DialogError::Dismissed)
                    || error.matches(gtk4::DialogError::Cancelled) =>
            {
                return;
            }
            Err(error) => {
                tracing::error!(%error, "import errors view: export dialog failed");
                return;
            }
        };
        let Some(path) = file.path() else {
            tracing::warn!("import errors view: export target is not a local path");
            return;
        };
        if let Err(error) = std::fs::write(path, export_text(&shared)) {
            tracing::error!(%error, "import errors view: export failed");
            show_toast(
                &shared,
                &strings::issue_text(strings::IMPORT_ISSUE_EXPORT_FAILED),
            );
        }
    });
}

fn show_in_files(shared: &Shared, path: &str) {
    let Some(window) = shared.window.upgrade() else {
        return;
    };
    let launcher = gtk4::FileLauncher::new(Some(&gio::File::for_path(path)));
    glib::spawn_future_local(async move {
        if let Err(error) = launcher.open_containing_folder_future(Some(&window)).await {
            tracing::error!(%error, "import errors view: show in Files failed");
        }
    });
}

fn show_toast(shared: &Shared, text: &str) {
    match shared.toast_overlay.upgrade() {
        Some(overlay) => toasts::show(&overlay, text),
        None => tracing::warn!(text, "import errors view: toast overlay is gone; log-only"),
    }
}

fn notify_mutated(shared: &Rc<Shared>) {
    let callback = shared.on_mutated.borrow().clone();
    if let Some(callback) = callback {
        callback();
    } else {
        refresh(shared);
    }
}

#[cfg(test)]
mod task_3_3_tests {
    use super::*;

    #[test]
    fn every_import_error_kind_has_complete_human_copy() {
        let copies = [
            ImportErrorKind::UnreadableTags,
            ImportErrorKind::PermissionDenied,
            ImportErrorKind::UnsupportedFormat,
            ImportErrorKind::Io,
            ImportErrorKind::Unknown,
        ]
        .map(kind_copy);

        assert_eq!(
            copies.clone().map(|copy| copy.title),
            [
                "Unreadable tags",
                "Permission denied",
                "Unsupported format",
                "Read error",
                "Unclassified",
            ]
        );
        assert_eq!(
            copies[0].row_text,
            "Tags unreadable — the file itself can usually still be played"
        );
    }

    #[test]
    fn imported_without_metadata_is_a_hint_with_no_retry() {
        assert_eq!(
            row_actions(true),
            vec![ImportRowAction::EditTags, ImportRowAction::Dismiss]
        );
        assert_eq!(
            row_actions(false),
            vec![
                ImportRowAction::Retry,
                ImportRowAction::Dismiss,
                ImportRowAction::ShowInFiles,
            ]
        );
    }
}
