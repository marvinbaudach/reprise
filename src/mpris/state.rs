//! Pure MPRIS domain types and mapping functions: `MprisState`, `MprisCommand`,
//! `MprisPlaybackStatus`, and the conversions between them and MPRIS-spec
//! wire values (`LoopStatus` strings, µs positions). No zbus/D-Bus/thread
//! machinery lives here — see `mpris` (the parent module)'s doc comment for
//! that; everything below is plain data and free functions, independently
//! unit-testable without any D-Bus session, which is also why the pure
//! mapping functions the Stage 3 Task 10 brief asked to TDD-first live here
//! rather than in `mod.rs` — see this file's `#[cfg(test)]` section.
//!
//! Split out of what used to be a single `mpris.rs` in Stage 3 Task 10
//! purely to keep both files under the project's file-size limit — no
//! behavioral seam is implied by the split. `pub(super)` on the non-`pub`
//! items here just means "visible to `mod.rs`", the same idiom `ui::mpris_
//! mirror.rs`/`ui::player_controller.rs` already use for their own
//! sibling-module seam; `mod.rs` re-exports the `pub` items (`MprisCommand`,
//! `MprisState`, `MprisPlaybackStatus`, the four pure mapping functions) so
//! `crate::mpris::X` paths elsewhere in the crate are unaffected by the
//! split.

use std::collections::HashMap;

use zbus::zvariant::{ObjectPath, OwnedValue, Value};

use crate::queue::Repeat;

/// `MprisState::volume`'s initial value until the bar or an MPRIS `Volume`
/// write sets a real one — "full volume", matching `ui::player_bar`'s own
/// `VOLUME_DEFAULT` (a separate constant: this module doesn't depend on
/// `ui`, only the other direction, so the two are kept in sync by
/// convention, not by sharing one definition).
const DEFAULT_VOLUME: f64 = 1.0;

/// Commands the MPRIS `Player` interface's transport methods (`Play`,
/// `Pause`, …) send into the app over an `async_channel`, drained by
/// `PlayerController` on the GTK main thread exactly like `PlayerEvent` is
/// (see `ui::player_controller`'s module doc comment) — `try_send` from
/// `mod.rs`'s D-Bus method handlers, `recv().await` inside a `glib::
/// spawn_future_local` loop on the receiving end.
///
/// `Seek`/`SetPosition` carry an already-µs-to-ms-converted value (see
/// `micros_to_ms`) — everything past `mod.rs`'s own D-Bus method bodies
/// stays in the app's usual milliseconds, exactly like every other
/// controller-facing type. `Seek`'s payload is a *relative* offset (may be
/// negative); `SetPosition`'s is the *absolute* target, already validated
/// against the current track's `mpris:trackid` and clamped to
/// `0..=duration_ms` by `MprisPlayer::set_position` before it's ever sent
/// (a trackid mismatch, per spec, is simply never dispatched at all — see
/// that method's doc comment).
///
/// Not `Eq` (only `PartialEq`): `SetVolume`'s `f64` payload doesn't
/// implement `Eq`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MprisCommand {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
    Seek(i64),
    SetPosition(i64),
    SetShuffle(bool),
    SetLoop(Repeat),
    SetVolume(f64),
}

/// Coarse playback status mirrored for MPRIS. Deliberately a separate type
/// from `player::PlaybackState` rather than reusing it: this module has no
/// other dependency on `crate::player` (or anything else app-specific)
/// today, which keeps it independently testable and makes the mapping from
/// `PlaybackState` an explicit, single, documented conversion (`ui::
/// player_controller::mpris_status_from_playback_state`) instead of an
/// implicit type alias two modules would otherwise both need to agree on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MprisPlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

impl MprisPlaybackStatus {
    /// The exact MPRIS spec string for `PlaybackStatus` — "Playing",
    /// "Paused", or "Stopped". `pub(super)` so `mod.rs`'s `playback_status`
    /// property getter and `emit_property_changes` can call it.
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Playing => "Playing",
            Self::Paused => "Paused",
            Self::Stopped => "Stopped",
        }
    }
}

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

