use super::remote::*;
use super::*;
use crate::fingerprint::{
    Fingerprint, FingerprintBackend, FingerprintCapability, FingerprintControl, FingerprintError,
    FingerprintOutcome, FingerprintProgress,
};
use std::path::Path;

fn metadata() -> RemoteTrackMetadata {
    RemoteTrackMetadata {
        title: Some("Real title".into()),
        artist: Some("Real artist".into()),
        album: Some("Real album".into()),
        album_artist: Some("Real album artist".into()),
        year: None,
        recording_mbid: Some("123e4567-e89b-12d3-a456-426614174000".into()),
        release_mbid: None,
        release_group_mbid: None,
        artist_mbid: None,
        release_artist_mbid: None,
        duration_ms: Some(180_000),
    }
}

fn identity(source: RemoteEvidenceSource, confidence: u8, title: &str) -> RemoteIdentity {
    RemoteIdentity {
        source,
        confidence,
        recording_mbid: Some("123e4567-e89b-12d3-a456-426614174000".into()),
        release_mbid: Some("123e4567-e89b-12d3-a456-426614174001".into()),
        release_group_mbid: Some("123e4567-e89b-12d3-a456-426614174002".into()),
        artist_mbid: Some("123e4567-e89b-12d3-a456-426614174003".into()),
        release_artist_mbid: Some("323e4567-e89b-12d3-a456-426614174003".into()),
        title: Some(title.into()),
        artist: Some("Canonical artist".into()),
        album: Some("Canonical album".into()),
        album_artist: Some("Canonical album artist".into()),
        release_year: Some(2024),
        original_release_year: Some(2023),
        duration_ms: Some(180_500),
    }
}

#[derive(Default)]
struct FakeProvider {
    calls: Vec<&'static str>,
    direct_lookups: Vec<RemoteDirectLookup>,
    direct: Vec<RemoteIdentity>,
    direct_responses: Vec<Vec<RemoteIdentity>>,
    searched: Vec<RemoteIdentity>,
    acoustid: Vec<RemoteIdentity>,
    direct_error: Option<RemoteProviderError>,
    search_error: Option<RemoteProviderError>,
    acoustid_error: Option<RemoteProviderError>,
}

struct FakeFingerprint;

impl FingerprintBackend for FakeFingerprint {
    fn capability(&self) -> FingerprintCapability {
        FingerprintCapability::Available {
            cache_namespace: "test".into(),
        }
    }

    fn fingerprint(
        &self,
        _: &Path,
        progress: &mut dyn FnMut(FingerprintProgress) -> FingerprintControl,
    ) -> Result<FingerprintOutcome, FingerprintError> {
        if progress(FingerprintProgress {
            processed_seconds: 1,
            duration_seconds: Some(180),
        }) == FingerprintControl::Cancel
        {
            return Ok(FingerprintOutcome::Cancelled);
        }
        Ok(FingerprintOutcome::Completed(Fingerprint {
            encoded: "fingerprint".into(),
            duration_seconds: 180,
            cache_namespace: "test".into(),
        }))
    }
}

impl RemoteProvider for FakeProvider {
    fn direct(
        &mut self,
        lookup: &RemoteDirectLookup,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        let response_index = self.direct_lookups.len();
        self.calls.push("direct");
        self.direct_lookups.push(lookup.clone());
        self.direct_error.clone().map_or_else(
            || {
                Ok(self
                    .direct_responses
                    .get(response_index)
                    .unwrap_or(&self.direct)
                    .clone())
            },
            Err,
        )
    }
    fn search_musicbrainz(
        &mut self,
        _: &RemoteTrackMetadata,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        self.calls.push("musicbrainz");
        self.search_error
            .clone()
            .map_or_else(|| Ok(self.searched.clone()), Err)
    }
    fn acoustid(
        &mut self,
        _: &RemoteTrackMetadata,
        _: &str,
        _: u64,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        self.calls.push("acoustid");
        self.acoustid_error
            .clone()
            .map_or_else(|| Ok(self.acoustid.clone()), Err)
    }
}

#[test]
fn doc_1b_embedded_mbids_precede_metadata_and_fingerprint() {
    let mut provider = FakeProvider {
        direct: vec![identity(
            RemoteEvidenceSource::MusicBrainz,
            100,
            "Canonical",
        )],
        ..Default::default()
    };
    let result = resolve_with_provider(&mut provider, &metadata(), Some(("fingerprint", 180)));
    assert_eq!(provider.calls, ["direct"]);
    assert!(result
        .proposals
        .iter()
        .any(|p| p.field == DoctorField::Title));
}

