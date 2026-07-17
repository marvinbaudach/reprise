use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::relink::{self, FolderRelinkReport, RelinkMismatch, RelinkTarget};
use reprise_core::library::settings;
use rusqlite::Connection;

use super::missing_progress::{RelinkCancellation, RelinkProgressView};
use crate::ui::{strings, toasts};

const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_RELINK: &str = "relink";

pub(super) type OnRelinked = Rc<dyn Fn()>;

#[derive(Clone)]
pub(super) struct LocateContext {
    pub(super) conn: Rc<RefCell<Connection>>,
    pub(super) db_path: Option<PathBuf>,
    pub(super) window: glib::WeakRef<adw::ApplicationWindow>,
    pub(super) toast_overlay: glib::WeakRef<adw::ToastOverlay>,
    pub(super) progress: RelinkProgressView,
    pub(super) on_relinked: Option<OnRelinked>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RelinkWarning {
    heading: String,
    body: String,
}

fn duration_label(duration_ms: i64) -> String {
    let seconds = duration_ms.max(0) / 1_000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn warning_copy(mismatch: Option<&RelinkMismatch>, outside_library: bool) -> Option<RelinkWarning> {
    if mismatch.is_none() && !outside_library {
        return None;
    }
    let heading = if mismatch.is_some() {
        strings::issue_text(strings::MISSING_DIFFERENT_RECORDING)
    } else {
        strings::issue_text(strings::MISSING_OUTSIDE_LIBRARY_HEADING)
    };
    let mut lines = Vec::new();
    if let Some(mismatch) = mismatch {
        lines.push(strings::missing_relink_duration(
            &duration_label(mismatch.old_duration_ms),
            &duration_label(mismatch.new_duration_ms),
        ));
        let new_title = mismatch
            .new_title
            .clone()
            .unwrap_or_else(|| strings::issue_text(strings::MISSING_NO_READABLE_TITLE));
        lines.push(strings::missing_relink_title(
            &mismatch.old_title,
            &new_title,
        ));
    }
    if outside_library {
        lines.push(strings::issue_text(strings::MISSING_OUTSIDE_LIBRARY));
    }
    Some(RelinkWarning {
        heading,
        body: lines.join("\n"),
    })
}

fn is_outside_library(path: &Path, library_root: Option<&Path>) -> bool {
    let Some(root) = library_root else {
        return false;
    };
    let normalized_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let normalized_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    !normalized_path.starts_with(normalized_root)
}

fn library_root(context: &LocateContext) -> Result<Option<PathBuf>, rusqlite::Error> {
    let conn = context.conn.borrow();
    settings::get_library_root(&conn).map(|root| root.map(PathBuf::from))
}

pub(super) fn locate_file(context: LocateContext, target: RelinkTarget) {
    let Some(window) = context.window.upgrade() else {
        return;
    };
    let dialog = gtk4::FileDialog::builder()
        .title(strings::issue_text(strings::MISSING_LOCATE_FILE_TITLE))
        .modal(true)
        .build();
    if let Some(parent) = target.old_path.parent() {
        dialog.set_initial_folder(Some(&gio::File::for_path(parent)));
    }
    glib::spawn_future_local(async move {
        let file = match dialog.open_future(Some(&window)).await {
            Ok(file) => file,
            Err(error)
                if error.matches(gtk4::DialogError::Dismissed)
                    || error.matches(gtk4::DialogError::Cancelled) =>
            {
                return;
            }
            Err(error) => {
                tracing::error!(%error, "missing locate: file dialog failed");
                return;
            }
        };
        let Some(new_path) = file.path() else {
            tracing::warn!("missing locate: selected file has no local path");
            return;
        };
        continue_file_relink(context, target, new_path);
    });
}

fn continue_file_relink(context: LocateContext, target: RelinkTarget, new_path: PathBuf) {
    let library_root = match library_root(&context) {
        Ok(root) => root,
        Err(error) => {
            tracing::error!(%error, "missing locate: failed to read library root");
            show_toast(&context, strings::MISSING_RELINK_FAILED);
            return;
        }
    };
    let outside = is_outside_library(&new_path, library_root.as_deref());
    let probe = {
        let conn = context.conn.borrow();
        relink::probe_relink(&conn, &target, &new_path)
    };
    let mismatch = match probe {
        Ok(mismatch) => mismatch,
        Err(error) => {
            tracing::error!(%error, "missing locate: replacement probe failed");
            show_toast(&context, strings::MISSING_RELINK_FAILED);
            notify_relinked(&context);
            return;
        }
    };
    let Some(warning) = warning_copy(mismatch.as_ref(), outside) else {
        apply_file_relink(&context, &target, &new_path);
        return;
    };
    let Some(window) = context.window.upgrade() else {
        return;
    };
    let dialog = adw::AlertDialog::builder()
        .heading(warning.heading)
        .body(warning.body)
        .close_response(RESPONSE_CANCEL)
        .build();
    dialog.add_response(RESPONSE_CANCEL, &strings::issue_text(strings::CANCEL));
    dialog.add_response(
        RESPONSE_RELINK,
        &strings::issue_text(strings::MISSING_RELINK_ANYWAY),
    );
    dialog.set_default_response(Some(RESPONSE_RELINK));
    dialog.choose(Some(&window), gio::Cancellable::NONE, move |response| {
        if response.as_str() == RESPONSE_RELINK {
            apply_file_relink(&context, &target, &new_path);
        }
    });
}

fn apply_file_relink(context: &LocateContext, target: &RelinkTarget, new_path: &Path) {
    let result = {
        let mut conn = context.conn.borrow_mut();
        relink::relink_track(&mut conn, target, new_path)
    };
    match result {
        Ok(()) => notify_relinked(context),
        Err(error) => {
            tracing::error!(%error, "missing locate: relink failed");
            show_toast(context, strings::MISSING_RELINK_FAILED);
            notify_relinked(context);
        }
    }
}

pub(super) fn search_folder(context: LocateContext, targets: Vec<RelinkTarget>) {
    if targets.is_empty() {
        return;
    }
    let Some(window) = context.window.upgrade() else {
        return;
    };
    let dialog = gtk4::FileDialog::builder()
        .title(strings::issue_text(strings::MISSING_SEARCH_FOLDER_TITLE))
        .modal(true)
        .build();
    if let Some(parent) = targets.first().and_then(|target| target.old_path.parent()) {
        dialog.set_initial_folder(Some(&gio::File::for_path(parent)));
    }
    glib::spawn_future_local(async move {
        let folder = match dialog.select_folder_future(Some(&window)).await {
            Ok(folder) => folder,
            Err(error)
                if error.matches(gtk4::DialogError::Dismissed)
                    || error.matches(gtk4::DialogError::Cancelled) =>
            {
                return;
            }
            Err(error) => {
                tracing::error!(%error, "missing locate: folder dialog failed");
                return;
            }
        };
        let Some(folder) = folder.path() else {
            tracing::warn!("missing locate: selected folder has no local path");
            return;
        };
        continue_folder_search(context, folder, targets);
    });
}

fn continue_folder_search(context: LocateContext, folder: PathBuf, targets: Vec<RelinkTarget>) {
    let library_root = match library_root(&context) {
        Ok(root) => root,
        Err(error) => {
            tracing::error!(%error, "missing locate: failed to read library root");
            show_toast(&context, strings::MISSING_RELINK_FAILED);
            return;
        }
    };
    if !is_outside_library(&folder, library_root.as_deref()) {
        spawn_folder_relink(context, folder, targets);
        return;
    }
    let Some(window) = context.window.upgrade() else {
        return;
    };
    let warning = warning_copy(None, true).expect("outside-library selection always warns");
    let dialog = adw::AlertDialog::builder()
        .heading(warning.heading)
        .body(warning.body)
        .close_response(RESPONSE_CANCEL)
        .build();
    dialog.add_response(RESPONSE_CANCEL, &strings::issue_text(strings::CANCEL));
    dialog.add_response(
        RESPONSE_RELINK,
        &strings::issue_text(strings::MISSING_RELINK_ANYWAY),
    );
    dialog.choose(Some(&window), gio::Cancellable::NONE, move |response| {
        if response.as_str() == RESPONSE_RELINK {
            spawn_folder_relink(context, folder, targets);
        }
    });
}

fn spawn_folder_relink(context: LocateContext, folder: PathBuf, targets: Vec<RelinkTarget>) {
    let Some(db_path) = context.db_path.clone() else {
        tracing::error!("missing locate: worker database path was not configured");
        show_toast(&context, strings::MISSING_RELINK_FAILED);
        return;
    };
    let group_size = u32::try_from(targets.len()).unwrap_or(u32::MAX);
    let cancellation = RelinkCancellation::default();
    if !context.progress.start(group_size, cancellation.clone()) {
        show_toast(&context, strings::MISSING_RELINK_ALREADY_RUNNING);
        return;
    }
    let cancel = cancellation.token();
    let (progress_sender, progress_receiver) = async_channel::bounded::<(u32, u32)>(1);
    let stale_receiver = progress_receiver.clone();
    let (result_sender, result_receiver) = async_channel::bounded(1);
    let (drained_sender, drained_receiver) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let result = reprise_core::db::open_migrated(Some(&db_path))
            .map_err(|error| error.to_string())
            .and_then(|mut conn| {
                relink::relink_from_folder(
                    &mut conn,
                    &folder,
                    &targets,
                    &cancel,
                    |processed, total| {
                        while stale_receiver.try_recv().is_ok() {}
                        let _ = progress_sender.try_send((processed, total));
                    },
                )
                .map_err(|error| error.to_string())
            });
        drop(progress_sender);
        drop(stale_receiver);
        let _ = result_sender.send_blocking(result);
    });

