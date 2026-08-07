//! Path projection for synchronized-lyrics attachments on a device, and the
//! one question about the library those paths lead to: is the sidecar still
//! there, and how large is it.

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

/// The size of the library sidecar at `source_path`, or `None` when there is
/// no regular file there.
///
/// `LYR-7` asks this twice with the same answer in mind. A copy needs the byte
/// count the transfer will announce; a removal needs to know the library still
/// holds the sidecar the device copy was mirrored from, because a `.lrc` with
/// no library counterpart is the user's own and stays. A directory or a broken
/// symlink is not a sidecar either, so `is_file` decides, not mere existence.
pub fn source_file_size(source_path: &Path) -> Option<u64> {
    let metadata = std::fs::metadata(source_path).ok()?;
    metadata.is_file().then_some(metadata.len())
}

pub fn is_sidecar_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("lrc"))
}

#[cfg(test)]
#[path = "lyrics_sidecar_tests.rs"]
mod tests;
