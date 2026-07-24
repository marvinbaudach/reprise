//! Real-bus round trip: proves the MCP playback tools put the right D-Bus
//! method and payload on the wire against a *registered* player.
//!
//! The sibling `tests/playback.rs` runs against a **player-less** private bus,
//! so its deterministic outcome is "no running Reprise app" — it exercises the
//! capability and argument checks up to, but never across, the bus. This file
//! closes that gap: it stands up a **stub player** owning the app's bus name
//! (`org.mpris.MediaPlayer2.reprise`) and both served interfaces
//! (`org.mpris.MediaPlayer2.Player` transport + the Reprise-specific
//! `org.reprise.Player1.PlayTrackIds`), spawns the real `reprise-mcp` on the
//! same bus, and asserts the stub recorded exactly the method and arguments the
//! app-side server expects.
//!
//! This is the client half of the D-Bus contract; the server half — that
//! `RepriseControl::play_track_ids` dispatches the matching command — is pinned
//! by `reprise-platform-linux`'s own unit tests. Together they lock the wire
//! contract from both ends without the MCP crate depending on the GTK stack.
//!
//! Environment-limited: skips cleanly when `dbus-daemon` is unavailable, like
//! the sibling bus tests. `--features mpris` only.
#![cfg(feature = "mpris")]

mod common;

use std::sync::{Arc, Mutex};

use common::{tool_success_text, McpClient, PrivateBus, SeedTrack};
use serde_json::json;
use tempfile::TempDir;
use zbus::interface;

/// The app's MPRIS well-known name and object path (mirrors both the client and
/// `reprise-platform-linux`'s server).
const BUS_NAME: &str = "org.mpris.MediaPlayer2.reprise";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";

/// One recorded incoming D-Bus call on the stub player, in arrival order.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Recorded {
    /// An `org.mpris.MediaPlayer2.Player` transport method, by its D-Bus name.
    Transport(&'static str),
    /// `org.reprise.Player1.PlayTrackIds(ids)`.
    PlayTrackIds(Vec<i64>),
}

type Calls = Arc<Mutex<Vec<Recorded>>>;

/// Stub for the standard MPRIS `Player` interface. Each Rust method maps to its
/// PascalCase D-Bus name (`play` → `Play`), matching what
/// `crate::playback::TransportAction::method` sends.
struct PlayerStub {
    calls: Calls,
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl PlayerStub {
    fn play(&self) {
        self.record("Play");
    }
    fn pause(&self) {
        self.record("Pause");
    }
    fn stop(&self) {
        self.record("Stop");
    }
    fn next(&self) {
        self.record("Next");
    }
    fn previous(&self) {
        self.record("Previous");
    }
}

impl PlayerStub {
    fn record(&self, method: &'static str) {
        self.calls
            .lock()
            .expect("calls lock")
            .push(Recorded::Transport(method));
    }
}

/// Stub for the Reprise-specific interface carrying `PlayTrackIds`.
struct RepriseStub {
    calls: Calls,
}

#[interface(name = "org.reprise.Player1")]
impl RepriseStub {
    fn play_track_ids(&self, ids: Vec<i64>) {
        self.calls
            .lock()
            .expect("calls lock")
            .push(Recorded::PlayTrackIds(ids));
    }
}

/// Registers a stub player (both interfaces) on `bus` under the app's bus name
/// and returns the live connection plus the shared call log. The connection
/// must stay alive for the whole test — dropping it releases the bus name.
fn start_stub_player(bus: &PrivateBus) -> (zbus::blocking::Connection, Calls) {
    let calls: Calls = Arc::new(Mutex::new(Vec::new()));
    let connection = zbus::blocking::connection::Builder::address(bus.address())
        .expect("valid bus address")
        .name(BUS_NAME)
        .expect("request bus name")
        .serve_at(
            OBJECT_PATH,
            PlayerStub {
                calls: calls.clone(),
            },
        )
        .expect("serve Player interface")
        .serve_at(
            OBJECT_PATH,
            RepriseStub {
                calls: calls.clone(),
            },
        )
        .expect("serve Player1 interface")
        .build()
        .expect("build stub player connection");
    (connection, calls)
}

/// Snapshots the recorded calls (releasing the lock immediately).
fn recorded(calls: &Calls) -> Vec<Recorded> {
    calls.lock().expect("calls lock").clone()
}

// --- music_play (targeted play) ------------------------------------------

#[test]
fn music_play_track_ids_reaches_the_player_over_the_bus() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("environment-limited: dbus-daemon unavailable; skipping the MPRIS bus roundtrip");
        return;
    };
    let (_conn, calls) = start_stub_player(&bus);

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    // An explicit id list is passed through unchecked (only an empty list is
    // rejected), so no track rows are needed to prove the wire payload.
    common::seed_tracks(&path, &[]);
    let mut client = McpClient::start_on_bus(&path, &bus);

    let response = client.call_tool("music_play", json!({ "track_ids": [101, 102, 103] }));
    let text = tool_success_text(&response);
    assert!(
        text.contains("Playing 3 track(s)"),
        "unexpected success summary: {text}"
    );

    assert_eq!(
        recorded(&calls),
        vec![Recorded::PlayTrackIds(vec![101, 102, 103])],
        "the explicit id list should reach PlayTrackIds verbatim and in order"
    );
}

