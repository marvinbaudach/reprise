//! Platform-neutral OS media-integration contract (core-destined): the
//! now-playing state mirror (`MprisState`), the transport-command vocabulary
//! (`MprisCommand`), the coarse `MprisPlaybackStatus`, the pure predicates and
//! ms/µs conversions between them and the app's units, and the
//! `MediaIntegrationHandles` a platform's `start(…)` constructor hands back.
//!
//! These types keep their MPRIS-derived names (accepted, documented naming
//! debt — see the plan's "Explicitly NOT doing" item): the semantics are
//! already platform-neutral (SMTC and MPNowPlayingInfoCenter have the same
//! shape). The D-Bus wire helpers that turn this state into MPRIS-specific
//! `zbus::zvariant` values live platform-side in `mpris/state.rs`.

use std::sync::{Arc, Mutex};

use crate::queue::Repeat;

/// `MprisState::volume`'s initial value until the bar or an MPRIS `Volume`
/// write sets a real one — "full volume". `pub(crate)` (Stage-3 close-out:
/// this used to be duplicated verbatim as its own private `const` in
/// `ui::player_controller`, which `PlayerController::volume`'s initial
/// `Cell` value now instead reads from here via `crate::mpris::
/// DEFAULT_VOLUME`, so there is exactly one definition to keep in sync).
/// Still kept separate from `ui::player_bar`'s own `VOLUME_DEFAULT` — that
/// one is the volume *slider widget*'s reset value, a UI concern this
/// module deliberately doesn't depend on (this module has no `ui`
/// dependency at all); the two are kept in sync by convention only, per
/// that constant's own doc comment.
pub const DEFAULT_VOLUME: f64 = 1.0;

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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Playing => "Playing",
            Self::Paused => "Paused",
            Self::Stopped => "Stopped",
        }
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
pub fn can_play(state: &MprisState) -> bool {
    state.track_id.is_some() || state.can_next || state.can_prev
}

/// `CanPause`: whether there is a current track that could be paused. Like
/// `can_play` (see there for the field bug), this is an intrinsic property
/// of having a track loaded, not of the current playing/paused status —
/// per spec, "its value should not depend on whether the track is
/// currently paused or playing". A `Pause` call while already paused or
/// stopped is simply a no-op `PlayerController::mpris_pause`
/// short-circuits.
pub fn can_pause(state: &MprisState) -> bool {
    state.track_id.is_some()
}

/// `CanSeek`: whether seeking is meaningful right now — exactly the same
/// intrinsic "is a track loaded" condition as [`can_pause`] (seeking a
/// track that isn't loaded is meaningless regardless of play/pause status,
/// same reasoning as that function's own doc comment), so this simply
/// reuses it rather than duplicating the identical check under a different
/// name.
pub fn can_seek(state: &MprisState) -> bool {
    can_pause(state)
}

/// Whether the `Metadata` dict-valued property needs re-emitting: any field
/// that feeds [`build_metadata`] changed.
pub fn metadata_differs(a: &MprisState, b: &MprisState) -> bool {
    a.track_id != b.track_id
        || a.title != b.title
        || a.artist != b.artist
        || a.album != b.album
        || a.duration_ms != b.duration_ms
}

/// `Arc<Mutex<_>>` handle to the mirror — `Arc` so both `PlayerController`
/// (writer, GTK main thread) and this module's `MprisPlayer` interface
/// object (reader, MPRIS thread) can hold it independently of either side's
/// lifetime.
pub type SharedMprisState = Arc<Mutex<MprisState>>;

/// Reads `state` through the same poisoned-recovery pattern `player.rs`
/// uses everywhere it locks: a panic on one side of the mutex must not
/// poison MPRIS (or the controller) permanently.
pub fn read_state(state: &SharedMprisState) -> MprisState {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// The OS media-integration contract (Linux: MPRIS/D-Bus in `mpris/`;
/// future macOS/Windows: MPNowPlayingInfoCenter / SMTC). Handle-shaped, not
/// a trait: the app writes now-playing snapshots into `shared_state`,
/// drains OS transport commands from `commands`, and feeds `seek_notify`
/// (µs) after every successful seek; the platform side owns whatever
/// threads/connections it needs. Each platform crate provides a
/// `start(…) -> MediaIntegrationHandles` constructor.
pub struct MediaIntegrationHandles {
    pub shared_state: SharedMprisState,
    pub commands: async_channel::Receiver<MprisCommand>,
    pub seek_notify: async_channel::Sender<i64>,
}

impl MediaIntegrationHandles {
    /// Dormant handles for when OS media integration is switched off (the
    /// `module.mpris.enabled = 0` path, module registry). Constructs the
    /// exact same channels a real platform `start(…)` does — a fresh default
    /// `SharedMprisState`, an `MprisCommand` receiver, and a seek-notify
    /// sender — but **spawns no thread and touches no bus**: the command
    /// receiver's sender is dropped here, so `recv()` never yields; the
    /// seek-notify sender's receiver is dropped, so the controller's
    /// after-seek `try_send`s are silently discarded.
    ///
    /// This is deliberately indistinguishable, from the app's perspective,
    /// from the platform side's "no session bus" degradation (see
    /// `reprise-platform-linux`'s `mpris` module `## Failure is never fatal`
    /// section): there too the controller keeps writing to a mirror nobody
    /// reads and `try_send`ing to a receiver nobody drains. The disabled
    /// module therefore needs no platform code at all — the whole
    /// dormant-handle construction lives in this cross-platform core.
    pub fn inert() -> Self {
        let shared_state: SharedMprisState = Arc::new(Mutex::new(MprisState::default()));
        let (_commands_sender, commands) = async_channel::unbounded::<MprisCommand>();
        let (seek_notify, _seek_receiver) = async_channel::unbounded::<i64>();

        Self {
            shared_state,
            commands,
            seek_notify,
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
