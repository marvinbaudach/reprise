use std::path::{Path, PathBuf};

use lofty::prelude::*;
use lofty::tag::ItemKey;
use rusqlite::Connection;

use super::*;

fn migrated_connection() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn
}

fn fixture_copy(dir: &Path, name: &str) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let destination = dir.join(name);
    std::fs::copy(source, &destination).unwrap();
    destination
}

fn write_tags(
    path: &Path,
    title: &str,
    artist: &str,
    album: &str,
    album_artist: &str,
    genre: &str,
) {
    let mut tagged = lofty::read_from_path(path).unwrap();
    let tag = tagged.primary_tag_mut().unwrap();
    tag.set_title(title.to_owned());
    tag.set_artist(artist.to_owned());
    tag.set_album(album.to_owned());
    if album_artist.is_empty() {
        tag.remove_key(ItemKey::AlbumArtist);
    } else {
        tag.insert_text(ItemKey::AlbumArtist, album_artist.to_owned());
    }
    tag.set_genre(genre.to_owned());
    tag.save_to_path(path, lofty::config::WriteOptions::default())
        .unwrap();
}

fn insert_track(conn: &Connection, id: i64, path: &Path, database_artist: &str) {
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, album_artist, genre, added_at, file_mtime, file_size) \
         VALUES (?1, ?2, 'Database title', ?3, 'Database album', '', 'Database genre', 0, 0, 0)",
        rusqlite::params![id, path.to_string_lossy(), database_artist],
    )
    .unwrap();
}

fn scan_selection(conn: &mut Connection, ids: Vec<i64>) -> DoctorScan {
    let mut doctor = LibraryDoctor::new(conn);
    match doctor
        .scan_local(
            &LocalScanRequest {
                scope: DoctorScopeRequest::Selection { track_ids: ids },
            },
            |_| ScanControl::Continue,
        )
        .unwrap()
    {
        DoctorScanOutcome::Completed(scan) => scan,
        outcome => panic!("expected a completed scan, got {outcome:?}"),
    }
}

#[test]
fn doc_1a_scan_is_readonly() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_copy(dir.path(), "readonly.flac");
    write_tags(
        &path,
        "  Actual title  ",
        "Actual Artist",
        "Actual Album",
        "Actual Artist",
        "Actual Genre",
    );
    let before = std::fs::read(&path).unwrap();
    let mut conn = migrated_connection();
    insert_track(&conn, 1, &path, "Stale database artist");

    let scan = scan_selection(&mut conn, vec![1]);

    assert_eq!(std::fs::read(&path).unwrap(), before);
    let database_artist: String = conn
        .query_row("SELECT artist FROM tracks WHERE id=1", [], |row| row.get(0))
        .unwrap();
    assert_eq!(database_artist, "Stale database artist");
    let title = scan
        .proposals
        .iter()
        .find(|proposal| proposal.field == DoctorField::Title)
        .unwrap();
    assert_eq!(title.current, DoctorValue::Text("  Actual title  ".into()));
    assert_eq!(title.proposed, DoctorValue::Text("Actual title".into()));
}

#[test]
fn doc_1a_local_rules_never_invent_spelling() {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = migrated_connection();
    for (id, artist) in [(1, "AC/DC"), (2, "AC/DC"), (3, "ac/dc")] {
        let path = fixture_copy(dir.path(), &format!("{id}.flac"));
        write_tags(&path, "Track", artist, "Album", artist, "Rock");
        insert_track(&conn, id, &path, "stale");
    }

    let scan = scan_selection(&mut conn, vec![1, 2, 3]);

    let artist_fixes = scan
        .proposals
        .iter()
        .filter(|proposal| proposal.field == DoctorField::Artist)
        .collect::<Vec<_>>();
    assert_eq!(artist_fixes.len(), 1);
    assert_eq!(artist_fixes[0].track_id, 3);
    assert_eq!(artist_fixes[0].proposed, DoctorValue::Text("AC/DC".into()));
    assert!(artist_fixes[0].preselected);
    assert_eq!(artist_fixes[0].source, ProposalSource::Local);
}

#[test]
fn doc_1a_edge_trim_survives_an_empty_normalized_group_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_copy(dir.path(), "combining.flac");
    write_tags(&path, "Track", " \u{301} ", "Album", "Artist", "Rock");
    let mut conn = migrated_connection();
    insert_track(&conn, 1, &path, "stale");

    let scan = scan_selection(&mut conn, vec![1]);

    let proposal = scan
        .proposals
        .iter()
        .find(|proposal| proposal.field == DoctorField::Artist)
        .unwrap();
    assert_eq!(proposal.current, DoctorValue::Text(" \u{301} ".into()));
    assert_eq!(proposal.proposed, DoctorValue::Text("\u{301}".into()));
}