#[test]
fn doc_1b_musicbrainz_precedes_acoustid() {
    let mut value = metadata();
    value.recording_mbid = None;
    let mut provider = FakeProvider {
        searched: vec![identity(RemoteEvidenceSource::MusicBrainz, 91, "Canonical")],
        ..Default::default()
    };
    let _ = resolve_with_provider(&mut provider, &value, Some(("fingerprint", 180)));
    assert_eq!(provider.calls, ["musicbrainz"]);
}

#[test]
fn all_relevant_embedded_mbids_are_resolved_before_metadata_search() {
    let mut value = metadata();
    value.recording_mbid = None;
    value.release_mbid = Some("123e4567-e89b-12d3-a456-426614174001".into());
    value.artist_mbid = Some("123e4567-e89b-12d3-a456-426614174003".into());
    let mut provider = FakeProvider::default();

    let _ = resolve_with_provider(&mut provider, &value, None);

    assert_eq!(provider.calls, ["direct", "direct", "musicbrainz"]);
    assert_eq!(
        provider.direct_lookups,
        [
            RemoteDirectLookup::Release("123e4567-e89b-12d3-a456-426614174001".into()),
            RemoteDirectLookup::Artist("123e4567-e89b-12d3-a456-426614174003".into()),
        ]
    );
}

#[test]
fn production_cascade_exhausts_embedded_ids_before_testing_completeness() {
    let mut value = metadata();
    value.release_mbid = Some("123e4567-e89b-12d3-a456-426614174001".into());
    value.release_group_mbid = Some("123e4567-e89b-12d3-a456-426614174002".into());
    value.artist_mbid = Some("123e4567-e89b-12d3-a456-426614174003".into());
    value.release_artist_mbid = Some("323e4567-e89b-12d3-a456-426614174003".into());

    let complete = identity(RemoteEvidenceSource::MusicBrainz, 100, "Canonical");
    let mut contradictory = complete.clone();
    contradictory.recording_mbid = Some("223e4567-e89b-12d3-a456-426614174000".into());
    contradictory.title = Some("Contradictory".into());
    let provider = FakeProvider {
        direct_responses: vec![
            vec![complete],
            Vec::new(),
            vec![contradictory],
            Vec::new(),
            Vec::new(),
        ],
        ..Default::default()
    };
    let mut resolver = ProviderRemoteResolver::new(provider);

    let result = RemoteResolver::resolve_track(
        &mut resolver,
        &value,
        Path::new("ignored"),
        None,
        &mut || ScanControl::Continue,
    )
    .unwrap();
    let provider = resolver.into_provider();

    assert_eq!(
        provider.direct_lookups,
        [
            RemoteDirectLookup::Recording("123e4567-e89b-12d3-a456-426614174000".into()),
            RemoteDirectLookup::Release("123e4567-e89b-12d3-a456-426614174001".into()),
            RemoteDirectLookup::ReleaseGroup("123e4567-e89b-12d3-a456-426614174002".into()),
            RemoteDirectLookup::Artist("123e4567-e89b-12d3-a456-426614174003".into()),
            RemoteDirectLookup::ReleaseArtist("323e4567-e89b-12d3-a456-426614174003".into()),
        ]
    );
    assert_eq!(
        provider.calls,
        [
            "direct",
            "direct",
            "direct",
            "direct",
            "direct",
            "musicbrainz",
        ]
    );
    assert!(result
        .groups
        .iter()
        .any(|group| group.field == DoctorField::Title));
    assert!(!result
        .proposals
        .iter()
        .any(|proposal| proposal.field == DoctorField::Title));
}

#[test]
fn doc_1b_remote_fields_are_allowlisted() {
    assert_eq!(
        REMOTE_WRITABLE_FIELDS,
        [
            DoctorField::Title,
            DoctorField::Artist,
            DoctorField::Album,
            DoctorField::AlbumArtist,
            DoctorField::Year,
            DoctorField::RecordingMbid
        ]
    );
}

#[test]
fn doc_1b_filename_placeholders_are_never_sent() {
    let value = RemoteTrackMetadata::from_actual_tags(
        "track-07.flac",
        "",
        "",
        "",
        "",
        None,
        &Default::default(),
        Some(1000),
    );
    assert_eq!(value.title, None);
    assert!(!serde_json::to_string(&value).unwrap().contains("track-07"));
}

