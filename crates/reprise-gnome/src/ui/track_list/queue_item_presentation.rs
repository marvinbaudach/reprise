//! Pure presentation decisions for typed rows in the shared Queue surfaces.

use reprise_core::models::Track;
use reprise_core::podcasts::{EpisodeRow, PodcastKind};
use reprise_core::queries::QueueItemMetadata;
use reprise_core::up_next::QueueItem;

pub(in crate::ui) fn track(item: &QueueItemMetadata) -> Option<&Track> {
    match item {
        QueueItemMetadata::Track(track) => Some(track),
        QueueItemMetadata::Episode(_) => None,
    }
}

pub(in crate::ui) fn episode(item: &QueueItemMetadata) -> Option<&EpisodeRow> {
    match item {
        QueueItemMetadata::Track(_) => None,
        QueueItemMetadata::Episode(episode) => Some(episode),
    }
}

pub(in crate::ui) fn title(item: &QueueItemMetadata) -> &str {
    match item {
        QueueItemMetadata::Track(track) => &track.title,
        QueueItemMetadata::Episode(episode) => &episode.title,
    }
}

pub(in crate::ui) fn cell_text(item: &QueueItemMetadata, column: &str) -> String {
    match item {
        QueueItemMetadata::Track(track) => match column {
            "track_no" => track
                .track_no
                .map(|value| value.to_string())
                .unwrap_or_default(),
            "artist" => track.artist.clone(),
            "album" => track.album.clone(),
            "genre" => track.genre.clone(),
            "year" => track
                .year
                .map(|value| value.to_string())
                .unwrap_or_default(),
            "duration_ms" => reprise_core::format::format_duration(track.duration_ms),
            "added_at" => reprise_core::format::format_unix_timestamp(track.added_at),
            "play_count" => track.play_count.to_string(),
            _ => String::new(),
        },
        QueueItemMetadata::Episode(episode) => match column {
            "artist" => episode.show.clone(),
            "duration_ms" => reprise_core::format::format_duration(
                episode
                    .duration_secs
                    .unwrap_or_default()
                    .saturating_mul(1_000),
            ),
            _ => String::new(),
        },
    }
}

pub(in crate::ui) fn source_icon(item: &QueueItemMetadata) -> Option<&'static str> {
    let kind = episode(item)?.kind;
    Some(match kind {
        PodcastKind::Rss => "application-rss+xml-symbolic",
        PodcastKind::Youtube => "video-x-generic-symbolic",
    })
}

/// Whether this row is the one currently playing.
///
/// A track id and an episode id are separate primary keys and can be the same
/// number, so the comparison is made per kind — asking "does the playing track
/// id equal this row's id" would mark the wrong row the moment
/// `tracks.id == podcast_episodes.id`. Kept pure and here rather than inline at
/// the four column call sites so the two halves cannot drift apart.
pub(in crate::ui) fn is_now_playing(
    item: &QueueItemMetadata,
    playing_track_id: Option<i64>,
    playing_episode_id: Option<i64>,
) -> bool {
    match item {
        QueueItemMetadata::Track(track) => playing_track_id == Some(track.id),
        QueueItemMetadata::Episode(episode) => playing_episode_id == Some(episode.id),
    }
}

pub(in crate::ui) fn rating_track_id(item: &QueueItemMetadata) -> Option<i64> {
    track(item).map(|track| track.id)
}

pub(in crate::ui) fn rating_write_target(item: QueueItem) -> Option<i64> {
    item.track_id()
}

#[cfg(test)]
mod tests {
    use reprise_core::podcasts::{EpisodeRow, PodcastKind};
    use reprise_core::queries::QueueItemMetadata;
    use reprise_core::up_next::QueueItem;

    use reprise_core::models::Track;

    use super::{
        cell_text, is_now_playing, rating_track_id, rating_write_target, source_icon, title,
    };

    fn episode(kind: PodcastKind) -> QueueItemMetadata {
        QueueItemMetadata::Episode(EpisodeRow {
            id: 7,
            subscription_id: 1,
            guid: "episode-seven".into(),
            title: "Episode Seven".into(),
            show: "Systems Weekly".into(),
            show_image_url: None,
            image_url: None,
            kind,
            audio_url: "https://example.test/seven.mp3".into(),
            page_url: None,
            published_at: None,
            duration_secs: Some(90),
            downloaded_path: None,
            downloaded_bytes: None,
            played_at: None,
            position_ms: 0,
            first_seen_at: 0,
            is_new: true,
        })
    }

    #[test]
    fn episode_row_projects_only_title_show_duration_and_source_glyph() {
        let rss = episode(PodcastKind::Rss);

        assert_eq!(title(&rss), "Episode Seven");
        assert_eq!(cell_text(&rss, "artist"), "Systems Weekly");
        assert_eq!(cell_text(&rss, "duration_ms"), "1:30");
        for column in [
            "album",
            "album_artist",
            "year",
            "track_no",
            "genre",
            "bitrate_kbps",
            "play_count",
            "last_played_at",
            "rating",
            "path",
            "added_at",
        ] {
            assert_eq!(cell_text(&rss, column), "", "{column} must stay blank");
        }
        assert_eq!(source_icon(&rss), Some("application-rss+xml-symbolic"));
        assert_eq!(
            source_icon(&episode(PodcastKind::Youtube)),
            Some("video-x-generic-symbolic")
        );
        assert_eq!(rating_track_id(&rss), None);
        assert_eq!(rating_write_target(QueueItem::Episode(7)), None);
        assert_eq!(rating_write_target(QueueItem::Track(7)), Some(7));
    }

    /// A track row with the same numeric id as the episode fixture. Built out
    /// in full because `Track` has no `Default` — the point of the test is the
    /// shared id, so it must be a real one.
    fn track_with_colliding_id() -> QueueItemMetadata {
        QueueItemMetadata::Track(Track {
            id: 7,
            path: "/music/seven.flac".into(),
            title: "Track Seven".into(),
            artist: "Someone".into(),
            album: String::new(),
            album_artist: String::new(),
            year: None,
            track_no: None,
            genre: String::new(),
            duration_ms: 90_000,
            bitrate_kbps: None,
            rating: 0,
            play_count: 0,
            last_played_at: None,
            added_at: 0,
            file_mtime: 0,
            missing_since: None,
            missing_reason: None,
            untagged: false,
            file_size: 0,
            device: None,
            inode: None,
            playlist_position: None,
            is_ai: false,
        })
    }

    /// The whole reason `is_now_playing` compares per kind rather than against
    /// a single id: a track id and an episode id are different keys, and 7 == 7.
    /// Without the per-kind match, playing episode 7 would light up track 7's
    /// row instead.
    #[test]
    fn que_9_a_colliding_episode_id_never_marks_a_track_row_as_playing() {
        let track = track_with_colliding_id();
        let episode = episode(PodcastKind::Youtube);

        // The episode is playing: only the episode row may be marked.
        assert!(!is_now_playing(&track, None, Some(7)));
        assert!(is_now_playing(&episode, None, Some(7)));

        // The track is playing: only the track row may be marked.
        assert!(is_now_playing(&track, Some(7), None));
        assert!(!is_now_playing(&episode, Some(7), None));

        // Nothing playing marks nothing.
        assert!(!is_now_playing(&track, None, None));
        assert!(!is_now_playing(&episode, None, None));
    }
}
