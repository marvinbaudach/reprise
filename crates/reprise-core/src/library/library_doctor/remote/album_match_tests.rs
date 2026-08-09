use super::album_match::{best_release, AlbumQuery};
use super::{ReleaseSecondaryType, RemoteEvidenceSource, RemoteIdentity};

fn release(
    mbid: &str,
    album_artist: &str,
    track_count: u32,
    titles: &[&str],
    secondary_types: Vec<ReleaseSecondaryType>,
) -> RemoteIdentity {
    RemoteIdentity {
        source: RemoteEvidenceSource::MusicBrainz,
        confidence: 100,
        recording_mbid: None,
        release_mbid: Some(mbid.into()),
        release_group_mbid: None,
        artist_mbid: None,
        release_artist_mbid: None,
        title: None,
        artist: None,
        album: Some("An Ocean Between Us".into()),
        album_artist: Some(album_artist.into()),
        release_year: Some(2007),
        original_release_year: Some(2007),
        duration_ms: None,
        secondary_types,
        release_track_count: Some(track_count),
        release_track_titles: titles.iter().map(|title| (*title).into()).collect(),
        release_distinct_track_artists: None,
    }
}

fn query() -> AlbumQuery {
    AlbumQuery {
        album_artist: "As I Lay Dying".into(),
        album: "An Ocean Between Us".into(),
        track_titles: ["Separation", "Nothing Left", "The Sound of Truth"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        track_count: 10,
        year: Some(2007),
    }
}

#[test]
fn doc_1e_a_single_artist_release_beats_a_compilation_containing_one_track() {
    let original = release(
        "00000000-0000-0000-0000-000000000001",
        "As I Lay Dying",
        10,
        &["Separation", "Nothing Left", "The Sound of Truth"],
        Vec::new(),
    );
    let compilation = release(
        "00000000-0000-0000-0000-000000000002",
        "Various Artists",
        28,
        &["The Sound of Truth"],
        vec![ReleaseSecondaryType::Compilation],
    );

    let matched = best_release(&query(), &[compilation, original]).unwrap();

    assert_eq!(
        matched.identity.release_mbid.as_deref(),
        Some("00000000-0000-0000-0000-000000000001")
    );
    assert!(matched.exact);
}

#[test]
fn doc_1e_a_locally_tagged_compilation_is_not_demoted() {
    let query = AlbumQuery {
        album_artist: "Various Artists".into(),
        album: "Compilation".into(),
        track_titles: vec!["First".into(), "Second".into()],
        track_count: 2,
        year: Some(2024),
    };
    let mut compilation = release(
        "00000000-0000-0000-0000-000000000003",
        "Various Artists",
        2,
        &["First", "Second"],
        vec![ReleaseSecondaryType::Compilation],
    );
    compilation.album = Some("Compilation".into());
    compilation.release_year = Some(2024);

    let matched = best_release(&query, &[compilation]).unwrap();

    assert_eq!(matched.score, 100);
    assert!(matched.exact);
}

#[test]
fn doc_1e_every_demoted_release_kind_can_be_named_by_the_local_album_title() {
    for (kind, album) in [
        (ReleaseSecondaryType::Compilation, "Greatest Hits"),
        (ReleaseSecondaryType::DjMix, "Fabric 39 (DJ Mix)"),
        (ReleaseSecondaryType::Live, "Live at Wacken"),
        (ReleaseSecondaryType::Mixtape, "The Mixtape"),
        (ReleaseSecondaryType::Remix, "Remixes"),
    ] {
        let query = AlbumQuery {
            album_artist: "As I Lay Dying".into(),
            album: album.into(),
            track_titles: vec!["Separation".into(), "Nothing Left".into()],
            track_count: 2,
            year: Some(2007),
        };
        let mut candidate = release(
            "00000000-0000-0000-0000-000000000008",
            "As I Lay Dying",
            2,
            &["Separation", "Nothing Left"],
            vec![kind.clone()],
        );
        candidate.album = Some(album.into());

        let matched = best_release(&query, &[candidate]).unwrap();

        assert_eq!(
            matched.score, 100,
            "{album} names a {kind:?} release, so the demotion must not apply"
        );
    }
}

#[test]
fn doc_1e_track_count_equality_outweighs_a_single_title_hit() {
    // Both candidates carry the queried artist credit so that the winner is
    // decided by the ranking rather than by `MINIMUM_RELEASE_SCORE`.
    let mut right_count = release(
        "00000000-0000-0000-0000-000000000004",
        "As I Lay Dying",
        10,
        &[],
        Vec::new(),
    );
    right_count.release_year = None;
    let mut one_title = release(
        "00000000-0000-0000-0000-000000000005",
        "As I Lay Dying",
        99,
        &["Separation"],
        Vec::new(),
    );
    one_title.release_year = None;

    let matched = best_release(&query(), &[one_title, right_count]).unwrap();

    assert_eq!(
        matched.identity.release_mbid.as_deref(),
        Some("00000000-0000-0000-0000-000000000004")
    );
}

#[test]
fn doc_1e_a_release_nobody_can_recognise_is_no_match_at_all() {
    let mut stranger = release(
        "00000000-0000-0000-0000-000000000009",
        "Someone Else",
        99,
        &["Separation"],
        Vec::new(),
    );
    stranger.release_year = None;

    assert!(
        best_release(&query(), &[stranger]).is_none(),
        "one accidental title hit is not a release match"
    );
}

#[test]
fn doc_1e_a_matching_track_count_with_title_overlap_stays_a_match() {
    let mut plausible = release(
        "00000000-0000-0000-0000-000000000010",
        "Someone Else",
        10,
        &["Separation", "Nothing Left", "The Sound of Truth"],
        Vec::new(),
    );
    plausible.release_year = None;

    assert!(best_release(&query(), &[plausible]).is_some());
}

#[test]
fn doc_1e_the_best_release_is_deterministic_for_equal_scores() {
    let first = release(
        "00000000-0000-0000-0000-000000000006",
        "As I Lay Dying",
        10,
        &["Separation", "Nothing Left", "The Sound of Truth"],
        Vec::new(),
    );
    let second = release(
        "00000000-0000-0000-0000-000000000007",
        "As I Lay Dying",
        10,
        &["Separation", "Nothing Left", "The Sound of Truth"],
        Vec::new(),
    );

    let forward = best_release(&query(), &[first.clone(), second.clone()]).unwrap();
    let reverse = best_release(&query(), &[second, first]).unwrap();

    assert_eq!(forward.identity.release_mbid, reverse.identity.release_mbid);
    assert_eq!(
        forward.identity.release_mbid.as_deref(),
        Some("00000000-0000-0000-0000-000000000006")
    );
}
