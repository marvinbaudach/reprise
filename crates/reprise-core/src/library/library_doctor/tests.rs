use std::path::{Path, PathBuf};

use lofty::prelude::*;
use lofty::tag::ItemKey;
use rusqlite::Connection;

use super::remote::{RemoteProviderError, RemoteResolution, RemoteResolver, RemoteTrackMetadata};
use super::*;
use crate::fingerprint::FingerprintBackend;

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

#[derive(Default)]
struct CapturingRemoteResolver {
    metadata: Vec<RemoteTrackMetadata>,
}

struct CollisionRemoteResolver;

impl RemoteResolver for CollisionRemoteResolver {
    fn resolve_track(
        &mut self,
        metadata: &RemoteTrackMetadata,
        _: &Path,
        _: Option<&dyn FingerprintBackend>,
        control: &mut dyn FnMut() -> ScanControl,
    ) -> Result<RemoteResolution, RemoteProviderError> {
        let _ = control();
        Ok(RemoteResolution {
            proposals: vec![DoctorProposal {
                track_id: 0,
                field: DoctorField::Title,
                current: DoctorValue::decode(DoctorField::Title, metadata.title.clone()),
                proposed: DoctorValue::Text("Remote canonical title".into()),
                source: ProposalSource::MusicBrainz,
                confidence: 100,
                preselected: false,
                problem_class: ProblemClass::CasingWhitespace,
                evidence: Vec::new(),
                local_fallback: None,
            }],
            groups: Vec::new(),
        })
    }
}

struct YearGroupResolver;

impl RemoteResolver for YearGroupResolver {
    fn resolve_track(
        &mut self,
        _: &RemoteTrackMetadata,
        _: &Path,
        _: Option<&dyn FingerprintBackend>,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> Result<RemoteResolution, RemoteProviderError> {
        let evidence = RemoteEvidence {
            source: RemoteEvidenceSource::MusicBrainz,
            confidence: 80,
            recording_mbid: None,
            release_mbid: None,
            release_group_mbid: None,
            artist_mbid: None,
            release_artist_mbid: None,
            title: None,
            artist: None,
            album: None,
            year: Some(2024),
            duration_ms: None,
            duration_delta_ms: None,
        };
        Ok(RemoteResolution {
            proposals: Vec::new(),
            groups: vec![DoctorUnresolvedGroup {
                field: DoctorField::Year,
                group_key: "remote:year".into(),
                candidates: vec![
                    DoctorCandidate {
                        value: DoctorValue::Year(2024),
                        count: 1,
                        evidence: vec![evidence.clone()],
                    },
                    DoctorCandidate {
                        value: DoctorValue::Year(2023),
                        count: 1,
                        evidence: vec![evidence],
                    },
                ],
                members: vec![DoctorGroupMember {
                    track_id: 0,
                    current: DoctorValue::Empty,
                }],
                local_fallback: None,
            }],
        })
    }
}

impl RemoteResolver for CapturingRemoteResolver {
    fn resolve_track(
        &mut self,
        metadata: &RemoteTrackMetadata,
        _: &Path,
        _: Option<&dyn FingerprintBackend>,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> Result<RemoteResolution, RemoteProviderError> {
        self.metadata.push(metadata.clone());
        Ok(RemoteResolution {
            proposals: vec![DoctorProposal {
                track_id: 0,
                field: DoctorField::Year,
                current: DoctorValue::Empty,
                proposed: DoctorValue::Year(2024),
                source: ProposalSource::MusicBrainz,
                confidence: 88,
                preselected: false,
                problem_class: ProblemClass::MissingWrongYear,
                evidence: vec![RemoteEvidence {
                    source: RemoteEvidenceSource::MusicBrainz,
                    confidence: 88,
                    recording_mbid: None,
                    release_mbid: None,
                    release_group_mbid: None,
                    artist_mbid: None,
                    release_artist_mbid: None,
                    title: Some("Actual title".into()),
                    artist: Some("Actual Artist".into()),
                    album: Some("Actual Album".into()),
                    year: Some(2024),
                    duration_ms: None,
                    duration_delta_ms: None,
                }],
                local_fallback: None,
            }],
            groups: Vec::new(),
        })
    }
}

#[test]
fn remote_scan_uses_actual_allowlisted_tags_and_persists_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_copy(dir.path(), "database-placeholder.flac");
    write_tags(
        &path,
        "Actual title",
        "Actual Artist",
        "Actual Album",
        "Actual Artist",
        "Rock",
    );
    let mut conn = migrated_connection();
    insert_track(&conn, 1, &path, "Database placeholder");
    let mut resolver = CapturingRemoteResolver::default();

    let outcome = LibraryDoctor::new(&mut conn)
        .scan_with_resolver(
            &DoctorScanRequest {
                scope: DoctorScopeRequest::Selection { track_ids: vec![1] },
                options: DoctorScanOptions {
                    remote_enabled: true,
                },
            },
            None,
            &mut resolver,
            &mut |_| ScanControl::Continue,
        )
        .unwrap();

