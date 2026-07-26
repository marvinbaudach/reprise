//! Radio favorite persistence and queries.

use rusqlite::{params, Connection, OptionalExtension, Row};

use super::StationRow;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewStation {
    pub uuid: Option<String>,
    pub name: String,
    pub stream_url: String,
    pub homepage: Option<String>,
    pub favicon_url: Option<String>,
    pub genre: Option<String>,
    pub codec: Option<String>,
    pub bitrate_kbps: Option<i64>,
    pub country_code: Option<String>,
    pub votes: Option<i64>,
}

pub fn add_or_restore(
    conn: &Connection,
    station: &NewStation,
    now: i64,
) -> Result<i64, rusqlite::Error> {
    let transaction = conn.unchecked_transaction()?;
    let existing_id = find_identity(&transaction, station)?;
    let id = if let Some(id) = existing_id {
        transaction.execute(
            "UPDATE radio_stations
             SET uuid = ?2, name = ?3, stream_url = ?4, homepage = ?5,
                 favicon_url = ?6, genre = ?7, codec = ?8, bitrate_kbps = ?9,
                 country_code = ?10, votes = ?11, removed_at = NULL
             WHERE id = ?1",
            params![
                id,
                station.uuid,
                station.name,
                station.stream_url,
                station.homepage,
                station.favicon_url,
                station.genre,
                station.codec,
                station.bitrate_kbps,
                station.country_code,
                station.votes,
            ],
        )?;
        id
    } else {
        transaction.execute(
            "INSERT INTO radio_stations
                (uuid, name, stream_url, homepage, favicon_url, genre, codec,
                 bitrate_kbps, country_code, votes, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                station.uuid,
                station.name,
                station.stream_url,
                station.homepage,
                station.favicon_url,
                station.genre,
                station.codec,
                station.bitrate_kbps,
                station.country_code,
                station.votes,
                now,
            ],
        )?;
        transaction.last_insert_rowid()
    };
    transaction.commit()?;
    Ok(id)
}

pub fn list(conn: &Connection) -> Result<Vec<StationRow>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT id, uuid, name, stream_url, homepage, favicon_url, genre, codec,
                bitrate_kbps, country_code, votes, added_at, removed_at
         FROM radio_stations
         WHERE removed_at IS NULL
         ORDER BY name COLLATE NOCASE, id",
    )?;
    let stations = statement
        .query_map([], row_to_station)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(stations)
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<StationRow>, rusqlite::Error> {
    conn.query_row(
        "SELECT id, uuid, name, stream_url, homepage, favicon_url, genre, codec,
                bitrate_kbps, country_code, votes, added_at, removed_at
         FROM radio_stations
         WHERE id = ?1 AND removed_at IS NULL",
        params![id],
        row_to_station,
    )
    .optional()
}

pub fn count_stations(conn: &Connection) -> Result<u64, rusqlite::Error> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM radio_stations WHERE removed_at IS NULL",
        [],
        |row| row.get(0),
    )?;
    Ok(count.try_into().unwrap_or_default())
}

pub fn tombstone(conn: &Connection, id: i64, now: i64) -> Result<bool, rusqlite::Error> {
    Ok(conn.execute(
        "UPDATE radio_stations SET removed_at = ?2
         WHERE id = ?1 AND removed_at IS NULL",
        params![id, now],
    )? != 0)
}

pub fn undo_remove(conn: &Connection, id: i64) -> Result<bool, rusqlite::Error> {
    Ok(conn.execute(
        "UPDATE radio_stations SET removed_at = NULL
         WHERE id = ?1 AND removed_at IS NOT NULL",
        params![id],
    )? != 0)
}

pub fn commit_remove(conn: &Connection, id: i64) -> Result<bool, rusqlite::Error> {
    Ok(conn.execute(
        "DELETE FROM radio_stations WHERE id = ?1 AND removed_at IS NOT NULL",
        params![id],
    )? != 0)
}

