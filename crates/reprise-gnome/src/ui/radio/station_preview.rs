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
        let same_uuid = self
            .uuid
            .as_deref()
            .is_some_and(|uuid| uuid == candidate.uuid);
        if !same_uuid && self.stream_url != candidate.url_resolved {
            return self;
        }
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

    pub(super) fn with_favicon_candidates(mut self, candidates: &[StationCandidate]) -> Self {
        if self.favicon_url.is_some() {
            return self;
        }
        if let Some(uuid) = self.uuid.as_deref() {
            if let Some(favicon) = candidates
                .iter()
                .filter(|candidate| candidate.uuid == uuid)
                .find_map(|candidate| candidate.favicon_url.clone())
            {
                self.favicon_url = Some(favicon);
                return self;
            }
        }
        if let Some(favicon) = candidates
            .iter()
            .filter(|candidate| candidate.url_resolved == self.stream_url)
            .find_map(|candidate| candidate.favicon_url.clone())
        {
            self.favicon_url = Some(favicon);
            return self;
        }
        let mut exact_names = candidates
            .iter()
            .filter(|candidate| candidate.name.trim().eq_ignore_ascii_case(self.name.trim()));
        let Some(candidate) = exact_names.next() else {
            return self;
        };
        if exact_names.next().is_none() {
            self.favicon_url.clone_from(&candidate.favicon_url);
        }
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
