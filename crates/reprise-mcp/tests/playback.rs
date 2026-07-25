//! `music_playback_control`/`music_play`: argument validation and the
//! capability path, mirroring `tests/create_playlist.rs`'s harness. The
//! actual D-Bus round trip (a live Reprise app) is out of scope here — tests
//! that reach the bus run under `dbus-run-session` (a private, player-less
//! session bus) so "no running Reprise app" is the deterministic outcome,
//! proving capability + resolution succeeded before the bus call ever
//! happens. Mirrors `reprise-cli`'s own `dbus-run-session` pattern
//! (`crates/reprise-cli/tests/playback.rs`). `--features mpris` only.
#![cfg(feature = "mpris")]

mod common;

use common::{set_bool_setting, tool_error_text, McpClient, SeedTrack};
use serde_json::json;
use tempfile::TempDir;

const CAP_PLAYBACK_CONTROL: &str = "agent.capability.playback:control";

/// Seeds one track and a playlist containing it; returns the db path and the
/// playlist id.
fn db_with_playlist(dir: &TempDir) -> (std::path::PathBuf, i64) {
    let path = dir.path().join("reprise.db");
    let ids = common::seed_tracks(&path, &[SeedTrack::simple("Track0", "Artist")]);
    let mut conn = reprise_core::db::open_migrated(Some(&path)).unwrap();
    let playlist_id = reprise_core::library::playlists::create(&conn, "Roadtrip").unwrap();
    reprise_core::library::playlists::add_tracks(&mut conn, playlist_id, &ids).unwrap();
    (path, playlist_id)
}

// --- music_play ---------------------------------------------------------

#[test]
fn music_play_without_track_ids_or_playlist_id_is_invalid_input() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    common::seed_tracks(&path, &[]);
    // playback:control is on by default — the failure here is the "provide
    // exactly one of track_ids or playlist_id" input check, not a capability
    // refusal, and it never touches the bus.
    let mut client = McpClient::start(&path);

    let response = client.call_tool("music_play", json!({}));
    let text = tool_error_text(&response);
    assert!(
        text.contains("track_ids") || text.contains("playlist_id"),
        "should name the missing id source: {text}"
    );
}

#[test]
fn music_play_refused_when_capability_revoked() {
    let dir = TempDir::new().unwrap();
    let (path, playlist_id) = db_with_playlist(&dir);
    set_bool_setting(&path, CAP_PLAYBACK_CONTROL, false);
    let mut client = McpClient::start(&path);

    let response = client.call_tool("music_play", json!({ "playlist_id": playlist_id }));
    let text = tool_error_text(&response);
    assert!(
        text.contains("Permission denied") && text.contains("playback:control"),
        "revoked capability should refuse: {text}"
    );
}

#[test]
fn music_play_resolves_playlist_then_reports_no_running_app() {
    let dir = TempDir::new().unwrap();
    let (path, playlist_id) = db_with_playlist(&dir);
    // playback:control is on by default — no explicit grant needed.
    let Some(mut client) = McpClient::start_under_private_bus(&path) else {
        eprintln!(
            "environment-limited: dbus-run-session unavailable; skipping the MPRIS bus roundtrip"
        );
        return;
    };

    let response = client.call_tool("music_play", json!({ "playlist_id": playlist_id }));
    let text = tool_error_text(&response);
    assert!(
        text.contains("no running Reprise app"),
        "capability + resolution should succeed, failing only at the bus: {text}"
    );
}

// --- music_playback_control ----------------------------------------------

#[test]
fn music_playback_control_rejects_unknown_action() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    common::seed_tracks(&path, &[]);
    let mut client = McpClient::start(&path);

    let response = client.call_tool("music_playback_control", json!({ "action": "rewind" }));
    let text = tool_error_text(&response);
    assert!(
        text.contains("unknown action"),
        "should reject an unrecognised action: {text}"
    );
}

#[test]
fn music_playback_control_refused_when_capability_revoked() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    common::seed_tracks(&path, &[]);
    set_bool_setting(&path, CAP_PLAYBACK_CONTROL, false);
    let mut client = McpClient::start(&path);

    let response = client.call_tool("music_playback_control", json!({ "action": "play" }));
    let text = tool_error_text(&response);
    assert!(
        text.contains("Permission denied") && text.contains("playback:control"),
        "revoked capability should refuse: {text}"
    );
}

#[test]
fn music_playback_control_reports_no_running_app() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    common::seed_tracks(&path, &[]);
    // playback:control is on by default — no explicit grant needed.
    let Some(mut client) = McpClient::start_under_private_bus(&path) else {
        eprintln!(
            "environment-limited: dbus-run-session unavailable; skipping the MPRIS bus roundtrip"
        );
        return;
    };

    let response = client.call_tool("music_playback_control", json!({ "action": "play" }));
    let text = tool_error_text(&response);
    assert!(
        text.contains("no running Reprise app"),
        "capability + action parsing should succeed, failing only at the bus: {text}"
    );
}

#[test]
fn music_set_playback_validates_action_specific_arguments() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    common::seed_tracks(&path, &[]);
    let mut client = McpClient::start(&path);

    for (params, expected) in [
        (json!({ "action": "set_volume" }), "volume"),
        (
            json!({ "action": "set_volume", "volume": 1.5 }),
            "between 0 and 1",
        ),
        (json!({ "action": "seek" }), "offset_seconds"),
        (json!({ "action": "set_shuffle" }), "enabled"),
        (json!({ "action": "set_repeat" }), "repeat"),
        (
            json!({ "action": "set_repeat", "repeat": "forever" }),
            "off, all, or one",
        ),
        (json!({ "action": "boost_bass" }), "unknown action"),
    ] {
        let response = client.call_tool("music_set_playback", params);
        let text = tool_error_text(&response);
        assert!(
            text.contains(expected),
            "expected {expected:?} in validation error: {text}"
        );
    }
}

#[test]
fn playback_state_and_settings_are_refused_when_capability_is_revoked() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    common::seed_tracks(&path, &[]);
    set_bool_setting(&path, CAP_PLAYBACK_CONTROL, false);
    let mut client = McpClient::start(&path);

    for (tool, params) in [
        ("music_get_playback_state", json!({})),
        (
            "music_set_playback",
            json!({ "action": "set_shuffle", "enabled": true }),
        ),
        ("music_queue", json!({ "action": "status" })),
    ] {
        let response = client.call_tool(tool, params);
        let text = tool_error_text(&response);
        assert!(
            text.contains("Permission denied") && text.contains("playback:control"),
            "revoked capability should refuse {tool}: {text}"
        );
    }
}

#[test]
fn music_queue_validates_action_specific_arguments() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    common::seed_tracks(&path, &[]);
    let mut client = McpClient::start(&path);

    for (params, expected) in [
        (json!({ "action": "add_next" }), "track_ids"),
        (
            json!({ "action": "add_last", "track_ids": [] }),
            "must not be empty",
        ),
        (json!({ "action": "remove" }), "unknown action"),
    ] {
        let response = client.call_tool("music_queue", params);
        let text = tool_error_text(&response);
        assert!(
            text.contains(expected),
            "expected {expected:?} in validation error: {text}"
        );
    }
}