/// Milliseconds — the app's unit everywhere outside this module — to
/// microseconds, MPRIS's unit for `Position` and `mpris:length`. Clamped at
/// 0 first: both callers ([`MprisPlayer`](super::MprisPlayer)'s `position`
/// property and `build_metadata`'s `mpris:length`) only ever convert values
/// that are never legitimately negative, and `saturating_mul` alone would
/// still produce a valid-looking (if wrong) negative microsecond value for
/// negative input.
pub fn ms_to_micros(ms: i64) -> i64 {
    ms.max(0).saturating_mul(1000)
}

/// Microseconds to milliseconds — the inverse of [`ms_to_micros`], used to
/// bring an incoming MPRIS `Seek`/`SetPosition` argument into the app's
/// internal ms unit. Deliberately *not* clamped at 0 here (unlike
/// `ms_to_micros`): `Seek`'s offset is relative and can legitimately be
/// negative (seeking backward) — clamping the resulting *absolute* target
/// position is the caller's job (`MprisPlayer::seek`/`set_position` and
/// `ui::mpris_mirror`'s handling of `MprisCommand::Seek`), not this
/// conversion's. Integer division truncates any sub-millisecond precision
/// MPRIS can technically send but the app never distinguishes.
pub fn micros_to_ms(us: i64) -> i64 {
    us / 1000
}

/// Shared state mirror: written by `PlayerController::update_mpris_mirror`
/// on every real state/track transition, read by `MprisPlayer`'s property
/// getters (always live, via the shared mutex) and diffed every poll
/// interval to drive `PropertiesChanged` (see `mod.rs`'s `## PropertiesChanged`
/// doc section). `can_next`/`can_prev` intentionally mirror the *same*
/// granularity `PlayerBar::set_transport_enabled` already uses — "queue is
/// non-empty", not a finer "not at the first/last track" distinction the
/// rest of the app doesn't compute anywhere either (see `update_mpris_
/// mirror`'s doc comment for the full reasoning).
///
/// `position_ms` is written far more often than the rest of this struct —
/// every ~500 ms position tick and every seek (`ui::mpris_mirror`'s
/// `update_mpris_position`), independently of the other fields'
/// `update_mpris_mirror` rebuilds — and is deliberately excluded from
/// `emit_property_changes`'s `PropertiesChanged` diff (see that function's
/// doc comment in `mod.rs`): the MPRIS spec exempts `Position` from
/// property-change notification entirely, since it would otherwise fire on
/// every poll tick during playback; `Seeked` is the only signal clients
/// should use to learn of position *jumps*, and continuous playback is
/// expected to be interpolated client-side from `Position` + `Rate`.
#[derive(Debug, Clone, PartialEq)]
pub struct MprisState {
    pub status: MprisPlaybackStatus,
    pub track_id: Option<i64>,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
    pub can_next: bool,
    pub can_prev: bool,
    /// See the struct doc comment's note on why this is excluded from
    /// `PropertiesChanged`.
    pub position_ms: i64,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub volume: f64,
}

impl Default for MprisState {
    fn default() -> Self {
        Self {
            status: MprisPlaybackStatus::Stopped,
            track_id: None,
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            duration_ms: 0,
            can_next: false,
            can_prev: false,
            position_ms: 0,
            shuffle: false,
            repeat: Repeat::Off,
            volume: DEFAULT_VOLUME,
        }
    }
}

