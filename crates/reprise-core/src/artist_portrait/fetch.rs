//! Source orchestration for the artist-portrait cache.

use std::path::{Path, PathBuf};

use crate::artist_news::{artist_search_url, parse_artist_mbid, ArtistMatch};
use crate::musicbrainz::FetchError;

use super::sidecar::PortraitRecord;
use super::{cache, deezer, placeholder, release_cover, wikimedia};
use super::{PortraitError, PortraitOutcome};

pub(crate) struct Fetchers<'a> {
    pub deezer_text: &'a mut dyn FnMut(&str) -> Result<String, FetchError>,
    pub musicbrainz_text: &'a mut dyn FnMut(&str) -> Result<String, FetchError>,
    pub wikimedia_text: &'a mut dyn FnMut(&str) -> Result<String, FetchError>,
    pub image: &'a mut dyn FnMut(&str) -> Result<Vec<u8>, FetchError>,
}

pub(crate) fn load_or_fetch(
    name: &str,
    mbid_hint: Option<&str>,
    now: i64,
    dir: &Path,
    fetchers: &mut Fetchers<'_>,
) -> Result<PortraitOutcome, PortraitError> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(PortraitOutcome::NotFound);
    }
    let stale = match cache::verdict(dir, name, now) {
        cache::CacheVerdict::FreshPortrait(path) => return Ok(PortraitOutcome::Found(path)),
        cache::CacheVerdict::FreshNegative => return Ok(PortraitOutcome::NotFound),
        // A remembered rejection stops the asking, not the serving of the portrait already held.
        cache::CacheVerdict::FreshRetry => {
            return Ok(cache::portrait_path_in(dir, name)
                .map_or(PortraitOutcome::NotFound, PortraitOutcome::Found))
        }
        cache::CacheVerdict::NeedsFetch { stale_portrait, .. } => stale_portrait,
    };

    let search_body = match (fetchers.deezer_text)(&deezer::search_url(name)) {
        Ok(body) => body,
        Err(error) => return stale_or(dir, name, stale, error.into()),
    };
    if !deezer::has_data_array(&search_body) {
        return stale_or(dir, name, stale, PortraitError::InvalidResponse);
    }
    if let Some(artist) = deezer::parse_best_artist(&search_body, name) {
        if let Some(url) = artist.picture_url {
            let albums = match (fetchers.deezer_text)(&release_cover::albums_url(artist.id)) {
                Ok(body) => body,
                Err(error) => return stale_or(dir, name, stale, error.into()),
            };
            let Some(covers) = release_cover::parse_release_cover_identifiers(&albums) else {
                return stale_or(dir, name, stale, PortraitError::InvalidResponse);
            };
            if release_cover::is_release_cover(&url, &covers) {
                cache::remove_portrait(dir, name);
                return fetch_commons(name, mbid_hint, None, None, dir, fetchers);
            }
            match fetch_deezer_image(name, &url, stale.clone(), dir, fetchers) {
                Ok(Some(outcome)) => return Ok(outcome),
                Ok(None) => {}
                // A refused download only reaches here without a stale portrait to keep.
                Err(PortraitError::Fetch(refusal @ FetchError::HttpStatus(status)))
                    if is_refusal(status) =>
                {
                    return fetch_commons(name, mbid_hint, None, Some(refusal), dir, fetchers);
                }
                Err(error) => return Err(error),
            }
        }
    }

    fetch_commons(name, mbid_hint, stale, None, dir, fetchers)
}

/// Classifies a portrait fetch error as network-shaped or artist-shaped.
///
/// Network-shaped errors indicate the connection is the problem; every artist would
/// fail the same way. These retries never consume artist-specific budgets.
///
/// Artist-shaped errors indicate the server answered and the answer is specific to
/// this request. These always consume one artist attempt.
pub(crate) fn is_network_shaped_error(error: &PortraitError) -> bool {
    matches!(
        error,
        PortraitError::Fetch(
            FetchError::Timeout
                | FetchError::Transport
                | FetchError::Body
                | FetchError::HttpStatus(429 | 500..=599)
        )
    )
}

const HTTP_CLIENT_ERRORS: std::ops::Range<u16> = 400..500;
const HTTP_TOO_MANY_REQUESTS: u16 = 429;

