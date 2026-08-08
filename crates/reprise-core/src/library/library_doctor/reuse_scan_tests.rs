use std::path::Path;

use super::super::remote::{RemoteProviderError, RemoteResolution, RemoteResolver};
use super::super::*;
use super::{fixture_copy, insert_track, migrated_connection, write_tags};
use crate::fingerprint::FingerprintBackend;

#[derive(Default)]
struct CountingResolver {
    calls: usize,
    proposal: Option<DoctorProposal>,
    group: Option<DoctorUnresolvedGroup>,
}

impl RemoteResolver for CountingResolver {
    fn resolve_track(
        &mut self,
        _: &RemoteTrackMetadata,
        _: &Path,
        _: Option<&dyn FingerprintBackend>,
        _: Option<&super::super::remote::AlbumMatch>,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> Result<RemoteResolution, RemoteProviderError> {
        self.calls += 1;
        Ok(RemoteResolution {
            proposals: self.proposal.clone().into_iter().collect(),
            groups: self.group.clone().into_iter().collect(),
        })
    }
}

fn scan(db: &crate::db::Db, resolver: &mut dyn RemoteResolver, remote_enabled: bool) -> DoctorScan {
    let request = DoctorScanRequest {
        scope: DoctorScopeRequest::Selection { track_ids: vec![1] },
        options: DoctorScanOptions { remote_enabled },
    };
    match LibraryDoctor::new(db)
        .scan_with_resolver(&request, None, resolver, &mut |_| ScanControl::Continue)
        .unwrap()
    {
        DoctorScanOutcome::Completed(scan) => scan,
        outcome => panic!("expected a completed scan, got {outcome:?}"),
    }
}

fn fixture() -> (tempfile::TempDir, crate::db::Db, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_copy(dir.path(), "unchanged.flac");
    write_tags(&path, "  Track  ", "Artist", "Album", "Artist", "Rock");
    let db = migrated_connection();
    insert_track(&db, 1, &path, "Artist");
    (dir, db, path)
}

#[test]
fn doc_1g_a_second_scan_of_an_unchanged_library_reads_no_file() {
    let (_dir, db, path) = fixture();
    let mut resolver = CountingResolver::default();
    let first = scan(&db, &mut resolver, true);
    assert_eq!(first.checked_tracks, 1);
    resolver.calls = 0;
    std::fs::remove_file(path).unwrap();

    let second = scan(&db, &mut resolver, true);

    assert_eq!(second.checked_tracks, 1);
    assert_eq!(second.skipped_tracks, 0);
    assert_eq!(resolver.calls, 0);
}

#[test]
fn doc_1g_a_changed_file_is_read_again() {
    let (_dir, db, path) = fixture();
    let mut resolver = CountingResolver::default();
    scan(&db, &mut resolver, false);
    std::fs::remove_file(path).unwrap();
    db.conn()
        .execute("UPDATE tracks SET file_mtime=1 WHERE id=1", [])
        .unwrap();

    let second = scan(&db, &mut resolver, false);

    assert_eq!(second.checked_tracks, 0);
    assert_eq!(second.skipped_tracks, 1);
}

#[test]
fn doc_1g_a_skipped_track_keeps_its_previous_proposals() {
    let (_dir, db, path) = fixture();
    let mut resolver = CountingResolver {
        proposal: Some(DoctorProposal {
            track_id: 0,
            field: DoctorField::Title,
            current: DoctorValue::Text("  Track  ".into()),
            proposed: DoctorValue::Text("Canonical track".into()),
            source: ProposalSource::MusicBrainz,
            confidence: 91,
            preselected: false,
            never_preselect: false,
            problem_class: ProblemClass::CasingWhitespace,
            resolved_release_mbid: None,
            evidence: Vec::new(),
            local_fallback: None,
        }),
        group: Some(DoctorUnresolvedGroup {
            field: DoctorField::Artist,
            group_key: "remote-artist-choice".into(),
            candidates: vec![DoctorCandidate {
                value: DoctorValue::Text("Canonical artist".into()),
                count: 1,
                evidence: vec![RemoteEvidence {
                    source: RemoteEvidenceSource::MusicBrainz,
                    confidence: 80,
                    recording_mbid: None,
                    release_mbid: None,
                    release_group_mbid: None,
                    artist_mbid: None,
                    release_artist_mbid: None,
                    title: None,
                    artist: Some("Canonical artist".into()),
                    album: None,
                    year: None,
                    duration_ms: None,
                    duration_delta_ms: None,
                }],
            }],
            members: vec![DoctorGroupMember {
                track_id: 0,
                current: DoctorValue::Text("Artist".into()),
            }],
            local_fallback: None,
        }),
        ..Default::default()
    };
    let first = scan(&db, &mut resolver, true);
    assert!(!first.proposals.is_empty());
    assert!(first
        .proposals
        .iter()
        .any(|proposal| proposal.source == ProposalSource::MusicBrainz));
    resolver.calls = 0;
    std::fs::remove_file(path).unwrap();

    let second = scan(&db, &mut resolver, true);

    assert_eq!(second.proposals, first.proposals);
    assert_eq!(second.unresolved_groups, first.unresolved_groups);
    assert_eq!(resolver.calls, 0);
}
