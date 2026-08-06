//! Report assembly for tag-edit writes.

use rusqlite::Connection;

use super::tag_edit::{TagBatchReport, TagWriteFailure};
use super::tag_edit_write::TrackWrite;
use super::tag_mutation::WriteErrorKind;

pub(super) fn push_failure(
    report: &mut TagBatchReport,
    id: i64,
    path: &std::path::Path,
    kind: WriteErrorKind,
    error: String,
) {
    report.failures.push(TagWriteFailure {
        id,
        path: path.to_path_buf(),
        kind,
        error,
    });
}

pub(super) fn apply_rating_only(
    conn: &Connection,
    write: &TrackWrite,
    report: &mut TagBatchReport,
) {
    let Some(rating) = write.patch.rating else {
        return;
    };
    match crate::library::stats::set_rating_for_registered_track(
        conn,
        write.id,
        &write.path,
        rating,
    ) {
        Ok(true) => report.updated_ids.push(write.id),
        Ok(false) => push_failure(
            report,
            write.id,
            &write.path,
            WriteErrorKind::Io,
            "track path changed before rating; refusing stale request".into(),
        ),
        Err(error) => push_failure(
            report,
            write.id,
            &write.path,
            WriteErrorKind::Io,
            format!("could not save rating: {error}"),
        ),
    }
}
