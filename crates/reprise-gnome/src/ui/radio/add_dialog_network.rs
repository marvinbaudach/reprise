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
    let mut preview = StationPreview::manual(
        probe
            .name
            .as_deref()
            .unwrap_or(strings::RADIO_STREAM_DETECTED),
        &stream_url,
    )
    .with_probe(probe);
    preview.playlist_kind = kind;
    if fetch_metadata {
        if let Some(candidate) = radio::search::find_by_url(&stream_url)? {
            preview = preview.with_candidate(candidate);
        }
    }
    Ok(preview)
}

pub(super) fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}
