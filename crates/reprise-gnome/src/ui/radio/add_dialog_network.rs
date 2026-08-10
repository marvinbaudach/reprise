//! Provider lookup and station conversion for the Radio add dialog.

use reprise_core::radio::search::StationCandidate;
use reprise_core::radio::{self, RadioError};

use super::add_dialog::playlist_kind;
use super::station_preview::StationPreview;
use crate::ui::strings;

pub(super) fn station_from_candidate(candidate: StationCandidate) -> radio::station::NewStation {
    radio::station::NewStation {
        uuid: Some(candidate.uuid),
        name: candidate.name,
        stream_url: candidate.url_resolved,
        homepage: None,
        favicon_url: candidate.favicon_url,
        genre: candidate.genre,
        codec: candidate.codec,
        bitrate_kbps: candidate.bitrate_kbps,
        country_code: candidate.country_code,
        votes: Some(candidate.votes),
    }
}

pub(super) fn preview_name_claim(icy_name: Option<&str>) -> (String, Option<String>) {
    match icy_name.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => (name.to_owned(), Some(name.to_owned())),
        None => (strings::RADIO_STREAM_DETECTED.to_owned(), None),
    }
}

pub(super) fn preview_url(url: &str, fetch_metadata: bool) -> Result<StationPreview, RadioError> {
    let kind = playlist_kind(url);
    let stream_url = match kind {
        Some(kind) => {
            let body = radio::http::get(url)?;
            if radio::playlist::is_hls_manifest(&body) {
                url.to_owned()
            } else {
                radio::playlist::resolve_playlist(&body, kind).ok_or_else(|| {
                    RadioError::Parse("playlist did not contain a playable stream URL".into())
                })?
            }
        }
        None => url.to_owned(),
    };
    let probe = radio::icy::probe(&stream_url)?;
    let (display_name, mut name_claim) = preview_name_claim(probe.name.as_deref());
    let mut preview = StationPreview::manual(&display_name, &stream_url).with_probe(probe);
    preview.playlist_kind = kind;
    if fetch_metadata {
        if let Ok(Some(candidate)) = radio::search::find_by_url(&stream_url) {
            preview = preview.with_candidate(candidate);
            if preview.uuid.is_some() {
                name_claim = Some(preview.name.clone());
            }
        }
        if preview.favicon_url.is_none() {
            if let Some(name) = name_claim.as_deref() {
                let candidates = radio::search::search(name, radio::search::SearchOrder::Votes)
                    .unwrap_or_default();
                preview = preview.with_favicon_candidates(&candidates);
            }
        }
    }
    Ok(preview)
}

pub(super) fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}
