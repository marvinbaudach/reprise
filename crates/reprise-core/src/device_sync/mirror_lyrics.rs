use super::*;

pub(super) fn plan_lyrics_sidecars(
    lyrics_files: &[ManagedDeviceFile],
    managed_files: &[ManagedDeviceFile],
    plan: &mut MirrorPlan,
) {
    let resident_audio = managed_files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect::<HashSet<_>>();
    let resident_lyrics = lyrics_files
        .iter()
        .map(|file| {
            (
                crate::device_sync::device_case::fold_path(&file.relative_path),
                file.size_bytes,
            )
        })
        .collect::<HashMap<_, _>>();
    let arriving_audio = arriving_audio_paths(&plan.copy, &plan.replace);
    for desired in &plan.desired_files {
        if !resident_audio.contains(desired.device_path.as_str())
            && !arriving_audio.contains(desired.device_path.as_str())
        {
            continue;
        }
        let Some(sidecar) = crate::device_sync::lyrics_sidecar::paths_for_track(
            &desired.track.source_path,
            &desired.device_path,
        ) else {
            continue;
        };
        let Some(size_bytes) =
            crate::device_sync::lyrics_sidecar::source_file_size(&sidecar.source_path)
        else {
            continue;
        };
        let existing_size_bytes = resident_lyrics
            .get(&crate::device_sync::device_case::fold_path(
                &sidecar.device_path,
            ))
            .copied();
        if existing_size_bytes == Some(size_bytes) {
            continue;
        }
        plan.lyrics_writes.push(LyricsSidecarWrite {
            track_id: desired.track.id,
            source_path: sidecar.source_path,
            device_path: sidecar.device_path,
            size_bytes,
            existing_size_bytes,
        });
        plan.transfer_bytes = plan.transfer_bytes.saturating_add(size_bytes);
        plan.target_bytes = plan.target_bytes.saturating_add(size_bytes);
    }
}
