use super::super::device_sync_runtime::{PickerEpisodeRow, PickerPlaylistRow};
use super::*;

fn podcast_episode(id: i64, played: bool) -> PickerEpisodeRow {
    PickerEpisodeRow {
        id,
        title: format!("Episode {id}"),
        published_at: Some(id),
        duration_secs: Some(60),
        position_ms: 0,
        size_bytes: Some(1_024),
        downloaded: true,
        played,
        pinned: false,
        selected: false,
    }
}

#[test]
fn mtp_50_select_all_respects_the_podcast_unplayed_standing_rule() {
    let mut snapshot = PickerSnapshot::Episodes {
        kind: SyncTargetKind::PodcastEpisodes,
        latest_per_group: 5,
        groups: vec![PickerEpisodeGroup {
            id: 10,
            name: "Show".into(),
            enabled: false,
            latest_override: None,
            episodes: vec![podcast_episode(1, true), podcast_episode(2, false)],
        }],
    };

    select_all_rows(&mut snapshot);
    refresh_episode_selection(&mut snapshot);

    let PickerSnapshot::Episodes { groups, .. } = snapshot else {
        unreachable!();
    };
    assert_eq!(
        groups[0]
            .episodes
            .iter()
            .map(|episode| (episode.pinned, episode.selected))
            .collect::<Vec<_>>(),
        [(false, false), (true, true)],
        "Select all may explicitly include unplayed episodes but must not revive played ones"
    );
}

#[test]
fn mtp_50_an_explicit_tick_pins_an_episode_already_selected_by_the_rule() {
    assert!(
        explicit_pin_after_toggle(true, false, false),
        "clicking a rule-selected row must turn the same durable episode flag into an override"
    );
    assert!(
        !explicit_pin_after_toggle(false, false, false),
        "an unselected row remains unpinned when it is switched off"
    );
}

#[test]
fn everything_and_named_playlist_rows_are_one_exclusive_selection() {
    let row = |source, selected| PickerPlaylistRow {
        source,
        name: "List".into(),
        smart: false,
        selected,
        track_count: 1,
        size_bytes: 1,
    };
    let mut snapshot = PickerSnapshot::Playlists {
        rows: vec![
            row(EVERYTHING_SOURCE, false),
            row(
                reprise_core::device_sync::SelectionSource::Playlist(7),
                true,
            ),
        ],
        keep_smart_updated: true,
    };

    set_playlist_row_selected(&mut snapshot, 0, true);
    let PickerSnapshot::Playlists { rows, .. } = &snapshot else {
        unreachable!();
    };
    assert_eq!(
        rows.iter().map(|row| row.selected).collect::<Vec<_>>(),
        [true, false]
    );

    set_playlist_row_selected(&mut snapshot, 1, true);
    let PickerSnapshot::Playlists { rows, .. } = snapshot else {
        unreachable!();
    };
    assert_eq!(
        rows.iter().map(|row| row.selected).collect::<Vec<_>>(),
        [false, true]
    );
}
