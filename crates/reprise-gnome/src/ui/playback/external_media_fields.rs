//! Typed field projections for external podcast and radio media.

use super::external_media_state::{EpisodeSource, ExternalMedia};

pub(super) fn podcast_fields(
    media: &ExternalMedia,
) -> (String, String, EpisodeSource, i64, Option<i64>) {
    let ExternalMedia::Podcast {
        title,
        show,
        source,
        resume_ms,
        duration_ms,
        ..
    } = media
    else {
        unreachable!("podcast fields requested from radio media")
    };
    (
        title.clone(),
        show.clone(),
        source.clone(),
        *resume_ms,
        *duration_ms,
    )
}

pub(super) fn session_id(media: &ExternalMedia) -> i64 {
    match media {
        ExternalMedia::Podcast { episode_id, .. } => *episode_id,
        ExternalMedia::Radio { station_id, .. } => *station_id,
    }
}

pub(super) fn radio_fields(media: &ExternalMedia) -> (String, String, Option<String>) {
    let ExternalMedia::Radio {
        name,
        stream_url,
        uuid,
        ..
    } = media
    else {
        unreachable!("radio fields requested from podcast media")
    };
    (name.clone(), stream_url.clone(), uuid.clone())
}
