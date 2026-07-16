//! MusicBrainz release lookup for the tag editor: searches by Artist + Album,
//! picks the first matching release, and extracts Year / Genre / Album Artist.

use crate::musicbrainz::{self, FetchError};

/// The result of a successful release lookup.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReleaseLookupResult {
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub album_artist: Option<String>,
}

/// Errors that can occur during a release lookup.
#[derive(Debug, thiserror::Error)]
pub enum ReleaseLookupError {
    /// The HTTP fetch failed.
    #[error("fetch failed: {0}")]
    Fetch(#[from] FetchError),
    /// The response could not be parsed.
    #[error("parse error: {0}")]
    Parse(String),
}

/// Searches MusicBrainz for a release matching `artist` + `album`, then
/// extracts metadata. Returns the first match's metadata, or an error
/// if the search fails or returns no results.
pub fn lookup_release(artist: &str, album: &str) -> Result<ReleaseLookupResult, ReleaseLookupError> {
    let query = format!(
        "artist:\"{}\" AND release:\"{}\"",
        artist.replace('"', "\\\""),
        album.replace('"', "\\\""),
    );
    let encoded = musicbrainz::urlencode(&query);
    let url = format!(
        "https://musicbrainz.org/ws/2/release/?query={encoded}&limit=5&fmt=json&inc=release-groups+genres"
    );
    let body = musicbrainz::get(&url)?;
    parse_release_response(&body)
}

fn parse_release_response(json: &str) -> Result<ReleaseLookupResult, ReleaseLookupError> {
    let parsed: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| ReleaseLookupError::Parse(e.to_string()))?;

    let releases = parsed["releases"]
        .as_array()
        .ok_or_else(|| ReleaseLookupError::Parse("no releases array".into()))?;

    let release = releases
        .first()
        .ok_or_else(|| ReleaseLookupError::Parse("no matching releases".into()))?;

    let year = release["date"]
        .as_str()
        .and_then(|d| d.split('-').next())
        .and_then(|y| y.parse::<u32>().ok());

    let genre = release["release-group"]["genres"]
        .as_array()
        .and_then(|genres| {
            genres
                .iter()
                .max_by_key(|g| g["count"].as_i64().unwrap_or(0))
                .and_then(|g| g["name"].as_str())
                .map(str::to_string)
        });

    let album_artist = release["artist-credit"]
        .as_array()
        .and_then(|credits| credits.first())
        .and_then(|c| c["name"].as_str())
        .map(str::to_string);

    Ok(ReleaseLookupResult {
        year,
        genre,
        album_artist,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RESPONSE: &str = r#"{
        "releases": [{
            "id": "abc-123",
            "title": "Relinquished",
            "date": "2025-03-15",
            "artist-credit": [{"name": "Cogitations"}],
            "release-group": {
                "primary-type": "Album",
                "genres": [{"name": "ambient", "count": 5}, {"name": "post-rock", "count": 2}]
            }
        }]
    }"#;

    #[test]
    fn parses_year_from_release_date() {
        let result = parse_release_response(SAMPLE_RESPONSE).unwrap();
        assert_eq!(result.year, Some(2025));
    }

    #[test]
    fn picks_highest_voted_genre() {
        let result = parse_release_response(SAMPLE_RESPONSE).unwrap();
        assert_eq!(result.genre.as_deref(), Some("ambient"));
    }

    #[test]
    fn extracts_artist_credit() {
        let result = parse_release_response(SAMPLE_RESPONSE).unwrap();
        assert_eq!(result.album_artist.as_deref(), Some("Cogitations"));
    }

    #[test]
    fn empty_releases_array_returns_error() {
        let json = r#"{"releases": []}"#;
        assert!(parse_release_response(json).is_err());
    }
}
