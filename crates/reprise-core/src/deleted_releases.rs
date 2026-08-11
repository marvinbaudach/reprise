//! Durable memory for releases the listener deliberately removed.

use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension};

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
) -> Result<(), rusqlite::Error> {
    if ids.is_empty() {
        return Ok(());
    }
    let removed_ids = ids.iter().copied().collect::<HashSet<_>>();
    let selected = ids
        .iter()
        .filter_map(|id| {
            conn.query_row(
                "SELECT artist, album_artist, album, title FROM tracks WHERE id = ?1",
                [id],
                |row| {
                    let album = row.get::<_, String>(2)?;
                    let title = row.get::<_, String>(3)?;
                    Ok(track_identity(row.get(0)?, row.get(1)?, &album, &title))
                },
            )
            .optional()
            .transpose()
        })
        .collect::<Result<Vec<_>, _>>()?;
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
        .filter(|identity| !identity.album_key.is_empty())
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
    Ok(())
}

pub(crate) fn apply_deleted_release_memory(conn: &Connection) -> Result<usize, rusqlite::Error> {
    let memories = load_memories(conn)?;
    let library = crate::artist_news_query::local_library_index(conn)?;
    let (acquired, mut remaining): (Vec<_>, Vec<_>) = memories.into_iter().partition(|memory| {
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
    for memory in &acquired {
        conn.execute(
            "DELETE FROM deleted_releases
             WHERE artist_key = ?1 AND title_key = ?2 AND scope = ?3",
            rusqlite::params![memory.artist_key, memory.title_key, memory.scope],
        )?;
    }
    let releases = conn
        .prepare(
            "SELECT release_group_mbid, artist_name, title, release_type, hidden
             FROM new_releases",
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, bool>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let forgotten_keys = releases
        .iter()
        .filter(|(_, artist, title, release_type, hidden)| {
            if !hidden {
                return false;
            }
            let key = (
                crate::artist_news::normalize(artist),
                crate::artist_news::normalize(title),
            );
            acquired
                .iter()
                .any(|memory| memory.matches(&key, release_type))
                && !remaining
                    .iter()
                    .any(|memory| memory.matches(&key, release_type))
        })
        .map(|(_, artist, title, _, _)| {
            (
                crate::artist_news::normalize(artist),
                crate::artist_news::normalize(title),
            )
        })
        .collect::<HashSet<_>>();
    for (mbid, artist, title, _, hidden) in &releases {
        let key = (
            crate::artist_news::normalize(artist),
            crate::artist_news::normalize(title),
        );
        if *hidden && forgotten_keys.contains(&key) {
            crate::artist_news_query::set_release_hidden_in(conn, mbid, false)?;
        }
    }
    remaining.retain(|memory| {
        !forgotten_keys.contains(&(memory.artist_key.clone(), memory.title_key.clone()))
    });
    let mut hidden_count = 0;
    for (mbid, artist, title, release_type, hidden) in releases {
        let key = (
            crate::artist_news::normalize(&artist),
            crate::artist_news::normalize(&title),
        );
        let remembered = remaining
            .iter()
            .any(|memory| memory.matches(&key, &release_type));
        if !hidden && remembered && !forgotten_keys.contains(&key) {
            crate::artist_news_query::set_release_hidden_in(conn, &mbid, true)?;
            hidden_count += 1;
        }
    }
    Ok(hidden_count)
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

pub(crate) fn forget_deleted_release_memory(
    conn: &Connection,
    release_group_mbid: &str,
) -> Result<(), rusqlite::Error> {
    let release = conn
        .query_row(
            "SELECT artist_name, title FROM new_releases WHERE release_group_mbid = ?1",
            [release_group_mbid],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let Some((artist, title)) = release else {
        return Ok(());
    };
    conn.execute(
        "DELETE FROM deleted_releases WHERE artist_key = ?1 AND title_key = ?2",
        rusqlite::params![
            crate::artist_news::normalize(&artist),
            crate::artist_news::normalize(&title)
        ],
    )?;
    Ok(())
}
