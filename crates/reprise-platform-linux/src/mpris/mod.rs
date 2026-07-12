//! MPRIS (Media Player Remote Interfacing Specification) integration —
//! Stage 2 Task 6; extended to the full `Player` surface (Position/Seek,
//! Shuffle, LoopStatus, Rate, Volume) in Stage 3 Task 10. Exposes
//! `org.mpris.MediaPlayer2` and `org.mpris.MediaPlayer2.Player` on the
//! session bus as `org.mpris.MediaPlayer2.reprise`, so GNOME Shell's media
//! widget, the lock screen, and hardware/media keys can see and control
//! playback.
//!
//! `MprisState`/`MprisCommand`/`MprisPlaybackStatus` and every pure mapping
//! function between them and MPRIS wire values (`LoopStatus` strings, µs
//! positions) live in the sibling `state` module (Stage 3 Task 10 split,
//! purely to keep both files under the project's file-size limit — see that
//! module's own doc comment) and are re-exported below so `crate::mpris::X`
//! paths elsewhere in the crate are unaffected by the split.
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
//! off. A second, dedicated thread (spawned in `run`) exists purely to
//! relay the `Seeked` signal promptly — see `run`'s doc comment.
//!
//! ## Shared state: `Arc<Mutex<state::MprisState>>`, poison-recovered like `player.rs`
//!
//! `PlayerController` (the GTK-main-thread owner of playback state) writes a
//! fresh `MprisState` snapshot into the shared mutex on every real state/
//! track transition (`update_mpris_mirror`, see `ui::player_controller`),
//! plus narrower single-field patches for state that changes independently
//! of a status/track transition (`update_mpris_position`/`update_mpris_
//! volume`/`update_mpris_shuffle`/`update_mpris_repeat`, all in `ui::mpris_
//! mirror`). This thread's `MprisPlayer` interface reads through the same
//! mutex on every D-Bus property `Get`/`GetAll` call, so reads are always
//! live — the mutex is genuinely the source of truth, not a cache that can
//! go stale between polls. Every lock site here uses the same
//! poisoned-recovery pattern `player.rs` established (`.lock().
//! unwrap_or_else(|poisoned| poisoned.into_inner())`): a panic on one side
//! (e.g. mid state-update on the GTK thread) must not take MPRIS down with
//! it, or vice versa.
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
//! **`Position` is the one property permanently excluded from this diff**
//! (Stage 3 Task 10) — see `state::MprisState`'s doc comment and `emit_
//! property_changes`'s doc comment for why: the MPRIS spec exempts it, and
//! `Seeked` is the signal clients are meant to use for jumps instead.
//!
//! ## Failure is never fatal
//!
//! [`start`] never returns a `Result`: it always hands back a working
//! `SharedMprisState`, an `async_channel::Receiver<MprisCommand>`, and a
//! seek-notification sender, and always spawns the thread. If there's no
//! session bus, the name is already taken, or anything else about claiming
//! it fails, the thread logs a `tracing::warn!` and returns — the returned
//! handles just sit there unused (the controller keeps writing to a mirror
//! nobody reads; `try_send`ing to a receiver nobody drains from the D-Bus
//! side, which simply never happens since no method calls can arrive). No
//! panics, no `Result` for callers to thread through `window::build`/
//! `main.rs`, and the rest of the app runs exactly as if this module didn't
//! exist.

mod state;

use state::{build_metadata, loop_status_to_repeat, repeat_to_loop_status, track_object_path};

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use zbus::blocking::connection;
use zbus::blocking::object_server::InterfaceRef;
use zbus::names::InterfaceName;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};
use zbus::{fdo, interface};

use reprise_core::media_integration::{
    can_pause, can_play, can_seek, metadata_differs, micros_to_ms, ms_to_micros, read_state,
    MediaIntegrationHandles, MprisCommand, MprisState, SharedMprisState,
};

