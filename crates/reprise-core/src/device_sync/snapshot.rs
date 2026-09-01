//! Database adapter for point-in-time manual and smart playlist snapshots.

use std::path::PathBuf;

use rusqlite::Connection;

use super::{
    everything_playlist_snapshot, MirrorPlaylistSnapshot, MirrorTrack, SelectionSource, SyncTrack,
    UnavailableTrack,
};
use crate::library::source::{
    LibraryLinkMode, LibraryPathPresence, LibrarySource, UnixLibrarySource,
};

/// A capped smart source may retain this many already-resident tracks below
/// its addition cap. Ten is a fixed, inspectable bound that covers the six-file
/// flap measured in the triggering runs without turning the cap into an open-
/// ended grace period. The device can therefore grow by at most ten resident
/// files per capped source; only the cap itself still authorises additions.
/// Candidates come from the same rule predicate, so a track that stops
/// matching the rules is removed immediately rather than protected here.
const SMART_CAP_STABILITY_MARGIN: i64 = 10;

pub fn load_mirror_playlist_snapshots(
    db: &crate::db::Db,
) -> Result<Vec<MirrorPlaylistSnapshot>, rusqlite::Error> {
    load_mirror_playlist_snapshots_with_source(&UnixLibrarySource, db)
}

pub fn load_mirror_playlist_snapshots_with_source(
    source: &dyn LibrarySource,
    db: &crate::db::Db,
) -> Result<Vec<MirrorPlaylistSnapshot>, rusqlite::Error> {
    let conn = db.conn();
    let mut snapshots = Vec::new();
    for playlist in crate::library::playlists::list(db)? {
        snapshots.push(MirrorPlaylistSnapshot {
            source: SelectionSource::Playlist(playlist.id),
            name: playlist.name,
            entries: load_manual_entries(source, conn, playlist.id)?,
            stability_margin_track_ids: Vec::new(),
        });
    }
    for playlist in crate::library::playlists::list_smart(db)? {
        let view_source = crate::view_source::ViewSource::Smart(playlist.id);
        let ids = crate::queries::query_track_ids(db, &view_source, "title", "asc", "", &[])?;
        let stability_margin_track_ids = load_smart_stability_margin(conn, &playlist, ids.len())?;
        let tracks = crate::queries::query_sync_tracks_with_source(source, db, &ids)?;
        snapshots.push(MirrorPlaylistSnapshot {
            source: SelectionSource::Smart(playlist.id),
            name: playlist.name,
            entries: tracks.into_iter().map(MirrorTrack::Available).collect(),
            stability_margin_track_ids,
        });
    }
    Ok(snapshots)
}

fn load_smart_stability_margin(
    conn: &Connection,
    playlist: &crate::library::playlists::SmartPlaylist,
    desired_count: usize,
) -> Result<Vec<i64>, rusqlite::Error> {
    if playlist.limit_count.is_none() {
        return Ok(Vec::new());
    }
    let (rules, mut params) =
        match crate::library::playlists::smart_rules_to_sql(&playlist.rules_json) {
            Ok(value) => value,
            Err(error) => {
                tracing::error!(
                    %error,
                    smart_id = playlist.id,
                    "invalid smart playlist rules; returning empty stability margin"
                );
                return Ok(Vec::new());
            }
        };
    let order = smart_member_order(&playlist.sort_field, &playlist.sort_dir);
    let sql = format!(
        "SELECT id FROM tracks \
         WHERE {} AND ({rules}) \
         ORDER BY {order} LIMIT ? OFFSET ?",
        crate::queries::PRESENT
    );
    params.push(rusqlite::types::Value::Integer(SMART_CAP_STABILITY_MARGIN));
    params.push(rusqlite::types::Value::Integer(
        i64::try_from(desired_count).unwrap_or(i64::MAX),
    ));
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(rusqlite::params_from_iter(params), |row| row.get(0))?;
    rows.collect()
}

