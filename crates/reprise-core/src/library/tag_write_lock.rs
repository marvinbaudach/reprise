use rusqlite::Connection;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("another tag-writing job is already running")]
pub struct TagWriteBusy;

/// Fails when any tag-write job of any kind is prepared or running.
/// Call this inside the same transaction that inserts the new job row.
///
/// A query that cannot be answered is reported as the database error it is,
/// never as `TagWriteBusy`: "wait, someone else is writing" invites a retry,
/// and a caller that retries against a broken database never stops.
pub(crate) fn claim_tag_write_slot<E>(conn: &Connection) -> Result<(), E>
where
    E: From<rusqlite::Error> + From<TagWriteBusy>,
{
    let occupied = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM tag_write_jobs \
         WHERE state IN ('prepared', 'running') LIMIT 1)",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if occupied {
        Err(TagWriteBusy.into())
    } else {
        Ok(())
    }
}
