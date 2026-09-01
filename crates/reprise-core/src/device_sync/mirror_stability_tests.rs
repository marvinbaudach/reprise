use super::*;

#[test]
fn resident_track_flapping_across_a_smart_cap_is_not_removed_then_recopied() {
    let source = SelectionSource::Smart(20);
    let first = track(1, "/music/one.mp3", Some(192), 10_000, 240_000);
    let resident = track(2, "/music/two.mp3", Some(192), 10_000, 240_000);
    let promoted = track(3, "/music/three.mp3", Some(192), 10_000, 240_000);
    let resident_path = "Album Artist/Album/02 Track 2.mp3";
    let resident_inventory = inventory(&resident, resident_path, "copy-original-v1");

    let mut dropped_snapshot = playlist(
        source.clone(),
        "Capped smart playlist",
        vec![
            MirrorTrack::Available(first.clone()),
            MirrorTrack::Available(promoted),
        ],
    );
    dropped_snapshot.stability_margin_track_ids = vec![resident.id];
    let mut dropped_input = input(vec![source.clone()], vec![dropped_snapshot]);
    dropped_input.inventory.push(resident_inventory.clone());

    let dropped_plan = plan_mirror(dropped_input);

    assert!(
        dropped_plan.remove.is_empty(),
        "a resident track immediately below the cap stays on the device"
    );
    assert_eq!(
        dropped_plan.target_bytes, 720_000,
        "retained margin files remain part of the truthful device target"
    );

    let mut returned_input = input(
        vec![source.clone()],
        vec![playlist(
            source,
            "Capped smart playlist",
            vec![
                MirrorTrack::Available(first),
                MirrorTrack::Available(resident.clone()),
            ],
        )],
    );
    returned_input.inventory.push(resident_inventory);

    let returned_plan = plan_mirror(returned_input);

    assert!(
        returned_plan
            .copy
            .iter()
            .all(|file| file.track.id != resident.id),
        "the returning resident is not copied again"
    );
    assert!(
        returned_plan
            .replace
            .iter()
            .all(|replacement| replacement.desired.track.id != resident.id),
        "the returning resident is not replaced"
    );
    assert!(returned_plan.remove.is_empty());
}

#[test]
fn stability_margin_retains_the_resident_analysis_sidecar() {
    let source = SelectionSource::Smart(20);
    let wanted = track(1, "/music/one.mp3", Some(192), 10_000, 240_000);
    let resident = track(2, "/music/two.mp3", Some(192), 10_000, 240_000);
    let resident_path = "Album Artist/Album/02 Track 2.mp3";
    let sidecar_path = "Album Artist/Album/02 Track 2.reprise-analysis";
    let mut snapshot = playlist(
        source.clone(),
        "Capped smart playlist",
        vec![MirrorTrack::Available(wanted)],
    );
    snapshot.stability_margin_track_ids = vec![resident.id];
    let mut mirror_input = input(vec![source], vec![snapshot]);
    mirror_input
        .inventory
        .push(inventory(&resident, resident_path, "copy-original-v1"));
    mirror_input.managed_files.extend([
        ManagedDeviceFile {
            relative_path: resident_path.into(),
            size_bytes: resident.size_bytes,
        },
        ManagedDeviceFile {
            relative_path: sidecar_path.into(),
            size_bytes: 123,
        },
    ]);

    let plan = plan_mirror(mirror_input);

    assert!(
        plan.remove.is_empty(),
        "the analysis sidecar follows its margin-protected audio file"
    );
}

#[test]
fn resident_track_outside_the_smart_stability_margin_is_still_removed() {
    let source = SelectionSource::Smart(20);
    let wanted = track(1, "/music/one.mp3", Some(192), 10_000, 240_000);
    let stale = track(2, "/music/two.mp3", Some(192), 10_000, 240_000);
    let mut mirror_input = input(
        vec![source.clone()],
        vec![playlist(
            source,
            "Capped smart playlist",
            vec![MirrorTrack::Available(wanted)],
        )],
    );
    mirror_input.inventory.push(inventory(
        &stale,
        "Album Artist/Album/02 Track 2.mp3",
        "copy-original-v1",
    ));

    let plan = plan_mirror(mirror_input);

    assert!(plan.remove.iter().any(|removal| matches!(
        removal,
        ManagedRemoval::Inventory(file) if file.track_id == stale.id
    )));
    assert_eq!(plan.bytes_freed, stale.size_bytes);
}
