//! Schema fixtures for the runtime protocol.
//!
//! Three properties are pinned here rather than trusted to review:
//!
//! 1. Every snapshot survives a real D-Bus encode/decode round trip,
//!    including its nested dictionaries and its absent optional fields.
//! 2. The wire field names are exactly the checked-in set. A rename or a
//!    reorder shows up as a failing diff instead of as a client that decodes
//!    the wrong column.
//! 3. No snapshot carries a local filesystem path.

use reprise_runtime_protocol::device_sync::{
    DeviceChangeCounts, DeviceControls, DeviceProgress, DeviceSnapshot, DeviceSourceSnapshot,
    DeviceStorageComposition, DeviceStorageSnapshot,
};
use reprise_runtime_protocol::jobs::{BatchProgress, JobSnapshot};
use reprise_runtime_protocol::playback::PlaybackSnapshot;
use reprise_runtime_protocol::queue::QueueSnapshot;
use reprise_runtime_protocol::PROTOCOL_VERSION;

fn dbus_context() -> zvariant::serialized::Context {
    zvariant::serialized::Context::new_dbus(zvariant::Endian::native(), 0)
}

/// Encodes with the real D-Bus format and decodes back. Returns the decoded
/// value so the caller can compare it against the original.
fn round_trip<T>(value: &T) -> T
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de> + zvariant::DynamicType + zvariant::Type,
{
    let encoded = zvariant::to_bytes(dbus_context(), value).expect("snapshot encodes for D-Bus");
    let (decoded, _) = encoded.deserialize::<T>().expect("snapshot decodes again");
    decoded
}

/// Every field set, every optional present — the shape a maximally
/// populated device produces.
fn populated_device() -> DeviceSnapshot {
    DeviceSnapshot {
        name: "Pixel 8".into(),
        connected: true,
        profile: "opus_160".into(),
        managed_tracks: 412,
        unique_track_count: 400,
        target_bytes: 3_221_225_472,
        sources: vec![DeviceSourceSnapshot {
            kind: "playlist".into(),
            id: 12,
            name: Some("Morning".into()),
            selected: true,
            available: true,
            entry_count: 40,
            unique_track_count: 38,
            unavailable_count: 2,
            target_bytes: 314_572_800,
            last_synced_at: Some(1_753_600_000),
        }],
        changes: DeviceChangeCounts {
            additions: 7,
            replacements: 2,
            removals: 1,
            retained_unavailable: 3,
            playlist_writes: 4,
            playlist_removals: 0,
            transfer_bytes: 104_857_600,
        },
        storage: DeviceStorageSnapshot {
            target_name: Some("Internal shared storage".into()),
            state: "insufficient".into(),
            shortfall_bytes: Some(52_428_800),
            transfer_bytes: 104_857_600,
            current: populated_composition(),
            after_sync: Some(populated_composition()),
            access: "writable".into(),
        },
        blockers: vec!["missing_playlist:playlist:12".into()],
        warnings: vec!["unavailable_not_on_device".into()],
        controls: DeviceControls {
            editable: true,
            can_start: true,
            can_cancel: false,
            can_eject: true,
        },
        phase: "copying".into(),
        progress: DeviceProgress {
            bytes_done: 52_428_800,
            bytes_total: 104_857_600,
            bytes_per_second: 5_242_880,
        },
        current_track: "Ghosts".into(),
        last_synced_at: Some(1_753_600_000),
    }
}

fn populated_composition() -> DeviceStorageComposition {
    DeviceStorageComposition {
        total_bytes: Some(64_424_509_440),
        reprise_music_bytes: 3_221_225_472,
        other_music_bytes: 1_073_741_824,
        other_used_bytes: Some(10_737_418_240),
        free_bytes: Some(49_392_123_904),
        knowledge: "complete".into(),
    }
}

#[test]
fn a_populated_device_snapshot_survives_a_dbus_round_trip() {
    let original = populated_device();
    assert_eq!(round_trip(&original), original);
}

/// The interesting half of the optional handling: the tuple encoding used
/// `(bool, T)` pairs, so an unset value still occupied a slot both sides had
/// to interpret identically. An absent dictionary key cannot be misread.
#[test]
fn absent_optionals_round_trip_as_absent_rather_than_as_defaults() {
    let sparse = DeviceSnapshot {
        name: "Unknown device".into(),
        connected: false,
        profile: "original".into(),
        storage: DeviceStorageSnapshot {
            target_name: None,
            state: "capacity_unknown".into(),
            shortfall_bytes: None,
            after_sync: None,
            current: DeviceStorageComposition {
                total_bytes: None,
                other_used_bytes: None,
                free_bytes: None,
                knowledge: "capacity_unknown".into(),
                ..DeviceStorageComposition::default()
            },
            access: "unknown".into(),
            ..DeviceStorageSnapshot::default()
        },
        sources: vec![DeviceSourceSnapshot {
            kind: "smart".into(),
            id: 3,
            name: None,
            last_synced_at: None,
            ..DeviceSourceSnapshot::default()
        }],
        phase: "idle".into(),
        last_synced_at: None,
        ..DeviceSnapshot::default()
    };

    let decoded = round_trip(&sparse);
    assert_eq!(decoded, sparse);
    assert!(decoded.last_synced_at.is_none());
    assert!(decoded.storage.after_sync.is_none());
    assert!(decoded.storage.shortfall_bytes.is_none());
    assert!(decoded.sources[0].name.is_none());
}

