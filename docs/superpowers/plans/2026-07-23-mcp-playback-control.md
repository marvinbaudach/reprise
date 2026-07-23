# MCP Playback Control Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let an agent drive the running Reprise app's playback through `reprise-mcp` — transport (play/pause/stop/next/previous) plus targeted "play this track / playlist".

**Architecture:** Transport reuses the standard MPRIS methods the app already serves; targeted play adds one Reprise-specific D-Bus primitive `org.reprise.Player1.PlayTrackIds(ax)`. The MCP resolves "play playlist X" to track ids itself (it reads the DB) and sends only ids, so the app stays a dumb command sink. The MCP talks to the app over the session bus with `zbus`, mirroring `reprise-cli`'s `playback` client.

**Tech Stack:** Rust, `zbus` 5 (D-Bus, behind an opt-in `mpris` feature), `rmcp` 2.2.0 (MCP), `async_channel` (the app's command channel), `rusqlite` (library reads).

## Global Constraints

- Rust edition 2021, rust-version 1.92 (workspace).
- `reprise-core` must not depend on gtk4/libadwaita/gstreamer/zbus (enforced by `scripts/check-architecture.sh`). The MCP's `zbus` use MUST be behind an opt-in cargo feature so it never enters the default dependency tree — exactly like `reprise-cli`'s `mpris` feature (Beschluss 3).
- Rust files stay < 800 lines (`scripts/check-architecture.sh`).
- Every commit passes `cargo clippy --all-targets --workspace -- -D warnings` and `cargo fmt --check`.
- Bus name `org.mpris.MediaPlayer2.reprise`, object path `/org/mpris/MediaPlayer2` (constants already in `crates/reprise-platform-linux/src/mpris/mod.rs:119-120` and `crates/reprise-cli/src/commands/playback.rs:15-18`).
- MPRIS Player interface `org.mpris.MediaPlayer2.Player`; new Reprise interface `org.reprise.Player1`.
- Playback control is gated by a new capability `agent.capability.playback:control`, **default `true`**, read live (revocation is immediate; no startup snapshot).

---

## File Structure

- `crates/reprise-core/src/media_integration.rs` — add `MprisCommand::PlayTrackIds(Vec<i64>)`; drop `Copy` from the enum.
- `crates/reprise-core/src/library/playlists.rs` — add `track_ids(conn, id)` reader.
- `crates/reprise-platform-linux/src/mpris/mod.rs` — add the `org.reprise.Player1` interface + register it.
- `crates/reprise-gnome/src/ui/mpris_mirror.rs` — handle `PlayTrackIds`.
- `crates/reprise-mcp/Cargo.toml` — `mpris` feature + optional `zbus`.
- `crates/reprise-mcp/src/playback.rs` — NEW: the zbus session-bus client.
- `crates/reprise-mcp/src/capability.rs` — add `playback:control`.
- `crates/reprise-mcp/src/dto.rs` — params/result types for the two tools.
- `crates/reprise-mcp/src/data.rs` — playlist→ids resolution + validation.
- `crates/reprise-mcp/src/server.rs` — the two new tools.
- `crates/reprise-mcp/src/main.rs` — thread the `mpris` cfg where needed (module decl).

---

## Task 1: Core — `MprisCommand::PlayTrackIds`

**Files:**
- Modify: `crates/reprise-core/src/media_integration.rs:49-63` (the `MprisCommand` enum)
- Test: same file's `#[cfg(test)]` section

**Interfaces:**
- Produces: `reprise_core::media_integration::MprisCommand::PlayTrackIds(Vec<i64>)` — a variant carrying an ordered list of track ids to seed the queue with and play.

- [ ] **Step 1: Write the failing test** — append to the module's tests:

```rust
#[test]
fn play_track_ids_carries_the_ordered_ids() {
    let command = MprisCommand::PlayTrackIds(vec![7, 1, 4]);
    match command {
        MprisCommand::PlayTrackIds(ids) => assert_eq!(ids, vec![7, 1, 4]),
        other => panic!("unexpected variant: {other:?}"),
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p reprise-core play_track_ids_carries_the_ordered_ids`
Expected: FAIL — `no variant named PlayTrackIds`.

- [ ] **Step 3: Implement** — in `MprisCommand`, remove `Copy` from the derive and add the variant:

```rust
// was: #[derive(Debug, Clone, Copy, PartialEq)]
#[derive(Debug, Clone, PartialEq)]
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
    /// Seed the queue from this ordered id list and start playing (empty =
    /// no-op). Carries a `Vec`, so the enum is no longer `Copy`.
    PlayTrackIds(Vec<i64>),
}
```

- [ ] **Step 4: Verify the whole workspace still compiles** (dropping `Copy` may surface reuse-after-move; the command flows only through `async_channel` send/recv today, so expect none):

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS. If any site relied on `Copy` (used `command` after moving it), give it an explicit `.clone()` at that exact site and re-run. Do not add `Copy` back.

- [ ] **Step 5: Run the test**

Run: `cargo test -p reprise-core play_track_ids_carries_the_ordered_ids`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/reprise-core/src/media_integration.rs
git commit -m "feat(core): add MprisCommand::PlayTrackIds (drop Copy for the Vec payload)"
```

---

## Task 2: Core — `playlists::track_ids`

**Files:**
- Modify: `crates/reprise-core/src/library/playlists.rs` (add a reader near `get`, line ~163)
- Test: the file's `#[cfg(test)]`/sibling tests (follow the file's existing test style)

**Interfaces:**
- Produces: `reprise_core::library::playlists::track_ids(conn: &Connection, playlist_id: i64) -> Result<Vec<i64>, rusqlite::Error>` — the playlist's track ids in stored order (empty if the playlist is empty or absent).

- [ ] **Step 1: Write the failing test:**

```rust
#[test]
fn track_ids_returns_playlist_members_in_order() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    for id in [10_i64, 11, 12] {
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (?1, ?2, 'T', 'A', 0)",
            rusqlite::params![id, format!("/m/{id}.flac")],
        )
        .unwrap();
    }
    let pid = create(&conn, "Road").unwrap();
    add_tracks(&conn, pid, &[12, 10, 11]).unwrap();
    assert_eq!(track_ids(&conn, pid).unwrap(), vec![12, 10, 11]);
    assert_eq!(track_ids(&conn, 9999).unwrap(), Vec::<i64>::new());
}
```

(Confirm the exact column names by reading the `playlist_tracks` insert in `add_tracks` at `crates/reprise-core/src/library/playlists.rs:186`; the query below assumes `playlist_tracks(playlist_id, track_id, position)`.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p reprise-core track_ids_returns_playlist_members_in_order`
Expected: FAIL — `cannot find function track_ids`.

- [ ] **Step 3: Implement** (adjust table/column names to match `add_tracks`):

```rust
/// The playlist's track ids in stored (`position`) order. Empty for an empty
/// or non-existent playlist — callers treat "no playable tracks" as invalid
/// input at their boundary.
pub fn track_ids(conn: &Connection, playlist_id: i64) -> Result<Vec<i64>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
    )?;
    let ids = stmt
        .query_map([playlist_id], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p reprise-core track_ids_returns_playlist_members_in_order`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-core/src/library/playlists.rs
git commit -m "feat(core): playlists::track_ids reader for MCP targeted play"
```

---

## Task 3: platform-linux — `org.reprise.Player1` D-Bus interface

**Files:**
- Modify: `crates/reprise-platform-linux/src/mpris/mod.rs` (new interface struct + `serve_at` registration near line 188-190)
- Test: same file's `#[cfg(test)]` section (mirror the existing dispatch tests)

**Interfaces:**
- Consumes: `MprisCommand::PlayTrackIds` (Task 1); the existing `async_channel::Sender<MprisCommand>` (`mpris/mod.rs:178`).
- Produces: D-Bus method `org.reprise.Player1.PlayTrackIds(ids: Vec<i64>)` on object `/org/mpris/MediaPlayer2`.

- [ ] **Step 1: Write the failing test** — assert the interface dispatches (mirror how the existing Player tests assert `dispatch`; they build the struct with a channel and read the receiver):

```rust
#[test]
fn reprise_control_play_track_ids_dispatches_the_command() {
    let (sender, receiver) = async_channel::unbounded::<MprisCommand>();
    let control = RepriseControl { commands: sender };
    control.play_track_ids(vec![3, 1, 2]);
    assert_eq!(
        receiver.try_recv().unwrap(),
        MprisCommand::PlayTrackIds(vec![3, 1, 2])
    );
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p reprise-platform-linux reprise_control_play_track_ids_dispatches_the_command`
Expected: FAIL — `cannot find type RepriseControl`.

- [ ] **Step 3: Implement** — add near the `MprisPlayer` interface (after its `impl`, ~line 690). Add the interface-name constant next to `PLAYER_INTERFACE_NAME` (line 128):

```rust
/// Reprise-specific control surface, alongside MPRIS on the same object, for
/// commands MPRIS has no vocabulary for. Today: play an explicit ordered list
/// of library track ids (a single track = one id; a playlist = its ids,
/// resolved by the caller). Kept minimal on purpose — the app stays a dumb
/// command sink; callers own any library resolution.
struct RepriseControl {
    commands: async_channel::Sender<MprisCommand>,
}

#[interface(name = "org.reprise.Player1")]
impl RepriseControl {
    /// Seed the queue from `ids` (in order) and start playing. An empty list
    /// is a no-op (never clears the current queue).
    fn play_track_ids(&self, ids: Vec<i64>) {
        if ids.is_empty() {
            return;
        }
        if let Err(error) = self.commands.try_send(MprisCommand::PlayTrackIds(ids)) {
            tracing::warn!(%error, "dropping PlayTrackIds: command channel closed");
        }
    }
}
```

Match the *exact* send idiom the existing `MprisPlayer::dispatch` (`mpris/mod.rs:432`) uses — if it calls a shared `dispatch` helper via `send`/`try_send`, reuse that idiom so the test's assumption matches. Then register the interface next to the existing two (`mpris/mod.rs:188-190`):

```rust
        .and_then(|builder| builder.serve_at(OBJECT_PATH, MprisRoot { desktop_entry }))
        .and_then(|builder| builder.serve_at(OBJECT_PATH, player_iface))
        .and_then(|builder| builder.serve_at(OBJECT_PATH, RepriseControl { commands: sender.clone() }))
```

(`sender` is the `async_channel::Sender<MprisCommand>` created at `mpris/mod.rs:156`; clone it so both the Player iface and RepriseControl hold a sender. Verify whether `player_iface` already moved `sender` — if so, `.clone()` at that earlier use and pass an owned clone here.)

- [ ] **Step 4: Run the test + clippy**

Run: `cargo test -p reprise-platform-linux reprise_control_play_track_ids_dispatches_the_command && cargo clippy -p reprise-platform-linux --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-platform-linux/src/mpris/mod.rs
git commit -m "feat(mpris): org.reprise.Player1.PlayTrackIds D-Bus method"
```

---

## Task 4: gnome — handle `PlayTrackIds` in the mirror

**Files:**
- Modify: `crates/reprise-gnome/src/ui/mpris_mirror.rs:316-328` (the `handle_mpris_command` match)
- Test: the module's existing mirror tests (follow their pattern; if the mirror is display-gated, mark `#[ignore]` like its siblings)

**Interfaces:**
- Consumes: `MprisCommand::PlayTrackIds` (Task 1); `PlayerController::play_from_view(ids: Vec<i64>, start_index: usize, origin: PlayOrigin)` (`crates/reprise-gnome/src/ui/playback/queue_transport.rs:273`); `PlayOrigin::library()` (`crates/reprise-gnome/src/ui/playback/play_origin.rs`).

- [ ] **Step 1: Add the match arm** — in `handle_mpris_command`, after the `Previous` arm (line 323):

```rust
            MprisCommand::PlayTrackIds(ids) => {
                self.play_from_view(ids, 0, crate::ui::playback::play_origin::PlayOrigin::library())
            }
```

Confirm the in-scope path to `play_from_view` in this file (the existing arms call `self.next()`, `self.seek(..)` etc., so the controller methods are reachable on `self`; use the same receiver). Confirm the `PlayOrigin` import path used elsewhere in this module and reuse it rather than a fully-qualified path if one is already imported.

- [ ] **Step 2: Write/extend a test** — if `mpris_mirror` has a unit test that feeds a command and asserts the controller reacted (e.g. queue seeded), add one for `PlayTrackIds`; otherwise assert routing at the level the sibling arms are tested. Follow the existing test's construction exactly (do not invent a new harness).

- [ ] **Step 3: Run tests + clippy**

Run: `cargo clippy -p reprise-gnome --all-targets -- -D warnings && cargo test -p reprise-gnome mpris_mirror`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/reprise-gnome/src/ui/mpris_mirror.rs
git commit -m "feat(gnome): route MprisCommand::PlayTrackIds to play_from_view"
```

---

## Task 5: MCP — `playback:control` capability

**Files:**
- Modify: `crates/reprise-mcp/src/capability.rs`
- Test: same file's `#[cfg(test)]` (mirror the existing capability tests if present, else add a small one)

**Interfaces:**
- Produces: `capability::CAP_PLAYBACK_CONTROL: &str`; `capability::playback_control_enabled(conn: &Connection) -> Result<bool, rusqlite::Error>` (live read, default `true`).

- [ ] **Step 1: Write the failing test:**

```rust
#[test]
fn playback_control_defaults_on_and_honors_revocation() {
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    assert!(playback_control_enabled(&conn).unwrap());
    reprise_core::library::settings::set_bool(&conn, CAP_PLAYBACK_CONTROL, false).unwrap();
    assert!(!playback_control_enabled(&conn).unwrap());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p reprise-mcp playback_control_defaults_on_and_honors_revocation`
Expected: FAIL — `cannot find value CAP_PLAYBACK_CONTROL`.

- [ ] **Step 3: Implement** — add alongside the other caps (`capability.rs:16-46`):

```rust
/// Settings key granting playback control (transport + targeted play).
pub const CAP_PLAYBACK_CONTROL: &str = "agent.capability.playback:control";
// Playback control starts audio but destroys no data — on by default, like the
// read surface, and revocable live.
const PLAYBACK_CONTROL_DEFAULT: bool = true;

/// Whether playback control is currently granted (live setting value).
pub fn playback_control_enabled(conn: &Connection) -> Result<bool, rusqlite::Error> {
    settings::get_bool(conn, CAP_PLAYBACK_CONTROL, PLAYBACK_CONTROL_DEFAULT)
}
```

- [ ] **Step 4: Run the test**

Run: `cargo test -p reprise-mcp playback_control_defaults_on_and_honors_revocation`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-mcp/src/capability.rs
git commit -m "feat(mcp): playback:control capability (default on, live)"
```

---

## Task 6: MCP — `mpris` feature + `zbus` dependency

**Files:**
- Modify: `crates/reprise-mcp/Cargo.toml`

**Interfaces:**
- Produces: cargo feature `mpris` enabling `zbus`; the `playback` module and playback tools compile only under it.

- [ ] **Step 1: Add the feature and optional dep** — after `[package]`/before `[dependencies]` add a `[features]` block, and add `zbus` to deps (mirror `crates/reprise-cli/Cargo.toml:31-32,59`):

```toml
[features]
# Opt-in so zbus never enters the default dependency tree (Beschluss 3 /
# check-architecture.sh). The desktop build enables it; a plain `cargo build`
# stays D-Bus-free and simply omits the playback tools.
default = []
mpris = ["dep:zbus"]
```

```toml
# --- PLAYBACK CONTROL (behind the `mpris` feature). A thin session-bus client
# to the running app's MPRIS + org.reprise.Player1, exactly like reprise-cli's
# playback command. Never pulls reprise-platform-linux.
zbus = { version = "5", optional = true }
```

- [ ] **Step 2: Verify both build shapes compile**

Run: `cargo check -p reprise-mcp --bins && cargo check -p reprise-mcp --bins --features mpris`
Expected: PASS (no `playback` module referenced yet — this task only wires the feature).

- [ ] **Step 3: Commit**

```bash
git add crates/reprise-mcp/Cargo.toml
git commit -m "build(mcp): add opt-in mpris feature and zbus dependency"
```

---

## Task 7: MCP — the `playback` zbus client

**Files:**
- Create: `crates/reprise-mcp/src/playback.rs`
- Modify: `crates/reprise-mcp/src/main.rs` (add `#[cfg(feature = "mpris")] mod playback;`)
- Modify: `crates/reprise-mcp/src/error.rs` (a `PlaybackError` / no-player variant if not reusing `DataError`)

**Interfaces:**
- Produces (all `#[cfg(feature = "mpris")]`):
  - `pub enum TransportAction { Play, Pause, Stop, Next, Previous }` with `pub fn from_str(&str) -> Option<TransportAction>` and `fn method(self) -> &'static str`.
  - `pub fn transport(action: TransportAction) -> Result<(), PlaybackError>`.
  - `pub fn play_track_ids(ids: Vec<i64>) -> Result<(), PlaybackError>`.
  - `pub enum PlaybackError { NoPlayer, Bus(String) }` mapped to tool errors by `server.rs`.

- [ ] **Step 1: Write the module** — model it on `crates/reprise-cli/src/commands/playback.rs` (`connect`, `is_absent_player`, `no_player_error`, blocking proxy). Constants:

```rust
//! Session-bus client for controlling the running Reprise app (feature `mpris`).
//! Transport goes to `org.mpris.MediaPlayer2.Player`; targeted play goes to the
//! Reprise-specific `org.reprise.Player1.PlayTrackIds`. Mirrors reprise-cli's
//! playback command (Beschluss 3: zbus DIRECTLY, no reprise-platform-linux dep).
//! Every call is blocking and MUST be run inside `tokio::task::spawn_blocking`.

const BUS_NAME: &str = "org.mpris.MediaPlayer2.reprise";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
const REPRISE_INTERFACE: &str = "org.reprise.Player1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportAction { Play, Pause, Stop, Next, Previous }

impl TransportAction {
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "play" => Some(Self::Play),
            "pause" => Some(Self::Pause),
            "stop" => Some(Self::Stop),
            "next" => Some(Self::Next),
            "previous" => Some(Self::Previous),
            _ => None,
        }
    }
    fn method(self) -> &'static str {
        match self {
            Self::Play => "Play",
            Self::Pause => "Pause",
            Self::Stop => "Stop",
            Self::Next => "Next",
            Self::Previous => "Previous",
        }
    }
}

#[derive(Debug)]
pub enum PlaybackError { NoPlayer, Bus(String) }

fn is_absent_player(name: &str) -> bool {
    matches!(
        name,
        "org.freedesktop.DBus.Error.ServiceUnknown" | "org.freedesktop.DBus.Error.NameHasNoOwner"
    )
}

fn proxy(interface: &'static str) -> Result<zbus::blocking::Proxy<'static>, PlaybackError> {
    let connection =
        zbus::blocking::Connection::session().map_err(|e| PlaybackError::Bus(e.to_string()))?;
    zbus::blocking::Proxy::new(&connection, BUS_NAME, OBJECT_PATH, interface).map_err(map_err)
}

fn map_err(error: zbus::Error) -> PlaybackError {
    if let zbus::Error::MethodError(name, _, _) = &error {
        if is_absent_player(name.as_str()) {
            return PlaybackError::NoPlayer;
        }
    }
    PlaybackError::Bus(error.to_string())
}

pub fn transport(action: TransportAction) -> Result<(), PlaybackError> {
    let proxy = proxy(PLAYER_INTERFACE)?;
    proxy.call_method(action.method(), &()).map(|_| ()).map_err(map_err)
}

pub fn play_track_ids(ids: Vec<i64>) -> Result<(), PlaybackError> {
    let proxy = proxy(REPRISE_INTERFACE)?;
    proxy.call_method("PlayTrackIds", &(ids,)).map(|_| ()).map_err(map_err)
}
```

Verify the exact zbus 5 error-variant shape against `reprise-cli/src/commands/playback.rs`'s `map_zbus_error` and copy its classification precisely (the `MethodError` destructuring above must match that file).

- [ ] **Step 2: Add the module declaration** — in `crates/reprise-mcp/src/main.rs`, next to the other `mod` lines:

```rust
#[cfg(feature = "mpris")]
mod playback;
```

- [ ] **Step 3: Test the pure parts** (the D-Bus round-trip needs a live app — not unit-tested, same boundary as the CLI):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transport_action_parses_the_five_verbs_and_rejects_others() {
        for (s, a) in [
            ("play", TransportAction::Play), ("pause", TransportAction::Pause),
            ("stop", TransportAction::Stop), ("next", TransportAction::Next),
            ("previous", TransportAction::Previous),
        ] {
            assert_eq!(TransportAction::from_str(s), Some(a));
        }
        assert_eq!(TransportAction::from_str("rewind"), None);
    }
}
```

- [ ] **Step 4: Build + test with the feature**

Run: `cargo test -p reprise-mcp --features mpris transport_action_parses && cargo clippy -p reprise-mcp --all-targets --features mpris -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-mcp/src/playback.rs crates/reprise-mcp/src/main.rs
git commit -m "feat(mcp): zbus playback client (transport + PlayTrackIds)"
```

---

## Task 8: MCP — DTOs + play resolution in `data.rs`

**Files:**
- Modify: `crates/reprise-mcp/src/dto.rs` (params/result structs)
- Modify: `crates/reprise-mcp/src/data.rs` (resolve + validate)
- Test: `crates/reprise-mcp/src/data.rs` unit tests / a `tests/` fixture

**Interfaces:**
- Produces:
  - `dto::PlaybackControlParams { action: String }`
  - `dto::PlayParams { track_ids: Option<Vec<i64>>, playlist_id: Option<i64> }`
  - `data::resolve_play_ids(path: &Path, params: &PlayParams) -> Result<Vec<i64>, DataError>` — enforces "exactly one of track_ids/playlist_id", resolves a playlist via `playlists::track_ids`, errors on empty/none/both.

- [ ] **Step 1: Add the DTOs** (in `dto.rs`, deriving what the other param structs derive — check `CreatePlaylistParams`):

```rust
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct PlaybackControlParams {
    /// One of: "play", "pause", "stop", "next", "previous".
    pub action: String,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct PlayParams {
    /// An explicit ordered list of track ids to play. Mutually exclusive with `playlist_id`.
    #[serde(default)]
    pub track_ids: Option<Vec<i64>>,
    /// A playlist id to play (resolved to its tracks). Mutually exclusive with `track_ids`.
    #[serde(default)]
    pub playlist_id: Option<i64>,
}
```

(Match the derive/attribute set of the existing param structs exactly — `schemars`/`JsonSchema` is how rmcp builds the tool input schema; copy from `CreatePlaylistParams`.)

- [ ] **Step 2: Write the failing resolution test** (in `data.rs` tests, using a temp DB like the other data tests):

```rust
#[test]
fn resolve_play_ids_enforces_exactly_one_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    let conn = reprise_core::db::open_migrated(Some(&path)).unwrap();
    conn.execute("INSERT INTO tracks (id,path,title,artist,added_at) VALUES (1,'/a.flac','A','X',0)", []).unwrap();
    let pid = reprise_core::library::playlists::create(&conn, "P").unwrap();
    reprise_core::library::playlists::add_tracks(&conn, pid, &[1]).unwrap();
    drop(conn);

    // playlist path
    let ids = resolve_play_ids(&path, &PlayParams { track_ids: None, playlist_id: Some(pid) }).unwrap();
    assert_eq!(ids, vec![1]);
    // explicit ids path
    let ids = resolve_play_ids(&path, &PlayParams { track_ids: Some(vec![1]), playlist_id: None }).unwrap();
    assert_eq!(ids, vec![1]);
    // neither
    assert!(matches!(
        resolve_play_ids(&path, &PlayParams { track_ids: None, playlist_id: None }),
        Err(DataError::InvalidInput(_))
    ));
    // both
    assert!(matches!(
        resolve_play_ids(&path, &PlayParams { track_ids: Some(vec![1]), playlist_id: Some(pid) }),
        Err(DataError::InvalidInput(_))
    ));
    // empty playlist
    let empty = reprise_core::library::playlists::create(
        &reprise_core::db::open_migrated(Some(&path)).unwrap(), "E").unwrap();
    assert!(matches!(
        resolve_play_ids(&path, &PlayParams { track_ids: None, playlist_id: Some(empty) }),
        Err(DataError::InvalidInput(_))
    ));
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p reprise-mcp resolve_play_ids_enforces_exactly_one_source`
Expected: FAIL — `cannot find function resolve_play_ids`.

- [ ] **Step 4: Implement** in `data.rs` (reuse the file's `open`/`require_read` helpers):

```rust
/// Resolves a `music_play` request to an ordered id list. Exactly one of
/// `track_ids`/`playlist_id` must be set; a playlist is resolved to its tracks;
/// an empty/absent result is invalid input (nothing to play).
pub fn resolve_play_ids(
    path: &Path,
    params: &crate::dto::PlayParams,
) -> Result<Vec<i64>, DataError> {
    let ids = match (&params.track_ids, params.playlist_id) {
        (Some(_), Some(_)) | (None, None) => {
            return Err(DataError::InvalidInput(
                "provide exactly one of track_ids or playlist_id".to_owned(),
            ));
        }
        (Some(track_ids), None) => track_ids.clone(),
        (None, Some(playlist_id)) => {
            let conn = open(path)?;
            require_read(&conn)?;
            reprise_core::library::playlists::track_ids(&conn, playlist_id).map_err(DataError::Db)?
        }
    };
    if ids.is_empty() {
        return Err(DataError::InvalidInput("no playable tracks to play".to_owned()));
    }
    Ok(ids)
}
```

- [ ] **Step 5: Run the test**

Run: `cargo test -p reprise-mcp resolve_play_ids_enforces_exactly_one_source`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/reprise-mcp/src/dto.rs crates/reprise-mcp/src/data.rs
git commit -m "feat(mcp): play params + resolve_play_ids (playlist->ids, validation)"
```

---

## Task 9: MCP — the two tools

**Files:**
- Modify: `crates/reprise-mcp/src/server.rs` (two `#[tool]` methods, gated by `mpris`)
- Modify: `crates/reprise-mcp/src/error.rs` (map `PlaybackError` → tool outcome)
- Test: `crates/reprise-mcp/tests/` — a new `playback.rs` integration test (arg validation + capability path, no live bus)

**Interfaces:**
- Consumes: `playback::{transport, play_track_ids, TransportAction, PlaybackError}` (Task 7); `data::resolve_play_ids` (Task 8); `capability::playback_control_enabled` (Task 5).
- Produces: MCP tools `music_playback_control` and `music_play`.

- [ ] **Step 1: Map `PlaybackError` to a tool outcome** — in `error.rs`, add (gated `#[cfg(feature = "mpris")]`):

```rust
#[cfg(feature = "mpris")]
pub fn playback_outcome(result: Result<(), crate::playback::PlaybackError>, ok_summary: String)
    -> Result<CallToolResult, ErrorData>
{
    use crate::playback::PlaybackError;
    match result {
        Ok(()) => Ok(tool_text(ok_summary)),
        Err(PlaybackError::NoPlayer) => Ok(tool_error(
            "no running Reprise app on the session bus — start the app first".to_owned(),
        )),
        Err(PlaybackError::Bus(message)) => {
            tracing::error!(message, "playback bus error");
            Err(ErrorData::internal_error("internal server error", None))
        }
    }
}
```

(Reuse the existing `tool_error`/`structured_ok` helpers already in `error.rs`; add a `tool_text` only if no plain-text success helper exists.)

- [ ] **Step 2: Add the tools** — inside the `#[tool_router] impl RepriseServer` block (`server.rs`), gate each with `#[cfg(feature = "mpris")]`. The capability is read live off the DB (open a short-lived connection like other tools):

```rust
#[cfg(feature = "mpris")]
#[tool(
    name = "music_playback_control",
    description = "Control the running Reprise app's playback: action is one of \
        'play', 'pause', 'stop', 'next', 'previous'. Requires the app to be \
        running and the 'playback:control' capability (on by default)."
)]
async fn music_playback_control(
    &self,
    Parameters(params): Parameters<crate::dto::PlaybackControlParams>,
) -> Result<CallToolResult, ErrorData> {
    let path = self.db_path.clone();
    let allowed = tokio::task::spawn_blocking(move || data::playback_allowed(path.as_path()))
        .await
        .map_err(|e| error::join_error(&e))?;
    match allowed {
        Ok(false) => return Ok(error::tool_error(
            "Permission denied: the 'playback:control' capability is not granted.".to_owned())),
        Err(err) => return error::into_tool_outcome(err),
        Ok(true) => {}
    }
    let Some(action) = crate::playback::TransportAction::from_str(&params.action) else {
        return Ok(error::tool_error(format!("unknown action '{}'", params.action)));
    };
    let result = tokio::task::spawn_blocking(move || crate::playback::transport(action))
        .await
        .map_err(|e| error::join_error(&e))?;
    error::playback_outcome(result, format!("Playback: {}", params.action))
}

#[cfg(feature = "mpris")]
#[tool(
    name = "music_play",
    description = "Start playing an explicit list of tracks or a whole playlist \
        in the running Reprise app. Provide exactly one of track_ids or \
        playlist_id. Requires the app running and the 'playback:control' \
        capability (on by default)."
)]
async fn music_play(
    &self,
    Parameters(params): Parameters<crate::dto::PlayParams>,
) -> Result<CallToolResult, ErrorData> {
    let path = self.db_path.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        if !data::playback_allowed(path.as_path())? {
            return Err(data::DataError::CapabilityDenied("playback:control"));
        }
        data::resolve_play_ids(path.as_path(), &params)
    })
    .await
    .map_err(|e| error::join_error(&e))?;
    let ids = match outcome {
        Ok(ids) => ids,
        Err(err) => return error::into_tool_outcome(err),
    };
    let count = ids.len();
    let result = tokio::task::spawn_blocking(move || crate::playback::play_track_ids(ids))
        .await
        .map_err(|e| error::join_error(&e))?;
    error::playback_outcome(result, format!("Playing {count} track(s)"))
}
```

Add the tiny `data::playback_allowed(path) -> Result<bool, DataError>` helper (opens the DB, calls `capability::playback_control_enabled`). Move `params` capture carefully — `PlayParams` is `Clone`; clone into the closure. Match the `Parameters`/`Parameters<..>` import and the `#[tool_router]`/`#[tool]` attributes exactly as the existing four tools use them.

- [ ] **Step 3: Integration test** — `crates/reprise-mcp/tests/playback.rs` (mirror `tests/create_playlist.rs`'s harness). Assert: `music_play` with neither id source returns an error result; with a revoked cap returns "Permission denied"; a valid playlist resolves without a bus (it will then fail at the bus with "no running app" — assert that error text, which proves resolution succeeded and the capability passed). Build the test with `--features mpris`.

- [ ] **Step 4: Run + lint**

Run: `cargo test -p reprise-mcp --features mpris playback && cargo clippy -p reprise-mcp --all-targets --features mpris -- -D warnings && cargo fmt --check`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-mcp/src/server.rs crates/reprise-mcp/src/error.rs crates/reprise-mcp/src/data.rs crates/reprise-mcp/tests/playback.rs
git commit -m "feat(mcp): music_playback_control and music_play tools"
```

---

## Task 10: Build wiring + full verification

**Files:**
- Modify: `meson.build` / `meson_options.txt` (if the MCP is built via meson, enable the `mpris` feature there — check how the CLI's `mpris`/`worker` features are passed; mirror it)
- Modify: `docs/ux-rules.md` only if a rule references MCP capabilities (likely not — skip if none)

- [ ] **Step 1: Ensure the installed MCP is built with `mpris`** — the release binary needs the feature or the tools are absent. Confirm the build path (meson `cargo build ... --features` or a wrapper) and add `mpris` there, next to however `worker`/`mpris` are enabled for the CLI. If the MCP is built ad hoc, document: `cargo build -p reprise-mcp --release --features mpris`.

- [ ] **Step 2: Full gates**

Run:
```bash
cargo fmt --check
cargo clippy --workspace --all-targets --features reprise-mcp/mpris -- -D warnings   # or per-crate as above
scripts/check-architecture.sh
env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --workspace
```
Expected: all PASS. `check-architecture.sh` must still show reprise-mcp's *default* tree zbus-free (the feature is opt-in).

- [ ] **Step 3: Manual smoke (real app)** — start the app, then drive the installed `reprise-mcp --features mpris` binary over stdio (reuse the earlier python MCP client): `music_playback_control {action:"play"}` starts audio; `{action:"stop"}` stops; `music_play {playlist_id: N}` plays a playlist. With the app **not** running, both return the "no running Reprise app" error.

- [ ] **Step 4: Commit any build wiring**

```bash
git add meson.build meson_options.txt
git commit -m "build: enable reprise-mcp mpris feature for the desktop build"
```

---

## Self-Review

- **Spec coverage:** transport (Tasks 7,9) ✓; targeted play track/playlist (Tasks 1–5,8,9) ✓; custom `org.reprise.Player1` (Task 3) ✓; MCP resolves playlist→ids (Tasks 2,8) ✓; capability default-on/live (Task 5) ✓; no-player error (Tasks 7,9) ✓; feature-gated zbus (Tasks 6,7) ✓; non-goal headless engine untouched ✓.
- **Placeholder scan:** each code step carries real code; the two spots that say "match the exact idiom in file X" point at a named file:line to copy, not vague guidance — acceptable because they mirror an existing sanctioned implementation (`reprise-cli` playback) whose exact zbus-5 shapes must be reproduced verbatim.
- **Type consistency:** `MprisCommand::PlayTrackIds(Vec<i64>)`, `play_from_view(Vec<i64>, usize, PlayOrigin)`, `playlists::track_ids(&Connection, i64) -> Result<Vec<i64>, _>`, `TransportAction`, `PlaybackError`, `PlayParams`, `resolve_play_ids(&Path, &PlayParams) -> Result<Vec<i64>, DataError>` are used consistently across tasks.
