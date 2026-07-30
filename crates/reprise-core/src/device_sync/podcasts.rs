//! Pure Android planning for explicitly selected podcast and YouTube
//! downloads (`POD-12`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::cap::{items_to_evict, CapItem};
use super::safe_component;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PodcastSyncSource {
    Rss,
    Youtube,
}

/// Which [`PodcastSyncSource`] kinds a device sync may draw from right now.
///
/// A module switched off in Preferences loses its sidebar entry (`SET-9`), so
/// while it is off it is not part of the app the user can see — and a source
/// the user cannot see must not keep pushing files onto their phone. `SET-9`
/// promises that switching a block off *keeps* its subscriptions; it never
/// promised they keep syncing, and this type settles that reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnabledSyncSources {
    pub rss: bool,
    pub youtube: bool,
}

impl EnabledSyncSources {
    #[must_use]
    pub fn allows(self, source: PodcastSyncSource) -> bool {
        match source {
            PodcastSyncSource::Rss => self.rss,
            PodcastSyncSource::Youtube => self.youtube,
        }
    }
}

/// Reads the two module switches and the global gate that sits above them.
///
/// Deliberately *not* [`crate::online_sources::network_allowed`], even though
/// the formula is identical: copying an already-downloaded file onto a phone
/// makes no request, so the two answer different questions and must stay free
/// to diverge. What they genuinely share is the reason — both switches take
/// the source out of the sidebar, and neither deletes anything.
///
/// A read failure propagates rather than defaulting to off, unlike
/// [`crate::online_sources::network_allowed_or_off`]. That is deliberate and
/// it is the safe direction here: the error aborts planning, so no plan is
/// produced at all — and a plan is the only thing that can copy or delete.
/// Defaulting to off would instead produce a *successful* empty plan, which
/// is a stronger claim than an unreadable database supports.
pub fn enabled_sync_sources(db: &crate::db::Db) -> Result<EnabledSyncSources, rusqlite::Error> {
    let conn = db.conn();
    let global =
        crate::library::settings::get_bool_in(conn, crate::online_sources::ENABLED_KEY, true)?;
    Ok(EnabledSyncSources {
        rss: global && crate::modules::is_enabled_in(conn, &crate::modules::PODCASTS_MODULE)?,
        youtube: global && crate::modules::is_enabled_in(conn, &crate::modules::YOUTUBE_MODULE)?,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PodcastSyncCandidate {
    pub episode_id: i64,
    pub source: PodcastSyncSource,
    pub source_path: PathBuf,
    pub device_path: String,
    pub title: String,
    pub show: String,
    pub size_bytes: u64,
    pub source_mtime: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PodcastDeviceFile {
    pub device_path: String,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PodcastSyncPlan {
    pub selected: usize,
    pub to_copy: Vec<PodcastSyncCandidate>,
    pub to_remove: Vec<String>,
    pub bytes: u64,
    /// Total size freed by everything in [`Self::to_remove`], kept
    /// separate from [`Self::bytes`] (which only ever counts bytes moving
    /// onto the device) so a deletions-only plan can report a truthful
    /// "0 B to copy · frees N MiB" instead of one blended figure — see
    /// `device_sync::category_diff` (`MTP-22`).
    pub bytes_freed: u64,
}

/// Queries every downloaded, selected episode for `device_id` across both
/// [`PodcastSyncSource`] kinds. `build_plan` (called once per source) does
/// the per-kind filtering, so this deliberately does not restrict `s.kind`
/// — RSS and YouTube subscriptions are equally eligible once selected for a
/// device (`POD-12`).
///
/// What it *does* restrict is a source whose module is switched off
/// (`MTP-46`): the gate lives here, at the one place the rows are read, and
/// not at the callers, so no future caller can reach the rows around it.
pub fn query_candidates_for_device(
    db: &crate::db::Db,
    device_id: &str,
) -> Result<Vec<PodcastSyncCandidate>, rusqlite::Error> {
    let conn = db.conn();
    let enabled = enabled_sync_sources(db)?;
    let mut statement = conn.prepare(
        "SELECT e.id, e.title, s.title, e.downloaded_path, e.downloaded_bytes, s.kind
         FROM podcast_episodes e
         JOIN podcast_subscriptions s ON s.id = e.subscription_id
         JOIN podcast_subscription_devices d
           ON d.subscription_id = s.id AND d.device_id = ?1
         WHERE s.removed_at IS NULL
           AND e.removed_at IS NULL
           AND e.downloaded_path IS NOT NULL
         ORDER BY s.title COLLATE NOCASE, e.published_at DESC, e.id DESC",
    )?;
    let rows = statement.query_map([device_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            PathBuf::from(row.get::<_, String>(3)?),
            row.get::<_, Option<i64>>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;
    let rows = rows.collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    let mut candidates = Vec::new();
    for row in rows {
        let (episode_id, title, show, source_path, recorded_bytes, kind) = row;
        let Some(source) = source_from_kind(&kind) else {
            continue;
        };
        if !enabled.allows(source) {
            continue;
        }
        let Ok(metadata) = std::fs::metadata(&source_path) else {
            continue;
        };
        if !metadata.is_file() || metadata.len() == 0 {
            continue;
        }
        let recorded_bytes = match recorded_bytes {
            Some(bytes) => {
                let Ok(bytes) = u64::try_from(bytes) else {
                    continue;
                };
                if metadata.len() != bytes {
                    continue;
                }
                bytes
            }
            None => {
                let bytes = metadata.len();
                crate::podcasts::store::set_downloaded_file(
                    db,
                    episode_id,
                    source_path.to_str(),
                    Some(bytes.min(i64::MAX as u64) as i64),
                )?;
                bytes
            }
        };
        candidates.push(PodcastSyncCandidate {
            episode_id,
            source,
            device_path: device_path(episode_id, &show, &title, &source_path),
            title,
            show,
            size_bytes: recorded_bytes,
            source_mtime: metadata
                .modified()
                .ok()
                .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                .and_then(|duration| i64::try_from(duration.as_secs()).ok())
                .unwrap_or(0),
            source_path,
        });
    }
    Ok(candidates)
}

// `MTP-45`: `query_selection_candidates_for_device` lives in its own sibling
// module (`podcasts_selection.rs`) purely to keep this file under the
// project's 800-line file-size rule — it is still conceptually part of this
// module's public query surface, so it is re-exported below rather than
// requiring callers to reach into `podcasts::podcasts_selection`.
#[path = "podcasts_selection.rs"]
mod podcasts_selection;
pub use podcasts_selection::query_selection_candidates_for_device;

fn source_from_kind(kind: &str) -> Option<PodcastSyncSource> {
    match kind {
        "rss" => Some(PodcastSyncSource::Rss),
        "youtube" => Some(PodcastSyncSource::Youtube),
        _ => None,
    }
}

/// Builds a plan for one [`PodcastSyncSource`] at a time — RSS episodes and
/// YouTube audio are planned identically, each against its own target
/// folder (`MTP-38`). `cap_bytes` is the target's optional size cap
/// (`MTP-39`/`MTP-25`): when the full desired set would exceed it, the
/// oldest candidates (by [`PodcastSyncCandidate::source_mtime`]) are
/// dropped from the desired set entirely before the copy/remove diff runs,
/// so an evicted-but-already-resident file is picked up by the ordinary
/// "not in desired" removal below rather than needing a second pass.
pub fn build_plan(
    candidates: Vec<PodcastSyncCandidate>,
    inventory: &[PodcastDeviceFile],
    remove_deleted: bool,
    source: PodcastSyncSource,
    cap_bytes: Option<u64>,
    enabled: EnabledSyncSources,
) -> PodcastSyncPlan {
    // `MTP-46`: a switched-off source is not a source with nothing selected.
    // The difference is destructive — with `remove_deleted` on, an empty
    // desired set makes *every* resident file of this source a removal, so
    // gating only the candidate query would have turned switching YouTube off
    // into "wipe YouTube off the phone on the next sync". `SET-9` promises the
    // opposite: nothing is deleted, and switching it back on restores the
    // previous sync. This lives here, in the function that produces
    // `to_remove`, rather than at the one caller that could forget it.
    if !enabled.allows(source) {
        return PodcastSyncPlan::default();
    }
    let candidates = candidates
        .into_iter()
        .filter(|candidate| {
            candidate.source == source && safe_relative_path(&candidate.device_path)
        })
        .collect::<Vec<_>>();
    let evicted = cap_bytes
        .map(|cap| evicted_paths(&candidates, cap))
        .unwrap_or_default();
    let candidates = candidates
        .into_iter()
        .filter(|candidate| !evicted.contains(&candidate.device_path))
        .collect::<Vec<_>>();
    let desired = candidates
        .iter()
        .map(|candidate| candidate.device_path.clone())
        .collect::<std::collections::HashSet<_>>();
    let existing = inventory
        .iter()
        .filter(|file| safe_relative_path(&file.device_path))
        .map(|file| (file.device_path.as_str(), file.size_bytes))
        .collect::<std::collections::HashMap<_, _>>();
    let to_copy = candidates
        .into_iter()
        .filter(|candidate| {
            existing.get(candidate.device_path.as_str()).copied() != Some(candidate.size_bytes)
        })
        .collect::<Vec<_>>();
    let bytes = to_copy.iter().map(|candidate| candidate.size_bytes).sum();
    let (to_remove, bytes_freed) = if remove_deleted {
        inventory
            .iter()
            .filter(|file| {
                safe_relative_path(&file.device_path)
                    && !desired.contains(file.device_path.as_str())
            })
            .fold((Vec::new(), 0_u64), |(mut paths, freed), file| {
                paths.push(file.device_path.clone());
                (paths, freed.saturating_add(file.size_bytes))
            })
    } else {
        (Vec::new(), 0)
    };
    PodcastSyncPlan {
        selected: desired.len(),
        to_copy,
        to_remove,
        bytes,
        bytes_freed,
    }
}

/// `MTP-39`/`MTP-25`: which desired device paths must leave to bring the
/// full candidate set back under `cap_bytes`, oldest (`source_mtime`)
/// first. Reuses [`items_to_evict`] rather than re-deriving the eviction
/// order — this is only the adapter from `PodcastSyncCandidate` to
/// `CapItem`.
fn evicted_paths(candidates: &[PodcastSyncCandidate], cap_bytes: u64) -> HashSet<String> {
    // `CapItem::Id` must be `Copy`, so candidates are identified by index
    // rather than by their (non-`Copy`) `device_path` String.
    let items = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| CapItem {
            id: index,
            size_bytes: candidate.size_bytes,
            age: candidate.source_mtime,
        })
        .collect::<Vec<_>>();
    items_to_evict(&items, cap_bytes)
        .into_iter()
        .map(|index| candidates[index].device_path.clone())
        .collect()
}

pub fn safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.chars().any(char::is_control)
        && std::path::Path::new(path)
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

fn device_path(episode_id: i64, show: &str, title: &str, source: &Path) -> String {
    let show = safe_component(show, "Unknown Podcast");
    let title = safe_component(title, "Untitled Episode");
    let extension = source
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| {
            !extension.is_empty()
                && extension.len() <= 8
                && extension
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        })
        .map(|extension| format!(".{}", extension.to_ascii_lowercase()))
        .unwrap_or_default();
    format!("{show}/{episode_id}-{title}{extension}")
}

#[cfg(test)]
#[path = "podcasts_tests.rs"]
mod tests;
