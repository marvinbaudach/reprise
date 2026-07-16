//! UTF-8 M3U snapshots for named device playlists.

use std::path::{Component, Path};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevicePlaylistEntry {
    pub relative_path: String,
    pub duration_secs: i64,
    pub display: String,
}

pub fn render_named_playlist(entries: &[DevicePlaylistEntry]) -> String {
    let mut output = String::from("#EXTM3U\n");
    for entry in entries {
        if !safe_relative_path(&entry.relative_path) {
            continue;
        }
        let display = entry
            .display
            .chars()
            .map(|character| {
                if character.is_control() {
                    ' '
                } else {
                    character
                }
            })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        output.push_str(&format!(
            "#EXTINF:{},{}\n{}\n",
            entry.duration_secs.max(0),
            display,
            entry.relative_path
        ));
    }
    output
}

fn safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.chars().any(char::is_control)
        && !Path::new(path).is_absolute()
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}
