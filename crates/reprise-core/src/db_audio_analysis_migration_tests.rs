//! Local audio-analysis schema migration regressions.

use super::*;

fn reset_fully_migrated_database_to_v17(conn: &Connection) {
    conn.execute_batch(
        "DROP TRIGGER tag_write_journal_identity_immutable;
         DROP TABLE tag_write_journal;
         DROP TABLE tag_write_job_files;
         DROP TABLE tag_write_jobs;
         DROP TABLE library_doctor_state;
         DROP TABLE library_doctor_group_members;
         DROP TABLE library_doctor_group_candidates;
         DROP TABLE library_doctor_groups;
         DROP TABLE library_doctor_proposals;
         DROP TABLE library_doctor_scan_tracks;
         DROP TABLE library_doctor_scans;
         DROP TABLE track_audio_analysis;
         PRAGMA user_version = 17;",
    )
    .unwrap();
}

#[test]
fn audio_analysis_schema_migrates_v17_and_preserves_library_data() {
    let conn = open(None).unwrap();
    migrate(&conn).unwrap();
    reset_fully_migrated_database_to_v17(&conn);
    conn.execute_batch(
        "INSERT INTO tracks
           (id, path, title, artist, added_at, file_mtime, file_size, waveform_peaks)
         VALUES (1, '/fixture.flac', 'Fixture', 'Artist', 1, 20, 30, X'0102');
         INSERT INTO playlists (id, name, position) VALUES (1, 'Keep', 0);
         INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (1, 1, 0);
         INSERT INTO listen_events (id, track_id, played_at, ms_played)
         VALUES (1, 1, 40, 50);",
    )
    .unwrap();

    migrate(&conn).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 21);
    let preserved: (String, Vec<u8>, i64, i64) = conn
        .query_row(
            "SELECT t.title, t.waveform_peaks,
                    (SELECT COUNT(*) FROM playlist_tracks WHERE track_id = t.id),
                    (SELECT COUNT(*) FROM listen_events WHERE track_id = t.id)
             FROM tracks t WHERE t.id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(preserved, ("Fixture".into(), vec![1, 2], 1, 1));

    let columns = conn
        .prepare("PRAGMA table_info(track_audio_analysis)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert!(columns.contains(&"extractor_version".to_owned()));
    assert!(columns.contains(&"profile_version".to_owned()));
    assert!(columns.contains(&"failure_kind".to_owned()));
}

#[test]
fn fresh_and_upgraded_databases_have_the_same_audio_analysis_schema() {
    let fresh = open(None).unwrap();
    migrate(&fresh).unwrap();
    let upgraded = open(None).unwrap();
    migrate(&upgraded).unwrap();
    reset_fully_migrated_database_to_v17(&upgraded);
    migrate(&upgraded).unwrap();

    let schema = |conn: &Connection| {
        conn.query_row(
            "SELECT sql FROM sqlite_master
             WHERE type = 'table' AND name = 'track_audio_analysis'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap()
    };
    assert_eq!(schema(&fresh), schema(&upgraded));
}
