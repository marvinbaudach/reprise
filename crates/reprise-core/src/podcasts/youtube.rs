//! YouTube podcast provider projections.

use super::ytdlp::{YtDlpPlaylist, YtDlpVideo};

const WATCH_URL_PREFIX: &str = "https://www.youtube.com/watch?v=";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YoutubeListing {
    pub title: Option<String>,
    pub episodes: Vec<YoutubeEpisode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YoutubeEpisode {
    /// The YouTube video ID is the stable episode identity.
    pub guid: String,
    pub title: String,
    /// The durable watch URL, never the expiring best-audio stream URL.
    pub audio_url: String,
    /// Flat playlists do not provide a dependable publication date.
    pub published_at: Option<i64>,
    pub duration_secs: Option<i64>,
}

pub fn project_playlist(playlist: YtDlpPlaylist) -> YoutubeListing {
    YoutubeListing {
        title: playlist.title,
        episodes: playlist.entries.into_iter().map(project_video).collect(),
    }
}

pub fn project_video(video: YtDlpVideo) -> YoutubeEpisode {
    YoutubeEpisode {
        audio_url: format!("{WATCH_URL_PREFIX}{}", video.id),
        guid: video.id,
        title: video.title,
        published_at: None,
        duration_secs: video.duration_secs,
    }
}

#[cfg(test)]
mod tests {
    use super::{project_playlist, project_video, YoutubeEpisode};
    use crate::podcasts::ytdlp::{YtDlpPlaylist, YtDlpVideo};

    #[test]
    fn flat_playlist_projects_stable_episode_identity_in_source_order() {
        let listing = project_playlist(YtDlpPlaylist {
            title: Some("The Channel".to_owned()),
            source_url: None,
            image_url: None,
            entries: vec![
                YtDlpVideo {
                    id: "second".to_owned(),
                    title: "Second in playlist order".to_owned(),
                    duration_secs: None,
                },
                YtDlpVideo {
                    id: "first".to_owned(),
                    title: "First in playlist order".to_owned(),
                    duration_secs: Some(75),
                },
            ],
        });

        assert_eq!(listing.title.as_deref(), Some("The Channel"));
        assert_eq!(
            listing.episodes,
            vec![
                YoutubeEpisode {
                    guid: "second".to_owned(),
                    title: "Second in playlist order".to_owned(),
                    audio_url: "https://www.youtube.com/watch?v=second".to_owned(),
                    published_at: None,
                    duration_secs: None,
                },
                YoutubeEpisode {
                    guid: "first".to_owned(),
                    title: "First in playlist order".to_owned(),
                    audio_url: "https://www.youtube.com/watch?v=first".to_owned(),
                    published_at: None,
                    duration_secs: Some(75),
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
        });

        assert_eq!(episode.guid, "video-id");
        assert_eq!(
            episode.audio_url,
            "https://www.youtube.com/watch?v=video-id"
        );
        assert!(!episode.audio_url.contains("googlevideo"));
    }
}