#[test]
fn doc_1b_mbid_canonical_name_wins_but_stays_remote() {
    let mut provider = FakeProvider {
        direct: vec![identity(
            RemoteEvidenceSource::MusicBrainz,
            100,
            "Canonical",
        )],
        ..Default::default()
    };
    let result = resolve_with_provider(&mut provider, &metadata(), None);
    let proposal = result
        .proposals
        .iter()
        .find(|p| p.field == DoctorField::Title)
        .unwrap();
    assert_eq!(proposal.source, ProposalSource::MusicBrainz);
    assert_eq!(proposal.confidence, 100);
    assert!(!proposal.preselected);
}

#[test]
fn doc_1b_one_value_per_track_field() {
    let mut value = metadata();
    value.recording_mbid = None;
    let mut provider = FakeProvider {
        searched: vec![
            identity(RemoteEvidenceSource::MusicBrainz, 91, "One"),
            identity(RemoteEvidenceSource::MusicBrainz, 80, "Two"),
        ],
        ..Default::default()
    };
    let result = resolve_with_provider(&mut provider, &value, None);
    let titles = result
        .proposals
        .iter()
        .filter(|p| p.field == DoctorField::Title)
        .count();
    assert_eq!(titles, 1);
}

#[test]
fn doc_4a_remote_is_never_preselected() {
    let mut value = metadata();
    value.recording_mbid = None;
    let mut provider = FakeProvider {
        searched: vec![identity(RemoteEvidenceSource::MusicBrainz, 99, "Canonical")],
        ..Default::default()
    };
    assert!(resolve_with_provider(&mut provider, &value, None)
        .proposals
        .iter()
        .all(|p| !p.preselected));
}

#[test]
fn agreeing_sources_retain_both_and_use_lower_confidence() {
    let mb = identity(RemoteEvidenceSource::MusicBrainz, 92, "Canonical");
    let ac = identity(RemoteEvidenceSource::AcoustId, 78, "Canonical");
    let result = arbitrate(&metadata(), &[mb, ac]);
    let title = result
        .proposals
        .iter()
        .find(|p| p.field == DoctorField::Title)
        .unwrap();
    assert_eq!(title.confidence, 78);
    assert_eq!(title.evidence.len(), 2);
}

#[test]
fn conflicting_sources_become_manual_candidates() {
    let one = identity(RemoteEvidenceSource::MusicBrainz, 90, "One");
    let two = identity(RemoteEvidenceSource::AcoustId, 90, "Two");
    let result = arbitrate(&metadata(), &[one, two]);
    assert!(!result
        .proposals
        .iter()
        .any(|p| p.field == DoctorField::Title));
    assert!(result.groups.iter().any(|g| g.field == DoctorField::Title));
}

#[test]
fn differing_musicbrainz_and_acoustid_values_stay_manual_despite_score_lead() {
    let one = identity(RemoteEvidenceSource::MusicBrainz, 90, "One");
    let two = identity(RemoteEvidenceSource::AcoustId, 70, "Two");
    let result = arbitrate(&metadata(), &[one, two]);
    assert!(!result
        .proposals
        .iter()
        .any(|proposal| proposal.field == DoctorField::Title));
    assert!(result.groups.iter().any(|group| {
        group.field == DoctorField::Title
            && group.candidates.iter().any(|candidate| {
                candidate
                    .evidence
                    .iter()
                    .any(|evidence| evidence.source == RemoteEvidenceSource::MusicBrainz)
            })
            && group.candidates.iter().any(|candidate| {
                candidate
                    .evidence
                    .iter()
                    .any(|evidence| evidence.source == RemoteEvidenceSource::AcoustId)
            })
    }));
}

#[test]
fn automatic_match_requires_ten_point_lead() {
    let one = identity(RemoteEvidenceSource::MusicBrainz, 90, "One");
    let two = identity(RemoteEvidenceSource::MusicBrainz, 81, "Two");
    assert!(arbitrate(&metadata(), &[one.clone(), two])
        .groups
        .iter()
        .any(|g| g.field == DoctorField::Title));
    let mut weaker = identity(RemoteEvidenceSource::MusicBrainz, 80, "Two");
    weaker.recording_mbid = Some("223e4567-e89b-12d3-a456-426614174000".into());
    assert!(arbitrate(&metadata(), &[one, weaker])
        .groups
        .iter()
        .any(|g| g.field == DoctorField::Title));

    let top = identity(RemoteEvidenceSource::MusicBrainz, 90, "One");
    let runner_up = identity(RemoteEvidenceSource::MusicBrainz, 80, "Two");
    assert!(arbitrate(&metadata(), &[top, runner_up])
        .proposals
        .iter()
        .any(|proposal| proposal.field == DoctorField::Title
            && proposal.proposed == DoctorValue::Text("One".into())));
}

