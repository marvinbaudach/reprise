//! MPRIS (Media Player Remote Interfacing Specification) integration —
//! Stage 2 Task 6. Exposes `org.mpris.MediaPlayer2` and
//! `org.mpris.MediaPlayer2.Player` on the session bus as
//! `org.mpris.MediaPlayer2.reprise`, so GNOME Shell's media widget, the
//! lock screen, and hardware/media keys can see and control playback.
//!
//! ## Thread model: a dedicated OS thread, `zbus::blocking`, no tokio
//!
//! The rest of the app has exactly one async runtime in it: `glib`'s
//! `MainContext`, driven by `gtk4::glib::spawn_future_local` (see
//! `ui::player_controller`'s module doc comment). Pulling zbus's `async-io`
//! executor onto that same main-thread context — or worse, adding tokio —
//! would mean two runtimes contending for the process, for no benefit: MPRIS
//! traffic (a handful of property reads and Play/Pause/Next-class method
//! calls) is trivially low-throughput. So this module runs on its own
//! `std::thread::spawn` thread, using `zbus::blocking::connection::Builder`
//! end to end: connect, claim the bus name, serve both interfaces, then
//! block that thread forever in a poll loop (see below). `zbus`'s default
//! features (`async-io` + `blocking-api`) are kept in `Cargo.toml` — that
//! `async-io` is zbus's own internal reactor the blocking API uses to drive
//! futures via `zbus::block_on`, not a runtime this module (or the rest of
//! the app) has to integrate with; the `tokio` feature, the thing the task's
//! "no tokio" constraint actually rules out, is off by default and stays
//! off.
//!
//! ## Shared state: `Arc<Mutex<MprisState>>`, poison-recovered like `player.rs`
//!
//! `PlayerController` (the GTK-main-thread owner of playback state) writes a
//! fresh `MprisState` snapshot into the shared mutex on every real state/
//! track transition (`update_mpris_mirror`, see `ui::player_controller`).
//! This thread's `MprisPlayer` interface reads through the same mutex on
//! every D-Bus property `Get`/`GetAll` call, so reads are always live — the
//! mutex is genuinely the source of truth, not a cache that can go stale
//! between polls. Every lock site here uses the same poisoned-recovery
//! pattern `player.rs` established (`.lock().unwrap_or_else(|poisoned|
//! poisoned.into_inner())`): a panic on one side (e.g. mid state-update on
//! the GTK thread) must not take MPRIS down with it, or vice versa.
//!
//! ## PropertiesChanged: 500 ms diff-poll, not push
//!
//! MPRIS clients (GNOME Shell's media widget in particular) expect a
//! `org.freedesktop.DBus.Properties.PropertiesChanged` signal on every
//! status/track change rather than polling `Get` themselves. `zbus`
//! supports pushing this directly from the interface's own methods via a
//! `SignalEmitter`, but that emitter is only reachable from *inside* this
//! module (via `ObjectServer::interface`), and the writes happen on the GTK
//! main thread inside `PlayerController` — wiring a cross-thread signal-
//! emitter handle back into `player_controller.rs` just to call `zbus::
//! block_on` from GTK code (blocking the main thread on D-Bus I/O, exactly
//! what must never happen there) is a worse trade than the alternative the
//! task brief explicitly allows: this thread, which already owns the
//! connection and blocks freely, wakes up every 500 ms, diffs the current
//! `MprisState` mirror against the last snapshot it saw, and emits
//! `PropertiesChanged` for whatever actually changed. 500 ms matches the
//! player's own position-tick cadence (see `player.rs`), so status/track
//! changes are visible to clients within one tick of the same granularity
//! the rest of the app already accepts — imperceptible for a "did playback
//! start/pause/skip" notification, and far simpler than threading a
//! `SignalEmitter` through the `PlayerEvent`/`MprisCommand` channel
//! machinery for a difference no user will ever notice.
//!
//! ## Failure is never fatal
//!
//! [`start`] never returns a `Result`: it always hands back a working
//! `SharedMprisState` and `async_channel::Receiver<MprisCommand>`, and
//! always spawns the thread. If there's no session bus, the name is already
//! taken, or anything else about claiming it fails, the thread logs a
//! `tracing::warn!` and returns — the returned state/receiver just sit
//! there unused (the controller keeps writing to a mirror nobody reads;
//! `try_send`ing to a receiver nobody drains from the D-Bus side, which
//! simply never happens since no method calls can arrive). No panics, no
//! `Result` for callers to thread through `window::build`/`main.rs`, and
//! the rest of the app runs exactly as if this module didn't exist.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use zbus::blocking::connection;
use zbus::blocking::object_server::InterfaceRef;
use zbus::names::InterfaceName;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};
use zbus::{fdo, interface};

