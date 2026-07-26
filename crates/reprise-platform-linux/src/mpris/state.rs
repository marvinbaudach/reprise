//! MPRIS/D-Bus wire helpers: the mappings between the platform-neutral
//! `media_integration` state/command types and MPRIS-spec wire values —
//! `LoopStatus` strings (`loop_status_to_repeat`/`repeat_to_loop_status`) and
//! the `zbus::zvariant`-typed `Metadata` dict (`build_metadata`,
//! `track_object_path`). Everything here is Linux/MPRIS-specific and depends
//! on `zbus`; the platform-neutral domain types and pure predicates it builds
//! on live in `reprise_core::media_integration`.
//!
//! Split out of what used to be a single `mpris.rs` in Stage 3 Task 10
//! purely to keep both files under the project's file-size limit — no
//! behavioral seam is implied by the split. `pub(super)` on the non-`pub`
//! items here just means "visible to `mod.rs`", the same idiom `ui::mpris_
//! mirror.rs`/`ui::player_controller.rs` already use for their own
//! sibling-module seam.

use std::collections::HashMap;

use zbus::zvariant::{ObjectPath, OwnedValue, Value};

use reprise_core::media_integration::{
    can_go_next, can_go_previous, can_seek, micros_to_ms, ms_to_micros, MprisCommand, MprisState,
};
use reprise_core::queue::Repeat;

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

fn external_object_path(external_ref: &str) -> String {
    format!("/org/reprise/Reprise/external/{external_ref}")
}

pub(super) fn current_media_object_path(state: &MprisState) -> Option<String> {
    state
        .external_ref
        .as_deref()
        .map(external_object_path)
        .or_else(|| state.track_id.map(track_object_path))
}

pub(super) fn next_command(state: &MprisState) -> Option<MprisCommand> {
    can_go_next(state).then_some(MprisCommand::Next)
}

pub(super) fn previous_command(state: &MprisState) -> Option<MprisCommand> {
    can_go_previous(state).then_some(MprisCommand::Previous)
}

pub(super) fn seek_command(state: &MprisState, offset_us: i64) -> Option<MprisCommand> {
    can_seek(state).then_some(MprisCommand::Seek(micros_to_ms(offset_us)))
}

pub(super) fn set_position_command(
    state: &MprisState,
    requested_path: &str,
    position_us: i64,
) -> Option<MprisCommand> {
    if !can_seek(state) || current_media_object_path(state).as_deref() != Some(requested_path) {
        return None;
    }
    let position_ms = micros_to_ms(position_us).clamp(0, state.duration_ms.max(0));
    Some(MprisCommand::SetPosition(position_ms))
}

/// Builds the MPRIS `Metadata` dict (`a{sv}`) from `state`. Empty
/// (`{}`, legal per spec) when nothing is loaded. Library `mpris:trackid`
/// values are built via [`track_object_path`] — track ids are DB row ids (see
/// `queries::TrackSummary`), always non-negative decimal digits, which is
/// already a valid D-Bus object path segment, so the `ObjectPath::try_from`
/// here can only fail on a bug elsewhere (e.g. a change to id generation);
/// on that unlikely failure the id is logged and `mpris:trackid` is simply
/// omitted rather than panicking — every other field still gets reported.
/// `mpris:length` is microseconds, not the app's usual milliseconds (MPRIS
/// spec unit) — `xesam:artist` is a list, not a scalar, per spec. When the
/// asynchronous cover pipeline has resolved an image, its file URI is
/// exposed as `mpris:artUrl`; without a cover that optional key is omitted.
pub(super) fn build_metadata(state: &MprisState) -> HashMap<String, OwnedValue> {
    let mut metadata = HashMap::new();

    let Some(identity) = current_media_object_path(state) else {
        return metadata;
    };

    match ObjectPath::try_from(identity.as_str()) {
        Ok(path) => {
            metadata.insert("mpris:trackid".to_string(), OwnedValue::from(path));
        }
        Err(error) => {
            tracing::warn!(%error, identity, "MPRIS: could not build a valid trackid object path");
        }
    }

    if !state.live_stream {
        let length_us = ms_to_micros(state.duration_ms);
        metadata.insert("mpris:length".to_string(), OwnedValue::from(length_us));
    }
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
    if let Some(art_url) = &state.art_url {
        insert_owned(&mut metadata, "mpris:artUrl", Value::from(art_url.clone()));
    }

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
    use reprise_core::media_integration::MprisPlaybackStatus;

    fn playing_state() -> MprisState {
        MprisState {
            status: MprisPlaybackStatus::Playing,
            track_id: Some(1),
            external_ref: None,
            live_stream: false,
            title: "Title".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            art_url: None,
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
    fn build_metadata_includes_art_url_when_available() {
        let state = MprisState {
            art_url: Some("file:///cache/reprise/covers/album.png".into()),
            ..playing_state()
        };
        let metadata = build_metadata(&state);

        let art_url: String = metadata
            .get("mpris:artUrl")
            .expect("mpris:artUrl present")
            .clone()
            .try_into()
            .expect("mpris:artUrl is a string");
        assert_eq!(art_url, "file:///cache/reprise/covers/album.png");
    }

    #[test]
    fn build_metadata_omits_art_url_without_a_cover() {
        let metadata = build_metadata(&playing_state());
        assert!(!metadata.contains_key("mpris:artUrl"));
    }

    #[test]
    fn rad_2_live_metadata_uses_external_identity_and_omits_length() {
        let state = MprisState {
            track_id: None,
            external_ref: Some("radio/7".into()),
            live_stream: true,
            title: "Current song".into(),
            artist: "Example Radio".into(),
            art_url: Some("https://example.test/radio.png".into()),
            ..playing_state()
        };
        let metadata = build_metadata(&state);
        let trackid: ObjectPath = metadata["mpris:trackid"]
            .clone()
            .try_into()
            .expect("external track id is an object path");
        assert_eq!(trackid.as_str(), "/org/reprise/Reprise/external/radio/7");
        assert!(!metadata.contains_key("mpris:length"));
        let art_url: String = metadata["mpris:artUrl"]
            .clone()
            .try_into()
            .expect("remote art URL remains a string");
        assert_eq!(art_url, "https://example.test/radio.png");
    }

    #[test]
    fn podcast_external_transport_keeps_seek_and_blocks_queue_navigation() {
        let state = MprisState {
            track_id: None,
            external_ref: Some("podcast/42".into()),
            live_stream: false,
            duration_ms: 180_000,
            can_next: true,
            can_prev: true,
            ..playing_state()
        };

        assert_eq!(next_command(&state), None);
        assert_eq!(previous_command(&state), None);
        assert_eq!(
            seek_command(&state, 5_500_000),
            Some(MprisCommand::Seek(5_500))
        );
        assert_eq!(
            set_position_command(
                &state,
                "/org/reprise/Reprise/external/podcast/42",
                42_000_000,
            ),
            Some(MprisCommand::SetPosition(42_000))
        );
        assert_eq!(
            set_position_command(
                &state,
                "/org/reprise/Reprise/external/podcast/41",
                42_000_000,
            ),
            None
        );
    }

    #[test]
    fn live_external_transport_rejects_every_seek_command() {
        let state = MprisState {
            track_id: None,
            external_ref: Some("radio/7".into()),
            live_stream: true,
            ..playing_state()
        };
        assert_eq!(seek_command(&state, 5_000_000), None);
        assert_eq!(
            set_position_command(&state, "/org/reprise/Reprise/external/radio/7", 5_000_000,),
            None
        );
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
