//! Where in its file a CUE track plays, as the phone hands it to Media3.
//!
//! A CUE sheet cuts one audio file into tracks; each is a row of its own
//! with the stretch of the file it covers. The file's last track ends where
//! the file does, and the end the catalog records for it is only the
//! duration the file's metadata claims. So the phone gives that track no end
//! at all and lets Media3 play it to the end of the source (decision 3 of
//! `docs/plans/cue-sheets-surfaces.md`). The last track is the one with the
//! highest segment index of its path, which is the order
//! [`queries::track_ids_for_path`] returns.

use std::collections::HashMap;

use reprise_core::db::Db;
use reprise_core::models::Track;
use reprise_core::queries;

use crate::browse::{TrackRow, TrackWindow};
use crate::LibraryError;

/// The stretch of a file one track plays, in milliseconds from the file's
/// start. `end_ms` is `None` for the file's last track, which plays to the
/// end of the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AndroidPlaybackSegment {
    pub start_ms: i64,
    pub end_ms: Option<i64>,
}

/// Remembers, per path, which track id is the last of its file, so a window
/// of rows asks once per file.
#[derive(Default)]
struct LastOfFile {
    by_path: HashMap<String, Option<i64>>,
}

impl LastOfFile {
    fn segment_of(
        &mut self,
        reader: &Db,
        track: &Track,
    ) -> Result<Option<AndroidPlaybackSegment>, LibraryError> {
        let Some(segment) = &track.segment else {
            return Ok(None);
        };
        let last = match self.by_path.get(&track.path) {
            Some(last) => *last,
            None => {
                let last = queries::track_ids_for_path(reader, &track.path)
                    .map_err(|error| LibraryError::Query {
                        detail: error.to_string(),
                    })?
                    .last()
                    .copied();
                self.by_path.insert(track.path.clone(), last);
                last
            }
        };
        Ok(Some(AndroidPlaybackSegment {
            start_ms: segment.start_ms,
            end_ms: (last != Some(track.id)).then_some(segment.end_ms),
        }))
    }
}

/// The stretch of its file `track` plays; `None` for a whole-file track.
pub(crate) fn playback_segment(
    reader: &Db,
    track: &Track,
) -> Result<Option<AndroidPlaybackSegment>, LibraryError> {
    LastOfFile::default().segment_of(reader, track)
}

/// The rows for `tracks`, each CUE track carrying its stretch.
pub(crate) fn track_rows(reader: &Db, tracks: Vec<Track>) -> Result<Vec<TrackRow>, LibraryError> {
    let mut last_of_file = LastOfFile::default();
    tracks
        .into_iter()
        .map(|track| {
            let segment = last_of_file.segment_of(reader, &track)?;
            Ok(TrackRow::new(track, segment))
        })
        .collect()
}

/// The row for one track; see [`track_rows`].
pub(crate) fn track_row(reader: &Db, track: Track) -> Result<TrackRow, LibraryError> {
    let segment = playback_segment(reader, &track)?;
    Ok(TrackRow::new(track, segment))
}

/// A window of rows; see [`track_rows`].
pub(crate) fn track_window(
    reader: &Db,
    window: queries::TrackWindow,
) -> Result<TrackWindow, LibraryError> {
    Ok(TrackWindow {
        total: window.total,
        rows: track_rows(reader, window.rows)?,
        has_more: window.has_more,
    })
}

#[cfg(test)]
#[path = "track_segment_tests.rs"]
mod tests;
