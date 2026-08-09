use std::path::Path;

use super::*;

/// Records every release lookup a scan performs, and answers none of them.
#[derive(Default)]
struct CountingAlbumResolver {
    queries: Vec<super::remote::AlbumQuery>,
}

impl RemoteResolver for CountingAlbumResolver {
    fn resolve_album(
        &mut self,
        request: &super::remote::AlbumRequest,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> Result<super::remote::AlbumResolution, RemoteProviderError> {
        self.queries.push(request.query.clone());
        Ok(super::remote::AlbumResolution::default())
    }

    fn resolve_track(
        &mut self,
        _: &RemoteTrackMetadata,
        _: &Path,
        _: Option<&dyn FingerprintBackend>,
        _: Option<&super::remote::AlbumMatch>,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> Result<RemoteResolution, RemoteProviderError> {
        Ok(RemoteResolution::default())
    }
}

fn scan_albums(albums: &[&str]) -> Vec<super::remote::AlbumQuery> {
    let dir = tempfile::tempdir().unwrap();
    let conn = migrated_connection();
    for (position, album) in albums.iter().enumerate() {
        let id = i64::try_from(position).unwrap() + 1;
        let path = fixture_copy(dir.path(), &format!("disc-{id}.flac"));
        write_tags(&path, &format!("Track {id}"), "Artist", album, "Artist", "Rock");
        insert_track(&conn, id, &path, "Artist");
    }
    let mut resolver = CountingAlbumResolver::default();
    LibraryDoctor::new(&conn)
        .scan_with_resolver(
            &DoctorScanRequest {
                scope: DoctorScopeRequest::Selection {
                    track_ids: (1..=i64::try_from(albums.len()).unwrap()).collect(),
                },
                options: DoctorScanOptions {
                    remote_enabled: true,
                },
            },
            None,
            &mut resolver,
            &mut |_| ScanControl::Continue,
        )
        .unwrap();
    resolver.queries
}

/// A multi-disc set whose discs carry different album titles used to be one
/// group — and therefore one release lookup, and one chosen release — per
/// disc. No disc held the full tracklist, so each scored worse than the set
/// does as a whole, and the two lookups could land on different releases.
#[test]
fn doc_1g_a_multi_disc_album_is_one_release_lookup() {
    let queries = scan_albums(&[
        "Album (Disc 1)",
        "Album (Disc 1)",
        "Album [CD2]",
        "Album, Disc 3",
    ]);

    assert_eq!(
        queries.len(),
        1,
        "one release, one lookup — got {:?}",
        queries
            .iter()
            .map(|query| query.album.clone())
            .collect::<Vec<_>>()
    );
    // The count the release is compared against is the whole set, which is
    // what MusicBrainz reports: `release_track_count` sums every medium.
    assert_eq!(queries[0].track_count, 4);
    // And the search asks after the album, not after one of its discs.
    assert_eq!(queries[0].album, "Album");
}

/// The suffix is stripped for grouping only. Two albums that merely share an
/// artist stay apart, and a title that is nothing but a disc marker keeps it
/// rather than collapsing into the empty key.
#[test]
fn doc_1g_albums_without_a_disc_suffix_keep_their_own_lookups() {
    let queries = scan_albums(&["First Album", "Second Album", "CD 1"]);

    let mut albums = queries
        .iter()
        .map(|query| query.album.clone())
        .collect::<Vec<_>>();
    albums.sort();
    assert_eq!(albums, vec!["CD 1", "First Album", "Second Album"]);
}
