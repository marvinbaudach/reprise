use super::super::remote::{
    AlbumMatch, ProviderRemoteResolver, RemoteDirectLookup, RemoteEvidenceSource, RemoteIdentity,
    RemoteProvider, RemoteProviderError, RemoteProviderResult, RemoteResolution, RemoteResolver,
    RemoteTrackMetadata,
};
use super::super::*;
use super::{fixture_copy, insert_track, migrated_connection, write_tags};

const ORIGINAL_RELEASE_MBID: &str = "123e4567-e89b-12d3-a456-426614174001";
const ORIGINAL_GROUP_MBID: &str = "123e4567-e89b-12d3-a456-426614174002";
const ORIGINAL_ARTIST_MBID: &str = "123e4567-e89b-12d3-a456-426614174003";
const COMPILATION_RELEASE_MBID: &str = "223e4567-e89b-12d3-a456-426614174001";
const COMPILATION_GROUP_MBID: &str = "223e4567-e89b-12d3-a456-426614174002";
const RECORDING_MBID: &str = "123e4567-e89b-12d3-a456-426614174000";

struct CompilationProvider {
    truncate_title: bool,
    named_compilation_credit: bool,
    release_calls: usize,
}

struct AgreeingRemoteResolver;

impl RemoteResolver for AgreeingRemoteResolver {
    fn resolve_track(
        &mut self,
        metadata: &RemoteTrackMetadata,
        _: &std::path::Path,
        _: Option<&dyn crate::fingerprint::FingerprintBackend>,
        _: Option<&AlbumMatch>,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> Result<RemoteResolution, RemoteProviderError> {
        Ok(RemoteResolution {
            proposals: vec![DoctorProposal {
                track_id: 0,
                field: DoctorField::Title,
                current: DoctorValue::decode(DoctorField::Title, metadata.title.clone()),
                proposed: DoctorValue::Text(
                    metadata
                        .title
                        .as_deref()
                        .unwrap_or_default()
                        .trim()
                        .to_owned(),
                ),
                source: ProposalSource::MusicBrainz,
                confidence: 100,
                preselected: false,
                never_preselect: false,
                problem_class: ProblemClass::CasingWhitespace,
                resolved_release_mbid: None,
                evidence: Vec::new(),
                local_fallback: None,
            }],
            groups: Vec::new(),
        })
    }
}

impl CompilationProvider {
    fn identity(&self, metadata: &RemoteTrackMetadata, compilation: bool) -> RemoteIdentity {
        let title = metadata.title.as_deref().map(|title| {
            if self.truncate_title {
                title.strip_suffix(" extended").unwrap_or(title).to_owned()
            } else {
                title.to_owned()
            }
        });
        RemoteIdentity {
            source: RemoteEvidenceSource::MusicBrainz,
            confidence: 100,
            recording_mbid: Some(RECORDING_MBID.into()),
            release_mbid: Some(
                if compilation {
                    COMPILATION_RELEASE_MBID
                } else {
                    ORIGINAL_RELEASE_MBID
                }
                .into(),
            ),
            release_group_mbid: Some(
                if compilation {
                    COMPILATION_GROUP_MBID
                } else {
                    ORIGINAL_GROUP_MBID
                }
                .into(),
            ),
            artist_mbid: Some(ORIGINAL_ARTIST_MBID.into()),
            release_artist_mbid: Some(
                if compilation {
                    super::super::remote::guard_rails::VARIOUS_ARTISTS_MBID
                } else {
                    ORIGINAL_ARTIST_MBID
                }
                .into(),
            ),
            title,
            artist: Some("As I Lay Dying".into()),
            album: Some("An Ocean Between Us".into()),
            album_artist: Some(
                if compilation {
                    if self.named_compilation_credit {
                        "Metal Hammer"
                    } else {
                        "Various Artists"
                    }
                } else {
                    "As I Lay Dying"
                }
                .into(),
            ),
            release_year: Some(2007),
            original_release_year: Some(2007),
            duration_ms: None,
            secondary_types: if compilation {
                vec![super::super::remote::ReleaseSecondaryType::Compilation]
            } else {
                Vec::new()
            },
            release_track_count: Some(if compilation { 28 } else { 10 }),
            release_track_titles: (1..=if compilation { 28 } else { 10 })
                .map(|id| format!("Track {id:02} extended"))
                .collect(),
            release_distinct_track_artists: Some(if compilation { 18 } else { 1 }),
        }
    }
}

impl RemoteProvider for CompilationProvider {
    fn direct(
        &mut self,
        _: &RemoteDirectLookup,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        Ok(Vec::new())
    }

    fn search_musicbrainz(
        &mut self,
        metadata: &RemoteTrackMetadata,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        Ok(vec![
            self.identity(metadata, false),
            self.identity(metadata, true),
        ])
    }

    fn search_release(
        &mut self,
        query: &super::super::remote::AlbumQuery,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        self.release_calls += 1;
        let metadata = RemoteTrackMetadata {
            title: query.track_titles.first().cloned(),
            artist: Some(query.album_artist.clone()),
            album: Some(query.album.clone()),
            album_artist: Some(query.album_artist.clone()),
            year: query.year,
            duration_ms: None,
            recording_mbid: None,
            release_mbid: None,
            release_group_mbid: None,
            artist_mbid: None,
            release_artist_mbid: None,
        };
        Ok(vec![
            self.identity(&metadata, false),
            self.identity(&metadata, true),
        ])
    }

