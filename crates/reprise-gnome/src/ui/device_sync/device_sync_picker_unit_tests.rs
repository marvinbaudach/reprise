use super::super::device_sync_runtime::{PickerPlaylistRow, PickerSnapshot};
use super::*;
use reprise_core::device_sync::SelectionSource;

fn row(source: SelectionSource, selected: bool) -> PickerPlaylistRow {
    PickerPlaylistRow {
        source,
        name: "List".into(),
        smart: false,
        selected,
        track_count: 1,
        size_bytes: 1,
    }
}

#[test]
fn mtp_51_everything_and_named_playlist_rows_are_one_exclusive_selection() {
    let mut snapshot = PickerSnapshot {
        rows: vec![
            row(EVERYTHING_SOURCE, false),
            row(SelectionSource::Playlist(7), true),
        ],
        keep_smart_updated: true,
    };

    set_playlist_row_selected(&mut snapshot, 0, true);
    assert_eq!(
        snapshot
            .rows
            .iter()
            .map(|row| row.selected)
            .collect::<Vec<_>>(),
        [true, false]
    );

    set_playlist_row_selected(&mut snapshot, 1, true);
    assert_eq!(
        snapshot
            .rows
            .iter()
            .map(|row| row.selected)
            .collect::<Vec<_>>(),
        [false, true]
    );
}

#[test]
fn mtp_51_select_all_selects_every_named_playlist_without_selecting_everything() {
    let mut snapshot = PickerSnapshot {
        rows: vec![
            row(EVERYTHING_SOURCE, false),
            row(SelectionSource::Playlist(7), false),
            row(SelectionSource::Smart(8), false),
        ],
        keep_smart_updated: true,
    };

    select_all_rows(&mut snapshot);

    assert_eq!(
        snapshot
            .rows
            .iter()
            .map(|row| row.selected)
            .collect::<Vec<_>>(),
        [false, true, true]
    );
}
