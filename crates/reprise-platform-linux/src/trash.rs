//! Linux trash selection and XDG desktop portal backend.
//!
//! Host applications use the desktop trash implementation from the `trash`
//! crate. Flatpak applications must ask the desktop portal to trash an open
//! read/write file descriptor. A portal failure is final: this module never
//! falls back to permanent deletion or a private sandbox trash directory.

use std::fs::OpenOptions;
use std::path::Path;

use zbus::zvariant::Fd;

const FLATPAK_INFO: &str = "/.flatpak-info";
const PORTAL_DESTINATION: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_INTERFACE: &str = "org.freedesktop.portal.Trash";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Backend {
    Host,
    Portal,
}

fn backend_for(flatpak_info_present: bool) -> Backend {
    if flatpak_info_present {
        Backend::Portal
    } else {
        Backend::Host
    }
}

enum SessionBackend {
    Host,
    Portal(zbus::blocking::Proxy<'static>),
}

/// Reusable trash backend state for one caller-defined batch.
///
/// The portal proxy is intentionally opened once and is not reconnected after
/// a mid-batch failure. Callers receive that failure for each affected file
/// and commit only the files the portal confirmed as trashed.
pub struct Session {
    backend: SessionBackend,
}

impl Session {
    pub fn open() -> Result<Self, String> {
        Self::open_for_backend(backend_for(Path::new(FLATPAK_INFO).is_file()))
    }

    fn open_for_backend(backend: Backend) -> Result<Self, String> {
        let backend = match backend {
            Backend::Host => SessionBackend::Host,
            Backend::Portal => {
                let connection = zbus::blocking::Connection::session().map_err(|error| {
                    format!("could not connect to session bus for Trash portal: {error}")
                })?;
                let proxy = zbus::blocking::Proxy::new_owned(
                    connection,
                    PORTAL_DESTINATION,
                    PORTAL_PATH,
                    PORTAL_INTERFACE,
                )
                .map_err(|error| format!("could not create Trash portal proxy: {error}"))?;
                SessionBackend::Portal(proxy)
            }
        };
        Ok(Self { backend })
    }

    pub fn delete(&self, path: &Path) -> Result<(), String> {
        self.delete_with_host(path, |path| {
            trash::delete(path).map_err(|error| error.to_string())
        })
    }

    fn delete_with_host(
        &self,
        path: &Path,
        host_delete: impl FnOnce(&Path) -> Result<(), String>,
    ) -> Result<(), String> {
        match &self.backend {
            SessionBackend::Host => host_delete(path),
            SessionBackend::Portal(proxy) => portal_delete(proxy, path),
        }
    }
}

/// Moves `path` to the desktop trash without ever permanently deleting it.
pub fn delete(path: &Path) -> Result<(), String> {
    Session::open()?.delete(path)
}

fn portal_delete(proxy: &zbus::blocking::Proxy<'_>, path: &Path) -> Result<(), String> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("could not open file read/write for Trash portal: {error}"))?;
    let result: u32 = proxy
        .call("TrashFile", &Fd::from(&file))
        .map_err(|error| format!("Trash portal call failed: {error}"))?;
    portal_result(result)
}

fn portal_result(result: u32) -> Result<(), String> {
    if result == 1 {
        Ok(())
    } else {
        Err(format!("Trash portal refused the file (result {result})"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_selects_host_or_portal_backend() {
        assert_eq!(backend_for(false), Backend::Host);
        assert_eq!(backend_for(true), Backend::Portal);
    }

    #[test]
    fn only_portal_result_one_is_success() {
        assert!(portal_result(1).is_ok());
        assert!(portal_result(0).is_err());
    }

    #[test]
    fn host_session_reports_per_file_results_without_using_the_desktop_trash() {
        let session = Session::open_for_backend(Backend::Host).expect("host trash session");
        let temp = tempfile::tempdir().unwrap();
        let existing = temp.path().join("existing.flac");
        let missing = temp.path().join("missing.flac");
        std::fs::write(&existing, b"scratch").unwrap();

        assert!(session
            .delete_with_host(&existing, |path| {
                std::fs::remove_file(path).map_err(|error| error.to_string())
            })
            .is_ok());
        assert!(session
            .delete_with_host(&missing, |path| {
                std::fs::remove_file(path).map_err(|error| error.to_string())
            })
            .is_err());
        assert!(!existing.exists());
    }
}
