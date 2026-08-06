//! Podcast subscriptions, episodes, refresh, and provider boundaries.

use std::time::Duration;

use crate::source_error::{SourceError, SourceErrorKind};

pub mod channel_window;
pub mod config;
pub mod discovery;
pub mod download_state;
pub mod downloads;
pub mod episode_tags;
pub mod feed;
pub mod http;
pub mod itunes;
mod media_character;
pub mod offline_add;
pub mod phone_sync;
pub mod pipeline;
pub mod query;
pub mod queued_downloads;
pub mod refresh;
pub mod source_artwork;
pub mod status;
pub mod store;
mod store_metadata;
pub mod url_detect;
pub mod wanted_on_device;
pub mod youtube;
pub mod ytdlp;
mod ytdlp_download;
pub mod ytdlp_search;

pub use media_character::{character_from_category, MediaCharacter};

#[cfg(test)]
#[path = "podcasts/media_character_tests.rs"]
mod media_character_tests;

pub const YOUTUBE_BROWSER_RECOVERY_MESSAGE: &str =
    "YouTube needs a signed-in browser — choose one in Plugins";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PodcastKind {
    Rss,
    Youtube,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EpisodeStatus {
    New,
    Resume,
    Played,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubscriptionRow {
    pub id: i64,
    pub kind: PodcastKind,
    pub feed_url: String,
    pub title: String,
    pub author: Option<String>,
    pub image_url: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub last_fetch_at: Option<i64>,
    pub last_outcome: Option<String>,
    pub auto_download: bool,
    pub sync_to_phone: bool,
    /// `MTP-36`: this channel's override of the global "latest N per
    /// channel" default, or `None` to use the default
    /// (`podcasts::config::PodcastConfig::latest_per_channel_default`). An
    /// explicit `Some(0)` means unlimited, not "no override".
    pub latest_per_channel: Option<i64>,
    /// `POD-5`: this channel's override of the global "keep N downloaded"
    /// default, or `None` to use the default
    /// (`podcasts::config::PodcastConfig::keep_downloaded_default`). An
    /// explicit `Some(0)` means unlimited, not "no override" — same shape as
    /// `latest_per_channel` (`O-5`).
    pub keep_downloaded: Option<i64>,
    pub added_at: i64,
    pub removed_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpisodeRow {
    pub id: i64,
    pub subscription_id: i64,
    pub guid: String,
    pub title: String,
    pub show: String,
    pub show_image_url: Option<String>,
    pub image_url: Option<String>,
    pub kind: PodcastKind,
    pub audio_url: String,
    pub page_url: Option<String>,
    pub published_at: Option<i64>,
    pub duration_secs: Option<i64>,
    pub media_category: Option<String>,
    pub downloaded_path: Option<String>,
    pub downloaded_bytes: Option<i64>,
    pub played_at: Option<i64>,
    pub position_ms: i64,
    pub first_seen_at: i64,
    pub is_new: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceGroup {
    pub subscription_id: i64,
    pub title: String,
    pub author: Option<String>,
    pub image_url: Option<String>,
    pub kind: PodcastKind,
    pub sync_to_phone: bool,
    pub episodes: Vec<EpisodeRow>,
}

#[derive(Debug, thiserror::Error)]
pub enum PodcastError {
    #[error("request timed out")]
    Timeout,
    #[error("network request failed: {0}")]
    Transport(String),
    #[error("server returned HTTP {0}")]
    HttpStatus(u16),
    #[error("feed returned HTTP {0} and has moved or ended")]
    SourceGone(u16),
    #[error("server returned HTTP 429")]
    RateLimited { retry_after: Option<Duration> },
    #[error("response body could not be read: {0}")]
    Body(String),
    #[error("response could not be parsed: {0}")]
    Parse(String),
    #[error("not modified")]
    NotModified,
    #[error("{}", kind.user_message())]
    YtDlpFailure {
        kind: ytdlp::YtDlpFailureKind,
        stderr: String,
    },
    #[error("YouTube request timed out — try again")]
    YtDlpTimeout,
    /// The subscription's kind (RSS or YouTube) is disabled, either at its
    /// own module or the global online-sources gate (`NET-1a`).
    #[error("{0}")]
    Disabled(String),
    /// `POD-17`: the episode arrived, but writing its tags into the file
    /// failed and may have left it truncated. Not a provider failure at all
    /// — it lives here because this is the error the download closure has to
    /// return so `downloads::download_atomically` deletes the destroyed
    /// temporary instead of publishing it. Deliberately payload-free: there
    /// is nothing to say about it that `episode_tags::EpisodeTagError`'s own
    /// classified log line has not already said.
    #[error("the download could not be tagged")]
    TagWrite,
}

impl PodcastError {
    /// `POD-13`: a fixed, classified reason safe for UI display and
    /// normal-level logs. The `Display` impl above is deliberately not used
    /// for that — `Transport`/`Body`/`Parse`/`YtDlp` all carry a payload
    /// string that can echo the raw provider transport error, which in turn
    /// can echo the request URL (and an episode's `audio_url` may carry a
    /// private per-subscriber token the same way a feed URL can, `SRC-5`),
    /// or, for yt-dlp specifically, a local download path
    /// (`ytdlp::finalize_download`'s messages embed `Path::display()`).
    /// This is the single classifier for a podcast provider failure —
    /// `source_actions::podcast_source_error` (MCP) and
    /// `pipeline::download_episode` (the download executor) both call this
    /// rather than keeping their own copy that could drift from it.
    #[must_use]
    pub fn classify(&self) -> &'static str {
        let kind = SourceErrorKind::from(self);
        match (kind, self) {
            (SourceErrorKind::Unreachable, PodcastError::Timeout) => "podcast source timed out",
            (SourceErrorKind::Unreachable, PodcastError::Transport(_)) => {
                "podcast source could not be reached"
            }
            (SourceErrorKind::Unreachable, PodcastError::HttpStatus(_)) => {
                "podcast source returned an HTTP error"
            }
            // These two pairings cannot occur — `SourceGone` and `RateLimited`
            // map to their own kinds, matched below — but the tuple match has
            // to stay exhaustive, so they carry the same text their real kind
            // produces rather than a different one that could drift.
            (SourceErrorKind::Unreachable, PodcastError::SourceGone(_)) => {
                "podcast source has moved or ended"
            }
            (SourceErrorKind::Unreachable, PodcastError::RateLimited { .. }) => {
                "podcast source is rate limited"
            }
            (SourceErrorKind::Unreachable, PodcastError::Body(_) | PodcastError::Parse(_)) => {
                "podcast source returned invalid data"
            }
            (SourceErrorKind::Unreachable, PodcastError::NotModified) => {
                "podcast source was not modified"
            }
            (
                SourceErrorKind::RateLimited { .. },
                PodcastError::YtDlpFailure {
                    kind: ytdlp::YtDlpFailureKind::VerificationRequired,
                    ..
                },
            ) => YOUTUBE_BROWSER_RECOVERY_MESSAGE,
            (
                SourceErrorKind::Unreachable
                | SourceErrorKind::RateLimited { .. }
                | SourceErrorKind::HelperOutdated,
                PodcastError::YtDlpFailure { .. },
            ) => "YouTube source could not be read with yt-dlp",
            (SourceErrorKind::Unreachable, PodcastError::YtDlpTimeout) => {
                "YouTube source timed out"
            }
            (SourceErrorKind::Unreachable, PodcastError::Disabled(_)) => {
                "this source is disabled in Reprise preferences"
            }
            (SourceErrorKind::Unreachable, PodcastError::TagWrite) => {
                "podcast download could not be tagged"
            }
            (SourceErrorKind::Offline, _) => "podcast source is offline",
            (SourceErrorKind::SourceGone, _) => "podcast source has moved or ended",
            (SourceErrorKind::RateLimited { .. }, _) => "podcast source is rate limited",
            (SourceErrorKind::HelperOutdated, _) => "YouTube source could not be read with yt-dlp",
        }
    }

    /// Delay for a background refresh retry under the shared source policy.
    #[must_use]
    pub fn retry_delay(&self, attempt: u32) -> Option<Duration> {
        let retry_after = match self {
            Self::RateLimited { retry_after } => *retry_after,
            Self::HttpStatus(500..=599) | Self::Timeout | Self::Transport(_) => None,
            Self::YtDlpFailure { kind, .. } => {
                let SourceErrorKind::RateLimited { retry_after } = SourceErrorKind::from(*kind)
                else {
                    return None;
                };
                return crate::source_error::source_backoff_delay(attempt, retry_after);
            }
            _ => return None,
        };
        crate::source_error::source_backoff_delay(attempt, retry_after)
    }
}

impl From<&PodcastError> for SourceErrorKind {
    fn from(error: &PodcastError) -> Self {
        match error {
            PodcastError::SourceGone(_) => Self::SourceGone,
            PodcastError::RateLimited { retry_after } => Self::RateLimited {
                retry_after: *retry_after,
            },
            PodcastError::YtDlpFailure { kind, .. } => Self::from(*kind),
            PodcastError::Timeout
            | PodcastError::Transport(_)
            | PodcastError::HttpStatus(_)
            | PodcastError::Body(_)
            | PodcastError::Parse(_)
            | PodcastError::NotModified
            | PodcastError::YtDlpTimeout
            | PodcastError::Disabled(_)
            // Writing a tag failed locally. Not a provider or transport
            // problem, but it is transient in the same way and offers the same
            // action, so it shares the "try again" bucket rather than earning
            // a state of its own that no surface would render differently.
            | PodcastError::TagWrite => Self::Unreachable,
        }
    }
}

impl From<PodcastError> for SourceError {
    fn from(error: PodcastError) -> Self {
        let kind = SourceErrorKind::from(&error);
        let technical_cause = match &error {
            PodcastError::YtDlpFailure { stderr, .. } => stderr.clone(),
            _ => error.to_string(),
        };
        Self::new(kind, "podcast source request failed", technical_cause)
    }
}

impl From<ytdlp::YtDlpFailureKind> for SourceErrorKind {
    fn from(kind: ytdlp::YtDlpFailureKind) -> Self {
        match kind {
            ytdlp::YtDlpFailureKind::VerificationRequired
            | ytdlp::YtDlpFailureKind::RateLimited => Self::RateLimited { retry_after: None },
            ytdlp::YtDlpFailureKind::ExtractorOutdated
            | ytdlp::YtDlpFailureKind::ConversionUnavailable
            | ytdlp::YtDlpFailureKind::HelperMissing
            | ytdlp::YtDlpFailureKind::HelperStartFailed
            | ytdlp::YtDlpFailureKind::ResponseUnreadable => Self::HelperOutdated,
            ytdlp::YtDlpFailureKind::UnsupportedUrl
            | ytdlp::YtDlpFailureKind::AccessRefused
            | ytdlp::YtDlpFailureKind::Unreachable
            | ytdlp::YtDlpFailureKind::AudioUnavailable
            | ytdlp::YtDlpFailureKind::VideoUnavailable
            | ytdlp::YtDlpFailureKind::DownloadStorage
            | ytdlp::YtDlpFailureKind::Other => Self::Unreachable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `POD-13`: whatever raw text a provider error carries, the classified
    /// reason must never echo it back — the whole point of classifying is
    /// that the fixed string does not depend on the payload's content.
    #[test]
    fn pod_13_classify_never_forwards_the_raw_payload() {
        let leaking = "https://cdn.example.test/ep.mp3?sig=abc123&token=SECRET \
             at /home/user/.local/share/reprise/podcasts/leak.mp3";
        let cases = [
            PodcastError::Timeout,
            PodcastError::Transport(leaking.to_owned()),
            PodcastError::HttpStatus(403),
            PodcastError::Body(leaking.to_owned()),
            PodcastError::Parse(leaking.to_owned()),
            PodcastError::NotModified,
            PodcastError::YtDlpFailure {
                kind: crate::podcasts::ytdlp::YtDlpFailureKind::Other,
                stderr: leaking.to_owned(),
            },
            PodcastError::YtDlpFailure {
                kind: ytdlp::YtDlpFailureKind::VerificationRequired,
                stderr: leaking.to_owned(),
            },
            PodcastError::YtDlpTimeout,
            PodcastError::Disabled(leaking.to_owned()),
            PodcastError::TagWrite,
        ];
        for error in cases {
            let classified = error.classify();
            assert!(!classified.contains("token"), "{classified}");
            assert!(!classified.contains("SECRET"), "{classified}");
            assert!(!classified.contains("sig="), "{classified}");
            assert!(!classified.contains("/home/"), "{classified}");
            assert!(!classified.contains("cdn.example.test"), "{classified}");

            let failure = pipeline::RefreshFailure::from_error(42, "Safe source title", &error);
            assert_eq!(failure.subscription_id, 42);
            assert_eq!(failure.title, "Safe source title");
            let summary_fields = format!("{failure:?}");
            for raw in ["token", "SECRET", "sig=", "/home/", "cdn.example.test"] {
                assert!(!summary_fields.contains(raw), "{summary_fields}");
            }
        }
    }

    #[test]
    fn pod_13_classify_still_distinguishes_the_failure_kinds() {
        assert_ne!(
            PodcastError::Timeout.classify(),
            PodcastError::Transport(String::new()).classify()
        );
        assert_ne!(
            PodcastError::YtDlpFailure {
                kind: crate::podcasts::ytdlp::YtDlpFailureKind::Other,
                stderr: String::new(),
            }
            .classify(),
            PodcastError::Disabled(String::new()).classify()
        );
    }

    #[test]
    fn pod_22_verification_required_names_the_signed_in_browser_recovery() {
        let leaking =
            "Sign in at https://youtube.example.test/watch?v=secret using /home/user/profile";
        let classified = PodcastError::YtDlpFailure {
            kind: ytdlp::YtDlpFailureKind::VerificationRequired,
            stderr: leaking.to_owned(),
        }
        .classify();

        assert_eq!(classified, YOUTUBE_BROWSER_RECOVERY_MESSAGE);
        for raw in ["secret", "youtube.example.test", "/home/", "profile"] {
            assert!(!classified.contains(raw), "{classified}");
        }
    }

    #[test]
    fn yt_dlp_projection_uses_the_typed_kind_not_message_substrings() {
        let legacy_message = PodcastError::YtDlpFailure {
            kind: crate::podcasts::ytdlp::YtDlpFailureKind::Other,
            stderr: "requires verification and says update yt-dlp".to_owned(),
        };

        assert_eq!(
            SourceErrorKind::from(&legacy_message),
            SourceErrorKind::Unreachable
        );
    }

    #[test]
    fn every_yt_dlp_failure_kind_maps_directly_to_a_source_error_kind() {
        use ytdlp::YtDlpFailureKind as YtDlp;

        let cases = [
            (
                YtDlp::VerificationRequired,
                SourceErrorKind::RateLimited { retry_after: None },
            ),
            (
                YtDlp::RateLimited,
                SourceErrorKind::RateLimited { retry_after: None },
            ),
            (YtDlp::UnsupportedUrl, SourceErrorKind::Unreachable),
            (YtDlp::AccessRefused, SourceErrorKind::Unreachable),
            (YtDlp::Unreachable, SourceErrorKind::Unreachable),
            (YtDlp::AudioUnavailable, SourceErrorKind::Unreachable),
            (YtDlp::VideoUnavailable, SourceErrorKind::Unreachable),
            (YtDlp::ExtractorOutdated, SourceErrorKind::HelperOutdated),
            (
                YtDlp::ConversionUnavailable,
                SourceErrorKind::HelperOutdated,
            ),
            (YtDlp::DownloadStorage, SourceErrorKind::Unreachable),
            (YtDlp::Other, SourceErrorKind::Unreachable),
        ];

        for (failure, expected) in cases {
            assert_eq!(SourceErrorKind::from(failure), expected);
        }
    }

    #[test]
    fn podcast_failures_project_without_displaying_the_raw_payload() {
        let raw = "https://private.example/feed?token=SECRET failed with HTTP 599";
        let error = crate::source_error::SourceError::from(PodcastError::Transport(raw.into()));

        assert_eq!(
            error.kind(),
            &crate::source_error::SourceErrorKind::Unreachable
        );
        assert!(!error.to_string().contains("private.example"));
        assert!(!error.to_string().contains("SECRET"));
        assert!(error.details("2026-07-30 14:12").to_string().contains(raw));
    }

    #[test]
    fn youtube_failure_kinds_project_to_rate_limited_or_helper_outdated() {
        let rate_limited = crate::source_error::SourceError::from(PodcastError::YtDlpFailure {
            kind: ytdlp::YtDlpFailureKind::VerificationRequired,
            stderr: "provider verification response".into(),
        });
        let helper = crate::source_error::SourceError::from(PodcastError::YtDlpFailure {
            kind: ytdlp::YtDlpFailureKind::ExtractorOutdated,
            stderr: "extractor response changed".into(),
        });
        let conversion = crate::source_error::SourceError::from(PodcastError::YtDlpFailure {
            kind: ytdlp::YtDlpFailureKind::ConversionUnavailable,
            stderr: "ffmpeg was unavailable".into(),
        });

        assert!(matches!(
            rate_limited.kind(),
            crate::source_error::SourceErrorKind::RateLimited { retry_after: None }
        ));
        assert_eq!(
            helper.kind(),
            &crate::source_error::SourceErrorKind::HelperOutdated
        );
        assert_eq!(
            conversion.kind(),
            &crate::source_error::SourceErrorKind::HelperOutdated
        );
    }

    #[test]
    fn retryable_podcast_failures_use_the_shared_backoff_policy() {
        let rate_limited = PodcastError::RateLimited {
            retry_after: Some(std::time::Duration::from_secs(6)),
        };

        assert_eq!(
            rate_limited.retry_delay(1),
            Some(std::time::Duration::from_secs(6))
        );
        assert_eq!(
            PodcastError::HttpStatus(503).retry_delay(2),
            Some(std::time::Duration::from_secs(4))
        );
        assert_eq!(PodcastError::HttpStatus(403).retry_delay(1), None);
    }
}
