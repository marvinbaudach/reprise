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

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use common::{
    set_bool_setting, structured_ok, tool_success_text, McpClient, PrivateBus, SeedTrack,
};
use serde_json::json;
use tempfile::TempDir;
use zbus::interface;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

/// The app's MPRIS well-known name and object path (mirrors both the client and
/// `reprise-platform-linux`'s server).
const BUS_NAME: &str = "org.mpris.MediaPlayer2.reprise";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";

/// One recorded incoming D-Bus call on the stub player, in arrival order.
#[derive(Debug, Clone, PartialEq)]
enum Recorded {
    /// An `org.mpris.MediaPlayer2.Player` transport method, by its D-Bus name.
    Transport(&'static str),
    /// `org.reprise.Player1.PlayTrackIds(ids)`.
    PlayTrackIds(Vec<i64>),
    Seek(i64),
    SetVolume(f64),
    SetShuffle(bool),
    SetLoopStatus(String),
    QueueAddNext(Vec<i64>),
    QueueAddLast(Vec<i64>),
    QueueClear,
    ConfigureDevice {
        device_name: String,
        sources: Vec<(String, i64)>,
        quality_kbps: u32,
    },
    StartDevice(String),
    CancelDevice(String),
}

type Calls = Arc<Mutex<Vec<Recorded>>>;
type DeviceSyncSourceRow = (String, i64, bool, String, bool, bool, u64, u64, u64, u64);
type DeviceSyncChangesRow = (u64, u64, u64, u64, u64, u64, u64);
type DeviceSyncStorageCompositionRow = (bool, u64, u64, u64, bool, u64, bool, u64, String);
type DeviceSyncStorageRow = (
    bool,
    String,
    String,
    bool,
    u64,
    u64,
    DeviceSyncStorageCompositionRow,
    bool,
    DeviceSyncStorageCompositionRow,
);
type DeviceSyncControlsRow = (bool, bool, bool);
type DeviceSyncProgressRow = (u64, u64, u64);
type DeviceSyncRow = (
    String,
    bool,
    u32,
    u64,
    u64,
    u64,
    Vec<DeviceSyncSourceRow>,
    DeviceSyncChangesRow,
    DeviceSyncStorageRow,
    Vec<String>,
    Vec<String>,
    DeviceSyncControlsRow,
    String,
    DeviceSyncProgressRow,
    String,
);

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

    fn seek(&self, offset: i64) {
        self.calls
            .lock()
            .expect("calls lock")
            .push(Recorded::Seek(offset));
    }

    #[zbus(property)]
    fn playback_status(&self) -> String {
        "Playing".to_owned()
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let mut metadata = HashMap::new();
        metadata.insert(
            "mpris:trackid".to_owned(),
            OwnedValue::from(
                ObjectPath::try_from("/org/reprise/Reprise/track/42")
                    .expect("valid fixture object path"),
            ),
        );
        metadata.insert("mpris:length".to_owned(), OwnedValue::from(240_000_000_i64));
        insert_owned(&mut metadata, "xesam:title", Value::from("Sun//Eater"));
        insert_owned(
            &mut metadata,
            "xesam:artist",
            Value::from(vec!["Lorna Shore"]),
        );
        insert_owned(&mut metadata, "xesam:album", Value::from("Pain Remains"));
        metadata
    }

    #[zbus(property)]
    fn position(&self) -> i64 {
        61_500_000
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        0.72
    }

    #[zbus(property)]
    fn set_volume(&self, value: f64) {
        self.calls
            .lock()
            .expect("calls lock")
            .push(Recorded::SetVolume(value));
    }