#[test]
fn music_play_playlist_resolves_to_ordered_ids_on_the_wire() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("environment-limited: dbus-daemon unavailable; skipping the MPRIS bus roundtrip");
        return;
    };
    let (_conn, calls) = start_stub_player(&bus);

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    // Seed three tracks and a playlist holding them in a deliberately
    // non-sorted order, to prove the resolution preserves playlist order.
    let ids = common::seed_tracks(
        &path,
        &[
            SeedTrack::simple("A", "Artist"),
            SeedTrack::simple("B", "Artist"),
            SeedTrack::simple("C", "Artist"),
        ],
    );
    let ordered = vec![ids[2], ids[0], ids[1]];
    let mut conn = reprise_core::db::open_migrated(Some(&path)).unwrap();
    let playlist_id = reprise_core::library::playlists::create(&conn, "Roadtrip").unwrap();
    reprise_core::library::playlists::add_tracks(&mut conn, playlist_id, &ordered).unwrap();
    drop(conn);

    let mut client = McpClient::start_on_bus(&path, &bus);
    let response = client.call_tool("music_play", json!({ "playlist_id": playlist_id }));
    let text = tool_success_text(&response);
    assert!(
        text.contains("Playing 3 track(s)"),
        "unexpected success summary: {text}"
    );

    assert_eq!(
        recorded(&calls),
        vec![Recorded::PlayTrackIds(ordered)],
        "the playlist should resolve to its ids in playlist order on the wire"
    );
}

// --- music_playback_control (transport) ----------------------------------

#[test]
fn music_playback_control_reaches_the_player_over_the_bus() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("environment-limited: dbus-daemon unavailable; skipping the MPRIS bus roundtrip");
        return;
    };
    let (_conn, calls) = start_stub_player(&bus);

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    common::seed_tracks(&path, &[]);
    let mut client = McpClient::start_on_bus(&path, &bus);

    // Exercise all five verbs so a mis-mapped verb (e.g. "stop" wired to Pause)
    // cannot pass — the verb→method mapping is asserted by the recorded vector
    // below.
    for verb in ["play", "pause", "stop", "next", "previous"] {
        let response = client.call_tool("music_playback_control", json!({ "action": verb }));
        let text = tool_success_text(&response);
        assert!(
            text.contains(&format!("Playback: {verb}")),
            "unexpected success summary for {verb}: {text}"
        );
    }

    assert_eq!(
        recorded(&calls),
        vec![
            Recorded::Transport("Play"),
            Recorded::Transport("Pause"),
            Recorded::Transport("Stop"),
            Recorded::Transport("Next"),
            Recorded::Transport("Previous"),
        ],
        "each verb should invoke its matching MPRIS transport method, in order"
    );
}
