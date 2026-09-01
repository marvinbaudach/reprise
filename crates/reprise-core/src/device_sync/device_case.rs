//! Resolves case-only path differences without renaming anything over MTP.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::mirror::{DesiredManagedFile, ManagedDeviceFile};
use super::settings::DeviceFileRecord;

/// The spelling the device already uses for a path, when it differs only in case.
pub(super) enum ResidentSpelling {
    /// Use this path instead of the desired one.
    Adopt(String),
    /// Nothing on the device folds equal; keep the desired path.
    Keep,
    /// Two spellings tie; the caller must not invent one.
    Ambiguous,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DirectorySpelling {
    Resident(String),
    Ambiguous { first: String, second: String },
}

/// Chooses the resident full path first, then the resident directory majority.
pub(super) fn adopt_resident_spelling(
    desired_path: &str,
    own_inventory_path: Option<&str>,
    directory_spellings: &HashMap<String, DirectorySpelling>,
) -> ResidentSpelling {
    if let Some(path) = own_inventory_path.filter(|path| folds_equal(path, desired_path)) {
        return ResidentSpelling::Adopt(path.to_owned());
    }
    let Some((desired_directory, file_name)) = desired_path.rsplit_once('/') else {
        return ResidentSpelling::Keep;
    };
    let Some(spelling) = directory_spellings.get(&desired_directory.to_lowercase()) else {
        return ResidentSpelling::Keep;
    };
    match spelling {
        DirectorySpelling::Resident(directory) if directory != desired_directory => {
            ResidentSpelling::Adopt(format!("{directory}/{file_name}"))
        }
        DirectorySpelling::Resident(_) => ResidentSpelling::Keep,
        DirectorySpelling::Ambiguous { .. } => ResidentSpelling::Ambiguous,
    }
}

/// Applies resident spellings before any mirror delta or sidecar is derived.
pub(super) fn rewrite_desired_paths(
    desired: &mut HashMap<i64, DesiredManagedFile>,
    inventory: &[DeviceFileRecord],
    inventory_by_id: &HashMap<i64, DeviceFileRecord>,
    managed_files: &[ManagedDeviceFile],
) -> Vec<String> {
    let spellings = build_directory_spellings(inventory, managed_files);
    let mut unplanned_resident_paths = Vec::new();
    let mut track_ids = desired.keys().copied().collect::<Vec<_>>();
    track_ids.sort_unstable();
    for track_id in track_ids {
        let desired_path = desired[&track_id].device_path.clone();
        let own_path = inventory_by_id
            .get(&track_id)
            .map(|file| file.device_path.as_str());
        match adopt_resident_spelling(&desired_path, own_path, &spellings) {
            ResidentSpelling::Adopt(path) => desired.get_mut(&track_id).unwrap().device_path = path,
            ResidentSpelling::Keep => {}
            ResidentSpelling::Ambiguous => {
                if let Some(resident_path) = own_path {
                    tracing::warn!(
                        track_id,
                        desired_path,
                        resident_path,
                        "ambiguous case-only device directory; keeping the track's inventory path"
                    );
                    desired.get_mut(&track_id).unwrap().device_path = resident_path.to_owned();
                } else {
                    let (first_spelling, second_spelling) =
                        ambiguous_spellings(&desired_path, &spellings)
                            .unwrap_or(("unknown", "unknown"));
                    tracing::warn!(
                        track_id,
                        first_spelling,
                        second_spelling,
                        "ambiguous case-only device directory; leaving track unplanned"
                    );
                    unplanned_resident_paths.push(desired_path);
                    desired.remove(&track_id);
                }
            }
        }
    }
    unplanned_resident_paths
}

fn build_directory_spellings(
    inventory: &[DeviceFileRecord],
    managed_files: &[ManagedDeviceFile],
) -> HashMap<String, DirectorySpelling> {
    let paths = inventory
        .iter()
        .map(|file| file.device_path.as_str())
        .chain(managed_files.iter().map(|file| file.relative_path.as_str()))
        .collect::<BTreeSet<_>>();
    let mut counts = BTreeMap::<String, BTreeMap<String, usize>>::new();
    for path in paths {
        let Some((directory, _)) = path.rsplit_once('/') else {
            continue;
        };
        *counts
            .entry(directory.to_lowercase())
            .or_default()
            .entry(directory.to_owned())
            .or_default() += 1;
    }
    counts
        .into_iter()
        .filter_map(|(key, spellings)| {
            let highest = spellings.values().copied().max()?;
            let winners = spellings
                .into_iter()
                .filter_map(|(spelling, count)| (count == highest).then_some(spelling))
                .collect::<Vec<_>>();
            let value = match winners.as_slice() {
                [winner] => DirectorySpelling::Resident(winner.clone()),
                [first, second, ..] => DirectorySpelling::Ambiguous {
                    first: first.clone(),
                    second: second.clone(),
                },
                [] => return None,
            };
            Some((key, value))
        })
        .collect()
}

fn ambiguous_spellings<'a>(
    desired_path: &str,
    spellings: &'a HashMap<String, DirectorySpelling>,
) -> Option<(&'a str, &'a str)> {
    let (directory, _) = desired_path.rsplit_once('/')?;
    match spellings.get(&directory.to_lowercase())? {
        DirectorySpelling::Ambiguous { first, second } => Some((first, second)),
        DirectorySpelling::Resident(_) => None,
    }
}

fn folds_equal(left: &str, right: &str) -> bool {
    left.to_lowercase() == right.to_lowercase()
}