#[test]
fn duration_and_year_ambiguity_never_auto_select() {
    let mut one = identity(RemoteEvidenceSource::MusicBrainz, 95, "One");
    let mut two = identity(RemoteEvidenceSource::AcoustId, 80, "Two");
    one.duration_ms = Some(180_000);
    two.duration_ms = Some(190_000);
    assert!(arbitrate(&metadata(), &[one, two])
        .groups
        .iter()
        .any(|group| group.field == DoctorField::Title));

    let mut first_year = identity(RemoteEvidenceSource::MusicBrainz, 90, "Canonical");
    let mut second_year = first_year.clone();
    first_year.release_year = Some(2020);
    second_year.release_year = Some(2021);
    assert!(!arbitrate(&metadata(), &[first_year, second_year])
        .proposals
        .iter()
        .any(|proposal| proposal.field == DoctorField::Year));
}

#[test]
fn low_confidence_remains_visible_and_unchecked() {
    let result = arbitrate(
        &metadata(),
        &[identity(RemoteEvidenceSource::AcoustId, 41, "Low")],
    );
    let row = result
        .proposals
        .iter()
        .find(|p| p.field == DoctorField::Title)
        .unwrap();
    assert_eq!(row.confidence, 41);
    assert!(!row.preselected);
}

#[test]
fn invalid_embedded_mbid_is_not_direct_identity() {
    let mut value = metadata();
    value.recording_mbid = Some("not-a-uuid".into());
    let mut provider = FakeProvider::default();
    let _ = resolve_with_provider(&mut provider, &value, None);
    assert_eq!(provider.calls, ["musicbrainz"]);
    assert_eq!(value.valid_recording_mbid(), None);
}

#[test]
fn production_cascade_continues_from_partial_direct_through_search_and_acoustid() {
    let mut direct = identity(RemoteEvidenceSource::MusicBrainz, 100, "Canonical");
    direct.album_artist = None;
    direct.release_year = None;
    direct.original_release_year = None;
    let mut searched = direct.clone();
    searched.album_artist = Some("Canonical album artist".into());
    let mut acoustid = searched.clone();
    acoustid.source = RemoteEvidenceSource::AcoustId;
    acoustid.release_year = Some(2024);
    let provider = FakeProvider {
        direct: vec![direct],
        searched: vec![searched],
        acoustid: vec![acoustid],
        ..Default::default()
    };
    let mut resolver = ProviderRemoteResolver::new(provider);
    let result = RemoteResolver::resolve_track(
        &mut resolver,
        &metadata(),
        Path::new("ignored"),
        Some(&FakeFingerprint),
        &mut || ScanControl::Continue,
    )
    .unwrap();
    assert_eq!(
        resolver.into_provider().calls,
        ["direct", "musicbrainz", "acoustid"]
    );
    assert!(result
        .proposals
        .iter()
        .any(|proposal| proposal.field == DoctorField::Year));
}

#[test]
fn production_cascade_isolates_source_failures_and_short_circuits_only_when_complete() {
    let mut complete = identity(RemoteEvidenceSource::MusicBrainz, 100, "Canonical");
    complete.release_year = Some(2024);
    let provider = FakeProvider {
        direct: Vec::new(),
        searched: vec![complete],
        ..Default::default()
    };
    let mut resolver = ProviderRemoteResolver::new(provider);
    RemoteResolver::resolve_track(
        &mut resolver,
        &metadata(),
        Path::new("ignored"),
        Some(&FakeFingerprint),
        &mut || ScanControl::Continue,
    )
    .unwrap();
    assert_eq!(resolver.into_provider().calls, ["direct", "musicbrainz"]);
}

#[test]
fn production_cascade_isolates_each_unavailable_source() {
    let mut complete = identity(RemoteEvidenceSource::AcoustId, 90, "Canonical");
    complete.release_year = Some(2024);
    let provider = FakeProvider {
        direct_error: Some(RemoteProviderError::Unavailable),
        search_error: Some(RemoteProviderError::InvalidResponse),
        acoustid: vec![complete],
        ..Default::default()
    };
    let mut resolver = ProviderRemoteResolver::new(provider);
    let result = RemoteResolver::resolve_track(
        &mut resolver,
        &metadata(),
        Path::new("ignored"),
        Some(&FakeFingerprint),
        &mut || ScanControl::Continue,
    )
    .unwrap();
    assert_eq!(
        resolver.into_provider().calls,
        ["direct", "musicbrainz", "acoustid"]
    );
    assert!(!result.proposals.is_empty());
}

