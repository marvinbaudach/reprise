//! Playlist selection projections for the single phone target (`MTP-45`).

use std::collections::HashSet;

use super::page::SyncPlaylistRow;
use super::{
    ManagedRemoval, MirrorPlan, MirrorPlaylistSnapshot, MirrorTrack, SelectionSource, SyncTrack,
};

/// Transient identity for the picker’s “Everything” projection.
pub const EVERYTHING_SOURCE: SelectionSource = SelectionSource::Smart(i64::MIN);

#[must_use]
pub fn everything_playlist_snapshot(tracks: Vec<SyncTrack>) -> MirrorPlaylistSnapshot {
    MirrorPlaylistSnapshot {
        source: EVERYTHING_SOURCE,
        name: "Everything".to_string(),
        entries: tracks.into_iter().map(MirrorTrack::Available).collect(),
    }
}

/// Preserves published smart-playlist membership when live updates are off.
pub fn apply_frozen_smart_playlist_policy(
    plan: &mut MirrorPlan,
    frozen_sources: &HashSet<SelectionSource>,
    frozen_track_ids: &HashSet<i64>,
) {
    if frozen_sources.is_empty() {
        return;
    }
    plan.playlist_writes
        .retain(|write| !frozen_sources.contains(&write.source));
    plan.remove.retain(|removal| match removal {
        ManagedRemoval::Inventory(file) => !frozen_track_ids.contains(&file.track_id),
        ManagedRemoval::Orphan(_) => true,
    });
    plan.bytes_freed = plan.remove.iter().fold(0_u64, |sum, removal| {
        let bytes = match removal {
            ManagedRemoval::Inventory(file) => file.device_size,
            ManagedRemoval::Orphan(file) => file.size_bytes,
        };
        sum.saturating_add(bytes)
    });
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlaylistSelectionSummary {
    pub selected: usize,
    pub available_total: usize,
    pub unique_track_count: usize,
}

#[must_use]
pub fn summarize_playlist_selection(
    rows: &[SyncPlaylistRow],
    unique_track_count: usize,
) -> PlaylistSelectionSummary {
    PlaylistSelectionSummary {
        selected: rows
            .iter()
            .filter(|row| row.available && row.selected)
            .count(),
        available_total: rows.iter().filter(|row| row.available).count(),
        unique_track_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(source: i64, selected: bool, available: bool) -> SyncPlaylistRow {
        SyncPlaylistRow {
            source: SelectionSource::Playlist(source),
            name: Some(format!("Playlist {source}")),
            smart: false,
            selected,
            available,
            entry_count: 0,
            unique_track_count: 0,
            unavailable_count: 0,
            target_bytes: 0,
            last_synced_at: None,
        }
    }

    #[test]
    fn mtp_45_playlist_selection_is_the_complete_intended_phone_set() {
        let rows = [
            row(1, true, true),
            row(2, false, true),
            row(3, true, true),
            row(4, true, false),
        ];
        assert_eq!(
            summarize_playlist_selection(&rows, 278),
            PlaylistSelectionSummary {
                selected: 2,
                available_total: 3,
                unique_track_count: 278,
            }
        );
    }
}
