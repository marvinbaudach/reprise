use std::collections::BTreeMap;
use std::path::PathBuf;

use reprise_core::podcasts::EpisodeRow;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum FileReveal {
    Hidden,
    Reveal(PathBuf),
    OpenFolder(PathBuf),
}

/// `CTX-13`: what the menu offers for a set of episodes, given each one's
/// downloaded file (or `None` when it has none / it is gone from disk).
pub(super) fn file_reveal(paths: &[Option<PathBuf>]) -> FileReveal {
    if paths.is_empty() {
        return FileReveal::Hidden;
    }
    if paths.iter().any(Option::is_none) {
        return FileReveal::Hidden;
    }
    match paths {
        [Some(path)] => FileReveal::Reveal(path.clone()),
        [Some(first), rest @ ..] => {
            let Some(parent) = first
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            else {
                return FileReveal::Hidden;
            };
            if rest
                .iter()
                .all(|path| path.as_ref().and_then(|path| path.parent()) == Some(parent))
            {
                return FileReveal::OpenFolder(parent.to_path_buf());
            }
            FileReveal::Hidden
        }
        _ => unreachable!("empty and None paths returned before matching"),
    }
}

/// The downloaded files of the episodes currently on screen, taken when the
/// list is rendered. A download that finishes afterwards is not in here —
/// the view re-renders on download state changes, and the action handler
/// resolves the paths again from the store before it acts, so a stale
/// snapshot can only hide the entry for a moment, never mislead it.
pub(super) struct EpisodePaths {
    by_episode_id: BTreeMap<i64, PathBuf>,
}

impl EpisodePaths {
    pub(super) fn from_rows(rows: &[EpisodeRow]) -> Self {
        let by_episode_id = rows
            .iter()
            .filter_map(|row| {
                let path = row
                    .downloaded_path
                    .as_deref()
                    .filter(|path| !path.is_empty())
                    .map(PathBuf::from)?;
                path.exists().then_some((row.id, path))
            })
            .collect();
        Self { by_episode_id }
    }

    pub(super) fn lookup(&self, ids: &[i64]) -> Vec<Option<PathBuf>> {
        ids.iter()
            .map(|id| self.by_episode_id.get(id).cloned())
            .collect()
    }
}

#[cfg(test)]
#[path = "podcasts_episode_files_tests.rs"]
mod tests;