/// `CanPlay`: whether this player has anything it could play at all — a
/// current/resumable track (`track_id` set) or somewhere the queue could
/// jump to and start (`can_next`/`can_prev`, which mirror "queue is
/// non-empty" — see the `MprisState` doc comment).
///
/// Deliberately **independent of the current playback status** (field bug,
/// Stage 3): the MPRIS spec says CanPlay "is related to whether there is a
/// 'current track': its value should not depend on whether the track is
/// currently paused or playing", and GNOME Shell enforces exactly that
/// reading — its `MprisSource` (js/ui/mpris.js) filters the media widget's
/// player list by `CanPlay` and emits `player-removed` the moment it flips
/// false. The previous `status != Playing && …` implementation therefore
/// made the lock-screen/quick-settings media controls *vanish the moment
/// playback started* and reappear only while paused.
pub(super) fn can_play(state: &MprisState) -> bool {
    state.track_id.is_some() || state.can_next || state.can_prev
}

/// `CanPause`: whether there is a current track that could be paused. Like
/// `can_play` (see there for the field bug), this is an intrinsic property
/// of having a track loaded, not of the current playing/paused status —
/// per spec, "its value should not depend on whether the track is
/// currently paused or playing". A `Pause` call while already paused or
/// stopped is simply a no-op `PlayerController::mpris_pause`
/// short-circuits.
pub(super) fn can_pause(state: &MprisState) -> bool {
    state.track_id.is_some()
}

/// `CanSeek`: whether seeking is meaningful right now — exactly the same
/// intrinsic "is a track loaded" condition as [`can_pause`] (seeking a
/// track that isn't loaded is meaningless regardless of play/pause status,
/// same reasoning as that function's own doc comment), so this simply
/// reuses it rather than duplicating the identical check under a different
/// name.
pub(super) fn can_seek(state: &MprisState) -> bool {
    can_pause(state)
}

/// Builds the `mpris:trackid` object path for `track_id` — shared by
/// `build_metadata` and `MprisPlayer::set_position`'s trackid-match check
/// (see that method's doc comment), so the exact path format
/// (`/org/reprise/Reprise/track/<id>`) exists in exactly one place.
pub(super) fn track_object_path(track_id: i64) -> String {
    format!("/org/reprise/Reprise/track/{track_id}")
}