    fn acoustid(
        &mut self,
        _: &RemoteTrackMetadata,
        _: &str,
        _: &str,
        _: u64,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        Ok(Vec::new())
    }
}

fn compilation_scan(truncate_title: bool, named_compilation_credit: bool) -> (DoctorScan, usize) {
    let dir = tempfile::tempdir().unwrap();
    let db = migrated_connection();
    let ids = (1..=10).collect::<Vec<_>>();
    for id in &ids {
        let path = fixture_copy(dir.path(), &format!("ocean-{id:02}.flac"));
        write_tags(
            &path,
            &format!("Track {id:02} extended"),
            "As I Lay Dying",
            "An Ocean Between Us",
            "As I Lay Dying",
            "Metalcore",
        );
        insert_track(&db, *id, &path, "As I Lay Dying");
    }
    let mut resolver = ProviderRemoteResolver::new(CompilationProvider {
        truncate_title,
        named_compilation_credit,
        release_calls: 0,
    });
    let outcome = LibraryDoctor::new(&db)
        .scan_with_resolver(
            &DoctorScanRequest {
                scope: DoctorScopeRequest::Selection { track_ids: ids },
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
    (scan, resolver.into_provider().release_calls)
}

#[test]
fn doc_1f_a_single_artist_album_on_a_compilation_produces_no_album_artist_proposal() {
    let (scan, _) = compilation_scan(false, false);

    assert_eq!(
        scan.proposals
            .iter()
            .filter(|proposal| proposal.field == DoctorField::AlbumArtist)
            .count(),
        0
    );
    assert!(!scan.unresolved_groups.iter().any(|group| {
        group.field == DoctorField::AlbumArtist
            && group.candidates.iter().any(|candidate| {
                matches!(&candidate.value, DoctorValue::Text(value) if value == "Various Artists")
            })
    }));
}

#[test]
fn doc_1e_a_single_artist_album_whose_tracks_are_on_a_compilation_produces_no_album_artist_proposal(
) {
    let (scan, release_calls) = compilation_scan(false, true);

    assert_eq!(release_calls, 1, "the album must be resolved exactly once");
    assert!(!scan
        .proposals
        .iter()
        .any(|proposal| proposal.field == DoctorField::AlbumArtist));
}

#[test]
fn doc_1e_the_network_is_asked_once_per_album_not_once_per_track() {
    let (_, release_calls) = compilation_scan(false, false);

    assert_eq!(release_calls, 1);
}

#[test]
fn doc_1e_an_albums_album_fields_all_carry_the_same_resolved_release_mbid() {
    let (scan, _) = compilation_scan(false, false);
    let album_fields = scan
        .proposals
        .iter()
        .filter(|proposal| {
            matches!(
                proposal.field,
                DoctorField::Album | DoctorField::AlbumArtist | DoctorField::Year
            )
        })
        .collect::<Vec<_>>();

    assert!(!album_fields.is_empty());
    assert!(album_fields.iter().all(|proposal| {
        proposal.resolved_release_mbid.as_deref() == Some(ORIGINAL_RELEASE_MBID)
    }));
}

#[test]
fn doc_1a_a_local_and_a_remote_proposal_with_the_same_target_keep_the_local_row() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_copy(dir.path(), "agreeing-collision.flac");
    write_tags(
        &path,
        "  Local title  ",
        "Artist",
        "Album",
        "Artist",
        "Rock",
    );
    let conn = migrated_connection();
    insert_track(&conn, 1, &path, "Artist");

    let outcome = LibraryDoctor::new(&conn)
        .scan_with_resolver(
            &DoctorScanRequest {
                scope: DoctorScopeRequest::Selection { track_ids: vec![1] },
                options: DoctorScanOptions {
                    remote_enabled: true,
                },
            },
            None,
            &mut AgreeingRemoteResolver,
            &mut |_| ScanControl::Continue,
        )
        .unwrap();
    let DoctorScanOutcome::Completed(scan) = outcome else {
        panic!("scan must complete")
    };
    let title_rows = scan
        .proposals
        .iter()
        .filter(|proposal| proposal.field == DoctorField::Title)
        .collect::<Vec<_>>();

    assert_eq!(title_rows.len(), 1);
    assert_eq!(title_rows[0].source, ProposalSource::Local);
    assert_eq!(title_rows[0].local_fallback, None);
}

#[test]
fn doc_4c_a_capped_row_reaches_the_review_unselected() {
    let (scan, _) = compilation_scan(true, false);
    let review = DoctorReviewSession::from_scan(scan, DoctorReviewFilter::NeedsReview);
    let title_rows = review
        .rows()
        .iter()
        .filter(|row| row.field == DoctorField::Title)
        .collect::<Vec<_>>();

    assert_eq!(title_rows.len(), 10);
    assert!(title_rows.iter().all(|row| row.confidence <= 49));
    assert!(title_rows.iter().all(|row| row.never_preselect));
    assert!(title_rows.iter().all(|row| !row.selected));
}
