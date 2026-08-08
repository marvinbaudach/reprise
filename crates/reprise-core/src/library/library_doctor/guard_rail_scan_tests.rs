use super::super::remote::{
    ProviderRemoteResolver, RemoteDirectLookup, RemoteEvidenceSource, RemoteIdentity,
    RemoteProvider, RemoteProviderResult, RemoteTrackMetadata,
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
                    "Various Artists"
                } else {
                    "As I Lay Dying"
                }
                .into(),
            ),
            release_year: Some(2007),
            original_release_year: Some(2007),
            duration_ms: None,
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

fn compilation_scan(truncate_title: bool) -> DoctorScan {
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
    let mut resolver = ProviderRemoteResolver::new(CompilationProvider { truncate_title });
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
    scan
}

#[test]
fn doc_1f_a_single_artist_album_on_a_compilation_produces_no_album_artist_proposal() {
    let scan = compilation_scan(false);

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
fn doc_4c_a_capped_row_reaches_the_review_unselected() {
    let scan = compilation_scan(true);
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