#[test]
fn playback_queue_and_job_snapshots_survive_a_dbus_round_trip() {
    let playback = PlaybackSnapshot {
        status: "playing".into(),
        track_id: Some(42),
        title: "Ghosts".into(),
        artist: "Nine Inch Nails".into(),
        album: "Ghosts I-IV".into(),
        duration_ms: 214_000,
        position_ms: 61_000,
        volume: 0.75,
        shuffle: true,
        repeat: "all".into(),
    };
    assert_eq!(round_trip(&playback), playback);

    let queue = QueueSnapshot {
        current_track_id: Some(42),
        play_next_track_ids: vec![43, 44],
        context_track_ids: vec![45, 46, 47],
        play_next_total: 2,
        context_total: 412,
    };
    assert_eq!(round_trip(&queue), queue);

    let job = JobSnapshot {
        job_id: 9,
        kind: "instrumental".into(),
        state: "running".into(),
        progress_permille: 640,
        batch_id: Some("b-1".into()),
        source_track_id: Some(42),
        result_track_id: None,
        cancel_requested: false,
        error_kind: None,
    };
    assert_eq!(round_trip(&job), job);

    let batch = BatchProgress {
        batch_id: "b-1".into(),
        total: 4,
        finished: 2,
        failed: 1,
        progress_permille: 500,
    };
    assert_eq!(round_trip(&batch), batch);
}

fn field_names(value: &impl serde::Serialize) -> Vec<String> {
    let json = serde_json::to_value(value).expect("snapshot serializes as a map");
    let mut names: Vec<String> = json
        .as_object()
        .expect("snapshot is a map, not a positional tuple")
        .keys()
        .cloned()
        .collect();
    names.sort();
    names
}

#[test]
fn the_wire_field_names_are_the_checked_in_contract() {
    assert_eq!(
        field_names(&populated_device()),
        [
            "blockers",
            "changes",
            "connected",
            "controls",
            "current_track",
            "last_synced_at",
            "managed_tracks",
            "name",
            "phase",
            "profile",
            "progress",
            "sources",
            "storage",
            "target_bytes",
            "unique_track_count",
            "warnings",
        ]
    );
    assert_eq!(
        field_names(&populated_device().changes),
        [
            "additions",
            "playlist_removals",
            "playlist_writes",
            "removals",
            "replacements",
            "retained_unavailable",
            "transfer_bytes",
        ]
    );
    assert_eq!(
        field_names(&populated_device().storage),
        [
            "access",
            "after_sync",
            "current",
            "shortfall_bytes",
            "state",
            "target_name",
            "transfer_bytes",
        ]
    );
    assert_eq!(
        field_names(&populated_composition()),
        [
            "free_bytes",
            "knowledge",
            "other_music_bytes",
            "other_used_bytes",
            "reprise_music_bytes",
            "total_bytes",
        ]
    );
}

/// Walks every string in a serialized snapshot and rejects anything that
/// looks like a local filesystem path. Deliberately blunt: a false positive
/// here means a display value contains a slash, which is worth a look.
fn strings(value: &serde_json::Value, found: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => found.push(text.clone()),
        serde_json::Value::Array(items) => items.iter().for_each(|item| strings(item, found)),
        serde_json::Value::Object(map) => map.values().for_each(|item| strings(item, found)),
        _ => {}
    }
}

#[test]
fn no_snapshot_carries_a_local_filesystem_path() {
    let device = serde_json::to_value(populated_device()).unwrap();
    let mut values = Vec::new();
    strings(&device, &mut values);
    assert!(
        !values.is_empty(),
        "the fixture must actually carry strings"
    );

    for value in values {
        assert!(
            !value.starts_with('/') && !value.starts_with("~/") && !value.starts_with("file://"),
            "device snapshot leaked a path-like value: {value}"
        );
    }
}

#[test]
fn the_protocol_version_is_pinned() {
    assert_eq!(PROTOCOL_VERSION.major, 1);
    assert_eq!(PROTOCOL_VERSION.minor, 0);
}
