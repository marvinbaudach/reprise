//! GIO mount-event bridge for Missing-file evidence.
//!
//! GVolumeMonitor signals arrive on the GTK thread, while filesystem checks
//! and SQLite writes must not block it. Events are sent to one ordered
//! worker. Ordering is load-bearing: a quick remove/add sequence must mark
//! first and verify second, never let detached workers finish in reverse and
//! leave a mounted library grey.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use libadwaita as adw;
use reprise_core::library::watcher::WatcherHandle;
use rusqlite::Connection;

use super::scan_flow::ScanControls;
use super::sidebar::Sidebar;
use super::track_list::TrackList;

enum MountCommand {
    Added(PathBuf),
    Removed(PathBuf),
}

enum MountResult {
    Added {
        root: PathBuf,
        healed: Vec<i64>,
    },
    Removed {
        root: PathBuf,
        marked: u32,
    },
    Failed {
        operation: &'static str,
        error: String,
    },
}

#[derive(Clone, Copy)]
pub(in crate::ui) struct MountWiring<'a> {
    pub(in crate::ui) conn: &'a Rc<RefCell<Connection>>,
    pub(in crate::ui) db_path: &'a Path,
    pub(in crate::ui) controls: &'a ScanControls,
    pub(in crate::ui) toast_overlay: &'a adw::ToastOverlay,
    pub(in crate::ui) track_list: &'a Rc<TrackList>,
    pub(in crate::ui) sidebar: &'a Rc<Sidebar>,
    pub(in crate::ui) watcher_state: &'a Rc<RefCell<Option<WatcherHandle>>>,
}

pub(in crate::ui) fn install(args: &MountWiring<'_>) {
    let MountWiring {
        conn,
        db_path,
        controls,
        toast_overlay,
        track_list,
        sidebar,
        watcher_state,
    } = *args;
    let initial_root = library_root(conn);
    let initially_unavailable = initial_root.as_ref().is_some_and(|root| !root.is_dir());
    controls.set_library_root_unavailable(initially_unavailable);
    if initially_unavailable {
        track_list.set_library_root_unavailable(initial_root);
    }

    let (command_tx, command_rx) = async_channel::unbounded();
    let (result_tx, result_rx) = async_channel::unbounded();
    let worker_db_path = db_path.to_path_buf();
    std::thread::spawn(move || run_worker(&worker_db_path, &command_rx, &result_tx));

    let monitor = gio::VolumeMonitor::get();
    let added_tx = command_tx.clone();
    monitor.connect_mount_added(move |_, mount| {
        if let Some(root) = mount.root().path() {
            if let Err(error) = added_tx.send_blocking(MountCommand::Added(root)) {
                tracing::warn!(%error, "mount-added event dropped: worker is gone");
            }
        }
    });
    monitor.connect_mount_removed(move |_, mount| {
        if let Some(root) = mount.root().path() {
            if let Err(error) = command_tx.send_blocking(MountCommand::Removed(root)) {
                tracing::warn!(%error, "mount-removed event dropped: worker is gone");
            }
        }
    });

    let conn = conn.clone();
    let controls = controls.clone();
    let toast_overlay = toast_overlay.clone();
    let db_path = db_path.to_path_buf();
    let track_list = Rc::downgrade(track_list);
    let sidebar = Rc::downgrade(sidebar);
    let watcher_state = watcher_state.clone();
    glib::spawn_future_local(async move {
        while let Ok(result) = result_rx.recv().await {
            match result {
                MountResult::Added { root, healed } => {
                    tracing::info!(
                        mount = %root.display(),
                        healed = healed.len(),
                        "mount added: verified unavailable tracks"
                    );
                    refresh_views(&track_list, &sidebar, "mount added");
                    let affects_library = library_root(&conn)
                        .as_ref()
                        .is_some_and(|library| mount_contains(&root, library));
                    if affects_library && controls.library_root_unavailable() {
                        let Some(track_list) = track_list.upgrade() else {
                            continue;
                        };
                        let Some(sidebar) = sidebar.upgrade() else {
                            continue;
                        };
                        super::scan_flow::trigger_rescan_of_library_root(
                            &conn,
                            &controls,
                            &toast_overlay,
                            db_path.clone(),
                            track_list,
                            sidebar,
                            &watcher_state,
                        );
                    }
                }
                MountResult::Removed { root, marked } => {
                    tracing::info!(
                        mount = %root.display(),
                        marked,
                        "mount removed: marked live tracks unavailable"
                    );
                    let persisted_root = library_root(&conn);
                    let affects_library = persisted_root
                        .as_ref()
                        .is_some_and(|library| mount_contains(&root, library));
                    if affects_library {
                        controls.set_library_root_unavailable(true);
                        if let Some(library_root) = persisted_root {
                            controls.show_root_unavailable(&library_root);
                            if let Some(track_list) = track_list.upgrade() {
                                track_list.set_library_root_unavailable(Some(library_root));
                            }
                        }
                    }
                    refresh_views(&track_list, &sidebar, "mount removed");
                }
                MountResult::Failed { operation, error } => {
                    tracing::error!(operation, %error, "mount event reconciliation failed");
                }
            }
        }
    });
}

fn run_worker(
    db_path: &Path,
    commands: &async_channel::Receiver<MountCommand>,
    results: &async_channel::Sender<MountResult>,
) {
    while let Ok(command) = commands.recv_blocking() {
        let result = match reprise_core::db::open_migrated(Some(db_path)) {
            Ok(conn) => apply_command(&conn, command),
            Err(error) => MountResult::Failed {
                operation: "open database",
                error: error.to_string(),
            },
        };
        if results.send_blocking(result).is_err() {
            return;
        }
    }
}

fn apply_command(conn: &Connection, command: MountCommand) -> MountResult {
    match command {
        MountCommand::Added(root) => match reprise_core::queries::verify_unmounted_tracks(conn) {
            Ok(healed) => MountResult::Added { root, healed },
            Err(error) => MountResult::Failed {
                operation: "verify mounted tracks",
                error: error.to_string(),
            },
        },
        MountCommand::Removed(root) => {
            let mount = root.to_string_lossy();
            match reprise_core::queries::mark_mount_unavailable(conn, &mount, now_unix()) {
                Ok(marked) => MountResult::Removed { root, marked },
                Err(error) => MountResult::Failed {
                    operation: "mark removed mount unavailable",
                    error: error.to_string(),
                },
            }
        }
    }
}

fn refresh_views(track_list: &Weak<TrackList>, sidebar: &Weak<Sidebar>, reason: &str) {
    if let Some(track_list) = track_list.upgrade() {
        track_list.reload();
    }
    if let Some(sidebar) = sidebar.upgrade() {
        sidebar.refresh(reason);
    }
}

fn library_root(conn: &Rc<RefCell<Connection>>) -> Option<PathBuf> {
    let result = {
        let conn = conn.borrow();
        reprise_core::library::settings::get_library_root(&conn)
    };
    match result {
        Ok(root) => root.map(PathBuf::from),
        Err(error) => {
            tracing::error!(%error, "mount event: failed to read library root");
            None
        }
    }
}

fn mount_contains(mount_root: &Path, path: &Path) -> bool {
    path.starts_with(mount_root)
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_mount_containing_the_library_root_triggers_root_reconcile() {
        assert!(mount_contains(
            Path::new("/media/nas"),
            Path::new("/media/nas/Music")
        ));
        assert!(!mount_contains(
            Path::new("/media/usb"),
            Path::new("/media/nas/Music")
        ));
        assert!(!mount_contains(
            Path::new("/media/nas-archive"),
            Path::new("/media/nas/Music")
        ));
    }
}
