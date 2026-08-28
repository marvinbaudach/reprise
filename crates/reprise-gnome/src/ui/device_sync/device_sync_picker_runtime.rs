//! Database-backed inputs and writes for the playlists device-content picker.

use std::collections::HashSet;

use reprise_core::device_sync::{
    DeviceSelection, MirrorPlaylistSnapshot, MirrorTrack, SelectionSource, EVERYTHING_SOURCE,
};

use super::*;

pub(crate) const KEEP_SMART_UPDATED_KEY: &str = "device_sync.keep_smart_playlists_updated";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PickerSnapshot {
    pub rows: Vec<PickerPlaylistRow>,
    pub keep_smart_updated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PickerPlaylistRow {
    pub source: SelectionSource,
    pub name: String,
    pub smart: bool,
    pub selected: bool,
    pub track_count: usize,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PickerSave {
    pub playlist_changes: Vec<(SelectionSource, bool)>,
    pub keep_smart_updated: Option<bool>,
}

impl DeviceSyncRuntime {
    pub(crate) fn picker_snapshot_fresh(
        self: &Rc<Self>,
        device_id: &str,
    ) -> Result<PickerSnapshot, String> {
        self.recompute_if_stale(device_id)?;
        self.picker_snapshot(device_id)
    }

    fn picker_snapshot(&self, device_id: &str) -> Result<PickerSnapshot, String> {
        // This cached read is intentional: privacy makes picker_snapshot_fresh
        // the only production entry point, and it refreshes a stale device before delegating here.
        let device = self
            .device_states
            .borrow()
            .iter()
            .find(|device| device.descriptor.id == device_id)
            .map(DeviceState::view)
            .ok_or_else(|| "device is not connected".to_string())?;
        let mut rows = device
            .page
            .playlists
            .into_iter()
            .filter(|row| row.available)
            .map(picker_playlist_row)
            .collect::<Vec<_>>();
        let everything_selected = device.settings.selection == DeviceSelection::EntireLibrary;
        let everything = reprise_core::device_sync::load_everything_playlist_snapshot(&self.conn)
            .map_err(|error| error.to_string())?;
        let everything_row = reprise_core::device_sync::project_sync_page(
            reprise_core::device_sync::SyncPageInput {
                selected: everything_selected
                    .then_some(EVERYTHING_SOURCE)
                    .into_iter()
                    .collect(),
                playlists: vec![everything],
                profile: device.settings.profile,
                ..Default::default()
            },
        )
        .page
        .playlists
        .into_iter()
        .next()
        .ok_or_else(|| "Everything playlist projection is missing".to_string())?;
        rows.insert(0, picker_playlist_row(everything_row));
        let keep_smart_updated =
            reprise_core::library::settings::get_bool(&self.conn, KEEP_SMART_UPDATED_KEY, true)
                .map_err(|error| error.to_string())?;
        Ok(PickerSnapshot {
            rows,
            keep_smart_updated,
        })
    }

    #[cfg(test)]
    pub(crate) fn picker_snapshot_cached_for_test(
        &self,
        device_id: &str,
    ) -> Result<PickerSnapshot, String> {
        self.picker_snapshot(device_id)
    }

    pub(crate) fn save_picker(
        self: &Rc<Self>,
        device_id: &str,
        changes: PickerSave,
    ) -> Result<(), String> {
        let keep_smart_updated_now =
            reprise_core::library::settings::get_bool(&self.conn, KEEP_SMART_UPDATED_KEY, true)
                .map_err(|error| error.to_string())?;
        let newly_selected_smart = changes
            .playlist_changes
            .iter()
            .filter_map(|(source, selected)| {
                (*selected
                    && matches!(source, SelectionSource::Smart(_))
                    && source != &EVERYTHING_SOURCE)
                    .then_some(source.clone())
            })
            .collect::<Vec<_>>();
        if !keep_smart_updated_now && !newly_selected_smart.is_empty() {
            capture_frozen_smart_snapshots(&self.conn, device_id, &newly_selected_smart)?;
        }
        if changes.keep_smart_updated == Some(false) {
            let settings = self.settings_for_update(device_id)?;
            let selected = match settings.selection {
                DeviceSelection::Sources(sources) => sources,
                DeviceSelection::EntireLibrary => Vec::new(),
            };
            capture_frozen_smart_snapshots(&self.conn, device_id, &selected)?;
        }
        if !changes.playlist_changes.is_empty() {
            let mut settings = self.settings_for_update(device_id)?;
            let mut sources = match settings.selection {
                DeviceSelection::Sources(sources) => sources,
                DeviceSelection::EntireLibrary => Vec::new(),
            };
            let select_everything = changes
                .playlist_changes
                .iter()
                .any(|(source, selected)| source == &EVERYTHING_SOURCE && *selected);
            for (source, selected) in changes.playlist_changes {
                if source == EVERYTHING_SOURCE {
                    continue;
                }
                sources.retain(|candidate| candidate != &source);
                if selected {
                    sources.push(source);
                }
            }
            settings.selection = if select_everything {
                DeviceSelection::EntireLibrary
            } else {
                DeviceSelection::Sources(sources)
            };
            self.update_settings(settings)?;
        }
        if let Some(keep_updated) = changes.keep_smart_updated {
            reprise_core::library::settings::set_bool(
                &self.conn,
                KEEP_SMART_UPDATED_KEY,
                keep_updated,
            )
            .map_err(|error| error.to_string())?;
        }
        self.recompute_delta(device_id)
    }
}

fn picker_playlist_row(row: reprise_core::device_sync::SyncPlaylistRow) -> PickerPlaylistRow {
    PickerPlaylistRow {
        smart: matches!(row.source, SelectionSource::Smart(_)) && row.source != EVERYTHING_SOURCE,
        source: row.source,
        name: row.name.unwrap_or_else(|| {
            super::device_sync_strings::text(super::device_sync_strings::UNAVAILABLE_PLAYLIST)
        }),
        selected: row.selected,
        track_count: row.unique_track_count,
        size_bytes: row.target_bytes,
    }
}

pub(super) fn apply_frozen_smart_snapshots(
    db: &Db,
    device_id: &str,
    selected: &[SelectionSource],
    snapshots: &mut [MirrorPlaylistSnapshot],
) -> Result<(HashSet<SelectionSource>, HashSet<i64>), String> {
    if reprise_core::library::settings::get_bool(db, KEEP_SMART_UPDATED_KEY, true)
        .map_err(|error| error.to_string())?
    {
        return Ok((HashSet::new(), HashSet::new()));
    }
    let mut frozen = HashSet::new();
    let mut frozen_track_ids = HashSet::new();
    for source in selected {
        let SelectionSource::Smart(id) = source else {
            continue;
        };
        if source == &EVERYTHING_SOURCE {
            continue;
        }
        let key = frozen_smart_key(device_id, *id);
        let live = snapshots
            .iter()
            .find(|snapshot| &snapshot.source == source)
            .map(|snapshot| snapshot.entries.as_slice())
            .unwrap_or_default();
        let ids = match reprise_core::library::settings::get_setting(db, &key)
            .map_err(|error| error.to_string())?
        {
            Some(json) => serde_json::from_str::<Vec<i64>>(&json)
                .map_err(|error| format!("invalid frozen smart-playlist snapshot: {error}"))?,
            None => {
                let ids = live.iter().map(mirror_track_id).collect::<Vec<_>>();
                let json = serde_json::to_string(&ids).map_err(|error| error.to_string())?;
                reprise_core::library::settings::set_setting(db, &key, &json)
                    .map_err(|error| error.to_string())?;
                ids
            }
        };
        let tracks = reprise_core::queries::query_sync_tracks(db, &ids)
            .map_err(|error| error.to_string())?;
        if let Some(snapshot) = snapshots
            .iter_mut()
            .find(|snapshot| &snapshot.source == source)
        {
            snapshot.entries = tracks.into_iter().map(MirrorTrack::Available).collect();
        }
        frozen_track_ids.extend(ids);
        frozen.insert(source.clone());
    }
    Ok((frozen, frozen_track_ids))
}

fn mirror_track_id(track: &MirrorTrack) -> i64 {
    match track {
        MirrorTrack::Available(track) => track.id,
        MirrorTrack::Unavailable(track) => track.track_id,
    }
}

fn frozen_smart_key(device_id: &str, playlist_id: i64) -> String {
    let device = device_id
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("device_sync.frozen_smart.{device}.{playlist_id}")
}

fn capture_frozen_smart_snapshots(
    db: &Db,
    device_id: &str,
    sources: &[SelectionSource],
) -> Result<(), String> {
    let snapshots = reprise_core::device_sync::load_mirror_playlist_snapshots(db)
        .map_err(|error| error.to_string())?;
    for source in sources {
        let SelectionSource::Smart(id) = source else {
            continue;
        };
        if source == &EVERYTHING_SOURCE {
            continue;
        }
        let ids = snapshots
            .iter()
            .find(|snapshot| &snapshot.source == source)
            .map(|snapshot| {
                snapshot
                    .entries
                    .iter()
                    .map(mirror_track_id)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let json = serde_json::to_string(&ids).map_err(|error| error.to_string())?;
        reprise_core::library::settings::set_setting(db, &frozen_smart_key(device_id, *id), &json)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