/// Whether an HTTP status is the server declining this request for good (a 4xx
/// other than 429), as opposed to a rate limit or server error that passes.
pub(crate) fn is_refusal(status: u16) -> bool {
    HTTP_CLIENT_ERRORS.contains(&status) && status != HTTP_TOO_MANY_REQUESTS
}

/// Serves the stale portrait when a refresh fails. A failure that is not the network's
/// (the server answered and refused or garbled it) is remembered for a day, so the next
/// start does not ask again; a network-shaped failure writes nothing.
fn stale_or(
    dir: &Path,
    name: &str,
    cached_path: Option<PathBuf>,
    error: PortraitError,
) -> Result<PortraitOutcome, PortraitError> {
    match cached_path.filter(|path| path.exists()) {
        Some(path) => {
            if !is_network_shaped_error(&error) {
                cache::write_retry(dir, name);
            }
            Ok(PortraitOutcome::Found(path))
        }
        None => Err(error),
    }
}

fn fetch_deezer_image(
    name: &str,
    url: &str,
    stale: Option<PathBuf>,
    dir: &Path,
    fetchers: &mut Fetchers<'_>,
) -> Result<Option<PortraitOutcome>, PortraitError> {
    let bytes = match (fetchers.image)(url) {
        Ok(bytes) => bytes,
        Err(error) => return stale_or(dir, name, stale, error.into()).map(Some),
    };
    let decoded = crate::cover_download::decode_image(&bytes);
    let distance = decoded
        .as_ref()
        .map(|image| placeholder::placeholder_distance(image.image()));
    if let Some(distance) = distance.filter(|value| *value <= placeholder::PLACEHOLDER_RMSE_MAX) {
        warn_placeholder(
            name,
            url,
            distance,
            "artist portrait rejected as a known Deezer placeholder",
        );
        if let Some(path) = stale.as_ref().filter(|path| path.exists()) {
            let refreshed = cache::refresh_image(dir, name, path).unwrap_or_else(|| {
                tracing::warn!(
                    artist = name,
                    cached_path = %path.display(),
                    "artist portrait could not refresh the cached image after placeholder rejection"
                );
                path.clone()
            });
            return Ok(Some(PortraitOutcome::Found(refreshed)));
        }
        return Ok(None);
    }
    let Some(extension) = decoded
        .as_ref()
        .and_then(crate::cover_download::DecodedImage::validated_extension)
    else {
        return Ok(None);
    };
    if let Some(distance) =
        distance.filter(|value| *value < placeholder::PLACEHOLDER_WARNING_RMSE_MAX)
    {
        warn_placeholder(
            name,
            url,
            distance,
            "artist portrait accepted near the placeholder threshold",
        );
    }
    let outcome = cache::store_portrait(dir, name, &bytes, extension, &PortraitRecord::deezer())
        .map(PortraitOutcome::Found)
        .ok_or(PortraitError::InvalidResponse)?;
    Ok(Some(outcome))
}

