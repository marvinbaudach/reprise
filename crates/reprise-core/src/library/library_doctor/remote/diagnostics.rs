use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};

use super::{arbitration, NetworkProvider, RemoteProvider, RemoteTrackMetadata};
use crate::library::library_doctor::{DoctorField, ScanControl};

const DEFAULT_DIAGNOSTIC_DB_URI: &str =
    "file:///home/marvin/.local/share/reprise/reprise.db?mode=ro";
const DIAGNOSTIC_TITLES: [&str; 3] = ["Carry Me Away", "An Ocean Between Us", "The Sound Of Truth"];

#[test]
#[ignore = "reads a real database through a read-only URI and contacts MusicBrainz"]
fn diag_remote_identity_dump_for_a_known_bad_album() {
    let uri = std::env::var("REPRISE_DIAG_DB_URI")
        .unwrap_or_else(|_| DEFAULT_DIAGNOSTIC_DB_URI.to_owned());
    assert!(
        uri.starts_with("file:") && uri.contains("mode=ro"),
        "REPRISE_DIAG_DB_URI must be a read-only SQLite URI"
    );
    let conn = Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .expect("open the diagnostic database read-only");

    let mut dumps = Vec::new();
    for title in DIAGNOSTIC_TITLES {
        let (path, database_tags) = conn
            .query_row(
                "SELECT path, title, artist, album, album_artist, year, duration_ms
                 FROM tracks WHERE title=?1 AND removed_at IS NULL
                 ORDER BY id LIMIT 1",
                [title],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        RemoteTrackMetadata {
                            title: row.get(1)?,
                            artist: row.get(2)?,
                            album: row.get(3)?,
                            album_artist: row.get(4)?,
                            year: row
                                .get::<_, Option<i64>>(5)?
                                .and_then(|value| u32::try_from(value).ok()),
                            recording_mbid: None,
                            release_mbid: None,
                            release_group_mbid: None,
                            artist_mbid: None,
                            release_artist_mbid: None,
                            duration_ms: row
                                .get::<_, Option<i64>>(6)?
                                .and_then(|value| u64::try_from(value).ok()),
                        },
                    ))
                },
            )
            .unwrap_or_else(|error| panic!("load diagnostic track {title}: {error}"));
        let metadata = super::read_remote_metadata(std::path::Path::new(&path))
            .map(|(_, metadata)| metadata)
            .unwrap_or(database_tags);
        let mut provider = NetworkProvider::new();
        let mut control = || ScanControl::Continue;
        let mut lookup_dumps = Vec::new();
        let mut identities = Vec::new();
        for lookup in metadata.direct_lookups() {
            let resolved = provider
                .direct(&lookup, &mut control)
                .unwrap_or_else(|error| panic!("resolve {title} through {lookup:?}: {error}"));
            lookup_dumps.push(json!({
                "path": format!("direct:{lookup:?}"),
                "identities": resolved,
            }));
            identities.extend(resolved);
        }
        let searched = provider
            .search_musicbrainz(&metadata, &mut control)
            .unwrap_or_else(|error| panic!("search MusicBrainz for {title}: {error}"));
        lookup_dumps.push(json!({
            "path": "recording_search",
            "identities": searched,
        }));
        identities.extend(searched);

        let fields = super::REMOTE_WRITABLE_FIELDS
            .into_iter()
            .map(|field| field_dump(&metadata, &identities, field))
            .collect::<Vec<_>>();
        let hundred_percent = identities
            .iter()
            .filter(|identity| identity.confidence == 100)
            .map(identity_origin)
            .collect::<Vec<_>>();
        let various_artists = identities
            .iter()
            .filter(|identity| {
                identity
                    .album_artist
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("Various Artists"))
            })
            .map(identity_origin)
            .collect::<Vec<_>>();
        assert!(
            !identities.is_empty(),
            "MusicBrainz returned no identities for {title}"
        );
        dumps.push(json!({
            "track": title,
            "metadata": metadata,
            "lookups": lookup_dumps,
            "ranked_fields": fields,
            "hundred_percent_identity_paths": hundred_percent,
            "various_artists_identity_paths": various_artists,
        }));
    }

    println!(
        "DIAG-1_REMOTE_IDENTITY_DUMP={} ",
        serde_json::to_string_pretty(&dumps).unwrap()
    );
    assert_eq!(dumps.len(), DIAGNOSTIC_TITLES.len());
}

fn field_dump(
    metadata: &RemoteTrackMetadata,
    identities: &[super::RemoteIdentity],
    field: DoctorField,
) -> Value {
    let ranked = arbitration::ranked_candidates(identities, field);
    let candidates = ranked
        .iter()
        .map(|(value, supporting)| {
            json!({
                "value": value,
                "support": supporting.iter().map(|identity| identity_origin(identity)).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "field": format!("{field:?}"),
        "candidates": candidates,
        "has_clear_lead": (!ranked.is_empty())
            .then(|| arbitration::has_clear_lead(&ranked)),
        "candidates_contradict": (!ranked.is_empty())
            .then(|| arbitration::candidates_contradict(metadata, field, &ranked)),
    })
}

fn identity_origin(identity: &super::RemoteIdentity) -> Value {
    json!({
        "source": identity.source,
        "confidence": identity.confidence,
        "recording_mbid": identity.recording_mbid,
        "release_mbid": identity.release_mbid,
        "release_group_mbid": identity.release_group_mbid,
        "artist_mbid": identity.artist_mbid,
        "release_artist_mbid": identity.release_artist_mbid,
        "album_artist": identity.album_artist,
    })
}