#[test]
fn local_tie_is_visible_without_a_default_proposal() {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = migrated_connection();
    for (id, artist) in [(1, "CHVRCHES"), (2, "chvrches")] {
        let path = fixture_copy(dir.path(), &format!("tie-{id}.flac"));
        write_tags(&path, "Track", artist, "Album", artist, "Rock");
        insert_track(&conn, id, &path, "stale");
    }

    let scan = scan_selection(&mut conn, vec![1, 2]);

    assert!(!scan
        .proposals
        .iter()
        .any(|proposal| proposal.field == DoctorField::Artist));
    let group = scan
        .unresolved_groups
        .iter()
        .find(|group| group.field == DoctorField::Artist)
        .unwrap();
    assert_eq!(
        group.candidates,
        vec![
            DoctorCandidate {
                value: DoctorValue::Text("CHVRCHES".into()),
                count: 1,
            },
            DoctorCandidate {
                value: DoctorValue::Text("chvrches".into()),
                count: 1,
            },
        ]
    );
    assert_eq!(
        group.members,
        vec![
            DoctorGroupMember {
                track_id: 1,
                current: DoctorValue::Text("CHVRCHES".into()),
            },
            DoctorGroupMember {
                track_id: 2,
                current: DoctorValue::Text("chvrches".into()),
            },
        ]
    );
}

#[test]
fn missing_album_artist_uses_same_track_artist() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_copy(dir.path(), "album-artist.flac");
    write_tags(&path, "Track", "deadmau5", "Album", "", "Electronic");
    let mut conn = migrated_connection();
    insert_track(&conn, 1, &path, "stale");

    let scan = scan_selection(&mut conn, vec![1]);

    let proposal = scan
        .proposals
        .iter()
        .find(|proposal| proposal.field == DoctorField::AlbumArtist)
        .unwrap();
    assert_eq!(proposal.current, DoctorValue::Empty);
    assert_eq!(proposal.proposed, DoctorValue::Text("deadmau5".into()));
    assert_eq!(proposal.problem_class, ProblemClass::MissingAlbumArtist);
}

#[test]
fn doc_2a_scope_freezes_present_track_ids() {
    let mut conn = migrated_connection();
    let transaction = conn.transaction().unwrap();
    {
        let mut insert = transaction
            .prepare("INSERT INTO tracks (id, path, title, added_at) VALUES (?1, ?2, 'Track', 0)")
            .unwrap();
        for id in 1..=10_025_i64 {
            insert
                .execute(rusqlite::params![id, format!("/fixture/{id}.flac")])
                .unwrap();
        }
    }
    transaction.commit().unwrap();
    conn.execute(
        "UPDATE tracks SET missing_since=1, missing_reason='deleted' WHERE id=5000",
        [],
    )
    .unwrap();
    let snapshot = DoctorViewSnapshot {
        source: crate::view_source::ViewSource::Library,
        sort_field: "title".into(),
        sort_dir: "asc".into(),
        filter: String::new(),
        browse: crate::queries::BrowseFilter::default(),
        queue_ids: Vec::new(),
    };
    let mut doctor = LibraryDoctor::new(&mut conn);

    let scope = doctor
        .freeze_scope(&DoctorScopeRequest::CurrentView(Box::new(snapshot)))
        .unwrap();

    let FrozenScope::Tracks(tracks) = scope else {
        panic!("a populated current view must freeze");
    };
    assert_eq!(tracks.len(), 10_024);
    assert_eq!(tracks.first().unwrap().track_id, 1);
    assert_eq!(tracks.last().unwrap().track_id, 10_025);
    assert!(!tracks.iter().any(|track| track.track_id == 5000));
}

#[test]
fn current_queue_scope_crosses_a_stale_page_and_rejects_tombstones() {
    let dir = tempfile::tempdir().unwrap();
    let first = fixture_copy(dir.path(), "queue-first.flac");
    let tombstone = fixture_copy(dir.path(), "queue-tombstone.flac");
    let last = fixture_copy(dir.path(), "queue-last.flac");
    let mut conn = migrated_connection();
    insert_track(&conn, 201, &first, "Artist");
    insert_track(&conn, 202, &tombstone, "Artist");
    insert_track(&conn, 203, &last, "Artist");
    conn.execute("UPDATE tracks SET removed_at=1 WHERE id=202", [])
        .unwrap();
    let snapshot = DoctorViewSnapshot {
        source: crate::view_source::ViewSource::Queue,
        sort_field: "title".into(),
        sort_dir: "asc".into(),
        filter: String::new(),
        browse: crate::queries::BrowseFilter::default(),
        queue_ids: (1..=203).collect(),
    };
    let mut doctor = LibraryDoctor::new(&mut conn);

    let scope = doctor
        .freeze_scope(&DoctorScopeRequest::CurrentView(Box::new(snapshot)))
        .unwrap();

    let FrozenScope::Tracks(tracks) = scope else {
        panic!("valid queue rows must freeze");
    };
    assert_eq!(
        tracks
            .iter()
            .map(|track| track.track_id)
            .collect::<Vec<_>>(),
        vec![201, 203]
    );
}