/// Well-known bus name this app claims — must match the MPRIS spec's
/// `org.mpris.MediaPlayer2.<name>` convention (GNOME Shell and friends
/// discover media players by enumerating names under this prefix).
pub const BUS_NAME: &str = "org.mpris.MediaPlayer2.reprise";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const IDENTITY: &str = "Reprise";

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

/// Fixed playback rate this app reports — see `MprisPlayer::rate`'s doc
/// comment.
const FIXED_RATE: f64 = 1.0;

/// Starts the MPRIS integration: spawns the dedicated D-Bus thread (see the
/// module's `## Thread model` doc section) and returns the shared state
/// mirror, the command receiver, and a sender for seek notifications —
/// unconditionally, never a `Result`. See the module's `## Failure is never
/// fatal` doc section for what happens if the thread can't actually claim
/// the bus name.
///
/// The returned `async_channel::Sender<i64>` is the *opposite* direction
/// from `commands`: `ui::player_controller::PlayerController::seek` (the one
/// method behind every seek in the app, whatever originated it — see its
/// doc comment) sends the just-seeked position in µs into it after every
/// successful seek, and this module's dedicated relay thread (spawned in
/// `run`) drains it to emit the `Seeked` signal — see `run`'s doc
/// comment for why that's a separate thread from the `PropertiesChanged`
/// poll loop.
pub fn start(desktop_entry: &'static str) -> MediaIntegrationHandles {
    let state: SharedMprisState = Arc::new(Mutex::new(MprisState::default()));
    let (sender, receiver) = async_channel::unbounded::<MprisCommand>();
    let (seek_sender, seek_receiver) = async_channel::unbounded::<i64>();

    let thread_state = state.clone();
    std::thread::spawn(move || run(&thread_state, sender, seek_receiver, desktop_entry));

    MediaIntegrationHandles {
        shared_state: state,
        commands: receiver,
        seek_notify: seek_sender,
    }
}

/// The MPRIS thread's entire body: connect, claim the name, serve both
/// interfaces, then poll-diff forever. Every fallible step logs a
/// `tracing::warn!` and returns (never panics) on failure — see the
/// module's `## Failure is never fatal` doc section. Takes `state` by
/// reference: this function only ever clones it (into `MprisPlayer`) or
/// borrows it further (into `poll_and_emit_changes`), never needs to own it
/// itself — the owning `Arc` stays with `start`'s thread closure.
fn run(
    state: &SharedMprisState,
    commands: async_channel::Sender<MprisCommand>,
    seek_receiver: async_channel::Receiver<i64>,
    desktop_entry: &'static str,
) {
    let player_iface = MprisPlayer {
        state: state.clone(),
        commands,
    };

    let connection = connection::Builder::session()
        .and_then(|builder| builder.name(BUS_NAME))
        .and_then(|builder| builder.serve_at(OBJECT_PATH, MprisRoot { desktop_entry }))
        .and_then(|builder| builder.serve_at(OBJECT_PATH, player_iface))
        .and_then(connection::Builder::build);
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

    // Dedicated relay thread for `Seeked`, separate from the 500 ms
    // `PropertiesChanged` poll loop below: `Seeked` must fire promptly after
    // every seek (app-internal or MPRIS-initiated — see `PlayerController::
    // seek`'s doc comment), not batched onto the poll's cadence the way
    // ordinary property changes are. `zbus::blocking::object_server::
    // InterfaceRef` itself isn't `Clone`, but the `SignalEmitter` it hands
    // out is (it just owns a cheap-to-clone `Connection` handle plus the
    // object path) — cloning that out is exactly the "emit a signal from
    // outside a dispatched handler" pattern `InterfaceRef::signal_emitter`'s
    // own doc example shows, just handed to a second thread instead of used
    // inline.
    let seek_emitter = iface_ref.signal_emitter().clone();
    std::thread::spawn(move || {
        while let Ok(position_us) = seek_receiver.recv_blocking() {
            emit_seeked(position_us, &seek_emitter);
        }
    });

    poll_and_emit_changes(state, &iface_ref);
}