#[test]
fn production_cascade_honors_cancellation_before_a_source_call() {
    let mut resolver = ProviderRemoteResolver::new(FakeProvider::default());
    let result = RemoteResolver::resolve_track(
        &mut resolver,
        &metadata(),
        Path::new("ignored"),
        Some(&FakeFingerprint),
        &mut || ScanControl::Cancel,
    );
    assert_eq!(result, Err(RemoteProviderError::Cancelled));
    assert!(resolver.into_provider().calls.is_empty());
}

#[test]
fn matching_current_candidate_participates_in_ranking() {
    let matching = identity(RemoteEvidenceSource::MusicBrainz, 95, "Real title");
    let weaker = identity(RemoteEvidenceSource::AcoustId, 70, "Wrong title");
    let result = arbitrate(&metadata(), &[matching, weaker]);
    assert!(!result
        .proposals
        .iter()
        .any(|proposal| proposal.field == DoctorField::Title));
}

#[test]
fn local_identity_or_duration_conflict_keeps_same_value_visible_for_manual_review() {
    let mut conflict = identity(RemoteEvidenceSource::MusicBrainz, 100, "Real title");
    conflict.recording_mbid = Some("223e4567-e89b-12d3-a456-426614174000".into());
    conflict.duration_ms = Some(190_000);
    let result = arbitrate(&metadata(), &[conflict]);
    let group = result
        .groups
        .iter()
        .find(|group| group.field == DoctorField::Title)
        .expect("same-value contradiction must remain visible");
    assert_eq!(group.candidates.len(), 1);
}

#[test]
fn single_candidates_are_guarded_by_field_specific_embedded_ids() {
    let mut value = metadata();
    value.artist_mbid = Some("423e4567-e89b-12d3-a456-426614174003".into());
    value.release_group_mbid = Some("423e4567-e89b-12d3-a456-426614174002".into());
    value.release_artist_mbid = Some("423e4567-e89b-12d3-a456-426614174004".into());
    let result = arbitrate(
        &value,
        &[identity(
            RemoteEvidenceSource::MusicBrainz,
            100,
            "Canonical",
        )],
    );
    for field in [
        DoctorField::Artist,
        DoctorField::Album,
        DoctorField::AlbumArtist,
    ] {
        assert!(result.groups.iter().any(|group| group.field == field));
        assert!(!result
            .proposals
            .iter()
            .any(|proposal| proposal.field == field));
    }
}

#[test]
fn multiple_releases_do_not_suppress_canonical_recording_title_or_artist() {
    let one = identity(RemoteEvidenceSource::MusicBrainz, 100, "Canonical");
    let mut two = one.clone();
    two.release_mbid = Some("223e4567-e89b-12d3-a456-426614174001".into());
    let result = arbitrate(&metadata(), &[one, two]);
    assert!(result
        .proposals
        .iter()
        .any(|proposal| proposal.field == DoctorField::Title));
    assert!(result
        .proposals
        .iter()
        .any(|proposal| proposal.field == DoctorField::Artist));
}

#[test]
fn year_uses_release_for_one_edition_and_original_year_for_one_release_group() {
    let one = identity(RemoteEvidenceSource::MusicBrainz, 100, "Canonical");
    let single = arbitrate(&metadata(), std::slice::from_ref(&one));
    assert!(single.proposals.iter().any(|proposal| {
        proposal.field == DoctorField::Year && proposal.proposed == DoctorValue::Year(2024)
    }));

    let mut second_edition = one.clone();
    second_edition.release_mbid = Some("223e4567-e89b-12d3-a456-426614174001".into());
    second_edition.release_year = Some(2025);
    let group = arbitrate(&metadata(), &[one, second_edition]);
    assert!(group.proposals.iter().any(|proposal| {
        proposal.field == DoctorField::Year && proposal.proposed == DoctorValue::Year(2023)
    }));
}

#[test]
fn year_is_absent_for_ambiguous_release_groups() {
    let one = identity(RemoteEvidenceSource::MusicBrainz, 100, "Canonical");
    let mut other_group = one.clone();
    other_group.release_mbid = Some("223e4567-e89b-12d3-a456-426614174001".into());
    other_group.release_group_mbid = Some("223e4567-e89b-12d3-a456-426614174002".into());
    other_group.release_year = Some(2025);
    other_group.original_release_year = Some(2022);
    let result = arbitrate(&metadata(), &[one, other_group]);
    assert!(!result
        .proposals
        .iter()
        .any(|proposal| proposal.field == DoctorField::Year));
    assert!(!result
        .groups
        .iter()
        .any(|group| group.field == DoctorField::Year));
}
