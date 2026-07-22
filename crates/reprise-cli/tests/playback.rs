//! MPRIS playback client. Unit-level argument/error mapping is covered by the
//! `commands::playback` module tests; this file is the real-bus roundtrip,
//! which runs only when `dbus-run-session` is trivially available (it starts a
//! private session bus with no player registered, so "no player" is the
//! deterministic outcome). Where it is absent the test documents itself as
//! environment-limited and does not fake a bus. `--features mpris` only.
#![cfg(feature = "mpris")]

use std::process::Command;

/// Whether `dbus-run-session` can be spawned to give us a private session bus.
fn dbus_run_session_available() -> bool {
    Command::new("dbus-run-session")
        .arg("--help")
        .output()
        .is_ok()
}

/// Runs `reprise-cli <args>` under a fresh private session bus (no player).
fn under_private_bus(args: &[&str]) -> std::process::Output {
    Command::new("dbus-run-session")
        .arg("--")
        .arg(env!("CARGO_BIN_EXE_reprise-cli"))
        .args(args)
        .output()
        .expect("run reprise-cli under dbus-run-session")
}

#[test]
fn status_on_a_bus_with_no_player_reports_no_player() {
    if !dbus_run_session_available() {
        eprintln!(
            "environment-limited: dbus-run-session unavailable; skipping the MPRIS bus roundtrip"
        );
        return;
    }
    let out = under_private_bus(&["playback", "status"]);
    assert_eq!(
        out.status.code(),
        Some(8),
        "no player present is an Unavailable exit"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no running Reprise player"),
        "clear no-player message expected, got: {stderr}"
    );
}

#[test]
fn a_control_action_on_a_bus_with_no_player_reports_no_player() {
    if !dbus_run_session_available() {
        eprintln!(
            "environment-limited: dbus-run-session unavailable; skipping the MPRIS bus roundtrip"
        );
        return;
    }
    let out = under_private_bus(&["playback", "play-pause"]);
    assert_eq!(out.status.code(), Some(8));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("org.mpris.MediaPlayer2.reprise"),
        "the message should name the MPRIS bus, got: {stderr}"
    );
}