/// Mirrors the smart query's validated member ordering without changing that
/// general-purpose query merely to ask for the retention-only rows after its
/// cap. Unknown fields retain the query's title fallback.
fn smart_member_order(sort_field: &str, sort_dir: &str) -> String {
    let expression = match sort_field {
        "title" => "title COLLATE NOCASE",
        "artist" => "artist COLLATE NOCASE, year, album COLLATE NOCASE, track_no",
        "album" => "album COLLATE NOCASE, track_no",
        "track_no" => "track_no",
        "genre" => "genre COLLATE NOCASE, artist COLLATE NOCASE",
        "year" => "year",
        "duration_ms" => "duration_ms",
        "rating" => "rating",
        "play_count" => "play_count",
        "added_at" => "added_at",
        "album_canonical" => {
            "CASE WHEN disc_no IS NULL THEN 1 ELSE disc_no END, \
             CASE WHEN track_no IS NULL THEN 1 ELSE 0 END, track_no, path COLLATE NOCASE, id"
        }
        _ => "title COLLATE NOCASE",
    };
    let direction = if sort_dir.eq_ignore_ascii_case("desc") {
        "DESC"
    } else {
        "ASC"
    };
    expression
        .split(',')
        .map(|term| format!("{} {direction}", term.trim()))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn load_everything_playlist_snapshot(
    db: &crate::db::Db,
) -> Result<MirrorPlaylistSnapshot, rusqlite::Error> {
    load_everything_playlist_snapshot_with_source(&UnixLibrarySource, db)
}

pub fn load_everything_playlist_snapshot_with_source(
    source: &dyn LibrarySource,
    db: &crate::db::Db,
) -> Result<MirrorPlaylistSnapshot, rusqlite::Error> {
    let library_ids = crate::queries::query_track_ids(
        db,
        &crate::view_source::ViewSource::Library,
        "title",
        "asc",
        "",
        &[],
    )?;
    let library_tracks = crate::queries::query_sync_tracks_with_source(source, db, &library_ids)?;
    Ok(everything_playlist_snapshot(library_tracks))
}

fn load_manual_entries(
    source: &dyn LibrarySource,
    conn: &Connection,
    playlist_id: i64,
) -> Result<Vec<MirrorTrack>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT tracks.id, tracks.path, tracks.title, tracks.artist, tracks.album, \
                tracks.album_artist, tracks.track_no, tracks.duration_ms, \
                tracks.bitrate_kbps, tracks.missing_since \
         FROM playlist_tracks \
         JOIN tracks ON tracks.id = playlist_tracks.track_id \
         WHERE playlist_tracks.playlist_id = ?1 AND tracks.removed_at IS NULL \
         ORDER BY playlist_tracks.position",
    )?;
    let rows = statement.query_map([playlist_id], |row| {
        Ok(SnapshotTrack {
            id: row.get(0)?,
            path: row.get(1)?,
            title: row.get(2)?,
            artist: row.get(3)?,
            album: row.get(4)?,
            album_artist: row.get(5)?,
            track_number: row.get(6)?,
            duration_ms: row.get(7)?,
            bitrate_kbps: row.get(8)?,
            missing_since: row.get(9)?,
        })
    })?;
    rows.map(|row| row.map(|track| track.into_mirror_track(source)))
        .collect()
}

struct SnapshotTrack {
    id: i64,
    path: String,
    title: String,
    artist: String,
    album: String,
    album_artist: String,
    track_number: Option<u32>,
    duration_ms: i64,
    bitrate_kbps: Option<i64>,
    missing_since: Option<i64>,
}

impl SnapshotTrack {
    fn into_mirror_track(self, source: &dyn LibrarySource) -> MirrorTrack {
        if self.missing_since.is_some() {
            return self.unavailable();
        }
        let source_path = PathBuf::from(&self.path);
        let LibraryPathPresence::Present(metadata) =
            source.probe(&source_path, LibraryLinkMode::Follow)
        else {
            return self.unavailable();
        };
        if !metadata.is_file {
            return self.unavailable();
        }
        let Some(size_bytes) = metadata.size else {
            return self.unavailable();
        };
        let Some(original_name) = source_path.file_name() else {
            return self.unavailable();
        };
        MirrorTrack::Available(SyncTrack {
            id: self.id,
            source_path: source_path.clone(),
            original_name: original_name.to_string_lossy().into_owned(),
            title: self.title,
            artist: self.artist,
            album: self.album,
            album_artist: self.album_artist,
            track_number: self.track_number,
            duration_ms: self.duration_ms,
            bitrate_kbps: self
                .bitrate_kbps
                .and_then(|value| u32::try_from(value).ok())
                .filter(|value| *value > 0),
            size_bytes,
            source_mtime: metadata
                .modified
                .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                .and_then(|duration| i64::try_from(duration.as_secs()).ok())
                .unwrap_or(0),
        })
    }