    #[zbus(property)]
    fn shuffle(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn set_shuffle(&self, value: bool) {
        self.calls
            .lock()
            .expect("calls lock")
            .push(Recorded::SetShuffle(value));
    }

    #[zbus(property)]
    fn loop_status(&self) -> String {
        "Playlist".to_owned()
    }

    #[zbus(property)]
    fn set_loop_status(&self, value: &str) {
        self.calls
            .lock()
            .expect("calls lock")
            .push(Recorded::SetLoopStatus(value.to_owned()));
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

fn insert_owned(metadata: &mut HashMap<String, OwnedValue>, key: &str, value: Value<'_>) {
    metadata.insert(
        key.to_owned(),
        OwnedValue::try_from(value).expect("fixture metadata converts"),
    );
}

/// Stub for the Reprise-specific interface carrying `PlayTrackIds`.
struct RepriseStub {
    calls: Calls,
}

struct DeviceSyncStub {
    calls: Calls,
}

#[interface(name = "org.reprise.DeviceSync1")]
impl DeviceSyncStub {
    fn snapshot(&self) -> Vec<DeviceSyncRow> {
        vec![(
            "Pixel".into(),
            true,
            320,
            75,
            200,
            80,
            vec![
                (
                    "playlist".into(),
                    3,
                    true,
                    "Lorna Shore & Similar".into(),
                    true,
                    true,
                    220,
                    200,
                    2,
                    80,
                ),
                (
                    "smart".into(),
                    7,
                    true,
                    "Heavy rotation".into(),
                    false,
                    true,
                    50,
                    50,
                    0,
                    20,
                ),
            ],
            (120, 5, 75, 2, 1, 0, 60),
            (
                true,
                "Internal storage".into(),
                "fits".into(),
                false,
                0,
                60,
                (true, 100, 20, 10, true, 30, true, 40, "complete".into()),
                true,
                (true, 100, 80, 10, true, 10, true, 0, "complete".into()),
            ),
            Vec::new(),
            vec!["unavailable_not_on_device".into()],
            (false, false, true),
            "copying".into(),
            (20, 60, 10),
            "Sun//Eater — Lorna Shore".into(),
        )]
    }

    fn configure(&self, device_name: &str, sources: Vec<(String, i64)>, quality_kbps: u32) {
        self.calls
            .lock()
            .expect("calls lock")
            .push(Recorded::ConfigureDevice {
                device_name: device_name.to_owned(),
                sources,
                quality_kbps,
            });
    }

    fn start(&self, device_name: &str) {
        self.calls
            .lock()
            .expect("calls lock")
            .push(Recorded::StartDevice(device_name.to_owned()));
    }

    fn cancel(&self, device_name: &str) {
        self.calls
            .lock()
            .expect("calls lock")
            .push(Recorded::CancelDevice(device_name.to_owned()));
    }
}

#[interface(name = "org.reprise.Player1")]
impl RepriseStub {
    fn play_track_ids(&self, ids: Vec<i64>) {
        self.calls
            .lock()
            .expect("calls lock")
            .push(Recorded::PlayTrackIds(ids));
    }

    fn queue_snapshot(&self) -> (i64, Vec<i64>, Vec<i64>, u64, u64) {
        (42, vec![7, 8], vec![9, 10, 11], 2, 3)
    }

    fn queue_add_next(&self, ids: Vec<i64>) {
        self.calls
            .lock()
            .expect("calls lock")
            .push(Recorded::QueueAddNext(ids));
    }

    fn queue_add_last(&self, ids: Vec<i64>) {
        self.calls
            .lock()
            .expect("calls lock")
            .push(Recorded::QueueAddLast(ids));
    }

    fn queue_clear(&self) {
        self.calls
            .lock()
            .expect("calls lock")
            .push(Recorded::QueueClear);
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
        .serve_at(
            OBJECT_PATH,
            DeviceSyncStub {
                calls: calls.clone(),
            },
        )
        .expect("serve DeviceSync1 interface")
        .build()
        .expect("build stub player connection");
    (connection, calls)
}

/// Snapshots the recorded calls (releasing the lock immediately).
fn recorded(calls: &Calls) -> Vec<Recorded> {
    calls.lock().expect("calls lock").clone()
}

#[test]
fn device_sync_state_and_commands_round_trip_without_internal_identity() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("environment-limited: dbus-daemon unavailable");
        return;
    };
    let (_conn, calls) = start_stub_player(&bus);
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    common::seed_tracks(&path, &[]);
    set_bool_setting(&path, "agent.capability.device:sync", true);
    let mut client = McpClient::start_on_bus(&path, &bus);

    let state = client.call_tool("music_get_device_sync_state", json!({}));
    let body = structured_ok(&state);
    assert_eq!(body["devices"][0]["name"], "Pixel");
    assert_eq!(body["devices"][0]["quality_kbps"], 320);
    assert_eq!(body["devices"][0]["playlists"][0]["kind"], "playlist");
    assert_eq!(body["devices"][0]["playlists"][1]["kind"], "smart");
    assert_eq!(body["devices"][0]["changes"]["replacements"], 5);
    assert_eq!(body["devices"][0]["storage"]["current"]["free_bytes"], 40);
    assert_eq!(body["devices"][0]["storage"]["after_sync"]["free_bytes"], 0);
    assert_eq!(body["devices"][0]["progress"]["bytes_per_second"], 10);
    assert_eq!(body["devices"][0]["controls"]["can_cancel"], true);
    assert!(!state.to_string().contains("serial"));
    assert!(!state.to_string().contains("path"));

    for params in [
        json!({
            "action": "configure",
            "device_name": "Pixel",
            "sources": [
                { "kind": "playlist", "id": 3 },
                { "kind": "smart", "id": 7 }
            ],
            "quality_kbps": 320
        }),
        json!({ "action": "start", "device_name": "Pixel" }),
        json!({ "action": "cancel", "device_name": "Pixel" }),
    ] {
        let response = client.call_tool("music_device_sync", params);
        assert!(!tool_success_text(&response).is_empty());
    }

    assert_eq!(
        recorded(&calls),
        vec![
            Recorded::ConfigureDevice {
                device_name: "Pixel".into(),
                sources: vec![("playlist".into(), 3), ("smart".into(), 7)],
                quality_kbps: 320,
            },
            Recorded::StartDevice("Pixel".into()),
            Recorded::CancelDevice("Pixel".into()),
        ]
    );
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

#[test]
fn music_get_playback_state_returns_live_path_free_properties() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("environment-limited: dbus-daemon unavailable; skipping the MPRIS bus roundtrip");
        return;
    };
    let (_conn, _calls) = start_stub_player(&bus);

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    common::seed_tracks(&path, &[]);
    let mut client = McpClient::start_on_bus(&path, &bus);

    let response = client.call_tool("music_get_playback_state", json!({}));
    let body = structured_ok(&response);
    assert_eq!(body["status"], "playing");
    assert_eq!(body["track_id"], 42);
    assert_eq!(body["title"], "Sun//Eater");
    assert_eq!(body["artist"], "Lorna Shore");
    assert_eq!(body["album"], "Pain Remains");
    assert_eq!(body["duration_ms"], 240_000);
    assert_eq!(body["position_ms"], 61_500);
    assert_eq!(body["volume"], 0.72);
    assert_eq!(body["shuffle"], true);
    assert_eq!(body["repeat"], "all");
    assert!(
        !response.to_string().contains("/music/"),
        "playback state must never expose a music path: {response}"
    );
}

#[test]
fn music_set_playback_reaches_each_mpris_setting() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("environment-limited: dbus-daemon unavailable; skipping the MPRIS bus roundtrip");
        return;
    };
    let (_conn, calls) = start_stub_player(&bus);

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    common::seed_tracks(&path, &[]);
    let mut client = McpClient::start_on_bus(&path, &bus);

    for params in [
        json!({ "action": "set_volume", "volume": 0.35 }),
        json!({ "action": "seek", "offset_seconds": -12.5 }),
        json!({ "action": "set_shuffle", "enabled": false }),
        json!({ "action": "set_repeat", "repeat": "one" }),
    ] {
        let response = client.call_tool("music_set_playback", params);
        assert!(
            !tool_success_text(&response).is_empty(),
            "setting should return a confirmation: {response}"
        );
    }

    let recorded = recorded(&calls);
    assert_eq!(recorded.len(), 4);
    assert!(matches!(
        recorded[0],
        Recorded::SetVolume(value) if (value - 0.35).abs() < f64::EPSILON
    ));
    assert_eq!(recorded[1], Recorded::Seek(-12_500_000));
    assert_eq!(recorded[2], Recorded::SetShuffle(false));
    assert_eq!(recorded[3], Recorded::SetLoopStatus("Track".to_owned()));
}