    let progress = context.progress.clone();
    glib::spawn_future_local(async move {
        while let Ok((processed, total)) = progress_receiver.recv().await {
            progress.show(processed, total, group_size);
        }
        let _ = drained_sender.try_send(());
    });
    glib::spawn_future_local(async move {
        let result = result_receiver.recv().await;
        let _ = drained_receiver.recv().await;
        context.progress.finish();
        match result {
            Ok(Ok(report)) => finish_folder_relink(&context, report),
            Ok(Err(error)) => {
                tracing::error!(%error, "missing locate: folder relink failed");
                show_toast(&context, strings::MISSING_RELINK_FAILED);
                notify_relinked(&context);
            }
            Err(error) => tracing::warn!(%error, "missing locate: worker result dropped"),
        }
    });
}

fn finish_folder_relink(context: &LocateContext, report: FolderRelinkReport) {
    show_toast_text(
        context,
        &strings::missing_relink_result(report.relinked, report.group_size),
    );
    notify_relinked(context);
}

fn notify_relinked(context: &LocateContext) {
    if let Some(callback) = &context.on_relinked {
        callback();
    }
}

fn show_toast(context: &LocateContext, message: &str) {
    show_toast_text(context, &strings::issue_text(message));
}

fn show_toast_text(context: &LocateContext, message: &str) {
    if let Some(overlay) = context.toast_overlay.upgrade() {
        toasts::show(&overlay, message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mismatch_warning_is_symmetric_and_keeps_the_outside_library_hint() {
        let warning = warning_copy(
            Some(&RelinkMismatch {
                old_duration_ms: 61_000,
                new_duration_ms: 123_000,
                old_title: "Old".into(),
                new_title: Some("New".into()),
            }),
            true,
        )
        .unwrap();

        assert_eq!(warning.heading, "This looks like a different recording");
        assert!(warning.body.contains("Duration: 1:01 → 2:03"));
        assert!(warning.body.contains("Title: Old → New"));
        assert!(warning.body.contains("won't be watched or rescanned"));
    }
}
