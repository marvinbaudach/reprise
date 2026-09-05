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
pub struct Session {
    backend: SessionBackend,
}

impl Session {
    pub fn open() -> Result<Self, String> {
        let backend = match backend_for(Path::new(FLATPAK_INFO).is_file()) {
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
        match &self.backend {
            SessionBackend::Host => trash::delete(path).map_err(|error| error.to_string()),
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
    fn host_session_deletes_each_path_and_reports_per_file_failures() {
        assert_eq!(backend_for(false), Backend::Host);
        let session = Session::open().expect("host trash session");
        let temp = tempfile::tempdir().unwrap();
        let existing = temp.path().join("existing.flac");
        let missing = temp.path().join("missing.flac");
        std::fs::write(&existing, b"scratch").unwrap();

        assert!(session.delete(&existing).is_ok());
        assert!(session.delete(&missing).is_err());
    }
}
