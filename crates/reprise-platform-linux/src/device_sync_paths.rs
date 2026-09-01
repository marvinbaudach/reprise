use std::path::{Component, Path};

use super::DeviceIoError;

/// Splits a [`reprise_core::device_sync::SyncTarget`] path (e.g. `/Music/Selected`, `MTP-23`) into path components for building a `gio::File`
/// under the resolved storage volume. Unlike [`safe_relative_components`], a single leading `Component::RootDir` is accepted and dropped —
/// sync target paths are written as absolute-looking device paths, but every one of them is still resolved relative to the storage volume
/// returned by [`super::DeviceStorage::storage_root`].
pub(super) fn safe_target_components(path: &str) -> Result<Vec<String>, DeviceIoError> {
    safe_components(path, true)
}

pub(super) fn safe_relative_components(path: &str) -> Result<Vec<String>, DeviceIoError> {
    safe_components(path, false)
}

fn safe_components(path: &str, allow_root: bool) -> Result<Vec<String>, DeviceIoError> {
    if path.is_empty() || path.chars().any(char::is_control) {
        return Err(DeviceIoError::InvalidRelativePath);
    }
    let components = Path::new(path)
        .components()
        .filter_map(|component| match component {
            Component::RootDir if allow_root => None,
            Component::Normal(value) => Some(
                value
                    .to_str()
                    .map(str::to_string)
                    .ok_or(DeviceIoError::InvalidRelativePath),
            ),
            _ => Some(Err(DeviceIoError::InvalidRelativePath)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    (!components.is_empty())
        .then_some(components)
        .ok_or(DeviceIoError::InvalidRelativePath)
}

pub(super) fn join_relative(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

pub(super) fn is_audio_file(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "mp3" | "flac" | "ogg" | "opus" | "m4a" | "aac" | "wav" | "audio"
            )
        })
}
