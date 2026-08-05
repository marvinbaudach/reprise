use rusqlite::Connection;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("another tag-writing job is already running")]
pub struct TagWriteBusy;

/// Fails when any tag-write job of any kind is prepared or running.
/// Call this inside the same transaction that inserts the new job row.
pub(crate) fn claim_tag_write_slot(conn: &Connection) -> Result<(), TagWriteBusy> {
    let occupied = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM tag_write_jobs \
             WHERE state IN ('prepared', 'running') LIMIT 1)",
            [],
            |row| row.get::<_, bool>(0),
        )
        .unwrap_or(true);
    if occupied {
        Err(TagWriteBusy)
    } else {
        Ok(())
    }
}
