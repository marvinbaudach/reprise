use reprise_core::radio::StationRow;
use std::cmp::Ordering;

use crate::ui::strings;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct RadioLiveState {
    pub station_id: Option<i64>,
    pub connected: bool,
    pub title: Option<String>,
}

pub(super) fn format_bitrate(value: Option<i64>) -> String {
    value
        .filter(|value| *value > 0)
        .map_or_else(unknown, |value| format!("{value}k"))
}

pub(super) fn format_country(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map_or_else(unknown, str::to_ascii_uppercase)
}

pub(super) fn format_genre(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map_or_else(unknown, str::to_owned)
}

pub(super) fn now_playing(station_id: i64, live: &RadioLiveState) -> String {
    (live.station_id == Some(station_id) && live.connected)
        .then_some(live.title.as_deref())
        .flatten()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map_or_else(unknown, str::to_owned)
}

pub(super) fn row_is_accented(station_id: i64, live: &RadioLiveState) -> bool {
    live.station_id == Some(station_id) && live.connected
}

pub(super) fn sort_rows(rows: &mut [StationRow]) {
    rows.sort_by(|left, right| {
        let names = left.name.to_lowercase().cmp(&right.name.to_lowercase());
        if names == Ordering::Equal {
            left.id.cmp(&right.id)
        } else {
            names
        }
    });
}

fn unknown() -> String {
    strings::text(strings::RADIO_UNKNOWN_NOW_PLAYING)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn station(id: i64, name: &str) -> StationRow {
        StationRow {
            id,
            uuid: Some(format!("station-{id}")),
            name: name.into(),
            stream_url: format!("https://radio.example/{id}"),
            homepage: None,
            favicon_url: None,
            genre: Some("Metal".into()),
            codec: Some("MP3".into()),
            bitrate_kbps: Some(320),
            country_code: Some("ch".into()),
            votes: Some(42),
            added_at: 10,
            removed_at: None,
        }
    }

    #[test]
    fn radio_rows_format_compact_metadata_and_sort_by_station_name() {
        assert_eq!(format_bitrate(Some(320)), "320k");
        assert_eq!(format_bitrate(None), "—");
        assert_eq!(format_country(Some("ch")), "CH");
        assert_eq!(format_genre(None), "—");

        let mut rows = vec![station(2, "zeta"), station(1, "Alpha")];
        sort_rows(&mut rows);
        assert_eq!(rows.iter().map(|row| row.id).collect::<Vec<_>>(), [1, 2]);
    }

    #[test]
    fn rad_1_now_playing_exists_only_for_the_connected_station() {
        let live = RadioLiveState {
            station_id: Some(1),
            connected: true,
            title: Some("Artist — Song".into()),
        };
        assert_eq!(now_playing(1, &live), "Artist — Song");
        assert_eq!(now_playing(2, &live), "—");

        let paused = RadioLiveState {
            connected: false,
            ..live
        };
        assert_eq!(now_playing(1, &paused), "—");
        assert!(!row_is_accented(1, &paused));
    }
}