/// Emits the `Seeked` signal for `position_us` — the relay thread spawned in
/// [`run`]'s only caller. Failure (e.g. the connection going away during
/// teardown) is logged, not propagated: there is no D-Bus method call this
/// could fail *back to*, unlike a property setter.
fn emit_seeked(position_us: i64, emitter: &SignalEmitter<'static>) {
    match zbus::block_on(MprisPlayer::seeked(emitter, position_us)) {
        Ok(()) => tracing::debug!(position_us, "MPRIS: emitted Seeked"),
        Err(error) => tracing::warn!(%error, position_us, "MPRIS: failed to emit Seeked"),
    }
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
///
/// **`position_ms` is deliberately never diffed here** — see `state::
/// MprisState`'s doc comment for why `Position` is exempt from
/// `PropertiesChanged` per the MPRIS spec. `position_ms` still changes on
/// nearly every call this function gets during playback (the poll loop's
/// own `current != last` check in `poll_and_emit_changes` doesn't know to
/// ignore it), which just means `changed` often ends up empty and this
/// returns early — no signal sent, exactly as intended.
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
    if can_seek(previous) != can_seek(current) {
        changed.insert("CanSeek", Value::from(can_seek(current)));
    }
    if previous.shuffle != current.shuffle {
        changed.insert("Shuffle", Value::from(current.shuffle));
    }
    if previous.repeat != current.repeat {
        changed.insert(
            "LoopStatus",
            Value::from(repeat_to_loop_status(current.repeat).to_string()),
        );
    }
    if previous.volume != current.volume {
        changed.insert("Volume", Value::from(current.volume));
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

/// `org.mpris.MediaPlayer2` — the base interface every MPRIS player must
/// expose. `CanQuit`/`CanRaise` are `false` "for now" per the task brief:
/// this app has no in-process quit/raise action wired to MPRIS yet (YAGNI —
/// revisit alongside a future lock-screen polish pass). `HasTrackList`/
/// `SupportedUriSchemes`/`SupportedMimeTypes` are optional per the spec
/// and implemented below as required constants (no track list UI; no URI
/// scheme or MIME type filtering needed).
///
/// `desktop_entry` is passed in by the frontend (`mpris::start`'s
/// `desktop_entry` parameter, sourced from the GNOME frontend's `APP_ID`)
/// rather than read from a `crate::APP_ID` const: the desktop-entry name
/// belongs to the frontend (a KDE frontend ships its own `.desktop`), so
/// this platform module must not reach up into it.
struct MprisRoot {
    desktop_entry: &'static str,
}

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
        self.desktop_entry.to_string()
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
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

    /// Stage 3 Task 10: `CanSeek` is intrinsic to "is a track loaded",
    /// exactly like `can_pause` — see [`can_seek`]'s doc comment.
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        can_seek(&self.snapshot())
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }

    /// `Position`, in µs (MPRIS's unit) — converted from the mirror's
    /// `position_ms` via [`ms_to_micros`]. Read via the mirror like every
    /// other property here (see the module's `## Shared state` doc
    /// section): the 500 ms position tick and every seek keep it current
    /// (`ui::mpris_mirror`'s `update_mpris_position`). Exempt from
    /// `PropertiesChanged` — see `state::MprisState`'s doc comment.
    #[zbus(property)]
    fn position(&self) -> i64 {
        ms_to_micros(self.snapshot().position_ms)
    }

    /// `Shuffle`, read from the mirror's `shuffle` field (kept current by
    /// `ui::mpris_mirror` from `Queue::is_shuffled` — see that module's
    /// `update_mpris_shuffle`).
    ///
    /// `emits_changed_signal = "false"`: zbus's default (`"true"`) would,
    /// right after *every* successful `Set` call, automatically re-invoke
    /// this getter and emit a `PropertiesChanged` built from whatever it
    /// returns — but `set_shuffle` below only dispatches an async command;
    /// the controller hasn't applied it yet by the time that auto-emission
    /// would fire, so it would announce the *stale* pre-write value (worse:
    /// confirmed by this task's own busctl E2E — setting `Shuffle` to
    /// `true` produced an immediate spurious `Shuffle: false` notification,
    /// "corrected" 500 ms later by this module's own diff-poll once the
    /// mirror actually caught up). This module already owns `Properties
    /// Changed` end to end via that diff-poll (see the module's `##
    /// PropertiesChanged` doc section) — disabling zbus's own redundant,
    /// premature emission here leaves exactly one, correct, notification
    /// per real change.
    #[zbus(property(emits_changed_signal = "false"))]
    fn shuffle(&self) -> bool {
        self.snapshot().shuffle
    }

    /// `Shuffle` write: dispatches [`MprisCommand::SetShuffle`] rather than
    /// touching the queue directly (this struct lives on the MPRIS thread —
    /// see the module's `## Shared state` doc section) — `ui::mpris_mirror`'s
    /// `handle_mpris_command` applies it to `Queue::set_shuffle` and syncs
    /// the bar's shuffle toggle back (guarded against re-dispatching — see
    /// `ui::player_bar`'s `set_shuffle_indicator`).
    #[zbus(property)]
    fn set_shuffle(&self, value: bool) {
        self.dispatch(MprisCommand::SetShuffle(value));
    }

    /// `LoopStatus`, read from the mirror's `repeat` field via
    /// [`repeat_to_loop_status`]. `emits_changed_signal = "false"` — see
    /// `shuffle`'s doc comment for why (same async-setter/stale-getter
    /// reasoning applies verbatim).
    #[zbus(property(emits_changed_signal = "false"))]
    fn loop_status(&self) -> String {
        repeat_to_loop_status(self.snapshot().repeat).to_string()
    }

    /// `LoopStatus` write: parses `value` via [`loop_status_to_repeat`] —
    /// an invalid string (anything other than the three exact spec values)
    /// is rejected with `InvalidArgs` rather than silently ignored or
    /// panicking, matching the task brief's explicit requirement. A valid
    /// value dispatches [`MprisCommand::SetLoop`], applied the same way
    /// `Shuffle` writes are (see `set_shuffle`'s doc comment).
    #[zbus(property)]
    fn set_loop_status(&self, value: &str) -> zbus::fdo::Result<()> {
        match loop_status_to_repeat(value) {
            Some(repeat) => {
                self.dispatch(MprisCommand::SetLoop(repeat));
                Ok(())
            }
            None => Err(zbus::fdo::Error::InvalidArgs(format!(
                "invalid LoopStatus {value:?}; expected \"None\", \"Playlist\", or \"Track\""
            ))),
        }
    }

    /// Fixed at 1.0: this app has no variable-speed playback. Required by
    /// the MPRIS spec regardless (`Player` "must" expose `Rate`).
    /// `emits_changed_signal = "false"`: `set_rate` never actually changes
    /// anything (see its own doc comment), so zbus's default post-`Set`
    /// auto-emission would just be redundant no-op noise on every write
    /// attempt — not the stale-value bug `shuffle`'s doc comment describes
    /// (this getter is a pure constant, never stale), but suppressed for
    /// the same "this module owns its own PropertiesChanged" reason.
    #[zbus(property(emits_changed_signal = "false"))]
    fn rate(&self) -> f64 {
        FIXED_RATE
    }

    /// Accepts the write (never an error — nothing in the spec requires
    /// rejecting it) but ignores any value that isn't ~1.0: this app can't
    /// actually change playback speed, and `MinimumRate == MaximumRate ==
    /// 1.0` already tells well-behaved clients not to offer the control.
    #[zbus(property)]
    fn set_rate(&self, value: f64) {
        if (value - FIXED_RATE).abs() > f64::EPSILON {
            tracing::debug!(
                value,
                "MPRIS: rate change requested but unsupported (fixed at 1.0); ignoring"
            );
        }
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        FIXED_RATE
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        FIXED_RATE
    }

    /// `Volume`, read from the mirror's `volume` field (kept current by
    /// `ui::mpris_mirror` — see `update_mpris_volume` and `update_mpris_
    /// mirror`'s doc comment for where `PlayerController`'s own volume state
    /// comes from, since `Player` itself has no volume getter).
    /// `emits_changed_signal = "false"` — see `shuffle`'s doc comment for
    /// why (same async-setter/stale-getter reasoning applies verbatim).
    #[zbus(property(emits_changed_signal = "false"))]
    fn volume(&self) -> f64 {
        self.snapshot().volume
    }

    /// `Volume` write: clamped to the spec's `0.0..=1.0` range here (the
    /// same clamp `Player::set_volume` applies on the app-internal path),
    /// then dispatched as [`MprisCommand::SetVolume`] — applied to both
    /// `Player::set_volume` and the bar's volume control (guarded — see
    /// `ui::player_bar`'s `set_volume_indicator`).
    #[zbus(property)]
    fn set_volume(&self, value: f64) {
        self.dispatch(MprisCommand::SetVolume(value.clamp(0.0, 1.0)));
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

    /// `Seek(offset_µs)`: a *relative* seek. Converts to ms via
    /// [`micros_to_ms`] (deliberately unclamped — see that function's doc
    /// comment) and dispatches [`MprisCommand::Seek`]; `ui::mpris_mirror`'s
    /// `handle_mpris_command` resolves it against the mirror's current
    /// `position_ms` and clamps the *result* to `0..=duration_ms` before
    /// calling `PlayerController::seek` — the one method behind every seek
    /// in the app (see its doc comment), which is also what emits `Seeked`
    /// afterward.
    fn seek(&self, offset: i64) {
        self.dispatch(MprisCommand::Seek(micros_to_ms(offset)));
    }

    /// `SetPosition(TrackId, Position_µs)`: an *absolute* seek, but only if
    /// `track_id` matches the currently loaded track's `mpris:trackid` —
    /// per spec, "If the [TrackId] does not match the id of the currently-
    /// playing track, the call is ignored as stale." No track loaded, or a
    /// mismatched id, is therefore silently a no-op (logged at debug, not a
    /// D-Bus error — the spec calls for silent ignoring, not rejection). A
    /// match converts `position` to ms and clamps it to `0..=duration_ms`
    /// (mirroring `ui::player_bar`'s own scale-position clamp) before
    /// dispatching [`MprisCommand::SetPosition`] — `ui::mpris_mirror`'s
    /// `handle_mpris_command` applies it via the same `PlayerController::
    /// seek` `Seek` uses.
    // `ObjectPath` (unlike `&str`) doesn't implement zvariant's borrowed
    // `DynamicDeserialize` from a `&Message`, so — unlike `set_loop_status`'s
    // `&str` — this parameter can't be taken by reference; clippy's
    // `needless_pass_by_value` doesn't know that constraint.
    #[allow(clippy::needless_pass_by_value)]
    fn set_position(&self, track_id: ObjectPath<'_>, position: i64) {
        let snapshot = self.snapshot();
        let Some(current_id) = snapshot.track_id else {
            tracing::debug!("MPRIS SetPosition: no current track; ignoring");
            return;
        };
        let expected = track_object_path(current_id);
        if track_id.as_str() != expected {
            tracing::debug!(
                requested = track_id.as_str(),
                expected,
                "MPRIS SetPosition: trackid mismatch; ignoring per spec"
            );
            return;
        }
        let position_ms = micros_to_ms(position).clamp(0, snapshot.duration_ms.max(0));
        self.dispatch(MprisCommand::SetPosition(position_ms));
    }

    /// Emitted after every successful seek — app-internal (the bar's seek
    /// scale) and MPRIS-initiated alike — by `emit_seeked`, called from
    /// `run`'s dedicated relay thread. `position` is µs, per spec. See
    /// the module's `## PropertiesChanged` doc section and `state::
    /// MprisState`'s doc comment for why this signal (not a `Position`
    /// property-change notification) is how clients learn of position
    /// jumps.
    #[zbus(signal)]
    async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;
}
