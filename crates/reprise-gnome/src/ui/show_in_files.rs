//! Reveals selected tracks in one file-manager window.
//!
//! Menu sensitivity guarantees that all paths share a folder and are
//! present (CTX-10), so this function trusts that contract. FileManager1's
//! `ShowItems` provides reveal-and-select behavior; unsupported desktops
//! fall back to opening the shared parent folder.

use std::path::{Path, PathBuf};

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::glib::variant::ToVariant;

#[allow(dead_code)] // Wired to the live context-menu action in Task 7.
pub(in crate::ui) fn show_in_files(paths: &[PathBuf]) {
    if paths.is_empty() {
        return;
    }
    let uris = paths_to_uris(paths);
    if let Err(error) = show_items(&uris) {
        tracing::warn!(%error, "FileManager1.ShowItems failed; opening the folder instead");
        open_parent_folder(&paths[0]);
    }
}

fn paths_to_uris(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| gio::File::for_path(path).uri().to_string())
        .collect()
}

fn show_items(uris: &[String]) -> Result<(), glib::Error> {
    let proxy = gio::DBusProxy::for_bus_sync(
        gio::BusType::Session,
        gio::DBusProxyFlags::NONE,
        None::<&gio::DBusInterfaceInfo>,
        "org.freedesktop.FileManager1",
        "/org/freedesktop/FileManager1",
        "org.freedesktop.FileManager1",
        gio::Cancellable::NONE,
    )?;
    proxy.call_sync(
        "ShowItems",
        Some(&(uris.to_vec(), String::new()).to_variant()),
        gio::DBusCallFlags::NONE,
        -1,
        gio::Cancellable::NONE,
    )?;
    Ok(())
}

fn open_parent_folder(path: &Path) {
    let folder = path.parent().unwrap_or(path);
    let uri = gio::File::for_path(folder).uri();
    if let Err(error) = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>) {
        tracing::warn!(%error, %uri, "could not open parent folder");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_file_uris_for_paths() {
        let uris = paths_to_uris(&[PathBuf::from("/m/a.flac"), PathBuf::from("/m/b.flac")]);
        assert_eq!(uris, ["file:///m/a.flac", "file:///m/b.flac"]);
    }
}
