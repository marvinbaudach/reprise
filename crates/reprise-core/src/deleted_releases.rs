//! Durable memory for releases the listener deliberately removed.

use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension};

#[cfg(test)]
thread_local! {
    static FULL_RECONCILIATION_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_full_reconciliation_call_count() {
    FULL_RECONCILIATION_CALLS.with(|calls| calls.set(0));
}

#[cfg(test)]
pub(crate) fn full_reconciliation_call_count() -> usize {
    FULL_RECONCILIATION_CALLS.with(std::cell::Cell::get)
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TrackIdentity {
    artist_key: String,
    album_key: String,
    title_key: String,
}

fn track_identity(artist: String, album_artist: String, album: &str, title: &str) -> TrackIdentity {
    let release_artist = if album_artist.trim().is_empty() {
        artist
    } else {
        album_artist
    };
    TrackIdentity {
        artist_key: crate::artist_news::normalize(&release_artist),
        album_key: crate::artist_news::normalize(album),
        title_key: crate::artist_news::normalize(title),
    }
}

pub(crate) fn remember_deleted_releases(
    conn: &Connection,
    ids: &[i64],
    now: i64,
) -> Result<MemoryReconciliation, rusqlite::Error> {
    if ids.is_empty() {
        return Ok(MemoryReconciliation::default());
    }
    let removed_ids = ids.iter().copied().collect::<HashSet<_>>();
    let mut identity_statement =
        conn.prepare_cached("SELECT artist, album_artist, album, title FROM tracks WHERE id = ?1")?;
    let mut selected = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(identity) = identity_statement
            .query_row([id], |row| {
                let album = row.get::<_, String>(2)?;
                let title = row.get::<_, String>(3)?;
                Ok(track_identity(row.get(0)?, row.get(1)?, &album, &title))
            })
            .optional()?
        {
            selected.push(identity);
        }
    }
    drop(identity_statement);
    // This is deliberately wider than `local_library_index`: a missing row
    // still owns its metadata here, so an unmounted drive cannot fabricate a
    // deletion memory. Only a row already committed to removal is excluded.
    let mut statement = conn.prepare(
        "SELECT id, artist, album_artist, album, title
         FROM tracks WHERE removed_at IS NULL",
    )?;
    let survivors = statement
        .query_map([], |row| {
            let album = row.get::<_, String>(3)?;
            let title = row.get::<_, String>(4)?;
            Ok((
                row.get::<_, i64>(0)?,
                track_identity(row.get(1)?, row.get(2)?, &album, &title),
            ))
        })?
        .filter_map(|row| match row {
            Ok((id, identity)) if !removed_ids.contains(&id) => Some(Ok(identity)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>, _>>()?;

    let album_keys = selected
        .iter()
        .filter(|identity| !identity.artist_key.is_empty() && !identity.album_key.is_empty())
        .map(|identity| (identity.artist_key.clone(), identity.album_key.clone()))
        .collect::<HashSet<_>>();
    for (artist_key, title_key) in album_keys {
        let survives = survivors
            .iter()
            .any(|identity| identity.artist_key == artist_key && identity.album_key == title_key);
        if !survives {
            conn.execute(
                "INSERT INTO deleted_releases (artist_key, title_key, scope, deleted_at)
                 VALUES (?1, ?2, 'album', ?3)
                 ON CONFLICT DO NOTHING",
                rusqlite::params![artist_key, title_key, now],
            )?;
        }
    }
    let track_keys = selected
        .into_iter()
        .filter(|identity| !identity.artist_key.is_empty() && !identity.title_key.is_empty())
        .map(|identity| (identity.artist_key, identity.title_key))
        .collect::<HashSet<_>>();
    for (artist_key, title_key) in track_keys {
        let survives = survivors
            .iter()
            .any(|identity| identity.artist_key == artist_key && identity.title_key == title_key);
        if !survives {
            conn.execute(
                "INSERT INTO deleted_releases (artist_key, title_key, scope, deleted_at)
                 VALUES (?1, ?2, 'track', ?3)
                 ON CONFLICT DO NOTHING",
                rusqlite::params![artist_key, title_key, now],
            )?;
        }
    }
    forget_acquired_memories(conn, |memory| {
        survivors.iter().any(|identity| {
            identity.artist_key == memory.artist_key
                && if memory.scope == "album" {
                    identity.album_key == memory.title_key
                } else {
                    identity.title_key == memory.title_key
                }
        })
    })
}

pub(crate) fn apply_deleted_release_memory(conn: &Connection) -> Result<usize, rusqlite::Error> {
    #[cfg(test)]
    FULL_RECONCILIATION_CALLS.with(|calls| calls.set(calls.get() + 1));
    let memories = load_memories(conn)?;
    if memories.is_empty() {
        return Ok(0);
    }
    let library = crate::artist_news_query::local_library_index(conn)?;
    let (acquired, remaining): (Vec<_>, Vec<_>) = memories.into_iter().partition(|memory| {
        if memory.scope == "album" {
            library
                .album_track_counts
                .contains_key(&(memory.artist_key.clone(), memory.title_key.clone()))
        } else {
            library
                .track_titles
                .contains(&(memory.artist_key.clone(), memory.title_key.clone()))
        }
    });
    delete_memories(conn, &acquired)?;
    reconcile_release_rows(conn, &acquired, &remaining, true)
}

pub(crate) fn hide_deleted_release_memory(
    conn: &Connection,
    reconciliation: &MemoryReconciliation,
) -> Result<usize, rusqlite::Error> {
    if reconciliation.forgotten.is_empty() && reconciliation.remaining.is_empty() {
        return Ok(0);
    }
    reconcile_release_rows(
        conn,
        &reconciliation.forgotten,
        &reconciliation.remaining,
        true,
    )
}

pub(crate) fn hide_deleted_release_rows(
    conn: &Connection,
    release_group_mbids: &[String],
) -> Result<usize, rusqlite::Error> {
    let memories = load_memories(conn)?;
    if memories.is_empty() || release_group_mbids.is_empty() {
        return Ok(0);
    }
    let mut statement = conn.prepare_cached(
        "SELECT artist_name, title, release_type, hidden
         FROM new_releases WHERE release_group_mbid = ?1",
    )?;
    let mut hidden_count = 0;
    for mbid in release_group_mbids {
        let release = statement
            .query_row([mbid], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, bool>(3)?,
                ))
            })
            .optional()?;
        let Some((artist, title, release_type, hidden)) = release else {
            continue;
        };
        let key = (
            crate::artist_news::normalize(&artist),
            crate::artist_news::normalize(&title),
        );
        if !hidden && is_release_covered(&memories, &key, &release_type) {
            hidden_count += usize::from(
                crate::artist_news_query::hide_release_by_deleted_memory_in(conn, mbid)?,
            );
        }
    }
    Ok(hidden_count)
}

pub(crate) fn reconcile_restored_tracks(
    conn: &Connection,
    restored_ids: &[i64],
) -> Result<(), rusqlite::Error> {
    if restored_ids.is_empty() {
        return Ok(());
    }
    let mut statement = conn.prepare_cached(
        "SELECT artist, album_artist, album, title
         FROM tracks WHERE id = ?1 AND removed_at IS NULL",
    )?;
    let identities = restored_ids
        .iter()
        .filter_map(|id| {
            match statement
                .query_row([id], |row| {
                    let album = row.get::<_, String>(2)?;
                    let title = row.get::<_, String>(3)?;
                    Ok(track_identity(row.get(0)?, row.get(1)?, &album, &title))
                })
                .optional()
            {
                Ok(Some(identity)) => Some(Ok(identity)),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    let reconciliation = forget_acquired_memories(conn, |memory| {
        identities.iter().any(|identity| {
            identity.artist_key == memory.artist_key
                && if memory.scope == "album" {
                    identity.album_key == memory.title_key
                } else {
                    identity.title_key == memory.title_key
                }
        })
    })?;
    reconcile_forgotten_release_rows(conn, &reconciliation.forgotten, &reconciliation.remaining)?;
    Ok(())
}

#[derive(Default)]
pub(crate) struct MemoryReconciliation {
    forgotten: Vec<DeletedReleaseMemory>,
    remaining: Vec<DeletedReleaseMemory>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DeletedReleaseMemory {
    artist_key: String,
    title_key: String,
    scope: String,
}

impl DeletedReleaseMemory {
    fn matches(&self, key: &(String, String), release_type: &str) -> bool {
        self.artist_key == key.0
            && self.title_key == key.1
            && (self.scope == "album"
                || (self.scope == "track" && release_type.eq_ignore_ascii_case("single")))
    }
}

fn is_release_covered(
    memories: &[DeletedReleaseMemory],
    key: &(String, String),
    release_type: &str,
) -> bool {
    memories
        .iter()
        .any(|memory| memory.matches(key, release_type))
}

fn forget_acquired_memories(
    conn: &Connection,
    is_acquired: impl Fn(&DeletedReleaseMemory) -> bool,
) -> Result<MemoryReconciliation, rusqlite::Error> {
    let (forgotten, remaining): (Vec<_>, Vec<_>) =
        load_memories(conn)?.into_iter().partition(is_acquired);
    delete_memories(conn, &forgotten)?;
    Ok(MemoryReconciliation {
        forgotten,
        remaining,
    })
}

fn delete_memories(
    conn: &Connection,
    memories: &[DeletedReleaseMemory],
) -> Result<(), rusqlite::Error> {
    for memory in memories {
        conn.execute(
            "DELETE FROM deleted_releases
             WHERE artist_key = ?1 AND title_key = ?2 AND scope = ?3",
            rusqlite::params![memory.artist_key, memory.title_key, memory.scope],
        )?;
    }
    Ok(())
}

fn reconcile_release_rows(
    conn: &Connection,
    forgotten: &[DeletedReleaseMemory],
    remaining: &[DeletedReleaseMemory],
    hide_remaining: bool,
) -> Result<usize, rusqlite::Error> {
    let mut hidden_count = 0;
    for (mbid, artist, title, release_type, hidden, memory_owned) in load_releases(conn)? {
        let key = (
            crate::artist_news::normalize(&artist),
            crate::artist_news::normalize(&title),
        );
        let still_covered = is_release_covered(remaining, &key, &release_type);
        if hidden
            && memory_owned
            && is_release_covered(forgotten, &key, &release_type)
            && !still_covered
        {
            crate::artist_news_query::unhide_release_by_deleted_memory_in(conn, &mbid)?;
        } else if hide_remaining && !hidden && still_covered {
            hidden_count += usize::from(
                crate::artist_news_query::hide_release_by_deleted_memory_in(conn, &mbid)?,
            );
        }
    }
    Ok(hidden_count)
}

fn reconcile_forgotten_release_rows(
    conn: &Connection,
    forgotten: &[DeletedReleaseMemory],
    remaining: &[DeletedReleaseMemory],
) -> Result<(), rusqlite::Error> {
    for (mbid, artist, title, release_type, _, _) in load_memory_owned_releases(conn)? {
        let key = (
            crate::artist_news::normalize(&artist),
            crate::artist_news::normalize(&title),
        );
        if is_release_covered(forgotten, &key, &release_type)
            && !is_release_covered(remaining, &key, &release_type)
        {
            crate::artist_news_query::unhide_release_by_deleted_memory_in(conn, &mbid)?;
        }
    }
    Ok(())
}

fn load_memories(conn: &Connection) -> Result<Vec<DeletedReleaseMemory>, rusqlite::Error> {
    conn.prepare("SELECT artist_key, title_key, scope FROM deleted_releases")?
        .query_map([], |row| {
            Ok(DeletedReleaseMemory {
                artist_key: row.get(0)?,
                title_key: row.get(1)?,
                scope: row.get(2)?,
            })
        })?
        .collect()
}

type ReleaseRow = (String, String, String, String, bool, bool);

fn load_releases(conn: &Connection) -> Result<Vec<ReleaseRow>, rusqlite::Error> {
    conn.prepare(
        "SELECT release_group_mbid, artist_name, title, release_type, hidden,
                hidden_by_deleted_memory
         FROM new_releases",
    )?
    .query_map([], |row| {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
        ))
    })?
    .collect()
}

fn load_memory_owned_releases(conn: &Connection) -> Result<Vec<ReleaseRow>, rusqlite::Error> {
    conn.prepare(
        "SELECT release_group_mbid, artist_name, title, release_type, hidden,
                hidden_by_deleted_memory
         FROM new_releases
         WHERE hidden_by_deleted_memory = 1",
    )?
    .query_map([], |row| {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
        ))
    })?
    .collect()
}

pub(crate) fn forget_deleted_release_memory(
    conn: &Connection,
    release_group_mbid: &str,
) -> Result<Vec<String>, rusqlite::Error> {
    let release = conn
        .query_row(
            "SELECT artist_name, title, release_type
             FROM new_releases WHERE release_group_mbid = ?1",
            [release_group_mbid],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((artist, title, release_type)) = release else {
        return Ok(Vec::new());
    };
    let key = (
        crate::artist_news::normalize(&artist),
        crate::artist_news::normalize(&title),
    );
    let forgotten = load_memories(conn)?
        .into_iter()
        .filter(|memory| memory.matches(&key, &release_type))
        .collect::<Vec<_>>();
    if forgotten.is_empty() {
        return Ok(vec![release_group_mbid.to_owned()]);
    }
    for memory in &forgotten {
        conn.execute(
            "DELETE FROM deleted_releases
             WHERE artist_key = ?1 AND title_key = ?2 AND scope = ?3",
            rusqlite::params![memory.artist_key, memory.title_key, memory.scope],
        )?;
    }
    let remaining = load_memories(conn)?;
    Ok(load_releases(conn)?
        .into_iter()
        .filter_map(|(mbid, artist, title, release_type, _, memory_owned)| {
            let key = (
                crate::artist_news::normalize(&artist),
                crate::artist_news::normalize(&title),
            );
            (mbid == release_group_mbid
                || (memory_owned
                    && is_release_covered(&forgotten, &key, &release_type)
                    && !is_release_covered(&remaining, &key, &release_type)))
            .then_some(mbid)
        })
        .collect())
}
