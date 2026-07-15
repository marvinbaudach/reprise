//! Read-only import of per-track statistics from Rhythmbox's `rhythmdb.xml`.

use std::collections::HashSet;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;
use rusqlite::{Connection, OptionalExtension};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RhythmboxTrackStats {
    pub path: PathBuf,
    pub rating: Option<i32>,
    pub play_count: Option<i64>,
    pub added_at: Option<i64>,
    pub last_played_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RhythmboxImportChoices {
    pub ratings: bool,
    pub play_counts_and_last_played: bool,
    pub added_at: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RhythmboxImportSummary {
    pub parsed: usize,
    pub matched: usize,
    pub ratings_imported: usize,
    pub play_counts_raised: usize,
    pub dates_imported: usize,
    pub last_played_imported: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RhythmboxRollbackEntry {
    pub path: String,
    pub rating: i32,
    pub play_count: i64,
    pub added_at: i64,
    pub last_played_at: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RhythmboxRollback {
    pub entries: Vec<RhythmboxRollbackEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RhythmboxPlaylist {
    pub name: String,
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RhythmboxPrescanResult {
    pub total_entries: usize,
    pub song_entries: usize,
    pub non_song_entries: usize,
    pub rated_tracks: usize,
    pub tracks_with_history: usize,
    pub tracks_with_date_added: usize,
    pub matched: usize,
    pub outside_library: usize,
    pub missing_on_disk: usize,
    pub playlist_count: usize,
    pub playlist_track_count: usize,
    pub last_modified: Option<std::time::SystemTime>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RhythmboxPlaylistSummary {
    pub parsed: usize,
    pub imported: usize,
    pub tracks_added: usize,
    pub skipped_tracks: usize,
}

#[derive(Debug, Error)]
pub enum RhythmboxImportError {
    #[error("could not read Rhythmbox database: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid Rhythmbox XML: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("invalid Rhythmbox text encoding: {0}")]
    Encoding(#[from] quick_xml::encoding::EncodingError),
    #[error("invalid Rhythmbox XML text: {0}")]
    Escape(#[from] quick_xml::escape::EscapeError),
    #[error("Rhythmbox XML ended before all elements were closed")]
    UnexpectedEof,
    #[error("could not update Reprise statistics: {0}")]
    Database(#[from] rusqlite::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Location,
    Rating,
    PlayCount,
    FirstSeen,
    LastPlayed,
}

#[derive(Default)]
struct EntryBuilder {
    location: String,
    rating: String,
    play_count: String,
    first_seen: String,
    last_played: String,
}

impl EntryBuilder {
    fn push(&mut self, field: Field, value: &str) {
        match field {
            Field::Location => self.location.push_str(value),
            Field::Rating => self.rating.push_str(value),
            Field::PlayCount => self.play_count.push_str(value),
            Field::FirstSeen => self.first_seen.push_str(value),
            Field::LastPlayed => self.last_played.push_str(value),
        }
    }

    fn finish(self) -> Option<RhythmboxTrackStats> {
        let url = url::Url::parse(self.location.trim()).ok()?;
        let path = url.to_file_path().ok()?;
        let rating = self
            .rating
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|value| value.round() as i32)
            .filter(|value| (1..=5).contains(value));
        let play_count = self
            .play_count
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|value| *value >= 0);
        let added_at = self
            .first_seen
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|value| *value > 0);
        let last_played_at = self
            .last_played
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|value| *value > 0);
        (rating.is_some() || play_count.is_some() || added_at.is_some() || last_played_at.is_some())
            .then_some(RhythmboxTrackStats {
                path,
                rating,
                play_count,
                added_at,
                last_played_at,
            })
    }
}

fn field_for(name: &[u8]) -> Option<Field> {
    match name {
        b"location" => Some(Field::Location),
        b"rating" => Some(Field::Rating),
        b"play-count" => Some(Field::PlayCount),
        b"first-seen" => Some(Field::FirstSeen),
        b"last-played" => Some(Field::LastPlayed),
        _ => None,
    }
}

pub fn parse_rhythmdb(path: &Path) -> Result<Vec<RhythmboxTrackStats>, RhythmboxImportError> {
    let file = File::open(path)?;
    let mut reader = Reader::from_reader(BufReader::new(file));
    reader.config_mut().trim_text(true);
    reader.config_mut().check_end_names = true;
    let mut buffer = Vec::new();
    let mut depth = 0usize;
    let mut entry = None;
    let mut field = None;
    let mut tracks = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(element) => {
                depth += 1;
                if element.name().as_ref() == b"entry" {
                    let is_song = element.attributes().flatten().any(|attribute| {
                        attribute.key.as_ref() == b"type"
                            && attribute
                                .decoded_and_normalized_value(
                                    quick_xml::XmlVersion::Implicit1_0,
                                    reader.decoder(),
                                )
                                .is_ok_and(|value| value == "song")
                    });
                    entry = is_song.then(EntryBuilder::default);
                    field = None;
                } else if entry.is_some() {
                    field = field_for(element.name().as_ref());
                }
            }
            Event::Text(text) => {
                if let (Some(entry), Some(field)) = (&mut entry, field) {
                    let decoded = text.decode()?;
                    entry.push(field, &decoded);
                }
            }
            Event::CData(text) => {
                if let (Some(entry), Some(field)) = (&mut entry, field) {
                    let decoded = text.decode()?;
                    entry.push(field, &decoded);
                }
            }
            Event::GeneralRef(reference) => {
                if let (Some(entry), Some(field)) = (&mut entry, field) {
                    let reference = reference.decode()?;
                    let escaped = format!("&{reference};");
                    let decoded = quick_xml::escape::unescape(&escaped)?;
                    entry.push(field, &decoded);
                }
            }
            Event::End(element) => {
                depth = depth.saturating_sub(1);
                if element.name().as_ref() == b"entry" {
                    if let Some(track) = entry.take().and_then(EntryBuilder::finish) {
                        tracks.push(track);
                    }
                    field = None;
                } else if field_for(element.name().as_ref()).is_some() {
                    field = None;
                }
            }
            Event::Eof => {
                if depth != 0 {
                    return Err(RhythmboxImportError::UnexpectedEof);
                }
                return Ok(tracks);
            }
            _ => {}
        }
        buffer.clear();
    }
}

pub fn merge_stats(
    conn: &mut Connection,
    tracks: &[RhythmboxTrackStats],
    choices: RhythmboxImportChoices,
    on_progress: Option<&dyn Fn(usize)>,
) -> Result<(RhythmboxImportSummary, RhythmboxRollback), RhythmboxImportError> {
    let transaction = conn.transaction()?;
    let mut summary = RhythmboxImportSummary {
        parsed: tracks.len(),
        ..RhythmboxImportSummary::default()
    };
    let mut rollback = RhythmboxRollback::default();

    for (index, track) in tracks.iter().enumerate() {
        let path = track.path.to_string_lossy();
        let current = transaction
            .query_row(
                "SELECT rating, play_count, added_at, last_played_at FROM tracks WHERE path = ?1",
                [&path],
                |row| {
                    Ok((
                        row.get::<_, i32>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((current_rating, current_play_count, current_added_at, current_last_played)) =
            current
        else {
            summary.skipped += 1;
            if let Some(cb) = on_progress {
                cb(index + 1);
            }
            continue;
        };
        summary.matched += 1;

        let next_rating = if choices.ratings && current_rating == 0 {
            track.rating.unwrap_or(current_rating)
        } else {
            current_rating
        };
        let next_play_count = if choices.play_counts_and_last_played {
            track.play_count.map_or(current_play_count, |imported| {
                current_play_count.max(imported)
            })
        } else {
            current_play_count
        };
        let next_added_at = if choices.added_at {
            track.added_at.map_or(current_added_at, |imported| {
                if current_added_at > 0 {
                    current_added_at.min(imported)
                } else {
                    imported
                }
            })
        } else {
            current_added_at
        };
        let next_last_played = if choices.play_counts_and_last_played {
            match (current_last_played, track.last_played_at) {
                (Some(current), Some(imported)) => Some(current.max(imported)),
                (None, Some(imported)) => Some(imported),
                (current, None) => current,
            }
        } else {
            current_last_played
        };
        summary.ratings_imported += usize::from(next_rating != current_rating);
        summary.play_counts_raised += usize::from(next_play_count != current_play_count);
        summary.dates_imported += usize::from(next_added_at != current_added_at);
        summary.last_played_imported += usize::from(next_last_played != current_last_played);

        if next_rating != current_rating
            || next_play_count != current_play_count
            || next_added_at != current_added_at
            || next_last_played != current_last_played
        {
            rollback.entries.push(RhythmboxRollbackEntry {
                path: path.to_string(),
                rating: current_rating,
                play_count: current_play_count,
                added_at: current_added_at,
                last_played_at: current_last_played,
            });
            transaction.execute(
                "UPDATE tracks SET rating = ?1, play_count = ?2, added_at = ?3, last_played_at = ?4 WHERE path = ?5",
                rusqlite::params![
                    next_rating,
                    next_play_count,
                    next_added_at,
                    next_last_played,
                    path
                ],
            )?;
        }
        if let Some(cb) = on_progress {
            cb(index + 1);
        }
    }

    transaction.commit()?;
    Ok((summary, rollback))
}

pub fn undo_rhythmbox_import(
    conn: &mut Connection,
    rollback: &RhythmboxRollback,
) -> Result<usize, RhythmboxImportError> {
    let transaction = conn.transaction()?;
    let mut restored = 0usize;
    for entry in &rollback.entries {
        let affected = transaction.execute(
            "UPDATE tracks SET rating = ?1, play_count = ?2, added_at = ?3, last_played_at = ?4 WHERE path = ?5",
            rusqlite::params![
                entry.rating,
                entry.play_count,
                entry.added_at,
                entry.last_played_at,
                entry.path,
            ],
        )?;
        restored += affected;
    }
    transaction.commit()?;
    Ok(restored)
}

pub fn prescan_rhythmdb(
    rhythmdb_path: &Path,
    playlists_path: &Path,
    conn: &Connection,
    library_root: Option<&str>,
) -> Result<RhythmboxPrescanResult, RhythmboxImportError> {
    let last_modified = std::fs::metadata(rhythmdb_path)
        .and_then(|m| m.modified())
        .ok();

    let file = File::open(rhythmdb_path)?;
    let mut reader = Reader::from_reader(BufReader::new(file));
    reader.config_mut().trim_text(true);
    reader.config_mut().check_end_names = true;
    let mut buffer = Vec::new();
    let mut depth = 0usize;
    let mut result = RhythmboxPrescanResult {
        last_modified,
        ..RhythmboxPrescanResult::default()
    };

    let mut entry_builder: Option<EntryBuilder> = None;
    let mut field: Option<Field> = None;

    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(element) => {
                depth += 1;
                if element.name().as_ref() == b"entry" {
                    let entry_type = element
                        .attributes()
                        .flatten()
                        .find_map(|attr| {
                            (attr.key.as_ref() == b"type").then(|| {
                                attr.decoded_and_normalized_value(
                                    quick_xml::XmlVersion::Implicit1_0,
                                    reader.decoder(),
                                )
                                .ok()
                                .map(|v| v.into_owned())
                            })
                        })
                        .flatten();
                    result.total_entries += 1;
                    match entry_type.as_deref() {
                        Some("song") => {
                            result.song_entries += 1;
                            entry_builder = Some(EntryBuilder::default());
                        }
                        _ => {
                            result.non_song_entries += 1;
                            entry_builder = None;
                        }
                    }
                    field = None;
                } else if entry_builder.is_some() {
                    field = field_for(element.name().as_ref());
                }
            }
            Event::Text(text) => {
                if let (Some(builder), Some(f)) = (&mut entry_builder, field) {
                    let decoded = text.decode()?;
                    builder.push(f, &decoded);
                }
            }
            Event::CData(text) => {
                if let (Some(builder), Some(f)) = (&mut entry_builder, field) {
                    let decoded = text.decode()?;
                    builder.push(f, &decoded);
                }
            }
            Event::GeneralRef(reference) => {
                if let (Some(builder), Some(f)) = (&mut entry_builder, field) {
                    let reference = reference.decode()?;
                    let escaped = format!("&{reference};");
                    let decoded = quick_xml::escape::unescape(&escaped)?;
                    builder.push(f, &decoded);
                }
            }
            Event::End(element) => {
                depth = depth.saturating_sub(1);
                if element.name().as_ref() == b"entry" {
                    if let Some(builder) = entry_builder.take() {
                        if let Some(track) = builder.finish() {
                            if track.rating.is_some() {
                                result.rated_tracks += 1;
                            }
                            if track.play_count.unwrap_or(0) > 0 || track.last_played_at.is_some() {
                                result.tracks_with_history += 1;
                            }
                            if track.added_at.is_some() {
                                result.tracks_with_date_added += 1;
                            }
                            let path_str = track.path.to_string_lossy();
                            let in_db = conn
                                .query_row(
                                    "SELECT 1 FROM tracks WHERE path = ?1",
                                    [&path_str],
                                    |_| Ok(()),
                                )
                                .optional()
                                .unwrap_or(None)
                                .is_some();
                            if in_db {
                                result.matched += 1;
                            } else {
                                let under_root = library_root
                                    .is_some_and(|root| path_str.starts_with(root));
                                if !under_root {
                                    result.outside_library += 1;
                                } else if !track.path.exists() {
                                    result.missing_on_disk += 1;
                                } else {
                                    result.outside_library += 1;
                                }
                            }
                        }
                    }
                    field = None;
                } else if field_for(element.name().as_ref()).is_some() {
                    field = None;
                }
            }
            Event::Eof => {
                if depth != 0 {
                    return Err(RhythmboxImportError::UnexpectedEof);
                }
                break;
            }
            _ => {}
        }
        buffer.clear();
    }

    if playlists_path.is_file() {
        if let Ok(playlists) = parse_playlists(playlists_path) {
            result.playlist_count = playlists.len();
            result.playlist_track_count = playlists.iter().map(|p| p.paths.len()).sum();
        }
    }

    Ok(result)
}

pub fn parse_playlists(path: &Path) -> Result<Vec<RhythmboxPlaylist>, RhythmboxImportError> {
    let file = File::open(path)?;
    let mut reader = Reader::from_reader(BufReader::new(file));
    reader.config_mut().trim_text(true);
    reader.config_mut().check_end_names = true;
    let mut buffer = Vec::new();
    let mut depth = 0usize;
    let mut playlist: Option<RhythmboxPlaylist> = None;
    let mut location = None::<String>;
    let mut playlists = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(element) => {
                depth += 1;
                if element.name().as_ref() == b"playlist" {
                    let mut name = None;
                    let mut playlist_type = None;
                    for attribute in element.attributes().flatten() {
                        let value = attribute.decoded_and_normalized_value(
                            quick_xml::XmlVersion::Implicit1_0,
                            reader.decoder(),
                        )?;
                        match attribute.key.as_ref() {
                            b"name" => name = Some(value.into_owned()),
                            b"type" => playlist_type = Some(value.into_owned()),
                            _ => {}
                        }
                    }
                    playlist = match (name, playlist_type.as_deref()) {
                        (Some(name), Some("static")) if !name.trim().is_empty() => {
                            Some(RhythmboxPlaylist {
                                name,
                                paths: Vec::new(),
                            })
                        }
                        _ => None,
                    };
                } else if element.name().as_ref() == b"location" && playlist.is_some() {
                    location = Some(String::new());
                }
            }
            Event::Text(text) => {
                if let Some(location) = &mut location {
                    location.push_str(&text.decode()?);
                }
            }
            Event::CData(text) => {
                if let Some(location) = &mut location {
                    location.push_str(&text.decode()?);
                }
            }
            Event::GeneralRef(reference) => {
                if let Some(location) = &mut location {
                    let reference = reference.decode()?;
                    let escaped = format!("&{reference};");
                    location.push_str(&quick_xml::escape::unescape(&escaped)?);
                }
            }
            Event::End(element) => {
                depth = depth.saturating_sub(1);
                if element.name().as_ref() == b"location" {
                    if let (Some(playlist), Some(location)) = (&mut playlist, location.take()) {
                        if let Ok(url) = url::Url::parse(location.trim()) {
                            if let Ok(path) = url.to_file_path() {
                                playlist.paths.push(path);
                            }
                        }
                    }
                } else if element.name().as_ref() == b"playlist" {
                    if let Some(playlist) = playlist.take() {
                        playlists.push(playlist);
                    }
                    location = None;
                }
            }
            Event::Eof => {
                if depth != 0 {
                    return Err(RhythmboxImportError::UnexpectedEof);
                }
                return Ok(playlists);
            }
            _ => {}
        }
        buffer.clear();
    }
}

pub fn merge_playlists(
    conn: &mut Connection,
    playlists: &[RhythmboxPlaylist],
) -> Result<RhythmboxPlaylistSummary, RhythmboxImportError> {
    let mut summary = RhythmboxPlaylistSummary {
        parsed: playlists.len(),
        ..RhythmboxPlaylistSummary::default()
    };

    for playlist in playlists {
        let mut seen = HashSet::new();
        let mut track_ids = Vec::new();
        for path in &playlist.paths {
            let path = path.to_string_lossy();
            let track_id = conn
                .query_row("SELECT id FROM tracks WHERE path=?1", [&path], |row| {
                    row.get::<_, i64>(0)
                })
                .optional()?;
            match track_id {
                Some(track_id) if seen.insert(track_id) => track_ids.push(track_id),
                Some(_) => {}
                None => summary.skipped_tracks += 1,
            }
        }
        if track_ids.is_empty() {
            continue;
        }

        let existing = conn
            .query_row(
                "SELECT id FROM playlists WHERE name=?1 ORDER BY position LIMIT 1",
                [&playlist.name],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        let added = if let Some(playlist_id) = existing {
            crate::library::playlist_membership::add_unique_tracks(conn, playlist_id, &track_ids)?
                as usize
        } else {
            crate::library::playlists::create_with_tracks(conn, &playlist.name, &track_ids)?;
            track_ids.len()
        };
        summary.imported += 1;
        summary.tracks_added += added;
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn prescan_counts_entries_and_classifies_skips() {
        let dir = tempdir().unwrap();
        let music_dir = dir.path().join("music");
        fs::create_dir_all(&music_dir).unwrap();
        let existing = music_dir.join("song.ogg");
        fs::write(&existing, b"fake").unwrap();
        let existing_uri = url::Url::from_file_path(&existing).unwrap();
        let missing_uri =
            url::Url::from_file_path(music_dir.join("gone.ogg")).unwrap();
        let outside_uri =
            url::Url::from_file_path(dir.path().join("elsewhere.ogg")).unwrap();
        let xml = format!(
            r#"<?xml version="1.0"?>
<rhythmdb version="2.0">
  <entry type="song"><location>{existing_uri}</location><rating>4</rating><play-count>10</play-count><first-seen>1700000000</first-seen><last-played>1700000500</last-played></entry>
  <entry type="song"><location>{missing_uri}</location><rating>3</rating></entry>
  <entry type="song"><location>{outside_uri}</location><play-count>5</play-count></entry>
  <entry type="podcast-post"><location>file:///podcast.ogg</location><rating>5</rating></entry>
</rhythmdb>"#
        );
        let rhythmdb = dir.path().join("rhythmdb.xml");
        fs::write(&rhythmdb, xml).unwrap();
        let playlists_path = dir.path().join("playlists.xml");
        fs::write(
            &playlists_path,
            r#"<?xml version="1.0"?>
<rhythmdb-playlists>
  <playlist name="Gym" type="static">
    <location>file:///a.ogg</location>
    <location>file:///b.ogg</location>
  </playlist>
</rhythmdb-playlists>"#,
        )
        .unwrap();

        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (path, added_at, rating, play_count) VALUES (?1, 0, 0, 0)",
            [existing.to_string_lossy()],
        )
        .unwrap();

        let library_root = music_dir.to_string_lossy().to_string();
        let result = prescan_rhythmdb(
            &rhythmdb,
            &playlists_path,
            &conn,
            Some(&library_root),
        )
        .unwrap();

        assert_eq!(result.total_entries, 4);
        assert_eq!(result.song_entries, 3);
        assert_eq!(result.non_song_entries, 1);
        assert_eq!(result.rated_tracks, 2);
        assert_eq!(result.tracks_with_history, 2);
        assert_eq!(result.tracks_with_date_added, 1);
        assert_eq!(result.matched, 1);
        assert_eq!(result.outside_library, 1);
        assert_eq!(result.missing_on_disk, 1);
        assert_eq!(result.playlist_count, 1);
        assert_eq!(result.playlist_track_count, 2);
    }

    fn database(path: &Path, rating: i32, play_count: i64) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (path, added_at, rating, play_count) VALUES (?1, 0, ?2, ?3)",
            rusqlite::params![path.to_string_lossy(), rating, play_count],
        )
        .unwrap();
        conn
    }

    fn values(conn: &Connection) -> (i32, i64) {
        conn.query_row("SELECT rating, play_count FROM tracks", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap()
    }

    #[test]
    fn parser_keeps_only_songs_and_decodes_file_uris() {
        let dir = tempdir().unwrap();
        let track = dir.path().join("Artist & Album").join("Song 1.ogg");
        let uri = url::Url::from_file_path(&track).unwrap();
        let uri = quick_xml::escape::escape(uri.as_str());
        let xml = format!(
            r#"<?xml version="1.0"?>
<rhythmdb version="2.0">
  <entry type="song"><location>{uri}</location><rating>4</rating><play-count>17</play-count><first-seen>1700000000</first-seen><last-played>1700000500</last-played></entry>
  <entry type="podcast-post"><location>file:///ignored.ogg</location><rating>5</rating></entry>
</rhythmdb>"#
        );
        let path = dir.path().join("rhythmdb.xml");
        fs::write(&path, xml).unwrap();

        assert_eq!(
            parse_rhythmdb(&path).unwrap(),
            vec![RhythmboxTrackStats {
                path: track,
                rating: Some(4),
                play_count: Some(17),
                added_at: Some(1_700_000_000),
                last_played_at: Some(1_700_000_500),
            }]
        );
    }

    #[test]
    fn parser_skips_invalid_entries_but_rejects_broken_xml() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("rhythmdb.xml");
        fs::write(
            &path,
            r#"<rhythmdb>
<entry type="song"><location>https://example.com/song.ogg</location></entry>
<entry type="song"><location>file:///valid.ogg</location><rating>99</rating><play-count>-2</play-count><first-seen>-1</first-seen><last-played>0</last-played></entry>
</rhythmdb>"#,
        )
        .unwrap();
        assert!(parse_rhythmdb(&path).unwrap().is_empty());

        fs::write(&path, "<rhythmdb><entry>").unwrap();
        assert!(parse_rhythmdb(&path).is_err());
    }

    #[test]
    fn merge_preserves_local_rating_and_never_decreases_play_count() {
        let path = PathBuf::from("/music/song.ogg");
        let mut conn = database(&path, 5, 20);
        let (summary, _) = merge_stats(
            &mut conn,
            &[RhythmboxTrackStats {
                path,
                rating: Some(3),
                play_count: Some(12),
                added_at: None,
                last_played_at: None,
            }],
            RhythmboxImportChoices {
                ratings: true,
                play_counts_and_last_played: true,
                added_at: false,
            },
            None,
        )
        .unwrap();

        assert_eq!(values(&conn), (5, 20));
        assert_eq!(summary.matched, 1);
        assert_eq!(summary.ratings_imported, 0);
        assert_eq!(summary.play_counts_raised, 0);
    }

    #[test]
    fn merge_imports_missing_rating_and_higher_count_idempotently() {
        let path = PathBuf::from("/music/song.ogg");
        let mut conn = database(&path, 0, 2);
        let imported = [RhythmboxTrackStats {
            path,
            rating: Some(4),
            play_count: Some(11),
            added_at: None,
            last_played_at: None,
        }];
        let choices = RhythmboxImportChoices {
            ratings: true,
            play_counts_and_last_played: true,
            added_at: false,
        };

        let (first, _) = merge_stats(&mut conn, &imported, choices, None).unwrap();
        let (second, _) = merge_stats(&mut conn, &imported, choices, None).unwrap();

        assert_eq!(values(&conn), (4, 11));
        assert_eq!((first.ratings_imported, first.play_counts_raised), (1, 1));
        assert_eq!((second.ratings_imported, second.play_counts_raised), (0, 0));
    }

    #[test]
    fn merge_respects_choices_and_counts_unmatched_entries() {
        let path = PathBuf::from("/music/song.ogg");
        let mut conn = database(&path, 0, 1);
        let (summary, _) = merge_stats(
            &mut conn,
            &[
                RhythmboxTrackStats {
                    path,
                    rating: Some(5),
                    play_count: Some(8),
                    added_at: None,
                    last_played_at: None,
                },
                RhythmboxTrackStats {
                    path: PathBuf::from("/music/missing.ogg"),
                    rating: Some(3),
                    play_count: Some(4),
                    added_at: None,
                    last_played_at: None,
                },
            ],
            RhythmboxImportChoices {
                ratings: false,
                play_counts_and_last_played: true,
                added_at: false,
            },
            None,
        )
        .unwrap();

        assert_eq!(values(&conn), (0, 8));
        assert_eq!(summary.parsed, 2);
        assert_eq!(summary.matched, 1);
        assert_eq!(summary.skipped, 1);
    }

    #[test]
    fn merge_imports_only_an_older_positive_date_added_idempotently() {
        let path = PathBuf::from("/music/song.ogg");
        let mut conn = database(&path, 0, 0);
        conn.execute("UPDATE tracks SET added_at=200", []).unwrap();
        let imported = [RhythmboxTrackStats {
            path,
            rating: None,
            play_count: None,
            added_at: Some(100),
            last_played_at: None,
        }];
        let choices = RhythmboxImportChoices {
            ratings: false,
            play_counts_and_last_played: false,
            added_at: true,
        };

        let (first, _) = merge_stats(&mut conn, &imported, choices, None).unwrap();
        let (second, _) = merge_stats(&mut conn, &imported, choices, None).unwrap();
        let (newer, _) = merge_stats(
            &mut conn,
            &[RhythmboxTrackStats {
                path: PathBuf::from("/music/song.ogg"),
                rating: None,
                play_count: None,
                added_at: Some(300),
                last_played_at: None,
            }],
            choices,
            None,
        )
        .unwrap();
        let added_at = conn
            .query_row("SELECT added_at FROM tracks", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();

        assert_eq!(added_at, 100);
        assert_eq!(first.dates_imported, 1);
        assert_eq!(second.dates_imported, 0);
        assert_eq!(newer.dates_imported, 0);

        let missing_path = PathBuf::from("/music/without-date.ogg");
        let mut missing_conn = database(&missing_path, 0, 0);
        let (missing, _) = merge_stats(
            &mut missing_conn,
            &[RhythmboxTrackStats {
                path: missing_path,
                rating: None,
                play_count: None,
                added_at: Some(100),
                last_played_at: None,
            }],
            choices,
            None,
        )
        .unwrap();
        let imported_missing = missing_conn
            .query_row("SELECT added_at FROM tracks", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        assert_eq!(imported_missing, 100);
        assert_eq!(missing.dates_imported, 1);
    }

    #[test]
    fn merge_imports_only_a_newer_positive_last_played_idempotently() {
        let path = PathBuf::from("/music/song.ogg");
        let mut conn = database(&path, 0, 0);
        conn.execute("UPDATE tracks SET last_played_at=100", [])
            .unwrap();
        let imported = [RhythmboxTrackStats {
            path,
            rating: None,
            play_count: None,
            added_at: None,
            last_played_at: Some(200),
        }];
        let choices = RhythmboxImportChoices {
            ratings: false,
            play_counts_and_last_played: true,
            added_at: false,
        };

        let (first, _) = merge_stats(&mut conn, &imported, choices, None).unwrap();
        let (second, _) = merge_stats(&mut conn, &imported, choices, None).unwrap();
        let (older, _) = merge_stats(
            &mut conn,
            &[RhythmboxTrackStats {
                path: PathBuf::from("/music/song.ogg"),
                rating: None,
                play_count: None,
                added_at: None,
                last_played_at: Some(50),
            }],
            choices,
            None,
        )
        .unwrap();
        let last_played_at = conn
            .query_row("SELECT last_played_at FROM tracks", [], |row| {
                row.get::<_, Option<i64>>(0)
            })
            .unwrap();

        assert_eq!(last_played_at, Some(200));
        assert_eq!(first.last_played_imported, 1);
        assert_eq!(second.last_played_imported, 0);
        assert_eq!(older.last_played_imported, 0);

        let missing_path = PathBuf::from("/music/never-played.ogg");
        let mut missing_conn = database(&missing_path, 0, 0);
        let (missing, _) = merge_stats(
            &mut missing_conn,
            &[RhythmboxTrackStats {
                path: missing_path,
                rating: None,
                play_count: None,
                added_at: None,
                last_played_at: Some(200),
            }],
            choices,
            None,
        )
        .unwrap();
        let imported_missing = missing_conn
            .query_row("SELECT last_played_at FROM tracks", [], |row| {
                row.get::<_, Option<i64>>(0)
            })
            .unwrap();
        assert_eq!(imported_missing, Some(200));
        assert_eq!(missing.last_played_imported, 1);
    }

    #[test]
    fn merge_returns_rollback_and_undo_restores_original_values() {
        let path = PathBuf::from("/music/song.ogg");
        let mut conn = database(&path, 3, 5);
        conn.execute("UPDATE tracks SET added_at = 100, last_played_at = 200", [])
            .unwrap();

        let (summary, rollback) = merge_stats(
            &mut conn,
            &[RhythmboxTrackStats {
                path: path.clone(),
                rating: Some(5),
                play_count: Some(20),
                added_at: Some(50),
                last_played_at: Some(300),
            }],
            RhythmboxImportChoices {
                ratings: true,
                play_counts_and_last_played: true,
                added_at: true,
            },
            None,
        )
        .unwrap();

        // Verify import took effect
        assert_eq!(summary.play_counts_raised, 1);
        assert_eq!(summary.dates_imported, 1);
        assert_eq!(summary.last_played_imported, 1);
        assert_eq!(values(&conn), (3, 20)); // rating unchanged (was already set)

        // Undo
        let restored = undo_rhythmbox_import(&mut conn, &rollback).unwrap();
        assert_eq!(restored, 1);
        assert_eq!(values(&conn), (3, 5));
        let (added_at, last_played) = conn
            .query_row("SELECT added_at, last_played_at FROM tracks", [], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?))
            })
            .unwrap();
        assert_eq!(added_at, 100);
        assert_eq!(last_played, Some(200));
    }

    #[test]
    fn merge_calls_progress_for_each_track() {
        let path1 = PathBuf::from("/music/a.ogg");
        let path2 = PathBuf::from("/music/b.ogg");
        let conn_raw = Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn_raw).unwrap();
        conn_raw
            .execute(
                "INSERT INTO tracks (path, added_at, rating, play_count) VALUES (?1, 0, 0, 0)",
                [path1.to_string_lossy()],
            )
            .unwrap();
        conn_raw
            .execute(
                "INSERT INTO tracks (path, added_at, rating, play_count) VALUES (?1, 0, 0, 0)",
                [path2.to_string_lossy()],
            )
            .unwrap();
        let mut conn = conn_raw;

        let progress = std::cell::Cell::new(0usize);
        let (_, _) = merge_stats(
            &mut conn,
            &[
                RhythmboxTrackStats {
                    path: path1,
                    rating: Some(4),
                    play_count: None,
                    added_at: None,
                    last_played_at: None,
                },
                RhythmboxTrackStats {
                    path: path2,
                    rating: Some(3),
                    play_count: None,
                    added_at: None,
                    last_played_at: None,
                },
            ],
            RhythmboxImportChoices {
                ratings: true,
                play_counts_and_last_played: false,
                added_at: false,
            },
            Some(&|n| {
                progress.set(n);
            }),
        )
        .unwrap();

        assert_eq!(progress.get(), 2);
    }
}

#[cfg(test)]
#[path = "rhythmbox_playlist_import_tests.rs"]
mod playlist_tests;
