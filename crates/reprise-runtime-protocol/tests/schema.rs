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

use reprise_runtime_protocol::device_run::DeviceRunSnapshot;
use reprise_runtime_protocol::device_sync::{
    DeviceCategorySnapshot, DeviceChangeCounts, DeviceControls, DeviceProgress, DeviceSnapshot,
    DeviceSourceSnapshot, DeviceStorageComposition, DeviceStorageSnapshot,
};
use reprise_runtime_protocol::jobs::{BatchProgress, JobSnapshot};
use reprise_runtime_protocol::playback::PlaybackSnapshot;
use reprise_runtime_protocol::queue::QueueSnapshot;
use reprise_runtime_protocol::runtime::RuntimeSnapshot;
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
        categories: vec![
            DeviceCategorySnapshot {
                kind: "playlists".into(),
                target_path: "/Music/Reprise".into(),
                target_enabled: true,
                size_on_device_bytes: 3_221_225_472,
                cap_bytes: None,
                reading_kind: "diff".into(),
                files_to_copy: 14,
                bytes_to_copy: 2_791_728_742,
                files_to_remove: 3,
                bytes_freed: 155_189_248,
                files_waiting_for_download: 0,
                playlists_rewritten: 2,
            },
            // The two states that are not a diff have to survive the wire as
            // themselves (`MTP-22`): "never examined" must not arrive looking
            // like "examined and found nothing".
            DeviceCategorySnapshot {
                kind: "youtube_audio".into(),
                target_path: "/Music/Reprise-YouTube".into(),
                target_enabled: false,
                size_on_device_bytes: 0,
                cap_bytes: Some(8_589_934_592),
                reading_kind: "source_off".into(),
                ..DeviceCategorySnapshot::default()
            },
            DeviceCategorySnapshot {
                kind: "podcast_episodes".into(),
                target_path: "/Podcasts/Reprise".into(),
                target_enabled: true,
                size_on_device_bytes: 1_073_741_824,
                cap_bytes: Some(4_294_967_296),
                reading_kind: "unavailable_kept_on_phone".into(),
                ..DeviceCategorySnapshot::default()
            },
        ],
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
        failure_kind: Some("not_playable".into()),
        failure_track_id: Some(41),
        initiated_by: Some(7),
    };
    assert_eq!(round_trip(&playback), playback);

    let queue = QueueSnapshot {
        revision: 9,
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

    let outcome = reprise_runtime_protocol::command::CommandOutcome {
        queue_revision: 12,
        affected: 4,
    };
    assert_eq!(round_trip(&outcome), outcome);

    let batch = BatchProgress {
        batch_id: "b-1".into(),
        total: 4,
        finished: 2,
        failed: 1,
        progress_permille: 500,
    };
    assert_eq!(round_trip(&batch), batch);
}

/// Every field set, every optional present — same convention as
/// [`populated_device`], which is what makes it usable as the field-name
/// fixture (an absent optional serializes to no key at all).
fn populated_device_run() -> DeviceRunSnapshot {
    DeviceRunSnapshot {
        device: "Pixel 8".into(),
        phase: "verifying".into(),
        progress: DeviceProgress {
            bytes_done: 104_857_600,
            bytes_total: 3_221_225_472,
            bytes_per_second: 8_388_608,
        },
        current_track: "Ghosts".into(),
        failed_track_ids: vec![41, 42],
        outcome: Some("completed".into()),
    }
}

