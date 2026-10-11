//! Typed field projections for external podcast and radio media.

use super::external_media_state::{EpisodeSource, ExternalMedia};

/// The podcast half of `media`, or `None` for radio media, which a podcast
/// caller never passes.
pub(super) fn podcast_fields(
    media: &ExternalMedia,
) -> Option<(String, String, EpisodeSource, i64, Option<i64>)> {
    let ExternalMedia::Podcast {
        title,
        show,
        source,
        resume_ms,
        duration_ms,
        ..
    } = media
    else {
        invariant_broken!("podcast fields requested from radio media");
        return None;
    };
    Some((
        title.clone(),
        show.clone(),
        source.clone(),
        *resume_ms,
        *duration_ms,
    ))
}

pub(super) fn session_id(media: &ExternalMedia) -> i64 {
    match media {
        ExternalMedia::Podcast { episode_id, .. } => *episode_id,
        ExternalMedia::Radio { station_id, .. } => *station_id,
    }
}
