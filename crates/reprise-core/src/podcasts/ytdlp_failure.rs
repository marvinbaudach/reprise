//! The typed vocabulary of yt-dlp failures.
//!
//! Split out of `ytdlp.rs` to keep both files under the 800-line limit the
//! architecture gate enforces. Each kind owns the sentence the user reads and
//! the name the log records, so a new failure has to decide both in one place.

pub(super) const VERIFICATION_MESSAGE: &str =
    "YouTube requires verification — try again later or use another network";
pub(super) const RATE_LIMIT_MESSAGE: &str = "YouTube is rate-limiting requests — try again later";
pub(super) const UNSUPPORTED_URL_MESSAGE: &str = "This YouTube URL is not supported";
pub(super) const ACCESS_REFUSED_MESSAGE: &str = "YouTube refused the request — try again later";
pub(super) const UNREACHABLE_MESSAGE: &str = "YouTube could not be reached — check your connection";
pub(super) const AUDIO_UNAVAILABLE_MESSAGE: &str =
    "YouTube did not provide playable audio for this video";
pub(super) const VIDEO_UNAVAILABLE_MESSAGE: &str = "This YouTube video is unavailable or private";
pub(super) const EXTRACTOR_OUTDATED_MESSAGE: &str =
    "YouTube changed its response — update yt-dlp and try again";
pub(super) const CONVERSION_UNAVAILABLE_MESSAGE: &str =
    "Audio conversion is unavailable — install or repair FFmpeg";
pub(super) const INVALID_RESPONSE_MESSAGE: &str =
    "YouTube returned an unreadable response — update yt-dlp and try again";
pub(super) const DOWNLOAD_SAVE_MESSAGE: &str =
    "YouTube download could not be saved — check available space and permissions";
pub(super) const MISSING_MESSAGE: &str =
    "YouTube component is unavailable — reinstall or repair Reprise";
pub(super) const START_FAILED_MESSAGE: &str =
    "YouTube component could not start — check its path and permissions";
pub(super) const GENERIC_FAILURE: &str = "YouTube request failed — check the application log";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YtDlpFailureKind {
    VerificationRequired,
    RateLimited,
    UnsupportedUrl,
    AccessRefused,
    Unreachable,
    AudioUnavailable,
    VideoUnavailable,
    ExtractorOutdated,
    ConversionUnavailable,
    DownloadStorage,
    /// The helper binary is absent.
    HelperMissing,
    /// The helper binary is present but refuses to start — a different repair
    /// than a missing one, so it keeps its own message.
    HelperStartFailed,
    /// The helper answered with something unreadable, which its own copy
    /// already attributes to being out of date.
    ResponseUnreadable,
    Other,
}

impl YtDlpFailureKind {
    pub(super) const fn diagnostic_name(self) -> &'static str {
        match self {
            Self::VerificationRequired => "verification_required",
            Self::RateLimited => "rate_limited",
            Self::UnsupportedUrl => "unsupported_url",
            Self::AccessRefused => "access_refused",
            Self::Unreachable => "unreachable",
            Self::AudioUnavailable => "audio_unavailable",
            Self::VideoUnavailable => "video_unavailable",
            Self::ExtractorOutdated => "extractor_outdated",
            Self::ConversionUnavailable => "conversion_unavailable",
            Self::DownloadStorage => "download_storage",
            Self::HelperMissing => "helper_missing",
            Self::HelperStartFailed => "helper_start_failed",
            Self::ResponseUnreadable => "response_unreadable",
            Self::Other => "other",
        }
    }

    pub(crate) const fn user_message(self) -> &'static str {
        match self {
            Self::VerificationRequired => VERIFICATION_MESSAGE,
            Self::RateLimited => RATE_LIMIT_MESSAGE,
            Self::UnsupportedUrl => UNSUPPORTED_URL_MESSAGE,
            Self::AccessRefused => ACCESS_REFUSED_MESSAGE,
            Self::Unreachable => UNREACHABLE_MESSAGE,
            Self::AudioUnavailable => AUDIO_UNAVAILABLE_MESSAGE,
            Self::VideoUnavailable => VIDEO_UNAVAILABLE_MESSAGE,
            Self::ExtractorOutdated => EXTRACTOR_OUTDATED_MESSAGE,
            Self::ConversionUnavailable => CONVERSION_UNAVAILABLE_MESSAGE,
            Self::DownloadStorage => DOWNLOAD_SAVE_MESSAGE,
            Self::HelperMissing => MISSING_MESSAGE,
            Self::HelperStartFailed => START_FAILED_MESSAGE,
            Self::ResponseUnreadable => INVALID_RESPONSE_MESSAGE,
            Self::Other => GENERIC_FAILURE,
        }
    }
}
