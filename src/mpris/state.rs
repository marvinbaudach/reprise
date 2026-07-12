//! MPRIS/D-Bus wire helpers: the mappings between the platform-neutral
//! `media_integration` state/command types and MPRIS-spec wire values —
//! `LoopStatus` strings (`loop_status_to_repeat`/`repeat_to_loop_status`) and
//! the `zbus::zvariant`-typed `Metadata` dict (`build_metadata`,
//! `track_object_path`). Everything here is Linux/MPRIS-specific and depends
//! on `zbus`; the platform-neutral domain types and pure predicates it builds
//! on live in `crate::media_integration`.
//!
//! Split out of what used to be a single `mpris.rs` in Stage 3 Task 10
//! purely to keep both files under the project's file-size limit — no
//! behavioral seam is implied by the split. `pub(super)` on the non-`pub`
//! items here just means "visible to `mod.rs`", the same idiom `ui::mpris_
//! mirror.rs`/`ui::player_controller.rs` already use for their own
//! sibling-module seam.

use std::collections::HashMap;

use zbus::zvariant::{ObjectPath, OwnedValue, Value};

use crate::media_integration::{ms_to_micros, MprisState};
use crate::queue::Repeat;

/// Maps an MPRIS `LoopStatus` string to `queue::Repeat` — the three exact
/// spec strings ("None"/"Playlist"/"Track") map to `Off`/`All`/`One`
/// respectively; anything else (including a case mismatch — the spec's
/// strings are exact) is `None`, which `MprisPlayer::set_loop_status` turns
/// into a `zbus::fdo::Error::InvalidArgs` rather than panicking or silently
/// picking a default. Pure, so it's unit-testable without any D-Bus/zbus
/// machinery — see this module's `#[cfg(test)]` section.
pub fn loop_status_to_repeat(status: &str) -> Option<Repeat> {
    match status {
        "None" => Some(Repeat::Off),
        "Playlist" => Some(Repeat::All),
        "Track" => Some(Repeat::One),
        _ => None,
    }
}

/// The exact MPRIS spec string for a `queue::Repeat` value — the inverse of
/// [`loop_status_to_repeat`], used by `MprisPlayer::loop_status`'s getter
/// and `emit_property_changes`.
pub fn repeat_to_loop_status(repeat: Repeat) -> &'static str {
    match repeat {
        Repeat::Off => "None",
        Repeat::All => "Playlist",
        Repeat::One => "Track",
    }
}

/// Builds the `mpris:trackid` object path for `track_id` — shared by
/// `build_metadata` and `MprisPlayer::set_position`'s trackid-match check
/// (see that method's doc comment), so the exact path format
/// (`/org/reprise/Reprise/track/<id>`) exists in exactly one place.
pub(super) fn track_object_path(track_id: i64) -> String {
    format!("/org/reprise/Reprise/track/{track_id}")
}

/// Builds the MPRIS `Metadata` dict (`a{sv}`) from `state`. Empty
/// (`{}`, legal per spec) when nothing is loaded. `mpris:trackid` is built
/// via [`track_object_path`] — track ids are DB row ids (see
/// `queries::TrackSummary`), always non-negative decimal digits, which is
/// already a valid D-Bus object path segment, so the `ObjectPath::try_from`
/// here can only fail on a bug elsewhere (e.g. a change to id generation);
/// on that unlikely failure the id is logged and `mpris:trackid` is simply
/// omitted rather than panicking — every other field still gets reported.
/// `mpris:length` is microseconds, not the app's usual milliseconds (MPRIS
/// spec unit) — `xesam:artist` is a list, not a scalar, per spec.
pub(super) fn build_metadata(state: &MprisState) -> HashMap<String, OwnedValue> {
    let mut metadata = HashMap::new();

    let Some(track_id) = state.track_id else {
        return metadata;
    };

    match ObjectPath::try_from(track_object_path(track_id)) {
        Ok(path) => {
            metadata.insert("mpris:trackid".to_string(), OwnedValue::from(path));
        }
        Err(error) => {
            tracing::warn!(%error, track_id, "MPRIS: could not build a valid trackid object path");
        }
    }

    let length_us = ms_to_micros(state.duration_ms);
    metadata.insert("mpris:length".to_string(), OwnedValue::from(length_us));
    insert_owned(
        &mut metadata,
        "xesam:title",
        Value::from(state.title.clone()),
    );
    insert_owned(
        &mut metadata,
        "xesam:artist",
        Value::from(vec![state.artist.clone()]),
    );
    insert_owned(
        &mut metadata,
        "xesam:album",
        Value::from(state.album.clone()),
    );

    metadata
}