    let DoctorScanOutcome::Completed(scan) = outcome else {
        panic!("scan must complete")
    };
    assert_eq!(resolver.metadata[0].title.as_deref(), Some("Actual title"));
    assert!(!serde_json::to_string(&resolver.metadata[0])
        .unwrap()
        .contains("placeholder"));
    let restored = LibraryDoctor::new(&mut conn)
        .last_complete_scan()
        .unwrap()
        .unwrap();
    assert_eq!(restored.proposals, scan.proposals);
    assert_eq!(restored.proposals.last().unwrap().evidence.len(), 1);
}

#[test]
fn remote_merge_keeps_one_active_row_and_preserves_the_local_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_copy(dir.path(), "collision.flac");
    write_tags(
        &path,
        "  Local title  ",
        "Artist",
        "Album",
        "Artist",
        "Rock",
    );
    let mut conn = migrated_connection();
    insert_track(&conn, 1, &path, "Artist");

    let outcome = LibraryDoctor::new(&mut conn)
        .scan_with_resolver(
            &DoctorScanRequest {
                scope: DoctorScopeRequest::Selection { track_ids: vec![1] },
                options: DoctorScanOptions {
                    remote_enabled: true,
                },
            },
            None,
            &mut CollisionRemoteResolver,
            &mut |_| ScanControl::Continue,
        )
        .unwrap();
    let DoctorScanOutcome::Completed(scan) = outcome else {
        panic!("scan must complete")
    };
    let active = scan
        .proposals
        .iter()
        .filter(|proposal| proposal.track_id == 1 && proposal.field == DoctorField::Title)
        .collect::<Vec<_>>();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].source, ProposalSource::MusicBrainz);
    assert_eq!(
        active[0].local_fallback,
        Some(DoctorLocalFallback::Proposal {
            proposed: DoctorValue::Text("Local title".into()),
            confidence: 100,
            problem_class: ProblemClass::CasingWhitespace,
        })
    );
    assert!(!scan.unresolved_groups.iter().any(|group| {
        group.field == DoctorField::Title && group.members.iter().any(|member| member.track_id == 1)
    }));
    let restored = LibraryDoctor::new(&mut conn)
        .last_complete_scan()
        .unwrap()
        .unwrap();
    assert_eq!(
        restored.proposals[0].local_fallback,
        active[0].local_fallback
    );
}

#[test]
fn combined_scan_progress_is_monotonic_and_completes_after_remote_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = migrated_connection();
    for id in 1..=2 {
        let path = fixture_copy(dir.path(), &format!("progress-{id}.flac"));
        write_tags(&path, "Title", "Artist", "Album", "Artist", "Rock");
        insert_track(&conn, id, &path, "Artist");
    }
    let mut progress = Vec::new();
    let outcome = LibraryDoctor::new(&mut conn)
        .scan_with_resolver(
            &DoctorScanRequest {
                scope: DoctorScopeRequest::Selection {
                    track_ids: vec![1, 2],
                },
                options: DoctorScanOptions {
                    remote_enabled: true,
                },
            },
            None,
            &mut CollisionRemoteResolver,
            &mut |item| {
                progress.push((item.completed_tracks, item.total_tracks));
                ScanControl::Continue
            },
        )
        .unwrap();
    assert!(matches!(outcome, DoctorScanOutcome::Completed(_)));
    assert!(progress.windows(2).all(|pair| pair[0].0 <= pair[1].0));
    assert_eq!(progress.last(), Some(&(2, 2)));
    assert_eq!(progress.iter().filter(|item| **item == (2, 2)).count(), 1);
}

#[test]
fn remote_year_manual_candidates_roundtrip_with_typed_values_and_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_copy(dir.path(), "year-group.flac");
    write_tags(&path, "Title", "Artist", "Album", "Artist", "Rock");
    let mut conn = migrated_connection();
    insert_track(&conn, 1, &path, "Artist");
    LibraryDoctor::new(&mut conn)
        .scan_with_resolver(
            &DoctorScanRequest {
                scope: DoctorScopeRequest::Selection { track_ids: vec![1] },
                options: DoctorScanOptions {
                    remote_enabled: true,
                },
            },
            None,
            &mut YearGroupResolver,
            &mut |_| ScanControl::Continue,
        )
        .unwrap();
    let restored = LibraryDoctor::new(&mut conn)
        .last_complete_scan()
        .unwrap()
        .unwrap();
    let group = restored
        .unresolved_groups
        .iter()
        .find(|group| group.field == DoctorField::Year)
        .unwrap();
    assert_eq!(group.candidates[0].value, DoctorValue::Year(2024));
    assert_eq!(group.candidates[0].evidence[0].year, Some(2024));
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
                evidence: Vec::new(),
            },
            DoctorCandidate {
                value: DoctorValue::Text("chvrches".into()),
                count: 1,
                evidence: Vec::new(),
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
