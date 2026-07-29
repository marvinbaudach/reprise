//! A resolved, not-yet-confirmed radio station — either probed from a raw
//! stream URL (`with_probe`) or matched against a radio-browser search
//! candidate (`with_candidate`). Split out of `add_dialog.rs` purely for
//! file size; it has no dependency on `RadioAddDialog` itself.

use reprise_core::radio::icy::IcyProbe;
use reprise_core::radio::playlist::PlaylistKind;
use reprise_core::radio::search::StationCandidate;
use reprise_core::radio::{self};

use super::add_dialog::playlist_kind;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StationPreview {
    pub name: String,
    pub stream_url: String,
    pub uuid: Option<String>,
    pub favicon_url: Option<String>,
    pub genre: Option<String>,
    pub codec: Option<String>,
    pub bitrate_kbps: Option<i64>,
    pub country_code: Option<String>,
    pub votes: Option<i64>,
    pub playlist_kind: Option<PlaylistKind>,
}

impl StationPreview {
    pub(super) fn manual(name: &str, stream_url: &str) -> Self {
        Self {
            name: name.into(),
            stream_url: stream_url.into(),
            uuid: None,
            favicon_url: None,
            genre: None,
            codec: None,
            bitrate_kbps: None,
            country_code: None,
            votes: None,
            playlist_kind: playlist_kind(stream_url),
        }
    }

    pub(super) fn with_probe(mut self, probe: IcyProbe) -> Self {
        if let Some(name) = probe.name {
            self.name = name;
        }
        self.genre = probe.genre;
        self.codec = probe.content_type;
        self.bitrate_kbps = probe.bitrate_kbps;
        self
    }

    pub(super) fn with_candidate(mut self, candidate: StationCandidate) -> Self {
        self.uuid = Some(candidate.uuid);
        self.name = candidate.name;
        self.favicon_url = candidate.favicon_url;
        self.genre = candidate.genre;
        self.codec = candidate.codec;
        self.bitrate_kbps = candidate.bitrate_kbps;
        self.country_code = candidate.country_code;
        self.votes = Some(candidate.votes);
        self
    }

    pub(super) fn into_new_station(self) -> radio::station::NewStation {
        radio::station::NewStation {
            uuid: self.uuid,
            name: self.name,
            stream_url: self.stream_url,
            homepage: None,
            favicon_url: self.favicon_url,
            genre: self.genre,
            codec: self.codec,
            bitrate_kbps: self.bitrate_kbps,
            country_code: self.country_code,
            votes: self.votes,
        }
    }
}