#[test]
fn music_queue_reads_state_and_dispatches_safe_mutations() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("environment-limited: dbus-daemon unavailable; skipping the MPRIS bus roundtrip");
        return;
    };
    let (_conn, calls) = start_stub_player(&bus);

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    let ids = common::seed_tracks(
        &path,
        &[
            SeedTrack::simple("Queue 1", "Artist"),
            SeedTrack::simple("Queue 2", "Artist"),
            SeedTrack::simple("Queue 3", "Artist"),
            SeedTrack::simple("Queue 4", "Artist"),
        ],
    );
    let mut client = McpClient::start_on_bus(&path, &bus);

    let status = client.call_tool("music_queue", json!({ "action": "status" }));
    let body = structured_ok(&status);
    assert_eq!(body["current_track_id"], 42);
    assert_eq!(body["play_next_track_ids"], json!([7, 8]));
    assert_eq!(body["context_track_ids"], json!([9, 10, 11]));
    assert_eq!(body["play_next_total"], 2);
    assert_eq!(body["context_total"], 3);

    for params in [
        json!({ "action": "add_next", "track_ids": &ids[..2] }),
        json!({ "action": "add_last", "track_ids": &ids[2..] }),
        json!({ "action": "clear" }),
    ] {
        let response = client.call_tool("music_queue", params);
        assert!(!tool_success_text(&response).is_empty());
    }

    assert_eq!(
        recorded(&calls),
        vec![
            Recorded::QueueAddNext(ids[..2].to_vec()),
            Recorded::QueueAddLast(ids[2..].to_vec()),
            Recorded::QueueClear,
        ]
    );
}
