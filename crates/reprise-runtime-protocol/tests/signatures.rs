//! The D-Bus signatures of every wire type, pinned to the values the zvariant 5
//! build (the last release before the zbus 6 migration) produced.
//!
//! `schema.rs` round-trips through the same codec it encodes with, so it cannot
//! notice a signature that changed under both ends at once. A peer built before
//! the migration still sends and expects the old signatures, so these strings
//! are the compatibility contract, and they come from that build, not from the
//! code under test. Changing one is a wire break: bump `PROTOCOL_VERSION` and
//! say so in the changelog.
//!
//! Every dictionary type signs as `a{sv}` by attribute, so its fields are not
//! visible here; `schema.rs` pins their names. What this file pins is the shape
//! around them: which types are dictionaries, and which are positional structs.

use reprise_runtime_protocol::command::CommandOutcome;
use reprise_runtime_protocol::device_run::DeviceRunSnapshot;
use reprise_runtime_protocol::device_sync::{
    DeviceChangeCounts, DeviceControls, DeviceProgress, DeviceSnapshot, DeviceSourceSelection,
    DeviceSourceSnapshot, DeviceStorageComposition, DeviceStorageSnapshot, DeviceTargetSnapshot,
};
use reprise_runtime_protocol::effects::{EffectsRequest, EffectsSnapshot};
use reprise_runtime_protocol::jobs::{BatchProgress, JobSnapshot};
use reprise_runtime_protocol::playback::{ExternalMedia, PlaybackSnapshot};
use reprise_runtime_protocol::queue::{QueueItem, QueuePage, QueueSnapshot};
use reprise_runtime_protocol::runtime::RuntimeSnapshot;
use reprise_runtime_protocol::session::RestoredQueue;
use zbus::Type;

fn signature_of<T: Type>() -> String {
    T::SIGNATURE.to_string()
}

#[test]
fn every_wire_type_keeps_the_signature_the_zvariant_5_build_gave_it() {
    let pinned = [
        ("CommandOutcome", signature_of::<CommandOutcome>(), "a{sv}"),
        (
            "DeviceRunSnapshot",
            signature_of::<DeviceRunSnapshot>(),
            "a{sv}",
        ),
        (
            "DeviceChangeCounts",
            signature_of::<DeviceChangeCounts>(),
            "a{sv}",
        ),
        ("DeviceControls", signature_of::<DeviceControls>(), "a{sv}"),
        ("DeviceProgress", signature_of::<DeviceProgress>(), "a{sv}"),
        ("DeviceSnapshot", signature_of::<DeviceSnapshot>(), "a{sv}"),
        (
            "DeviceSourceSelection",
            signature_of::<DeviceSourceSelection>(),
            "(sx)",
        ),
        (
            "DeviceSourceSnapshot",
            signature_of::<DeviceSourceSnapshot>(),
            "a{sv}",
        ),
        (
            "DeviceStorageComposition",
            signature_of::<DeviceStorageComposition>(),
            "a{sv}",
        ),
        (
            "DeviceStorageSnapshot",
            signature_of::<DeviceStorageSnapshot>(),
            "a{sv}",
        ),
        (
            "DeviceTargetSnapshot",
            signature_of::<DeviceTargetSnapshot>(),
            "a{sv}",
        ),
        ("EffectsRequest", signature_of::<EffectsRequest>(), "a{sv}"),
        (
            "EffectsSnapshot",
            signature_of::<EffectsSnapshot>(),
            "a{sv}",
        ),
        ("BatchProgress", signature_of::<BatchProgress>(), "a{sv}"),
        ("JobSnapshot", signature_of::<JobSnapshot>(), "a{sv}"),
        ("ExternalMedia", signature_of::<ExternalMedia>(), "a{sv}"),
        (
            "PlaybackSnapshot",
            signature_of::<PlaybackSnapshot>(),
            "a{sv}",
        ),
        ("QueueItem", signature_of::<QueueItem>(), "a{sv}"),
        ("QueuePage", signature_of::<QueuePage>(), "a{sv}"),
        ("QueueSnapshot", signature_of::<QueueSnapshot>(), "a{sv}"),
        (
            "RuntimeSnapshot",
            signature_of::<RuntimeSnapshot>(),
            "a{sv}",
        ),
        ("RestoredQueue", signature_of::<RestoredQueue>(), "a{sv}"),
        // What the `org.reprise.DeviceSync1` interface returns and accepts.
        (
            "Vec<DeviceSnapshot>",
            signature_of::<Vec<DeviceSnapshot>>(),
            "aa{sv}",
        ),
        (
            "Vec<DeviceSourceSelection>",
            signature_of::<Vec<DeviceSourceSelection>>(),
            "a(sx)",
        ),
    ];
    let drifted: Vec<String> = pinned
        .iter()
        .filter(|(_, actual, expected)| actual != expected)
        .map(|(name, actual, expected)| {
            format!("{name}: {actual}, the zvariant 5 build had {expected}")
        })
        .collect();
    assert!(drifted.is_empty(), "wire signatures changed: {drifted:?}");
}
