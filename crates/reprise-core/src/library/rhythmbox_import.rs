//! Read-only import of per-track statistics from Rhythmbox's `rhythmdb.xml`.

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RhythmboxImportChoices {
    pub ratings: bool,
    pub play_counts: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RhythmboxImportSummary {
    pub parsed: usize,
    pub matched: usize,
    pub ratings_imported: usize,
    pub play_counts_raised: usize,
    pub skipped: usize,
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
}

#[derive(Default)]
struct EntryBuilder {
    location: String,
    rating: String,
    play_count: String,
}

impl EntryBuilder {
    fn push(&mut self, field: Field, value: &str) {
        match field {
            Field::Location => self.location.push_str(value),
            Field::Rating => self.rating.push_str(value),
            Field::PlayCount => self.play_count.push_str(value),
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
        (rating.is_some() || play_count.is_some()).then_some(RhythmboxTrackStats {
            path,
            rating,
            play_count,
        })
    }
}

fn field_for(name: &[u8]) -> Option<Field> {
    match name {
        b"location" => Some(Field::Location),
        b"rating" => Some(Field::Rating),
        b"play-count" => Some(Field::PlayCount),
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
) -> Result<RhythmboxImportSummary, RhythmboxImportError> {
    let transaction = conn.transaction()?;
    let mut summary = RhythmboxImportSummary {
        parsed: tracks.len(),
        ..RhythmboxImportSummary::default()
    };

    for track in tracks {
        let path = track.path.to_string_lossy();
        let current = transaction
            .query_row(
                "SELECT rating, play_count FROM tracks WHERE path = ?1",
                [&path],
                |row| Ok((row.get::<_, i32>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        let Some((current_rating, current_play_count)) = current else {
            summary.skipped += 1;
            continue;
        };
        summary.matched += 1;

        let next_rating = if choices.ratings && current_rating == 0 {
            track.rating.unwrap_or(current_rating)
        } else {
            current_rating
        };
        let next_play_count = if choices.play_counts {
            track.play_count.map_or(current_play_count, |imported| {
                current_play_count.max(imported)
            })
        } else {
            current_play_count
        };
        summary.ratings_imported += usize::from(next_rating != current_rating);
        summary.play_counts_raised += usize::from(next_play_count != current_play_count);

        if next_rating != current_rating || next_play_count != current_play_count {
            transaction.execute(
                "UPDATE tracks SET rating = ?1, play_count = ?2 WHERE path = ?3",
                rusqlite::params![next_rating, next_play_count, path],
            )?;
        }
    }

    transaction.commit()?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

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
  <entry type="song"><location>{uri}</location><rating>4</rating><play-count>17</play-count></entry>
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
<entry type="song"><location>file:///valid.ogg</location><rating>99</rating><play-count>-2</play-count></entry>
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
        let summary = merge_stats(
            &mut conn,
            &[RhythmboxTrackStats {
                path,
                rating: Some(3),
                play_count: Some(12),
            }],
            RhythmboxImportChoices {
                ratings: true,
                play_counts: true,
            },
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
        }];
        let choices = RhythmboxImportChoices {
            ratings: true,
            play_counts: true,
        };

        let first = merge_stats(&mut conn, &imported, choices).unwrap();
        let second = merge_stats(&mut conn, &imported, choices).unwrap();

        assert_eq!(values(&conn), (4, 11));
        assert_eq!((first.ratings_imported, first.play_counts_raised), (1, 1));
        assert_eq!((second.ratings_imported, second.play_counts_raised), (0, 0));
    }

    #[test]
    fn merge_respects_choices_and_counts_unmatched_entries() {
        let path = PathBuf::from("/music/song.ogg");
        let mut conn = database(&path, 0, 1);
        let summary = merge_stats(
            &mut conn,
            &[
                RhythmboxTrackStats {
                    path,
                    rating: Some(5),
                    play_count: Some(8),
                },
                RhythmboxTrackStats {
                    path: PathBuf::from("/music/missing.ogg"),
                    rating: Some(3),
                    play_count: Some(4),
                },
            ],
            RhythmboxImportChoices {
                ratings: false,
                play_counts: true,
            },
        )
        .unwrap();

        assert_eq!(values(&conn), (0, 8));
        assert_eq!(summary.parsed, 2);
        assert_eq!(summary.matched, 1);
        assert_eq!(summary.skipped, 1);
    }
}
