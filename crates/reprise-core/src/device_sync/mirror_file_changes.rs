use std::collections::{HashMap, HashSet};

use super::super::settings::DeviceFileRecord;
use super::cue::{CueGroups, SharedRecord};
use super::{
    inventory_matches, push_warning, safe_managed_path, DesiredManagedFile, ManagedRemoval,
    MirrorPlan, MirrorReplacement, MirrorWarning, UnavailableTrack,
};

#[derive(Clone, Copy)]
pub(super) struct FileChangeInput<'a> {
    pub desired: &'a HashMap<i64, DesiredManagedFile>,
    pub cue: &'a CueGroups,
    pub inventory: &'a [DeviceFileRecord],
    pub inventory_by_id: &'a HashMap<i64, DeviceFileRecord>,
    pub unavailable: &'a HashMap<i64, UnavailableTrack>,
    pub stability_margin_ids: &'a HashSet<i64>,
    pub managed_files_scanned: bool,
    pub managed_paths: &'a HashSet<String>,
}

pub(super) fn plan_file_changes(input: FileChangeInput<'_>, plan: &mut MirrorPlan) {
    let FileChangeInput {
        desired,
        cue,
        inventory,
        inventory_by_id,
        unavailable,
        stability_margin_ids,
        managed_files_scanned,
        managed_paths,
    } = input;
    let mut desired_ids = desired.keys().copied().collect::<Vec<_>>();
    desired_ids.sort_unstable();
    let mut planned_files = HashSet::new();
    for track_id in desired_ids.iter().copied() {
        if let Some(group) = cue.group_of(track_id) {
            if planned_files.insert(group) {
                let members: Vec<i64> = desired_ids
                    .iter()
                    .copied()
                    .filter(|id| cue.group_of(*id) == Some(group))
                    .collect();
                plan_cue_file(&input, &members, plan);
            }
            continue;
        }
        let file = &desired[&track_id];
        match inventory_by_id.get(&track_id) {
            None => plan.copy.push(file.clone()),
            Some(existing)
                if inventory_matches(existing, file)
                    && managed_files_scanned
                    && !managed_paths.contains(&existing.device_path.to_lowercase()) =>
            {
                plan.copy.push(file.clone());
            }
            Some(existing) if inventory_matches(existing, file) => {}
            Some(existing) if safe_managed_path(&existing.device_path) => {
                plan.replace.push(MirrorReplacement {
                    existing: existing.clone(),
                    desired: file.clone(),
                });
            }
            Some(existing) => {
                push_warning(
                    &mut plan.warnings,
                    MirrorWarning::UnsafeManagedPath {
                        path: existing.device_path.clone(),
                    },
                );
                plan.copy.push(file.clone());
            }
        }
    }

    let mut unavailable_ids = unavailable
        .keys()
        .copied()
        .filter(|track_id| !desired.contains_key(track_id))
        .collect::<Vec<_>>();
    unavailable_ids.sort_unstable();
    let mut retained_ids = HashSet::new();
    for track_id in unavailable_ids {
        if let Some(existing) = inventory_by_id.get(&track_id) {
            retained_ids.insert(track_id);
            plan.target_bytes = plan.target_bytes.saturating_add(existing.device_size);
            plan.retained_unavailable.push(existing.clone());
        } else {
            push_warning(
                &mut plan.warnings,
                MirrorWarning::UnavailableNotOnDevice { track_id },
            );
        }
    }

    // A file another row still needs stays on the device; the row alone goes.
    let held_paths: HashSet<&str> = desired
        .values()
        .map(|file| file.device_path.as_str())
        .chain(
            inventory
                .iter()
                .filter(|existing| {
                    !desired.contains_key(&existing.track_id)
                        && (retained_ids.contains(&existing.track_id)
                            || stability_margin_ids.contains(&existing.track_id))
                })
                .map(|existing| existing.device_path.as_str()),
        )
        .collect();
    for existing in inventory {
        if desired.contains_key(&existing.track_id) || retained_ids.contains(&existing.track_id) {
            continue;
        }
        if !stability_margin_ids.contains(&existing.track_id)
            && held_paths.contains(existing.device_path.as_str())
        {
            plan.remove.push(ManagedRemoval::Unshared(existing.clone()));
            continue;
        }
        if stability_margin_ids.contains(&existing.track_id) {
            plan.target_bytes = plan.target_bytes.saturating_add(existing.device_size);
            plan.retained_stable.push(existing.clone());
            continue;
        }
        if safe_managed_path(&existing.device_path) {
            plan.bytes_freed = plan.bytes_freed.saturating_add(existing.device_size);
            plan.remove
                .push(ManagedRemoval::Inventory(existing.clone()));
        } else {
            push_warning(
                &mut plan.warnings,
                MirrorWarning::UnsafeManagedPath {
                    path: existing.device_path.clone(),
                },
            );
        }
    }

    plan.transfer_bytes = plan
        .copy
        .iter()
        .map(|file| file.target_bytes)
        .chain(
            plan.replace
                .iter()
                .map(|replacement| replacement.desired.target_bytes),
        )
        .fold(0_u64, u64::saturating_add);
}

/// One transfer for all selected tracks of a CUE file, or none when a row
/// already says the unchanged file is on the device; every other track's row is
/// recorded beside it.
fn plan_cue_file(input: &FileChangeInput<'_>, members: &[i64], plan: &mut MirrorPlan) {
    let row_matches = |id: &i64| {
        input
            .inventory_by_id
            .get(id)
            .is_some_and(|existing| inventory_matches(existing, &input.desired[id]))
    };
    let resident = members.iter().find(|id| {
        row_matches(id)
            && !(input.managed_files_scanned
                && !input
                    .managed_paths
                    .contains(&input.inventory_by_id[*id].device_path.to_lowercase()))
    });
    if let Some(anchor) = resident {
        let size = input.inventory_by_id[anchor].device_size;
        for id in members.iter().filter(|id| !row_matches(id)) {
            plan.shared_records.push(SharedRecord {
                desired: input.desired[id].clone(),
                carrier_track_id: *anchor,
                resident_size: Some(size),
                previous: input.inventory_by_id.get(id).cloned(),
            });
        }
        return;
    }
    let carrier = members
        .iter()
        .copied()
        .find(|id| {
            input
                .inventory_by_id
                .get(id)
                .is_some_and(|existing| safe_managed_path(&existing.device_path))
        })
        .unwrap_or(members[0]);
    let file_bytes = members
        .iter()
        .map(|id| input.desired[id].target_bytes)
        .max()
        .unwrap_or_default();
    let carried = DesiredManagedFile {
        track: input.cue.carrier_track(&input.desired[&carrier].track),
        target_bytes: file_bytes,
        ..input.desired[&carrier].clone()
    };
    match input.inventory_by_id.get(&carrier) {
        Some(existing)
            if safe_managed_path(&existing.device_path)
                && !inventory_matches(existing, &carried) =>
        {
            plan.replace.push(MirrorReplacement {
                existing: existing.clone(),
                desired: carried,
            });
        }
        Some(existing) if !safe_managed_path(&existing.device_path) => {
            push_warning(
                &mut plan.warnings,
                MirrorWarning::UnsafeManagedPath {
                    path: existing.device_path.clone(),
                },
            );
            plan.copy.push(carried);
        }
        _ => plan.copy.push(carried),
    }
    for id in members.iter().copied().filter(|id| *id != carrier) {
        plan.shared_records.push(SharedRecord {
            desired: input.desired[&id].clone(),
            carrier_track_id: carrier,
            resident_size: None,
            previous: input.inventory_by_id.get(&id).cloned(),
        });
    }
}