/// `String`/`Vec<String>` (unlike the basic types `OwnedValue` converts
/// `From` directly — see the ones used above) only reach `OwnedValue` via
/// the fallible `Value` round-trip (`TryFrom<Value<'_>>`); the only failure
/// mode is a file-descriptor-carrying `Value`, which nothing built here ever
/// is, so a conversion error can only mean a bug — logged and the key is
/// omitted rather than panicking (every other metadata field still gets
/// reported).
fn insert_owned(metadata: &mut HashMap<String, OwnedValue>, key: &str, value: Value<'_>) {
    match OwnedValue::try_from(value) {
        Ok(owned) => {
            metadata.insert(key.to_string(), owned);
        }
        Err(error) => {
            tracing::warn!(%error, key, "MPRIS: failed to convert a metadata value");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media_integration::MprisPlaybackStatus;

    fn playing_state() -> MprisState {
        MprisState {
            status: MprisPlaybackStatus::Playing,
            track_id: Some(1),
            title: "Title".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            duration_ms: 180_000,
            can_next: true,
            can_prev: true,
            position_ms: 0,
            shuffle: false,
            repeat: Repeat::Off,
            volume: 1.0,
        }
    }

    #[test]
    fn build_metadata_is_empty_with_no_track() {
        let state = MprisState::default();
        assert!(build_metadata(&state).is_empty());
    }

    #[test]
    fn build_metadata_populates_expected_keys_and_units() {
        let metadata = build_metadata(&playing_state());

        let trackid = metadata
            .get("mpris:trackid")
            .expect("mpris:trackid present");
        let trackid_path: ObjectPath = trackid
            .clone()
            .try_into()
            .expect("mpris:trackid is an object path");
        assert_eq!(trackid_path.as_str(), "/org/reprise/Reprise/track/1");

        let length: i64 = metadata
            .get("mpris:length")
            .expect("mpris:length present")
            .clone()
            .try_into()
            .expect("mpris:length is an i64");
        // duration_ms (180_000) -> microseconds.
        assert_eq!(length, 180_000_000);

        let title: String = metadata
            .get("xesam:title")
            .expect("xesam:title present")
            .clone()
            .try_into()
            .expect("xesam:title is a string");
        assert_eq!(title, "Title");

        let artist: Vec<String> = metadata
            .get("xesam:artist")
            .expect("xesam:artist present")
            .clone()
            .try_into()
            .expect("xesam:artist is a string list");
        assert_eq!(artist, vec!["Artist".to_string()]);
    }

    #[test]
    fn track_object_path_matches_metadata_trackid() {
        let metadata = build_metadata(&playing_state());
        let trackid: ObjectPath = metadata
            .get("mpris:trackid")
            .expect("mpris:trackid present")
            .clone()
            .try_into()
            .expect("mpris:trackid is an object path");
        assert_eq!(trackid.as_str(), track_object_path(1));
    }

    // ## Pure mapping functions (TDD-first, Stage 3 Task 10)

    #[test]
    fn loop_status_to_repeat_maps_every_spec_string() {
        assert_eq!(loop_status_to_repeat("None"), Some(Repeat::Off));
        assert_eq!(loop_status_to_repeat("Playlist"), Some(Repeat::All));
        assert_eq!(loop_status_to_repeat("Track"), Some(Repeat::One));
    }

    #[test]
    fn loop_status_to_repeat_rejects_invalid_strings() {
        assert_eq!(loop_status_to_repeat("Bogus"), None);
        assert_eq!(loop_status_to_repeat(""), None);
        // Spec strings are exact-case; a case mismatch is still invalid.
        assert_eq!(loop_status_to_repeat("none"), None);
        assert_eq!(loop_status_to_repeat("playlist"), None);
    }

    #[test]
    fn repeat_to_loop_status_is_the_inverse_mapping() {
        assert_eq!(repeat_to_loop_status(Repeat::Off), "None");
        assert_eq!(repeat_to_loop_status(Repeat::All), "Playlist");
        assert_eq!(repeat_to_loop_status(Repeat::One), "Track");
    }

    #[test]
    fn loop_status_round_trips_through_both_mappings() {
        for repeat in [Repeat::Off, Repeat::All, Repeat::One] {
            let status = repeat_to_loop_status(repeat);
            assert_eq!(loop_status_to_repeat(status), Some(repeat));
        }
    }
}
