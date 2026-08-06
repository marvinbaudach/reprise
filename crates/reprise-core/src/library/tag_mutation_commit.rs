//! Filesystem and reconciliation phases for prepared tag mutations.

use rusqlite::Connection;

use super::tag_edit::TagEditError;
use super::tag_mutation::{
    affected_fields_still_match, apply_tag_patch_to_tagged, classify_write_error,
    editable_tags_from_tagged, reconcile_after_write, strip_and_rewrite_tag,
    validate_registered_track, PreparedTagMutation, TagMutationFailure, WriteErrorKind,
    IGNORE_DURATION,
};

pub(crate) fn commit_tag_mutation(
    conn: &Connection,
    prepared: &PreparedTagMutation,
    ignore_watcher: bool,
) -> Result<(), TagMutationFailure> {
    validate_registered_track(conn, prepared.id, &prepared.path).map_err(|error| {
        TagMutationFailure {
            kind: WriteErrorKind::Io,
            error,
            file_written: false,
        }
    })?;
    write_prepared_tag_mutation(prepared, ignore_watcher)?;
    reconcile_prepared_tag_mutation(conn, prepared)
}

/// Performs only the filesystem half of a prepared mutation.
///
/// This function deliberately accepts no database handle so batch callers can
/// run independent files concurrently after claiming their journal entries.
/// Reconciliation remains a separate, serial database phase.
pub(crate) fn write_prepared_tag_mutation(
    prepared: &PreparedTagMutation,
    ignore_watcher: bool,
) -> Result<(), TagMutationFailure> {
    if prepared.strip_and_rewrite {
        if ignore_watcher {
            super::watcher::ignore_path(&prepared.path, IGNORE_DURATION);
        }
        strip_and_rewrite_tag(&prepared.path, &prepared.patch).map_err(|error| {
            TagMutationFailure {
                kind: classify_write_error(&error),
                error: error.to_string(),
                file_written: true,
            }
        })?;
        return Ok(());
    }
    let mut tagged = lofty::read_from_path(&prepared.path).map_err(|error| {
        let error = TagEditError::from(error);
        TagMutationFailure {
            kind: classify_write_error(&error),
            error: error.to_string(),
            file_written: false,
        }
    })?;
    if !affected_fields_still_match(prepared, &editable_tags_from_tagged(&tagged)) {
        return Err(TagMutationFailure {
            kind: WriteErrorKind::Io,
            error: "tags changed after the mutation was prepared; refusing stale write".into(),
            file_written: false,
        });
    }
    if ignore_watcher {
        super::watcher::ignore_path(&prepared.path, IGNORE_DURATION);
    }
    apply_tag_patch_to_tagged(&mut tagged, &prepared.path, &prepared.patch).map_err(|error| {
        TagMutationFailure {
            kind: classify_write_error(&error),
            error: error.to_string(),
            file_written: false,
        }
    })
}

pub(crate) fn reconcile_prepared_tag_mutation(
    conn: &Connection,
    prepared: &PreparedTagMutation,
) -> Result<(), TagMutationFailure> {
    reconcile_after_write(conn, prepared.id, &prepared.path).map_err(|error| TagMutationFailure {
        kind: WriteErrorKind::Io,
        error,
        file_written: true,
    })
}