    fn unavailable(self) -> MirrorTrack {
        MirrorTrack::Unavailable(UnavailableTrack {
            track_id: self.id,
            title: self.title,
            artist: self.artist,
            duration_ms: self.duration_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::path::Path;

    use crate::library::source::{
        LibraryDirectoryEntry, LibraryPathMetadata, LibraryReadHandle, LibraryWalkOrder,
        LibraryWalkVisitor,
    };

    struct PresentSource;

    fn smart_sort_fields_from_query_whitelist() -> Vec<String> {
        let whitelist = include_str!("../queries/clauses.rs")
            .split_once("const SORT_WHITELIST")
            .unwrap()
            .1
            .split_once("= [")
            .unwrap()
            .1
            .split_once("];")
            .unwrap()
            .0;
        let literals = whitelist.split('"').skip(1).step_by(2).collect::<Vec<_>>();
        assert_eq!(literals.len() % 2, 0, "sort whitelist tuples changed shape");
        literals
            .into_iter()
            .step_by(2)
            .filter(|field| *field != "playlist_order")
            .map(str::to_owned)
            .collect()
    }

    fn query_member_order(sort_field: &str, sort_dir: &str) -> String {
        crate::queries::build_track_ids_query(sort_field, sort_dir, false)
            .split_once(" ORDER BY ")
            .unwrap()
            .1
            .rsplit_once(" LIMIT ")
            .unwrap()
            .0
            .to_owned()
    }

    impl LibrarySource for PresentSource {
        fn residence_token(&self, _at: &Path) -> Option<i64> {
            None
        }

        fn mount_point(&self, _at: &Path) -> Option<PathBuf> {
            None
        }

        fn display_name(&self, at: &Path) -> Option<String> {
            at.file_name()?.to_str().map(str::to_owned)
        }

        fn container_name(&self, _at: &Path) -> Option<String> {
            None
        }

        fn relative_path(&self, root: &Path, at: &Path) -> Option<PathBuf> {
            at.strip_prefix(root).ok().map(Path::to_path_buf)
        }

        fn open_read(&self, _at: &Path) -> std::io::Result<LibraryReadHandle> {
            Ok(LibraryReadHandle::new(Cursor::new(Vec::new())))
        }

        fn probe(&self, _at: &Path, _links: LibraryLinkMode) -> LibraryPathPresence {
            LibraryPathPresence::Present(LibraryPathMetadata {
                is_file: true,
                is_directory: false,
                size: Some(100),
                modified: None,
                identity: None,
            })
        }

        fn read_directory(&self, _directory: &Path) -> Option<Vec<LibraryDirectoryEntry>> {
            None
        }

        fn walk(
            &self,
            _root: &Path,
            _order: LibraryWalkOrder,
            _visitor: &mut dyn LibraryWalkVisitor,
        ) {
        }
    }

    #[test]
    fn mtp_51_everything_is_exposed_only_by_the_picker_not_as_a_playlist() {
        let db = crate::db::Db::open_in_memory().unwrap();

        let snapshots = load_mirror_playlist_snapshots(&db).unwrap();

        assert!(
            snapshots
                .iter()
                .all(|snapshot| snapshot.source != super::super::selection::EVERYTHING_SOURCE),
            "the ordinary playlist projection must not gain the picker's synthetic Everything row"
        );
    }

    #[test]
    fn smart_stability_order_matches_every_smart_query_sort() {
        let mut sort_fields = smart_sort_fields_from_query_whitelist();
        sort_fields.push("unknown-field".to_owned());

        for sort_field in sort_fields {
            for sort_dir in ["asc", "desc"] {
                assert_eq!(
                    smart_member_order(&sort_field, sort_dir),
                    query_member_order(&sort_field, sort_dir),
                    "smart stability ordering drifted for {sort_field} {sort_dir}"
                );
            }
        }
    }

    #[test]
    fn capped_smart_snapshot_keeps_ten_ranked_members_only_for_resident_stability() {
        let db = crate::db::Db::open_in_memory().unwrap();
        for id in 1..=13_i64 {
            db.conn()
                .execute(
                    "INSERT INTO tracks \
                     (id, path, title, artist, album, album_artist, duration_ms, rating, play_count, added_at) \
                     VALUES (?1, ?2, ?3, 'Artist', 'Album', 'Artist', 1000, 5, ?1, 1)",
                    rusqlite::params![id, format!("/music/{id}.mp3"), format!("Track {id}")],
                )
                .unwrap();
        }
        db.conn()
            .execute(
                "INSERT INTO tracks \
                 (id, path, title, artist, album, album_artist, duration_ms, rating, play_count, added_at) \
                 VALUES (99, '/music/99.mp3', 'Rule failure', 'Artist', 'Album', 'Artist', \
                         1000, 0, 99, 1)",
                [],
            )
            .unwrap();
        let smart_id = crate::library::playlists::create_smart(
            &db,
            "Capped",
            r#"[{"field":"rating","op":">=","value":1}]"#,
            "play_count",
            "desc",
            Some(2),
        )
        .unwrap();

        let snapshots = load_mirror_playlist_snapshots_with_source(&PresentSource, &db).unwrap();
        let snapshot = snapshots
            .iter()
            .find(|snapshot| snapshot.source == SelectionSource::Smart(smart_id))
            .unwrap();
        let desired_ids = snapshot
            .entries
            .iter()
            .map(|entry| match entry {
                MirrorTrack::Available(track) => track.id,
                MirrorTrack::Unavailable(track) => track.track_id,
            })
            .collect::<Vec<_>>();

        assert_eq!(desired_ids, vec![12, 13], "the addition cap stays at two");
        assert_eq!(snapshot.stability_margin_track_ids.len(), 10);
        assert!(snapshot.stability_margin_track_ids.contains(&2));
        assert!(snapshot.stability_margin_track_ids.contains(&11));
        assert!(!snapshot.stability_margin_track_ids.contains(&1));
        assert!(!snapshot.stability_margin_track_ids.contains(&99));
    }
}