/// Runs the credited Commons chain. `refusal` is the Deezer download error that sent
/// the fetch here: a Commons miss then returns it instead of recording a negative
/// marker, because the Deezer photo exists and the refusal may be network-dependent.
fn fetch_commons(
    name: &str,
    mbid_hint: Option<&str>,
    stale: Option<PathBuf>,
    refusal: Option<FetchError>,
    dir: &Path,
    fetchers: &mut Fetchers<'_>,
) -> Result<PortraitOutcome, PortraitError> {
    let hinted_mbid = mbid_hint.map(str::trim).filter(|value| !value.is_empty());
    let mbid = match hinted_mbid {
        Some(mbid) => Some(mbid.to_owned()),
        None => match exact_name_mbid(name, fetchers.musicbrainz_text) {
            Ok(mbid) => mbid,
            Err(error) => return stale_or(dir, name, stale, error),
        },
    };
    let Some(mbid) = mbid else {
        return cache_miss(dir, name, refusal);
    };

    let relations = match (fetchers.musicbrainz_text)(&wikimedia::musicbrainz_relations_url(&mbid))
    {
        Ok(body) => body,
        Err(FetchError::HttpStatus(404)) if hinted_mbid.is_some() => {
            let fallback_mbid = match exact_name_mbid(name, fetchers.musicbrainz_text) {
                Ok(mbid) => mbid,
                Err(error) => return stale_or(dir, name, stale, error),
            };
            let Some(fallback_mbid) = fallback_mbid else {
                return cache_miss(dir, name, refusal);
            };
            match (fetchers.musicbrainz_text)(&wikimedia::musicbrainz_relations_url(&fallback_mbid))
            {
                Ok(body) => body,
                Err(error) => return stale_or(dir, name, stale, error.into()),
            }
        }
        Err(error) => return stale_or(dir, name, stale, error.into()),
    };
    let qid = match wikimedia::parse_wikidata_qid(&relations) {
        Ok(qid) => qid,
        Err(()) => return stale_or(dir, name, stale, PortraitError::InvalidResponse),
    };
    let Some(qid) = qid else {
        return cache_miss(dir, name, refusal);
    };
    let claims = match (fetchers.wikimedia_text)(&wikimedia::wikidata_claims_url(&qid)) {
        Ok(body) => body,
        Err(error) => return stale_or(dir, name, stale, error.into()),
    };
    let filename = match wikimedia::parse_p18_filename(&claims, &qid) {
        Ok(filename) => filename,
        Err(()) => return stale_or(dir, name, stale, PortraitError::InvalidResponse),
    };
    let Some(filename) = filename else {
        return cache_miss(dir, name, refusal);
    };
    let info = match (fetchers.wikimedia_text)(&wikimedia::commons_imageinfo_url(&filename)) {
        Ok(body) => body,
        Err(error) => return stale_or(dir, name, stale, error.into()),
    };
    let image = match wikimedia::parse_commons_imageinfo(&info) {
        Ok(image) => image,
        Err(()) => return stale_or(dir, name, stale, PortraitError::InvalidResponse),
    };
    let Some(image) = image else {
        return cache_miss(dir, name, refusal);
    };
    if !wikimedia::is_download_url(&image.thumbnail_url) {
        return stale_or(dir, name, stale, PortraitError::InvalidResponse);
    }
    let bytes = match (fetchers.image)(&image.thumbnail_url) {
        Ok(bytes) => bytes,
        Err(error) => return stale_or(dir, name, stale, error.into()),
    };
    let Some(extension) = crate::cover_download::decode_image(&bytes)
        .as_ref()
        .and_then(crate::cover_download::DecodedImage::validated_extension)
    else {
        return stale_or(dir, name, stale, PortraitError::InvalidResponse);
    };
    let Some(path) = cache::store_portrait(
        dir,
        name,
        &bytes,
        extension,
        &PortraitRecord::commons(image.credit),
    ) else {
        return stale_or(dir, name, stale, PortraitError::InvalidResponse);
    };
    Ok(PortraitOutcome::Found(path))
}

fn exact_name_mbid(
    name: &str,
    musicbrainz_text: &mut dyn FnMut(&str) -> Result<String, FetchError>,
) -> Result<Option<String>, PortraitError> {
    let body = musicbrainz_text(&artist_search_url(name))?;
    if !musicbrainz_search_answered(&body) {
        return Err(PortraitError::InvalidResponse);
    }
    Ok(match parse_artist_mbid(&body, name) {
        ArtistMatch::Found(mbid) => Some(mbid),
        ArtistMatch::Ambiguous | ArtistMatch::NotFound => None,
    })
}

fn cache_miss(
    dir: &Path,
    name: &str,
    refusal: Option<FetchError>,
) -> Result<PortraitOutcome, PortraitError> {
    if let Some(refusal) = refusal {
        return Err(refusal.into());
    }
    cache::write_negative(dir, name);
    Ok(PortraitOutcome::NotFound)
}

fn musicbrainz_search_answered(json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|value| value.get("artists").cloned())
        .is_some_and(|artists| artists.is_array())
}

fn warn_placeholder(name: &str, url: &str, distance: f64, message: &'static str) {
    let image_identifier = deezer::image_identifier(url).unwrap_or_else(|| "unknown".into());
    tracing::warn!(
        artist = name,
        image_identifier,
        placeholder_distance = distance,
        message
    );
}
