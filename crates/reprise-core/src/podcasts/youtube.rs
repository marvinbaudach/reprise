//! YouTube podcast provider projections.

use chrono::NaiveDate;

use super::feed::ParsedFeed;
use super::ytdlp::{YtDlpPlaylist, YtDlpVideo};

const WATCH_URL_PREFIX: &str = "https://www.youtube.com/watch?v=";
const THUMBNAIL_URL_PREFIX: &str = "https://i.ytimg.com/vi/";
const CHANNEL_PATH: &str = "/channel/";
const LONG_FORM_FEED_PREFIX: &str = "https://www.youtube.com/feeds/videos.xml?playlist_id=UULF";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YoutubeListing {
    pub title: Option<String>,
    pub channel: Option<String>,
    pub episodes: Vec<YoutubeEpisode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YoutubeEpisode {
    /// The YouTube video ID is the stable episode identity.
    pub guid: String,
    pub title: String,
    /// The durable watch URL, never the expiring best-audio stream URL.
    pub audio_url: String,
    /// Day-granular upload timestamp requested from yt-dlp's channel listing.
    pub published_at: Option<i64>,
    pub duration_secs: Option<i64>,
    pub image_url: Option<String>,
}

pub fn project_playlist(playlist: YtDlpPlaylist) -> YoutubeListing {
    YoutubeListing {
        title: playlist.title,
        channel: playlist.channel,
        episodes: playlist.entries.into_iter().map(project_video).collect(),
    }
}

pub fn subscription_title(feed: &ParsedFeed) -> Option<&str> {
    feed.author
        .as_deref()
        .map(str::trim)
        .filter(|author| !author.is_empty())
}

pub fn project_video(video: YtDlpVideo) -> YoutubeEpisode {
    YoutubeEpisode {
        audio_url: format!("{WATCH_URL_PREFIX}{}", video.id),
        guid: video.id,
        title: video.title,
        published_at: video
            .timestamp
            .or_else(|| upload_date_timestamp(video.upload_date.as_deref())),
        duration_secs: video.duration_secs,
        image_url: video.image_url,
    }
}

/// The thumbnail URL for a video id, or `None` when the id is not shaped like
/// one.
///
/// The id reaches this function from a remote feed by way of yt-dlp, so it is
/// validated rather than trusted: only the YouTube id alphabet is admitted, so
/// a `/`, `?`, `..` or whitespace can never reshape the path this builds. The
/// origin is a hardcoded literal, so a malformed id could not have redirected
/// the fetch to another host either way — this keeps a bad id from becoming a
/// pointless request and a wasted cache entry, and turns "yt-dlp's `id` really
/// is a video id" from an implicit assumption into a checked one.
#[must_use]
pub fn thumbnail_url(video_id: &str) -> Option<String> {
    if video_id.is_empty()
        || !video_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    Some(format!("{THUMBNAIL_URL_PREFIX}{video_id}/hqdefault.jpg"))
}

fn upload_date_timestamp(value: Option<&str>) -> Option<i64> {
    NaiveDate::parse_from_str(value?, "%Y%m%d")
        .ok()?
        .and_hms_opt(0, 0, 0)
        .map(|date| date.and_utc().timestamp())
}

#[must_use]
pub fn long_form_feed_url(channel_url: &str) -> Option<String> {
    let url = url::Url::parse(channel_url).ok()?;
    let host = url.host_str()?.trim_start_matches("www.");
    if host != "youtube.com" || !url.path().starts_with(CHANNEL_PATH) {
        return None;
    }
    let channel_id = url.path().trim_start_matches(CHANNEL_PATH);
    let suffix = channel_id.strip_prefix("UC")?;
    if suffix.is_empty() || suffix.contains('/') {
        return None;
    }
    Some(format!("{LONG_FORM_FEED_PREFIX}{suffix}"))
}

#[cfg(test)]
mod tests {
    use super::{project_playlist, project_video, thumbnail_url, YoutubeEpisode};
    use crate::podcasts::ytdlp::{YtDlpPlaylist, YtDlpVideo};

    #[test]
    fn src_11_thumbnail_url_uses_the_plain_video_id() {
        assert_eq!(
            thumbnail_url("9fCfJzK0ZE4").as_deref(),
            Some("https://i.ytimg.com/vi/9fCfJzK0ZE4/hqdefault.jpg")
        );
    }

    #[test]
    fn src_11_thumbnail_url_rejects_an_id_that_could_reshape_the_path() {
        // The id arrives from a remote feed via yt-dlp; anything outside the
        // YouTube id alphabet must not be interpolated into the URL.
        for bogus in [
            "",
            "../../etc/passwd",
            "abc/def",
            "abc?x=1",
            "abc def",
            "abc#frag",
        ] {
            assert_eq!(thumbnail_url(bogus), None, "must reject {bogus:?}");
        }
    }

    #[test]
    fn src_11_project_video_passes_through_provider_artwork_without_deriving() {
        let supplied = project_video(YtDlpVideo {
            id: "supplied".to_owned(),
            title: "Supplied artwork".to_owned(),
            duration_secs: None,
            timestamp: None,
            upload_date: None,
            image_url: Some("https://img.test/provider.jpg".to_owned()),
        });
        let absent = project_video(YtDlpVideo {
            id: "absent".to_owned(),
            title: "Absent artwork".to_owned(),
            duration_secs: None,
            timestamp: None,
            upload_date: None,
            image_url: None,
        });

        assert_eq!(
            supplied.image_url.as_deref(),
            Some("https://img.test/provider.jpg")
        );
        assert_eq!(absent.image_url, None);
    }

    #[test]
    fn flat_playlist_projects_stable_episode_identity_in_source_order() {
        let listing = project_playlist(YtDlpPlaylist {
            title: Some("The Channel".to_owned()),
            channel: Some("Ferris Media".to_owned()),
            source_url: None,
            image_url: None,
            entries: vec![
                YtDlpVideo {
                    id: "second".to_owned(),
                    title: "Second in playlist order".to_owned(),
                    duration_secs: None,
                    timestamp: None,
                    upload_date: None,
                    image_url: None,
                },
                YtDlpVideo {
                    id: "first".to_owned(),
                    title: "First in playlist order".to_owned(),
                    duration_secs: Some(75),
                    timestamp: Some(1_700_000_000),
                    upload_date: None,
                    image_url: Some("https://img.test/first.jpg".to_owned()),
                },
            ],
        });

        assert_eq!(listing.title.as_deref(), Some("The Channel"));
        assert_eq!(listing.channel.as_deref(), Some("Ferris Media"));
        assert_eq!(
            listing.episodes,
            vec![
                YoutubeEpisode {
                    guid: "second".to_owned(),
                    title: "Second in playlist order".to_owned(),
                    audio_url: "https://www.youtube.com/watch?v=second".to_owned(),
                    published_at: None,
                    duration_secs: None,
                    image_url: None,
                },
                YoutubeEpisode {
                    guid: "first".to_owned(),
                    title: "First in playlist order".to_owned(),
                    audio_url: "https://www.youtube.com/watch?v=first".to_owned(),
                    published_at: Some(1_700_000_000),
                    duration_secs: Some(75),
                    image_url: Some("https://img.test/first.jpg".to_owned()),
                },
            ]
        );
    }

    #[test]
    fn provider_never_projects_an_ephemeral_audio_stream_url() {
        let episode = project_video(YtDlpVideo {
            id: "video-id".to_owned(),
            title: "Episode".to_owned(),
            duration_secs: Some(42),
            timestamp: None,
            upload_date: Some("20260730".to_owned()),
            image_url: None,
        });

        assert_eq!(episode.guid, "video-id");
        assert_eq!(
            episode.audio_url,
            "https://www.youtube.com/watch?v=video-id"
        );
        assert!(!episode.audio_url.contains("googlevideo"));
        assert_eq!(episode.published_at, Some(1_785_369_600));
    }

    #[test]
    fn project_video_keeps_a_timestamp_supplied_by_the_listing() {
        let episode = project_video(YtDlpVideo {
            id: "dated-video".to_owned(),
            title: "Dated episode".to_owned(),
            duration_secs: None,
            timestamp: Some(1_785_225_600),
            upload_date: None,
            image_url: None,
        });

        assert_eq!(episode.published_at, Some(1_785_225_600));
    }

    #[test]
    fn pod_10_channel_identity_maps_to_the_keyless_long_form_feed() {
        assert_eq!(
            super::long_form_feed_url("https://www.youtube.com/channel/UCabc123"),
            Some("https://www.youtube.com/feeds/videos.xml?playlist_id=UULFabc123".to_owned())
        );
        assert_eq!(
            super::long_form_feed_url("https://www.youtube.com/@channel"),
            None
        );
    }
}