/// Well-known bus name this app claims — must match the MPRIS spec's
/// `org.mpris.MediaPlayer2.<name>` convention (GNOME Shell and friends
/// discover media players by enumerating names under this prefix).
pub const BUS_NAME: &str = "org.mpris.MediaPlayer2.reprise";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const IDENTITY: &str = "Reprise";
/// Must match the app's `.desktop` file id / `APP_ID` in `main.rs` — MPRIS
/// clients use this to look up the app's icon/launcher entry.
const DESKTOP_ENTRY: &str = "org.reprise.Reprise";

/// Interface name literals duplicated as both the `#[interface(name = ..)]`
/// macro attribute (which requires a string literal, not a `const`
/// reference, so it can't reuse this directly) and this constant (used by
/// `emit_property_changes` to build the `PropertiesChanged` signal). Keep
/// both in sync if either ever changes.
const PLAYER_INTERFACE_NAME: &str = "org.mpris.MediaPlayer2.Player";

/// How often the MPRIS thread wakes up to diff the shared mirror against
/// its last-seen snapshot and emit `PropertiesChanged` for whatever
/// changed. See the module's `## PropertiesChanged` doc section.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Commands the MPRIS `Player` interface's transport methods (`Play`,
/// `Pause`, …) send into the app over an `async_channel`, drained by
/// `PlayerController` on the GTK main thread exactly like `PlayerEvent` is
/// (see `ui::player_controller`'s module doc comment) — `try_send` from
/// this module's D-Bus method handlers, `recv().await` inside a `glib::
/// spawn_future_local` loop on the receiving end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MprisCommand {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
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
    /// "Paused", or "Stopped".
    fn as_str(self) -> &'static str {
        match self {
            Self::Playing => "Playing",
            Self::Paused => "Paused",
            Self::Stopped => "Stopped",
        }
    }
}

/// Shared state mirror: written by `PlayerController::update_mpris_mirror`
/// on every real state/track transition, read by `MprisPlayer`'s property
/// getters (always live, via the shared mutex) and diffed every
/// [`POLL_INTERVAL`] to drive `PropertiesChanged` (see the module's
/// `## PropertiesChanged` doc section). `can_next`/`can_prev` intentionally
/// mirror the *same* granularity `PlayerBar::set_transport_enabled` already
/// uses — "queue is non-empty", not a finer "not at the first/last track"
/// distinction the rest of the app doesn't compute anywhere either (see
/// `update_mpris_mirror`'s doc comment for the full reasoning).
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
        }
    }
}

/// `Arc<Mutex<_>>` handle to the mirror — `Arc` so both `PlayerController`
/// (writer, GTK main thread) and this module's `MprisPlayer` interface
/// object (reader, MPRIS thread) can hold it independently of either side's
/// lifetime.
pub type SharedMprisState = Arc<Mutex<MprisState>>;

