//! Whether this process runs inside a Flatpak sandbox.

use std::path::Path;

/// The file the Flatpak runtime places in every sandbox.
const FLATPAK_INFO: &str = "/.flatpak-info";

/// True inside a Flatpak sandbox, which sees its own data directory instead of
/// the host's.
#[must_use]
pub fn running_in_flatpak() -> bool {
    sandbox_marker_exists(Path::new(FLATPAK_INFO))
}

fn sandbox_marker_exists(marker: &Path) -> bool {
    marker.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sandbox_marker_is_a_regular_file() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join(".flatpak-info");

        assert!(!sandbox_marker_exists(&marker));
        std::fs::write(&marker, "[Application]\n").unwrap();
        assert!(sandbox_marker_exists(&marker));
        assert!(
            !sandbox_marker_exists(root.path()),
            "a directory is not the marker"
        );
    }
}