pub fn update_stream_url(
    conn: &Connection,
    id: i64,
    stream_url: &str,
) -> Result<bool, rusqlite::Error> {
    Ok(conn.execute(
        "UPDATE radio_stations SET stream_url = ?2 WHERE id = ?1",
        params![id, stream_url],
    )? != 0)
}

pub fn update_details(
    conn: &Connection,
    id: i64,
    name: &str,
    genre: Option<&str>,
    stream_url: &str,
) -> Result<bool, rusqlite::Error> {
    Ok(conn.execute(
        "UPDATE radio_stations
         SET name = ?2, genre = ?3, stream_url = ?4
         WHERE id = ?1 AND removed_at IS NULL",
        params![id, name, genre, stream_url],
    )? != 0)
}

fn find_identity(conn: &Connection, station: &NewStation) -> Result<Option<i64>, rusqlite::Error> {
    conn.query_row(
        "SELECT id
         FROM radio_stations
         WHERE (?1 IS NOT NULL AND uuid = ?1) OR stream_url = ?2
         ORDER BY CASE WHEN uuid = ?1 THEN 0 ELSE 1 END
         LIMIT 1",
        params![station.uuid, station.stream_url],
        |row| row.get(0),
    )
    .optional()
}

fn row_to_station(row: &Row<'_>) -> Result<StationRow, rusqlite::Error> {
    Ok(StationRow {
        id: row.get(0)?,
        uuid: row.get(1)?,
        name: row.get(2)?,
        stream_url: row.get(3)?,
        homepage: row.get(4)?,
        favicon_url: row.get(5)?,
        genre: row.get(6)?,
        codec: row.get(7)?,
        bitrate_kbps: row.get(8)?,
        country_code: row.get(9)?,
        votes: row.get(10)?,
        added_at: row.get(11)?,
        removed_at: row.get(12)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
        conn
    }

    fn station() -> NewStation {
        NewStation {
            uuid: Some("station-1".into()),
            name: "Station One".into(),
            stream_url: "https://radio.example/live".into(),
            homepage: None,
            favicon_url: None,
            genre: Some("metal".into()),
            codec: Some("MP3".into()),
            bitrate_kbps: Some(192),
            country_code: Some("CH".into()),
            votes: Some(42),
        }
    }

    #[test]
    fn station_tombstone_cycle_changes_count_then_restores_or_commits() {
        let conn = conn();
        let id = add_or_restore(&conn, &station(), 10).unwrap();
        assert_eq!(count_stations(&conn).unwrap(), 1);

        tombstone(&conn, id, 20).unwrap();
        assert_eq!(count_stations(&conn).unwrap(), 0);
        assert!(list(&conn).unwrap().is_empty());
        assert!(get(&conn, id).unwrap().is_none());

        undo_remove(&conn, id).unwrap();
        assert_eq!(count_stations(&conn).unwrap(), 1);

        tombstone(&conn, id, 30).unwrap();
        commit_remove(&conn, id).unwrap();
        assert!(get(&conn, id).unwrap().is_none());
    }

    #[test]
    fn adding_same_stream_or_uuid_revives_without_duplication() {
        let conn = conn();
        let id = add_or_restore(&conn, &station(), 10).unwrap();
        tombstone(&conn, id, 20).unwrap();

        let revived = add_or_restore(
            &conn,
            &NewStation {
                name: "Renamed".into(),
                stream_url: "https://radio.example/changed".into(),
                ..station()
            },
            30,
        )
        .unwrap();

        assert_eq!(revived, id);
        assert_eq!(count_stations(&conn).unwrap(), 1);
        let row = get(&conn, id).unwrap().unwrap();
        assert_eq!(row.name, "Renamed");
        assert_eq!(row.stream_url, "https://radio.example/changed");
        assert_eq!(row.added_at, 10, "revival preserves original added time");
    }

    #[test]
    fn stream_updates_are_persisted_for_future_fallback() {
        let conn = conn();
        let id = add_or_restore(&conn, &station(), 10).unwrap();

        update_stream_url(&conn, id, "https://radio.example/fresh").unwrap();

        assert_eq!(
            get(&conn, id).unwrap().unwrap().stream_url,
            "https://radio.example/fresh"
        );
    }
}
