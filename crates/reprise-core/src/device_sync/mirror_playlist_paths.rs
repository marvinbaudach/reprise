//! Stable, collision-free device paths for mirrored playlists.

use std::collections::{HashMap, HashSet};

use super::super::sanitize::sanitize_component;
use super::super::settings::{DevicePlaylistRecord, SelectionSource};
use super::{safe_managed_path, MirrorPlaylistSnapshot};

pub(super) fn stable_playlist_paths(
    playlists: &[MirrorPlaylistSnapshot],
    inventory: &[DevicePlaylistRecord],
) -> HashMap<SelectionSource, String> {
    let mut playlists = playlists.iter().collect::<Vec<_>>();
    playlists.sort_by(|left, right| left.source.cmp(&right.source));
    let mut slots = HashMap::<String, PlaylistCollisionSlots>::new();
    let mut paths = HashMap::new();
    for playlist in playlists {
        let base = sanitize_component(&playlist.name, "Playlist");
        let key = base.to_lowercase();
        let collision = slots
            .entry(key.clone())
            .or_insert_with(|| PlaylistCollisionSlots::from_inventory(&key, inventory));
        let (index, existing_path) = collision.assign(&playlist.source);
        paths.insert(
            playlist.source.clone(),
            existing_path.unwrap_or_else(|| playlist_path(&base, index)),
        );
    }
    paths
}

#[derive(Default)]
struct PlaylistCollisionSlots {
    used: HashSet<usize>,
    owned: HashMap<SelectionSource, (usize, String)>,
}

impl PlaylistCollisionSlots {
    fn from_inventory(base_key: &str, inventory: &[DevicePlaylistRecord]) -> Self {
        let mut slots = Self::default();
        for record in inventory {
            let Some(index) = playlist_collision_index(&record.device_path, base_key) else {
                continue;
            };
            if slots.used.insert(index) {
                slots
                    .owned
                    .insert(record.source.clone(), (index, record.device_path.clone()));
            }
        }
        slots
    }

    fn assign(&mut self, source: &SelectionSource) -> (usize, Option<String>) {
        if let Some((index, path)) = self.owned.get(source) {
            return (*index, Some(path.clone()));
        }
        let mut index = 1;
        while !self.used.insert(index) {
            index = index.saturating_add(1);
        }
        (index, None)
    }
}

fn playlist_collision_index(path: &str, base_key: &str) -> Option<usize> {
    if !safe_managed_path(path) {
        return None;
    }
    let stem = path.strip_suffix(".m3u8")?.to_lowercase();
    if stem.contains('/') {
        return None;
    }
    if stem == base_key {
        return Some(1);
    }
    let suffix = stem.strip_prefix(base_key)?;
    let index = suffix.strip_prefix(" (")?.strip_suffix(')')?.parse().ok()?;
    (index >= 2).then_some(index)
}

pub(super) fn playlist_path(base: &str, collision_index: usize) -> String {
    let suffix = if collision_index > 1 {
        format!(" ({collision_index})")
    } else {
        String::new()
    };
    format!("{base}{suffix}.m3u8")
}
