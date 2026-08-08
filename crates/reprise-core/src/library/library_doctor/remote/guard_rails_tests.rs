use super::super::{DoctorField, DoctorValue};
use super::arbitration::arbitrate;
use super::guard_rails::{is_placeholder_artist, reduces_specificity, VARIOUS_ARTISTS_MBID};
use super::{RemoteEvidenceSource, RemoteIdentity, RemoteTrackMetadata};

fn metadata() -> RemoteTrackMetadata {
    RemoteTrackMetadata {
        title: Some("Real title".into()),
        artist: Some("Real artist".into()),
        album: Some("Real album".into()),
        album_artist: Some("Real album artist".into()),
        year: Some(2024),
        recording_mbid: Some("123e4567-e89b-12d3-a456-426614174000".into()),
        release_mbid: None,
        release_group_mbid: None,
        artist_mbid: None,
        release_artist_mbid: None,
        duration_ms: Some(180_000),
    }
}

fn identity(title: &str) -> RemoteIdentity {
    RemoteIdentity {
        source: RemoteEvidenceSource::MusicBrainz,
        confidence: 100,
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

#[test]
fn doc_1f_a_localised_placeholder_spelling_is_not_recognised() {
    assert!(!is_placeholder_artist("Verschiedene Interpreten", None));
}

#[test]
fn curated_placeholder_names_are_compared_after_trim_and_unicode_lowercasing() {
    assert!(is_placeholder_artist("  VARIOUS ARTISTS  ", None));
    assert!(is_placeholder_artist("various", None));
    assert!(is_placeholder_artist("va", None));
    assert!(is_placeholder_artist(
        "Harmless name",
        Some(VARIOUS_ARTISTS_MBID)
    ));
}

#[test]
fn doc_4c_a_truncated_title_is_a_specificity_loss() {
    assert!(reduces_specificity(
        &DoctorValue::Text("An Ocean Between Us".into()),
        &DoctorValue::Text("An Ocean".into()),
        DoctorField::Title,
    ));
    assert!(!reduces_specificity(
        &DoctorValue::Text("An Ocean".into()),
        &DoctorValue::Text("An Ocean Between Us".into()),
        DoctorField::Title,
    ));
}

#[test]
fn doc_4c_an_earlier_release_group_year_on_a_track_tag_is_a_specificity_loss() {
    assert!(reduces_specificity(
        &DoctorValue::Year(2024),
        &DoctorValue::Year(2007),
        DoctorField::Year,
    ));
    assert!(!reduces_specificity(
        &DoctorValue::Year(2024),
        &DoctorValue::Year(2025),
        DoctorField::Year,
    ));
}

#[test]
fn doc_1f_various_artists_never_overwrites_a_named_album_artist() {
    let mut value = metadata();
    value.album_artist = Some("Annisokay".into());
    let mut candidate = identity("Canonical");
    candidate.album_artist = Some("Various Artists".into());

    let result = arbitrate(&value, &[candidate]);

    assert!(!result
        .proposals
        .iter()
        .any(|proposal| proposal.field == DoctorField::AlbumArtist));
}

#[test]
fn doc_1f_a_placeholder_needs_evidence_of_several_track_artists() {
    let mut value = metadata();
    value.album_artist = None;
    let mut candidate = identity("Canonical");
    candidate.album_artist = Some("Various Artists".into());

    let result = arbitrate(&value, &[candidate]);

    assert!(!result
        .proposals
        .iter()
        .any(|proposal| proposal.field == DoctorField::AlbumArtist));
}

#[test]
fn doc_1f_the_placeholder_is_recognised_by_name_and_by_mbid() {
    let mut by_name = identity("Canonical");
    by_name.album_artist = Some("VA".into());
    assert!(!arbitrate(&metadata(), &[by_name])
        .proposals
        .iter()
        .any(|proposal| proposal.field == DoctorField::AlbumArtist));

    let mut by_mbid = identity("Canonical");
    by_mbid.album_artist = Some("Harmless name".into());
    by_mbid.release_artist_mbid = Some(VARIOUS_ARTISTS_MBID.into());
    assert!(!arbitrate(&metadata(), &[by_mbid])
        .proposals
        .iter()
        .any(|proposal| proposal.field == DoctorField::AlbumArtist));
}

#[test]
fn a_placeholder_never_becomes_an_unresolved_artist_candidate() {
    let original = identity("Canonical");
    let mut compilation = original.clone();
    compilation.album_artist = Some("Various Artists".into());
    compilation.release_mbid = Some("423e4567-e89b-12d3-a456-426614174001".into());
    compilation.release_group_mbid = Some("423e4567-e89b-12d3-a456-426614174002".into());
    compilation.release_artist_mbid = Some(VARIOUS_ARTISTS_MBID.into());

    let result = arbitrate(&metadata(), &[original, compilation]);

    assert!(!result.groups.iter().any(|group| {
        group.field == DoctorField::AlbumArtist
            && group
                .candidates
                .iter()
                .any(|candidate| candidate.value == DoctorValue::Text("Various Artists".into()))
    }));
}

#[test]
fn doc_4c_a_specificity_reducing_proposal_is_capped_and_never_preselected() {
    let mut value = metadata();
    value.title = Some("Real title extended".into());
    let candidate = identity("Real title");

    let result = arbitrate(&value, &[candidate]);
    let title = result
        .proposals
        .iter()
        .find(|proposal| proposal.field == DoctorField::Title)
        .expect("the reduced title stays reviewable");

    assert!(title.confidence <= 49);
    assert!(title.never_preselect);
}
