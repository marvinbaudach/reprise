use std::collections::{HashMap, HashSet};

use super::super::settings::DeviceFileRecord;
use super::{
    inventory_matches, push_warning, safe_managed_path, DesiredManagedFile, ManagedRemoval,
    MirrorPlan, MirrorReplacement, MirrorWarning, UnavailableTrack,
};

#[derive(Clone, Copy)]
pub(super) struct FileChangeInput<'a> {
    pub desired: &'a HashMap<i64, DesiredManagedFile>,
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
        inventory,
        inventory_by_id,
        unavailable,
        stability_margin_ids,
        managed_files_scanned,
        managed_paths,
    } = input;
    let mut desired_ids = desired.keys().copied().collect::<Vec<_>>();
    desired_ids.sort_unstable();
    for track_id in desired_ids {
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

    for existing in inventory {
        if desired.contains_key(&existing.track_id) || retained_ids.contains(&existing.track_id) {
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
