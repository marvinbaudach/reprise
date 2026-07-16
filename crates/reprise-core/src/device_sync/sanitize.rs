//! FAT-safe path construction for Reprise-managed device files.

use std::path::PathBuf;

const MAX_COMPONENT_BYTES: usize = 120;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevicePathMetadata {
    pub album_artist: String,
    pub artist: String,
    pub album: String,
    pub track_number: Option<u32>,
    pub title: String,
    pub source_path: PathBuf,
}

pub fn sanitize_component(input: &str, fallback: &str) -> String {
    let replaced = input
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | '?' | '*' | ':' | '"' | '<' | '>' | '|'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    let trimmed =
        replaced.trim_matches(|character: char| character == '.' || character.is_whitespace());
    let candidate = if trimmed.is_empty() {
        fallback
    } else {
        trimmed
    };
    truncate_utf8(candidate, MAX_COMPONENT_BYTES)
}

pub fn device_track_path(
    metadata: &DevicePathMetadata,
    forced_extension: Option<&str>,
    collision_index: usize,
) -> String {
    let album_artist = if metadata.album_artist.trim().is_empty() {
        &metadata.artist
    } else {
        &metadata.album_artist
    };
    let album_artist = sanitize_component(album_artist, "Unknown Artist");
    let album = sanitize_component(&metadata.album, "Unknown Album");
    let title = sanitize_component(&metadata.title, "Untitled");
    let number = metadata.track_number.unwrap_or(0);
    let number = if number < 100 {
        format!("{number:02}")
    } else {
        number.to_string()
    };
    let suffix = if collision_index > 1 {
        format!(" ({collision_index})")
    } else {
        String::new()
    };
    let extension = forced_extension
        .map(str::to_string)
        .or_else(|| {
            metadata
                .source_path
                .extension()
                .and_then(|value| value.to_str())
                .map(str::to_ascii_lowercase)
        })
        .unwrap_or_else(|| "audio".into());
    format!("{album_artist}/{album}/{number} {title}{suffix}.{extension}")
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end]
        .trim_end_matches(|character: char| character == '.' || character.is_whitespace())
        .to_string()
}
