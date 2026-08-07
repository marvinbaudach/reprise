use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Names learned from provider cursors while walking a SAF tree.
///
/// The maps deliberately never query the provider themselves. A name that was
/// not carried by an already-required cursor stays unknown instead of adding a
/// Binder round trip or deriving display text from an opaque URI.
#[derive(Default)]
pub(super) struct SourceNames {
    display: Mutex<HashMap<PathBuf, Option<String>>>,
    container: Mutex<HashMap<PathBuf, Option<String>>>,
    relative: Mutex<HashMap<PathBuf, PathBuf>>,
}

impl SourceNames {
    pub(super) fn remember_display_name(&self, at: PathBuf, name: Option<String>) {
        if let Ok(mut names) = self.display.lock() {
            names.insert(at, name);
        }
    }

    pub(super) fn remember_child(
        &self,
        at: PathBuf,
        display_name: Option<String>,
        container_name: Option<String>,
    ) {
        self.remember_display_name(at.clone(), display_name);
        if let Ok(mut names) = self.container.lock() {
            names.insert(at, container_name);
        }
    }

    pub(super) fn remember_relative_path(&self, at: PathBuf, relative: PathBuf) {
        if let Ok(mut paths) = self.relative.lock() {
            paths.insert(at, relative);
        }
    }

    pub(super) fn clear_relative_paths(&self) {
        if let Ok(mut paths) = self.relative.lock() {
            paths.clear();
        }
    }

    pub(super) fn display_name(&self, at: &Path) -> Option<String> {
        self.display.lock().ok()?.get(at)?.clone()
    }

    pub(super) fn container_name(&self, at: &Path) -> Option<String> {
        self.container.lock().ok()?.get(at)?.clone()
    }

    pub(super) fn relative_path(&self, at: &Path) -> Option<PathBuf> {
        self.relative.lock().ok()?.get(at).cloned()
    }
}