#[test]
fn invalid_context_requires_visible_scope_fallback() {
    let mut conn = migrated_connection();
    let mut doctor = LibraryDoctor::new(&mut conn);
    let scope = doctor
        .freeze_scope(&DoctorScopeRequest::Selection {
            track_ids: vec![44, 44],
        })
        .unwrap();

    assert_eq!(scope, FrozenScope::FallbackRequired);
}

#[test]
fn doc_2a_last_complete_scan_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("doctor.db");
    let track = fixture_copy(dir.path(), "persist.flac");
    write_tags(&track, "  Track  ", "Artist", "Album", "Artist", "Rock");
    {
        let mut conn = crate::db::open(Some(&database)).unwrap();
        crate::db::migrate(&conn).unwrap();
        insert_track(&conn, 7, &track, "stale");
        let scan = scan_selection(&mut conn, vec![7]);
        assert_eq!(scan.checked_tracks, 1);
    }

    let mut reopened = crate::db::open(Some(&database)).unwrap();
    crate::db::migrate(&reopened).unwrap();
    let doctor = LibraryDoctor::new(&mut reopened);
    let restored = doctor.last_complete_scan().unwrap().unwrap();

    assert_eq!(restored.track_ids, vec![7]);
    assert_eq!(restored.checked_tracks, 1);
    assert!(restored.created_at > 0);
    assert_eq!(restored.options, DoctorScanOptions::local_only());
    assert_eq!(restored.tracks.len(), 1);
    assert_eq!(restored.tracks[0].tags.as_ref().unwrap().title, "  Track  ");
    assert!(!restored.tracks[0].stale);
    assert!(restored
        .proposals
        .iter()
        .any(|proposal| proposal.field == DoctorField::Title));
}

#[test]
fn reopened_scan_marks_changed_database_identity_stale() {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("doctor-stale.db");
    let track = fixture_copy(dir.path(), "stale.flac");
    write_tags(&track, "Track", "Artist", "Album", "Artist", "Rock");
    {
        let mut conn = crate::db::open(Some(&database)).unwrap();
        crate::db::migrate(&conn).unwrap();
        insert_track(&conn, 9, &track, "Artist");
        conn.execute(
            "UPDATE tracks SET file_mtime=10, file_size=20, device=30, inode=40 WHERE id=9",
            [],
        )
        .unwrap();
        scan_selection(&mut conn, vec![9]);
        conn.execute("UPDATE tracks SET file_mtime=11 WHERE id=9", [])
            .unwrap();
    }

    let mut reopened = crate::db::open(Some(&database)).unwrap();
    crate::db::migrate(&reopened).unwrap();
    let restored = LibraryDoctor::new(&mut reopened)
        .last_complete_scan()
        .unwrap()
        .unwrap();

    assert_eq!(restored.stale_track_ids(), vec![9]);
    assert!(restored.tracks[0].stale);
}

#[test]
fn public_library_doctor_seam_is_available_at_the_crate_root() {
    let mut conn = migrated_connection();
    let _doctor = crate::library_doctor::LibraryDoctor::new(&mut conn);
}

#[test]
fn cancelled_scan_preserves_the_previous_complete_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let first = fixture_copy(dir.path(), "first.flac");
    let second = fixture_copy(dir.path(), "second.flac");
    write_tags(&first, "  First  ", "Artist", "Album", "Artist", "Rock");
    write_tags(&second, "  Second  ", "Artist", "Album", "Artist", "Rock");
    let mut conn = migrated_connection();
    insert_track(&conn, 1, &first, "stale");
    insert_track(&conn, 2, &second, "stale");
    let first_scan = scan_selection(&mut conn, vec![1]);
    let previous_id = first_scan.id;
    let mut doctor = LibraryDoctor::new(&mut conn);

    let outcome = doctor
        .scan_local(
            &LocalScanRequest {
                scope: DoctorScopeRequest::Selection {
                    track_ids: vec![1, 2],
                },
            },
            |_| ScanControl::Cancel,
        )
        .unwrap();

    assert_eq!(
        outcome,
        DoctorScanOutcome::Cancelled {
            previous_scan_id: Some(previous_id),
        }
    );
    assert_eq!(
        doctor.last_complete_scan().unwrap().unwrap().id,
        previous_id
    );
}

#[test]
fn unreadable_files_are_counted_as_skipped_not_checked() {
    let dir = tempfile::tempdir().unwrap();
    let unreadable = dir.path().join("broken.flac");
    std::fs::write(&unreadable, b"not a FLAC container").unwrap();
    let mut conn = migrated_connection();
    insert_track(&conn, 1, &unreadable, "stale");

    let scan = scan_selection(&mut conn, vec![1]);

    assert_eq!(scan.checked_tracks, 0);
    assert_eq!(scan.skipped_tracks, 1);
    assert_eq!(scan.track_ids, vec![1]);
    assert!(scan.proposals.is_empty());
}