#[test]
fn a_device_run_snapshot_survives_a_dbus_round_trip() {
    let finished = populated_device_run();
    assert_eq!(round_trip(&finished), finished);

    // A run in flight is the case that leaves the optional unset, and an
    // absent `outcome` is exactly what separates "still going" from "ended".
    let running = DeviceRunSnapshot {
        phase: "copying".into(),
        outcome: None,
        ..populated_device_run()
    };
    assert_eq!(round_trip(&running), running);
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
fn the_playback_wire_field_names_are_the_checked_in_contract() {
    // Pinned for the same reason the device snapshot is: a client decodes by
    // name, so a rename is a client reading the wrong column rather than a
    // compile error. `failure_*` and `initiated_by` are the ones a surface
    // branches on — a skipped track's toast and the quit policy.
    assert_eq!(
        field_names(&PlaybackSnapshot {
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
            failure_kind: Some("not_playable".into()),
            failure_track_id: Some(41),
            initiated_by: Some(7),
        }),
        [
            "album",
            "artist",
            "duration_ms",
            "failure_kind",
            "failure_track_id",
            "initiated_by",
            "position_ms",
            "repeat",
            "shuffle",
            "status",
            "title",
            "track_id",
            "volume",
        ]
    );
}

#[test]
fn the_wire_field_names_are_the_checked_in_contract() {
    assert_eq!(
        field_names(&populated_device()),
        [
            "blockers",
            "categories",
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
    assert_eq!(
        field_names(&populated_device_run()),
        [
            "current_track",
            "device",
            "failed_track_ids",
            "outcome",
            "phase",
            "progress",
        ]
    );
}

/// Walks every string in a serialized snapshot and rejects anything that
/// looks like a local filesystem path. Deliberately blunt: a false positive
/// here means a display value contains a slash, which is worth a look.
/// Fields whose value is a folder **on the connected phone**, not on this
/// machine (`MTP-38`). They are absolute in the device's own namespace, so
/// they start with `/` and are textually indistinguishable from a local path —
/// but showing them is the entire point of the field, and no local filesystem
/// location can reach them. Everything else stays under the blanket ban below.
const DEVICE_PATH_FIELDS: [&str; 1] = ["target_path"];

fn strings(value: &serde_json::Value, found: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => found.push(text.clone()),
        serde_json::Value::Array(items) => items.iter().for_each(|item| strings(item, found)),
        serde_json::Value::Object(map) => map
            .iter()
            .filter(|(key, _)| !DEVICE_PATH_FIELDS.contains(&key.as_str()))
            .for_each(|(_, item)| strings(item, found)),
        _ => {}
    }
}

/// Collects exactly the values [`strings`] skips, so the exemption stays
/// checked rather than merely trusted.
fn device_path_values(value: &serde_json::Value, found: &mut Vec<String>) {
    match value {
        serde_json::Value::Array(items) => items
            .iter()
            .for_each(|item| device_path_values(item, found)),
        serde_json::Value::Object(map) => {
            for (key, item) in map {
                if DEVICE_PATH_FIELDS.contains(&key.as_str()) {
                    // zvariant serializes each field as {"signature", "value"},
                    // so the string sits one level below the field name.
                    collect_field_strings(item, found);
                } else {
                    device_path_values(item, found);
                }
            }
        }
        _ => {}
    }
}

/// Every string under one serialized field, skipping the D-Bus type signature
/// that zvariant pairs with the value.
fn collect_field_strings(value: &serde_json::Value, found: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => found.push(text.clone()),
        serde_json::Value::Array(items) => items
            .iter()
            .for_each(|item| collect_field_strings(item, found)),
        serde_json::Value::Object(map) => map
            .iter()
            .filter(|(key, _)| key.as_str() != "signature")
            .for_each(|(_, item)| collect_field_strings(item, found)),
        _ => {}
    }
}

#[test]
fn no_snapshot_carries_a_local_filesystem_path() {
    let fixtures = [
        ("device", serde_json::to_value(populated_device()).unwrap()),
        (
            "device run",
            serde_json::to_value(populated_device_run()).unwrap(),
        ),
    ];

    for (label, fixture) in fixtures {
        let mut values = Vec::new();
        strings(&fixture, &mut values);
        assert!(
            !values.is_empty(),
            "the {label} fixture must actually carry strings"
        );

        for value in values {
            assert!(
                !value.starts_with('/')
                    && !value.starts_with("~/")
                    && !value.starts_with("file://"),
                "{label} snapshot leaked a path-like value: {value}"
            );
        }

        // The exemption above is not a blank cheque: a device path must still
        // be a device path. This catches a local location accidentally
        // assigned to `target_path`, which the walker no longer inspects.
        let mut device_paths = Vec::new();
        device_path_values(&fixture, &mut device_paths);
        for value in device_paths {
            assert!(
                value.starts_with('/')
                    && !value.starts_with("/home/")
                    && !value.starts_with("/run/")
                    && !value.contains("file://"),
                "{label} snapshot carries a local location in a device path field: {value}"
            );
        }
    }
}

/// The handshake payload nests every other snapshot, so its round trip is
/// the one that proves the whole tree survives together rather than only
/// piece by piece.
#[test]
fn the_whole_runtime_snapshot_survives_a_dbus_round_trip() {
    let snapshot = RuntimeSnapshot {
        protocol_major: PROTOCOL_VERSION.major,
        protocol_minor: PROTOCOL_VERSION.minor,
        client_id: 7,
        sequence: 41,
        playback: PlaybackSnapshot {
            status: "paused".into(),
            track_id: Some(42),
            title: "Ghosts".into(),
            artist: "Nine Inch Nails".into(),
            album: "Ghosts I-IV".into(),
            duration_ms: 214_000,
            position_ms: 61_000,
            volume: 0.75,
            shuffle: false,
            repeat: "off".into(),
            failure_kind: Some("backend".into()),
            failure_track_id: Some(41),
            initiated_by: Some(7),
        },
        queue: QueueSnapshot {
            revision: 9,
            current_track_id: Some(42),
            play_next_track_ids: vec![43],
            context_track_ids: vec![44, 45],
            play_next_total: 1,
            context_total: 2,
        },
        device_runs: vec![populated_device_run()],
        jobs: vec![JobSnapshot {
            job_id: 9,
            kind: "instrumental".into(),
            state: "staged".into(),
            progress_permille: 1_000,
            batch_id: None,
            source_track_id: Some(42),
            result_track_id: None,
            cancel_requested: false,
            error_kind: None,
        }],
    };

    assert_eq!(round_trip(&snapshot), snapshot);

    // The empty runtime is the state every fresh process starts in, and an
    // empty `Vec` inside a dictionary is exactly where an encoder is most
    // likely to disagree with its decoder about the signature.
    let empty = RuntimeSnapshot::default();
    assert_eq!(round_trip(&empty), empty);
}

#[test]
fn the_protocol_version_is_pinned() {
    assert_eq!(PROTOCOL_VERSION.major, 3);
    assert_eq!(PROTOCOL_VERSION.minor, 1);
}
