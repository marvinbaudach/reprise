//! Filesystem deletion and queue reconciliation exported to Android.

use std::path::Path;

use reprise_core::library::trash_tracks::trash_tracks_with;
use reprise_core::playback::PlaybackBackend;
use reprise_core::queries;

use super::{AndroidPlaybackError, AndroidPlaybackSession};
use crate::LibraryError;

#[uniffi::export(callback_interface)]
pub trait TrashAction: Send + Sync {
    /// Deletes the file at `uri`. Returns the error message on failure.
    fn trash(&self, uri: String) -> Option<String>;
}

/// One requested id that did not end up deleted, and why.
///
/// `uri` is empty for the internal `ALREADY_GONE` failure: the requested
/// library row had disappeared before any deletion could be attempted.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AndroidTrashFailure {
    pub track_id: i64,
    pub uri: String,
    pub error: String,
}

/// What a requested id whose library row has since disappeared reports.
///
/// It is a failure because the request was not carried out, not because
/// anything went wrong: dropping it silently would let a batch of twelve answer
/// "ten deleted" with nothing accounting for the other two. The sentence is
/// written for a person, since this string is what the report carries upwards.
const ALREADY_GONE: &str = "this track was already gone from the library";

#[derive(Clone, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct AndroidTrashReport {
    pub removed_ids: Vec<i64>,
    pub failures: Vec<AndroidTrashFailure>,
}

#[uniffi::export]
impl AndroidPlaybackSession {
    // UniFFI transfers callback objects by value across the ABI.
    #[allow(clippy::needless_pass_by_value)]
    pub fn trash_tracks(
        &self,
        track_ids: Vec<i64>,
        action: Box<dyn TrashAction>,
    ) -> Result<AndroidTrashReport, LibraryError> {
        let (report, already_gone) = {
            let database = self.inner.library.writer()?;
            let mut tracks = Vec::with_capacity(track_ids.len());
            let mut already_gone = Vec::new();
            for track_id in track_ids {
                let path = queries::track_source_path(&database, track_id).map_err(|error| {
                    LibraryError::Query {
                        detail: format!("could not resolve a track for deletion: {error}"),
                    }
                })?;
                match path {
                    Some(path) => tracks.push((track_id, path)),
                    None => already_gone.push(AndroidTrashFailure {
                        track_id,
                        uri: String::new(),
                        error: ALREADY_GONE.to_owned(),
                    }),
                }
            }
            (
                trash_tracks_with(&database, &tracks, |path| trash_path(action.as_ref(), path)),
                already_gone,
            )
        };

        let (removed_current, has_current, next_uri, queue_to_save) = {
            let mut state = self
                .inner
                .lock()
                .map_err(|error| playback_as_library_error(&error))?;
            let removed_current = state
                .queue
                .current()
                .is_some_and(|track_id| report.removed_ids.contains(&track_id));
            state.queue.remove_ids(&report.removed_ids);
            if removed_current && state.queue.current().is_some() {
                state.adopt_current();
            }
            (
                removed_current,
                state.queue.current().is_some(),
                state.next_uri(),
                state.queue.clone(),
            )
        };
        self.inner
            .persist_queue(&queue_to_save)
            .map_err(|error| playback_as_library_error(&error))?;

        if removed_current {
            if has_current {
                self.inner
                    .start_current()
                    .map_err(|error| playback_as_library_error(&error))?;
            } else {
                self.inner
                    .stop_backend()
                    .map_err(|error| playback_as_library_error(&error))?;
            }
        } else {
            self.inner
                .backend()
                .map_err(|error| playback_as_library_error(&error))?
                .set_next(next_uri.as_deref());
            self.inner.notify();
        }

        Ok(AndroidTrashReport {
            removed_ids: report.removed_ids,
            failures: already_gone
                .into_iter()
                .chain(
                    report
                        .failures
                        .into_iter()
                        .map(|failure| AndroidTrashFailure {
                            track_id: failure.id,
                            uri: failure.path.to_string_lossy().into_owned(),
                            error: failure.error,
                        }),
                )
                .collect(),
        })
    }
}

fn trash_path(action: &dyn TrashAction, path: &Path) -> Result<(), String> {
    match action.trash(path.to_string_lossy().into_owned()) {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn playback_as_library_error(error: &AndroidPlaybackError) -> LibraryError {
    LibraryError::Database {
        detail: error.to_string(),
    }
}