/// Reads `state` through the same poisoned-recovery pattern `player.rs`
/// uses everywhere it locks: a panic on one side of the mutex must not
/// poison MPRIS (or the controller) permanently.
fn read_state(state: &SharedMprisState) -> MprisState {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// Starts the MPRIS integration: spawns the dedicated D-Bus thread (see the
/// module's `## Thread model` doc section) and returns the shared state
/// mirror plus the command receiver, unconditionally — never a `Result`.
/// See the module's `## Failure is never fatal` doc section for what
/// happens if the thread can't actually claim the bus name.
pub fn start() -> (SharedMprisState, async_channel::Receiver<MprisCommand>) {
    let state: SharedMprisState = Arc::new(Mutex::new(MprisState::default()));
    let (sender, receiver) = async_channel::unbounded::<MprisCommand>();

    let thread_state = state.clone();
    std::thread::spawn(move || run(thread_state, sender));

    (state, receiver)
}

/// The MPRIS thread's entire body: connect, claim the name, serve both
/// interfaces, then poll-diff forever. Every fallible step logs a
/// `tracing::warn!` and returns (never panics) on failure — see the
/// module's `## Failure is never fatal` doc section.
fn run(state: SharedMprisState, commands: async_channel::Sender<MprisCommand>) {
    let player_iface = MprisPlayer {
        state: state.clone(),
        commands,
    };

    let connection = connection::Builder::session()
        .and_then(|builder| builder.name(BUS_NAME))
        .and_then(|builder| builder.serve_at(OBJECT_PATH, MprisRoot))
        .and_then(|builder| builder.serve_at(OBJECT_PATH, player_iface))
        .and_then(|builder| builder.build());
    let connection = match connection {
        Ok(connection) => connection,
        Err(error) => {
            tracing::warn!(
                %error,
                bus_name = BUS_NAME,
                "MPRIS unavailable: could not claim the session bus name; \
                 continuing without media-key/lock-screen integration"
            );
            return;
        }
    };

    let iface_ref = match connection
        .object_server()
        .interface::<_, MprisPlayer>(OBJECT_PATH)
    {
        Ok(iface_ref) => iface_ref,
        Err(error) => {
            tracing::warn!(
                %error,
                "MPRIS unavailable: could not obtain the Player interface reference \
                 needed for PropertiesChanged notifications"
            );
            return;
        }
    };

    tracing::info!(
        bus_name = BUS_NAME,
        path = OBJECT_PATH,
        "MPRIS: bus name claimed, serving org.mpris.MediaPlayer2 and .Player"
    );

    poll_and_emit_changes(&state, &iface_ref);
}

/// Loops forever (this is the entire lifetime of the MPRIS thread once
/// connected): wakes every [`POLL_INTERVAL`], and emits `PropertiesChanged`
/// for whatever differs from the last snapshot seen. See the module's
/// `## PropertiesChanged` doc section.
fn poll_and_emit_changes(state: &SharedMprisState, iface_ref: &InterfaceRef<MprisPlayer>) {
    let mut last = read_state(state);
    loop {
        std::thread::sleep(POLL_INTERVAL);
        let current = read_state(state);
        if current != last {
            emit_property_changes(&last, &current, iface_ref);
            last = current;
        }
    }
}

/// Diffs `previous`/`current` field by field and emits exactly one
/// `PropertiesChanged` signal batching every property that actually
/// changed (rather than one signal per property) — a no-op if nothing did.
fn emit_property_changes(
    previous: &MprisState,
    current: &MprisState,
    iface_ref: &InterfaceRef<MprisPlayer>,
) {
    let mut changed: HashMap<&str, Value<'_>> = HashMap::new();

    if previous.status != current.status {
        changed.insert(
            "PlaybackStatus",
            Value::from(current.status.as_str().to_string()),
        );
    }
    if metadata_differs(previous, current) {
        changed.insert("Metadata", Value::from(build_metadata(current)));
    }
    if previous.can_next != current.can_next {
        changed.insert("CanGoNext", Value::from(current.can_next));
    }
    if previous.can_prev != current.can_prev {
        changed.insert("CanGoPrevious", Value::from(current.can_prev));
    }
    if can_play(previous) != can_play(current) {
        changed.insert("CanPlay", Value::from(can_play(current)));
    }
    if can_pause(previous) != can_pause(current) {
        changed.insert("CanPause", Value::from(can_pause(current)));
    }

    if changed.is_empty() {
        return;
    }

    // `from_static_str_unchecked` trusts a compile-time-controlled literal
    // (mirrors what the `#[interface]` macro's own generated code does
    // internally for the same purpose — see zbus's `fdo::properties`
    // module) rather than a fallible `TryFrom` that would need an `unwrap`/
    // `expect` this module isn't allowed to use outside tests/`main()`.
    let interface_name = InterfaceName::from_static_str_unchecked(PLAYER_INTERFACE_NAME);
    let emitter = iface_ref.signal_emitter();
    let result = zbus::block_on(fdo::Properties::properties_changed(
        emitter,
        interface_name,
        changed,
        Cow::Borrowed(&[]),
    ));
    match result {
        Ok(()) => tracing::debug!("MPRIS: emitted PropertiesChanged"),
        Err(error) => tracing::warn!(%error, "MPRIS: failed to emit PropertiesChanged"),
    }
}

/// Whether the `Metadata` dict-valued property needs re-emitting: any field
/// that feeds [`build_metadata`] changed.
fn metadata_differs(a: &MprisState, b: &MprisState) -> bool {
    a.track_id != b.track_id
        || a.title != b.title
        || a.artist != b.artist
        || a.album != b.album
        || a.duration_ms != b.duration_ms
}

/// `CanPlay`: whether calling `Play` would do something. True once there is
/// a current or resumable track (`track_id` set, e.g. paused) or somewhere
/// the queue could jump to and start (`can_next`/`can_prev`, which mirror
/// "queue is non-empty" — see the `MprisState` doc comment) and playback
/// isn't already in progress.
fn can_play(state: &MprisState) -> bool {
    state.status != MprisPlaybackStatus::Playing
        && (state.track_id.is_some() || state.can_next || state.can_prev)
}

/// `CanPause`: whether calling `Pause` would do something — only while
/// actively playing (pausing an already-paused or stopped player is a
/// no-op `PlayerController::mpris_pause` already short-circuits).
fn can_pause(state: &MprisState) -> bool {
    state.status == MprisPlaybackStatus::Playing
}

/// Builds the MPRIS `Metadata` dict (`a{sv}`) from `state`. Empty
/// (`{}`, legal per spec) when nothing is loaded. `mpris:trackid` is built
/// as `/org/reprise/Reprise/track/<id>` — track ids are DB row ids (see
/// `queries::TrackSummary`), always non-negative decimal digits, which is
/// already a valid D-Bus object path segment, so the `ObjectPath::try_from`
/// here can only fail on a bug elsewhere (e.g. a change to id generation);
/// on that unlikely failure the id is logged and `mpris:trackid` is simply
/// omitted rather than panicking — every other field still gets reported.
/// `mpris:length` is microseconds, not the app's usual milliseconds (MPRIS
/// spec unit) — `xesam:artist` is a list, not a scalar, per spec.
fn build_metadata(state: &MprisState) -> HashMap<String, OwnedValue> {
    let mut metadata = HashMap::new();

    let Some(track_id) = state.track_id else {
        return metadata;
    };

    match ObjectPath::try_from(format!("/org/reprise/Reprise/track/{track_id}")) {
        Ok(path) => {
            metadata.insert("mpris:trackid".to_string(), OwnedValue::from(path));
        }
        Err(error) => {
            tracing::warn!(%error, track_id, "MPRIS: could not build a valid trackid object path");
        }
    }

    let length_us = state.duration_ms.max(0).saturating_mul(1000);
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

/// `org.mpris.MediaPlayer2` — the base interface every MPRIS player must
/// expose. `CanQuit`/`CanRaise` are `false` "for now" per the task brief:
/// this app has no in-process quit/raise action wired to MPRIS yet (YAGNI —
/// revisit alongside a future lock-screen polish pass). `HasTrackList`/
/// `SupportedUriSchemes`/`SupportedMimeTypes` are technically also part of
/// the spec's base interface but out of this task's explicit scope; see the
/// implementation report's self-review for the resulting gap against a
/// strict MPRIS validator (functionally harmless for GNOME Shell's media
/// widget and `busctl`, which only need `Identity`/`DesktopEntry` plus the
/// `Player` interface this module also serves).
struct MprisRoot;

#[interface(name = "org.mpris.MediaPlayer2")]
impl MprisRoot {
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> String {
        IDENTITY.to_string()
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> String {
        DESKTOP_ENTRY.to_string()
    }
}

/// `org.mpris.MediaPlayer2.Player` — playback status, metadata, and
/// transport controls. Property getters read `state` fresh on every D-Bus
/// `Get`/`GetAll` call (always live — see the module's `## Shared state`
/// doc section); transport methods `dispatch` an [`MprisCommand`] into
/// `commands` rather than touching playback directly, since this struct
/// lives on the MPRIS thread and the `Player`/`Queue` it would need to
/// drive live on the GTK main thread (see `ui::player_controller`).
struct MprisPlayer {
    state: SharedMprisState,
    commands: async_channel::Sender<MprisCommand>,
}

impl MprisPlayer {
    fn snapshot(&self) -> MprisState {
        read_state(&self.state)
    }

    /// Sends `command` to the controller's MPRIS drain loop (see `ui::
    /// player_controller`). `try_send` on an unbounded channel only fails
    /// once the receiving end is gone (app teardown) — logged, never
    /// propagated as a D-Bus method error, since from the caller's
    /// perspective the method call itself still legitimately succeeded
    /// (the command was accepted; whether it took visible effect is a
    /// separate, asynchronous concern — the same non-blocking `try_send`
    /// shape `player_controller.rs` already uses for `PlayerEvent`).
    fn dispatch(&self, command: MprisCommand) {
        if let Err(error) = self.commands.try_send(command) {
            tracing::warn!(%error, ?command, "MPRIS command dropped: controller receiver is gone");
        }
    }
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl MprisPlayer {
    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.snapshot().status.as_str().to_string()
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        build_metadata(&self.snapshot())
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        can_play(&self.snapshot())
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        can_pause(&self.snapshot())
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.snapshot().can_next
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.snapshot().can_prev
    }

    /// Position/Seek are explicitly out of scope for Stage 2 (task brief:
    /// "kommt mit Sperrbildschirm-Feinschliff") — `CanSeek=false` and no
    /// `Position`/`SetPosition`/`Seek` members exist at all (YAGNI).
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }

    fn play(&self) {
        self.dispatch(MprisCommand::Play);
    }

    fn pause(&self) {
        self.dispatch(MprisCommand::Pause);
    }

    fn play_pause(&self) {
        self.dispatch(MprisCommand::PlayPause);
    }

    fn stop(&self) {
        self.dispatch(MprisCommand::Stop);
    }

    fn next(&self) {
        self.dispatch(MprisCommand::Next);
    }

    fn previous(&self) {
        self.dispatch(MprisCommand::Previous);
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
        }
    }

    #[test]
    fn playback_status_strings_match_the_mpris_spec() {
        assert_eq!(MprisPlaybackStatus::Playing.as_str(), "Playing");
        assert_eq!(MprisPlaybackStatus::Paused.as_str(), "Paused");
        assert_eq!(MprisPlaybackStatus::Stopped.as_str(), "Stopped");
    }

    #[test]
    fn can_play_false_while_already_playing() {
        assert!(!can_play(&playing_state()));
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

    #[test]
    fn can_pause_only_while_playing() {
        assert!(can_pause(&playing_state()));
        let paused = MprisState {
            status: MprisPlaybackStatus::Paused,
            ..playing_state()
        };
        assert!(!can_pause(&paused));
        let stopped = MprisState {
            status: MprisPlaybackStatus::Stopped,
            ..playing_state()
        };
        assert!(!can_pause(&stopped));
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
    }
}
