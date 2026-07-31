//! Pure path projection for synchronized-lyrics attachments on a device.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LyricsSidecarPaths {
    pub source_path: PathBuf,
    pub device_path: String,
}

/// Derives both attachment paths from the audio paths. Provider data never
/// participates, and the device filename follows the transferred audio even
/// when that audio was transcoded to another extension.
pub fn paths_for_track(source_path: &Path, device_path: &str) -> Option<LyricsSidecarPaths> {
    Some(LyricsSidecarPaths {
        source_path: source_path.with_extension("lrc"),
        device_path: device_path_for_track(device_path)?,
    })
}

pub fn device_path_for_track(device_path: &str) -> Option<String> {
    let device_path = Path::new(device_path);
    device_path.file_name()?;
    Some(
        device_path
            .with_extension("lrc")
            .to_string_lossy()
            .into_owned(),
    )
}

pub fn is_sidecar_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("lrc"))
}

#[cfg(test)]
#[path = "lyrics_sidecar_tests.rs"]
mod tests;