/// Whether the `Metadata` dict-valued property needs re-emitting: any field
/// that feeds [`build_metadata`] changed.
pub(super) fn metadata_differs(a: &MprisState, b: &MprisState) -> bool {
    a.track_id != b.track_id
        || a.title != b.title
        || a.artist != b.artist
        || a.album != b.album
        || a.duration_ms != b.duration_ms
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
    fn playback_status_strings_match_the_mpris_spec() {
        assert_eq!(MprisPlaybackStatus::Playing.as_str(), "Playing");
        assert_eq!(MprisPlaybackStatus::Paused.as_str(), "Paused");
        assert_eq!(MprisPlaybackStatus::Stopped.as_str(), "Stopped");
    }

    /// Regression test for the lock-screen field bug: GNOME Shell removes a
    /// player from the media widget the moment `CanPlay` goes false, so it
    /// must stay true while a track is loaded — playing or paused alike
    /// (CanPlay is intrinsic to having a current track, per spec).
    #[test]
    fn can_play_true_while_playing_with_a_track() {
        assert!(can_play(&playing_state()));
    }

    #[test]
    fn can_play_true_when_paused_with_a_track() {
        let state = MprisState {
            status: MprisPlaybackStatus::Paused,
            ..playing_state()
        };
        assert!(can_play(&state));
    }

    #[test]
    fn can_play_true_when_stopped_but_queue_has_something() {
        let state = MprisState {
            status: MprisPlaybackStatus::Stopped,
            track_id: None,
            can_next: true,
            can_prev: false,
            ..playing_state()
        };
        assert!(can_play(&state));
    }

    #[test]
    fn can_play_false_when_stopped_with_nothing_queued() {
        let state = MprisState {
            status: MprisPlaybackStatus::Stopped,
            track_id: None,
            can_next: false,
            can_prev: false,
            ..playing_state()
        };
        assert!(!can_play(&state));
    }

    /// `CanPause` is intrinsic to having a track loaded (spec: independent
    /// of playing/paused status), not a "would Pause do something" flag.
    #[test]
    fn can_pause_true_whenever_a_track_is_loaded() {
        assert!(can_pause(&playing_state()));
        let paused = MprisState {
            status: MprisPlaybackStatus::Paused,
            ..playing_state()
        };
        assert!(can_pause(&paused));
    }

    #[test]
    fn can_pause_false_with_no_track_loaded() {
        let no_track = MprisState {
            track_id: None,
            ..playing_state()
        };
        assert!(!can_pause(&no_track));
        assert!(!can_pause(&MprisState::default()));
    }

    #[test]
    fn metadata_differs_detects_every_tracked_field() {
        let base = playing_state();
        assert!(!metadata_differs(&base, &base));

        let mut other = base.clone();
        other.track_id = Some(2);
        assert!(metadata_differs(&base, &other));

        let mut other = base.clone();
        other.title = "Other".into();
        assert!(metadata_differs(&base, &other));

        let mut other = base.clone();
        other.artist = "Other".into();
        assert!(metadata_differs(&base, &other));

        let mut other = base.clone();
        other.album = "Other".into();
        assert!(metadata_differs(&base, &other));

        let mut other = base.clone();
        other.duration_ms = 1;
        assert!(metadata_differs(&base, &other));

        // can_next/can_prev don't feed Metadata.
        let mut other = base.clone();
        other.can_next = !other.can_next;
        assert!(!metadata_differs(&base, &other));
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
    fn default_state_is_stopped_with_nothing_loaded() {
        let state = MprisState::default();
        assert_eq!(state.status, MprisPlaybackStatus::Stopped);
        assert_eq!(state.track_id, None);
        assert!(!state.can_next);
        assert!(!state.can_prev);
        assert_eq!(state.position_ms, 0);
        assert!(!state.shuffle);
        assert_eq!(state.repeat, Repeat::Off);
        assert_eq!(state.volume, DEFAULT_VOLUME);
    }

    /// Stage 3 Task 10: `CanSeek` mirrors `CanPause`'s "track loaded"
    /// semantics exactly (see `can_seek`'s doc comment) — proven by
    /// checking every case `can_pause`'s own tests already cover.
    #[test]
    fn can_seek_true_whenever_a_track_is_loaded() {
        assert!(can_seek(&playing_state()));
        let paused = MprisState {
            status: MprisPlaybackStatus::Paused,
            ..playing_state()
        };
        assert!(can_seek(&paused));
    }

    #[test]
    fn can_seek_false_with_no_track_loaded() {
        let no_track = MprisState {
            track_id: None,
            ..playing_state()
        };
        assert!(!can_seek(&no_track));
        assert!(!can_seek(&MprisState::default()));
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

    #[test]
    fn ms_to_micros_multiplies_by_a_thousand() {
        assert_eq!(ms_to_micros(0), 0);
        assert_eq!(ms_to_micros(1), 1_000);
        assert_eq!(ms_to_micros(1_500), 1_500_000);
    }

    #[test]
    fn ms_to_micros_clamps_negative_input_at_zero() {
        assert_eq!(ms_to_micros(-1), 0);
        assert_eq!(ms_to_micros(-5_000), 0);
    }

    #[test]
    fn micros_to_ms_divides_by_a_thousand() {
        assert_eq!(micros_to_ms(0), 0);
        assert_eq!(micros_to_ms(1_000), 1);
        assert_eq!(micros_to_ms(1_500_000), 1_500);
    }

    #[test]
    fn micros_to_ms_preserves_sign_for_relative_offsets() {
        // Seek's offset is relative and can legitimately be negative
        // (seeking backward) — unlike ms_to_micros, this must not clamp.
        assert_eq!(micros_to_ms(-2_000_000), -2_000);
    }

    #[test]
    fn micros_to_ms_truncates_sub_millisecond_precision() {
        assert_eq!(micros_to_ms(1_999), 1);
    }
}
