use std::path::Path;

use lofty::file::TaggedFile;
use lofty::prelude::*;
use lofty::tag::items::Timestamp;
use lofty::tag::{ItemKey, Tag};
use rusqlite::Connection;

use super::tag_edit::TagEditError;
use super::tag_mutation::{
    classify_write_error, reconcile_after_write, save_loaded_tagged, validate_registered_track,
    TagMutationFailure, WriteErrorKind, IGNORE_DURATION,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum GuardedTagField {
    Title,
    Artist,
    Album,
    AlbumArtist,
    Year,
    TrackNo,
    Genre,
    RecordingMbid,
}

impl GuardedTagField {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Artist => "artist",
            Self::Album => "album",
            Self::AlbumArtist => "album_artist",
            Self::Year => "year",
            Self::TrackNo => "track_no",
            Self::Genre => "genre",
            Self::RecordingMbid => "recording_mbid",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "title" => Some(Self::Title),
            "artist" => Some(Self::Artist),
            "album" => Some(Self::Album),
            "album_artist" => Some(Self::AlbumArtist),
            "year" => Some(Self::Year),
            "track_no" => Some(Self::TrackNo),
            "genre" => Some(Self::Genre),
            "recording_mbid" => Some(Self::RecordingMbid),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GuardedTagChange {
    pub(crate) field: GuardedTagField,
    pub(crate) expected: Option<String>,
    pub(crate) after: Option<String>,
}

#[derive(Debug)]
pub(crate) struct GuardedTagCommit {
    pub(crate) applied: Vec<GuardedTagField>,
    pub(crate) conflicts: Vec<GuardedTagField>,
    pub(crate) post_write_failure: Option<TagMutationFailure>,
}

fn tagged_value(tagged: &TaggedFile, field: GuardedTagField) -> Option<String> {
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
    match field {
        GuardedTagField::Title => Some(
            tag.and_then(Accessor::title)
                .unwrap_or_default()
                .to_string(),
        ),
        GuardedTagField::Artist => Some(
            tag.and_then(Accessor::artist)
                .unwrap_or_default()
                .to_string(),
        ),
        GuardedTagField::Album => Some(
            tag.and_then(Accessor::album)
                .unwrap_or_default()
                .to_string(),
        ),
        GuardedTagField::AlbumArtist => Some(
            tag.and_then(|tag| tag.get_string(ItemKey::AlbumArtist))
                .unwrap_or_default()
                .to_string(),
        ),
        GuardedTagField::Year => tag
            .and_then(Accessor::date)
            .map(|date| date.year.to_string()),
        GuardedTagField::TrackNo => tag.and_then(Accessor::track).map(|value| value.to_string()),
        GuardedTagField::Genre => Some(
            tag.and_then(Accessor::genre)
                .unwrap_or_default()
                .to_string(),
        ),
        GuardedTagField::RecordingMbid => Some(
            tag.and_then(|tag| tag.get_string(ItemKey::MusicBrainzRecordingId))
                .unwrap_or_default()
                .to_string(),
        ),
    }
}

pub(crate) fn read_tag_field_values(
    path: &Path,
    fields: &[GuardedTagField],
) -> Result<Vec<(GuardedTagField, Option<String>)>, TagEditError> {
    let tagged = lofty::read_from_path(path)?;
    Ok(fields
        .iter()
        .map(|field| (*field, tagged_value(&tagged, *field)))
        .collect())
}

fn ensure_writable_tag(tagged: &mut TaggedFile) -> Result<&mut Tag, TagEditError> {
    if tagged.primary_tag().is_none() && tagged.first_tag().is_none() {
        tagged.insert_tag(Tag::new(tagged.primary_tag_type()));
    }
    if tagged.primary_tag().is_some() {
        tagged.primary_tag_mut()
    } else {
        tagged.first_tag_mut()
    }
    .ok_or(TagEditError::NoWritableTag)
}

fn apply_guarded_value(tag: &mut Tag, change: &GuardedTagChange) {
    let text = change.after.clone().unwrap_or_default();
    match change.field {
        GuardedTagField::Title if text.is_empty() => tag.remove_title(),
        GuardedTagField::Title => tag.set_title(text),
        GuardedTagField::Artist if text.is_empty() => tag.remove_artist(),
        GuardedTagField::Artist => tag.set_artist(text),
        GuardedTagField::Album if text.is_empty() => tag.remove_album(),
        GuardedTagField::Album => tag.set_album(text),
        GuardedTagField::AlbumArtist if text.is_empty() => {
            tag.remove_key(ItemKey::AlbumArtist);
        }
        GuardedTagField::AlbumArtist => {
            tag.insert_text(ItemKey::AlbumArtist, text);
        }
        GuardedTagField::Year => {
            match change.after.as_deref().and_then(|value| value.parse().ok()) {
                Some(year) => tag.set_date(Timestamp {
                    year,
                    ..Timestamp::default()
                }),
                None => tag.remove_date(),
            }
        }
        GuardedTagField::TrackNo => {
            match change.after.as_deref().and_then(|value| value.parse().ok()) {
                Some(track_no) => tag.set_track(track_no),
                None => tag.remove_track(),
            }
        }
        GuardedTagField::Genre if text.is_empty() => tag.remove_genre(),
        GuardedTagField::Genre => tag.set_genre(text),
        GuardedTagField::RecordingMbid if text.is_empty() => {
            tag.remove_key(ItemKey::MusicBrainzRecordingId);
        }
        GuardedTagField::RecordingMbid => {
            tag.insert_text(ItemKey::MusicBrainzRecordingId, text);
        }
    }
}

fn after_value_is_valid(change: &GuardedTagChange) -> bool {
    match (change.field, change.after.as_deref()) {
        (GuardedTagField::Year, Some(value)) => value.parse::<u16>().is_ok(),
        (GuardedTagField::TrackNo, Some(value)) => value.parse::<u32>().is_ok(),
        _ => true,
    }
}

pub(crate) fn commit_guarded_tag_changes(
    conn: &mut Connection,
    id: i64,
    path: &Path,
    changes: &[GuardedTagChange],
    ignore_watcher: bool,
) -> Result<GuardedTagCommit, TagMutationFailure> {
    if changes.iter().any(|change| !after_value_is_valid(change)) {
        return Err(TagMutationFailure {
            kind: WriteErrorKind::Io,
            error: "guarded numeric tag value is invalid".into(),
            file_written: false,
        });
    }
    validate_registered_track(conn, id, path).map_err(|error| TagMutationFailure {
        kind: WriteErrorKind::NotFound,
        error,
        file_written: false,
    })?;
    let mut tagged = lofty::read_from_path(path).map_err(|error| {
        let error = TagEditError::from(error);
        TagMutationFailure {
            kind: classify_write_error(&error),
            error: error.to_string(),
            file_written: false,
        }
    })?;
    let mut valid = Vec::new();
    let mut conflicts = Vec::new();
    for change in changes {
        if tagged_value(&tagged, change.field) == change.expected {
            valid.push(change);
        } else {
            conflicts.push(change.field);
        }
    }
    if valid.is_empty() {
        return Ok(GuardedTagCommit {
            applied: Vec::new(),
            conflicts,
            post_write_failure: None,
        });
    }
    let tag = ensure_writable_tag(&mut tagged).map_err(|error| TagMutationFailure {
        kind: classify_write_error(&error),
        error: error.to_string(),
        file_written: false,
    })?;
    for change in &valid {
        apply_guarded_value(tag, change);
    }
    if ignore_watcher {
        super::watcher::ignore_path(path, IGNORE_DURATION);
    }
    save_loaded_tagged(&tagged, path).map_err(|error| TagMutationFailure {
        kind: classify_write_error(&error),
        error: error.to_string(),
        file_written: false,
    })?;
    let post_write_failure =
        reconcile_after_write(conn, id, path)
            .err()
            .map(|error| TagMutationFailure {
                kind: WriteErrorKind::Io,
                error,
                file_written: true,
            });
    Ok(GuardedTagCommit {
        applied: valid.iter().map(|change| change.field).collect(),
        conflicts,
        post_write_failure,
    })
}
