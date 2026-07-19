use lofty::prelude::*;
use lofty::tag::ItemKey;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::library::tag_edit::{EditableTags, TagEditError};

/// Embedded identities read with the editable tags in the same Lofty pass.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteEmbeddedIds {
    pub recording_mbid: Option<String>,
    pub release_mbid: Option<String>,
    pub release_group_mbid: Option<String>,
    pub artist_mbid: Option<String>,
    pub release_artist_mbid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RemoteDirectLookup {
    Recording(String),
    Release(String),
    ReleaseGroup(String),
    Artist(String),
    ReleaseArtist(String),
}

/// The complete allowlist crossing the remote resolver boundary.
///
/// Deliberately contains no track ID, path, filename, root, rating, history,
/// playlist, device or inode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteTrackMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<u32>,
    pub recording_mbid: Option<String>,
    pub release_mbid: Option<String>,
    pub release_group_mbid: Option<String>,
    pub artist_mbid: Option<String>,
    pub release_artist_mbid: Option<String>,
    pub duration_ms: Option<u64>,
}

impl RemoteTrackMetadata {
    #[allow(clippy::too_many_arguments)]
    pub fn from_actual_tags(
        _database_or_filename_title: &str,
        title: &str,
        artist: &str,
        album: &str,
        album_artist: &str,
        year: Option<u32>,
        ids: &RemoteEmbeddedIds,
        duration_ms: Option<u64>,
    ) -> Self {
        Self {
            title: raw_nonempty(title),
            artist: raw_nonempty(artist),
            album: raw_nonempty(album),
            album_artist: raw_nonempty(album_artist),
            year,
            recording_mbid: canonical_uuid(ids.recording_mbid.as_deref()),
            release_mbid: canonical_uuid(ids.release_mbid.as_deref()),
            release_group_mbid: canonical_uuid(ids.release_group_mbid.as_deref()),
            artist_mbid: canonical_uuid(ids.artist_mbid.as_deref()),
            release_artist_mbid: canonical_uuid(ids.release_artist_mbid.as_deref()),
            duration_ms: duration_ms.filter(|value| *value > 0),
        }
    }

    pub fn valid_recording_mbid(&self) -> Option<&str> {
        self.recording_mbid
            .as_deref()
            .filter(|value| canonical_uuid(Some(value)).is_some())
    }

    pub(crate) fn direct_lookups(&self) -> Vec<RemoteDirectLookup> {
        let mut lookups = Vec::with_capacity(5);
        push_lookup(
            &mut lookups,
            &self.recording_mbid,
            RemoteDirectLookup::Recording,
        );
        push_lookup(
            &mut lookups,
            &self.release_mbid,
            RemoteDirectLookup::Release,
        );
        push_lookup(
            &mut lookups,
            &self.release_group_mbid,
            RemoteDirectLookup::ReleaseGroup,
        );
        push_lookup(&mut lookups, &self.artist_mbid, RemoteDirectLookup::Artist);
        push_lookup(
            &mut lookups,
            &self.release_artist_mbid,
            RemoteDirectLookup::ReleaseArtist,
        );
        lookups
    }

    pub(super) fn lookup_title(&self) -> Option<&str> {
        lookup_text(self.title.as_deref())
    }

    pub(super) fn lookup_artist(&self) -> Option<&str> {
        lookup_text(self.artist.as_deref())
    }

    pub(super) fn lookup_album(&self) -> Option<&str> {
        lookup_text(self.album.as_deref())
    }
}

pub(crate) fn read_remote_metadata(
    path: &Path,
) -> Result<(EditableTags, RemoteTrackMetadata), TagEditError> {
    let tagged = lofty::read_from_path(path)?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
    let text = |key: ItemKey| {
        tag.and_then(|value| value.get_string(key))
            .map(str::to_owned)
            .unwrap_or_default()
    };
    let tags = EditableTags {
        title: tag
            .and_then(Accessor::title)
            .unwrap_or_default()
            .into_owned(),
        artist: tag
            .and_then(Accessor::artist)
            .unwrap_or_default()
            .into_owned(),
        album: tag
            .and_then(Accessor::album)
            .unwrap_or_default()
            .into_owned(),
        album_artist: text(ItemKey::AlbumArtist),
        year: tag
            .and_then(Accessor::date)
            .map(|date| u32::from(date.year)),
        track_no: tag.and_then(Accessor::track),
        genre: tag
            .and_then(Accessor::genre)
            .unwrap_or_default()
            .into_owned(),
    };
    let ids = RemoteEmbeddedIds {
        recording_mbid: tag_string(tag, ItemKey::MusicBrainzRecordingId),
        release_mbid: tag_string(tag, ItemKey::MusicBrainzReleaseId),
        release_group_mbid: tag_string(tag, ItemKey::MusicBrainzReleaseGroupId),
        artist_mbid: tag_string(tag, ItemKey::MusicBrainzArtistId),
        release_artist_mbid: tag_string(tag, ItemKey::MusicBrainzReleaseArtistId),
    };
    let duration_ms = u64::try_from(tagged.properties().duration().as_millis()).ok();
    let metadata = RemoteTrackMetadata::from_actual_tags(
        "",
        &tags.title,
        &tags.artist,
        &tags.album,
        &tags.album_artist,
        tags.year,
        &ids,
        duration_ms,
    );
    Ok((tags, metadata))
}

fn tag_string(tag: Option<&lofty::tag::Tag>, key: ItemKey) -> Option<String> {
    tag.and_then(|value| value.get_string(key))
        .and_then(|value| lookup_text(Some(value)).map(str::to_owned))
}

fn raw_nonempty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_owned())
}

fn lookup_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn push_lookup(
    lookups: &mut Vec<RemoteDirectLookup>,
    value: &Option<String>,
    create: impl FnOnce(String) -> RemoteDirectLookup,
) {
    if let Some(value) = canonical_uuid(value.as_deref()) {
        lookups.push(create(value));
    }
}

pub(super) fn canonical_uuid(value: Option<&str>) -> Option<String> {
    let value = value?.trim().to_ascii_lowercase();
    let bytes = value.as_bytes();
    let valid = bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        });
    valid.then_some(value)
}
