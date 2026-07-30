//! One safe projection for failures from every network-backed source.
//!
//! [`SourceError`] deliberately separates its human-facing [`Display`]
//! implementation from the technical payload exposed by [`SourceError::details`].

use std::fmt;
use std::time::Duration;

const BASE_BACKOFF_SECONDS: u64 = 2;
const MAX_BACKOFF_SECONDS: u64 = 60;
const MAX_BACKOFF_ATTEMPTS: u32 = 3;

/// Shared upper bound for feed, search, and provider requests.
pub const SOURCE_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// The complete set of failure states a source surface may render.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceErrorKind {
    Unreachable,
    RateLimited { retry_after: Option<Duration> },
    SourceGone,
    HelperOutdated,
    Offline,
}

/// The source place presenting a classified failure.
///
/// This is intentionally smaller than the module registry: it names only
/// distinctions that change failure copy or actions. New Releases and
/// Concerts can use `Generic` until their own approved wording needs a
/// distinct projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceSurface {
    Podcast,
    Youtube,
    Radio,
    Generic,
}

/// Whether a failed fetch augments cached content or replaces a genuinely
/// empty source area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureSurface {
    Banner,
    FullArea,
}

/// Safe copy identities. GTK translates these identities into localized
/// text; technical provider text is deliberately not part of this type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureHeadline {
    CouldNotCheckChannel,
    CouldNotReachYoutube,
    CouldNotReachSource,
    PodcastMovedOrEnded,
    YoutubeRateLimited,
    YoutubeHelperNeedsUpdate,
    RadioNotBroadcasting,
    Offline,
    SeveralSourcesCouldNotRefresh { count: usize },
}

/// User actions a classified failure may offer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureAction {
    TryAgain,
    CheckSubscription,
    Unsubscribe,
    OpenPreferences,
    FindNewUrl,
}

/// Pure presentation decision shared by banners and full-area states.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFailurePresentation {
    pub surface: FailureSurface,
    pub headline: FailureHeadline,
    pub actions: Vec<FailureAction>,
    pub cached_items: usize,
}

/// Decides how a classified provider failure is presented.
///
/// Cached items are never removed: any non-zero cache selects a banner.
/// Three or more failures collapse into one page-level notice instead of
/// repeating the same message per source.
#[must_use]
pub fn source_failure_presentation(
    source: SourceSurface,
    kind: &SourceErrorKind,
    cached_items: usize,
    failed_sources: usize,
) -> SourceFailurePresentation {
    let surface = if cached_items == 0 {
        FailureSurface::FullArea
    } else {
        FailureSurface::Banner
    };
    let (headline, actions) = if failed_sources >= 3 {
        (
            FailureHeadline::SeveralSourcesCouldNotRefresh {
                count: failed_sources,
            },
            vec![FailureAction::TryAgain],
        )
    } else {
        failure_copy(source, kind, surface)
    };
    SourceFailurePresentation {
        surface,
        headline,
        actions,
        cached_items,
    }
}

fn failure_copy(
    source: SourceSurface,
    kind: &SourceErrorKind,
    surface: FailureSurface,
) -> (FailureHeadline, Vec<FailureAction>) {
    match (source, kind, surface) {
        (_, SourceErrorKind::Offline, _) => {
            (FailureHeadline::Offline, vec![FailureAction::TryAgain])
        }
        (SourceSurface::Podcast, SourceErrorKind::SourceGone, _) => (
            FailureHeadline::PodcastMovedOrEnded,
            vec![FailureAction::CheckSubscription, FailureAction::Unsubscribe],
        ),
        (SourceSurface::Youtube, SourceErrorKind::RateLimited { .. }, _) => (
            FailureHeadline::YoutubeRateLimited,
            vec![FailureAction::TryAgain],
        ),
        (SourceSurface::Youtube, SourceErrorKind::HelperOutdated, _) => (
            FailureHeadline::YoutubeHelperNeedsUpdate,
            vec![FailureAction::OpenPreferences],
        ),
        (SourceSurface::Radio, _, _) => (
            FailureHeadline::RadioNotBroadcasting,
            vec![FailureAction::TryAgain, FailureAction::FindNewUrl],
        ),
        (SourceSurface::Youtube, _, FailureSurface::FullArea) => (
            FailureHeadline::CouldNotReachYoutube,
            vec![FailureAction::TryAgain],
        ),
        (SourceSurface::Podcast | SourceSurface::Youtube, _, FailureSurface::Banner) => (
            FailureHeadline::CouldNotCheckChannel,
            vec![FailureAction::TryAgain],
        ),
        (SourceSurface::Podcast | SourceSurface::Generic, _, FailureSurface::FullArea)
        | (SourceSurface::Generic, _, FailureSurface::Banner) => (
            FailureHeadline::CouldNotReachSource,
            vec![FailureAction::TryAgain],
        ),
    }
}

/// A classified source failure with technical context kept out of normal display.
#[derive(Clone, PartialEq, Eq)]
pub struct SourceError {
    kind: SourceErrorKind,
    operation: String,
    technical_cause: String,
}

impl SourceError {
    #[must_use]
    pub fn new(
        kind: SourceErrorKind,
        operation: impl Into<String>,
        technical_cause: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            operation: operation.into(),
            technical_cause: technical_cause.into(),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> &SourceErrorKind {
        &self.kind
    }

    /// Returns the explicitly requested, copyable technical detail projection.
    ///
    /// `occurred_at` is supplied by the caller so this pure projection never
    /// reads a clock and remains deterministic in tests.
    #[must_use]
    pub fn details<'a>(&'a self, occurred_at: &'a str) -> SourceErrorDetails<'a> {
        SourceErrorDetails {
            operation: &self.operation,
            technical_cause: &self.technical_cause,
            occurred_at,
            retry_after: match self.kind {
                SourceErrorKind::RateLimited { retry_after } => retry_after,
                _ => None,
            },
        }
    }
}

impl fmt::Debug for SourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceError")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.kind {
            SourceErrorKind::Unreachable => "This source could not be reached. Try again.",
            SourceErrorKind::RateLimited { .. } => {
                "This source is limiting requests. Reprise will try again."
            }
            SourceErrorKind::SourceGone => "This source has moved or ended.",
            SourceErrorKind::HelperOutdated => "The source helper needs an update.",
            SourceErrorKind::Offline => {
                "There is no network connection. Try again when you are online."
            }
        })
    }
}

impl std::error::Error for SourceError {}

/// Parses the delta-seconds form of an HTTP `Retry-After` header.
#[must_use]
pub fn parse_retry_after(value: Option<&str>) -> Option<Duration> {
    value?.trim().parse::<u64>().ok().map(Duration::from_secs)
}

/// Returns the shared exponential retry delay, or `None` when retries stop.
#[must_use]
pub fn source_backoff_delay(attempt: u32, retry_after: Option<Duration>) -> Option<Duration> {
    if attempt == 0 || attempt > MAX_BACKOFF_ATTEMPTS {
        return None;
    }
    if retry_after.is_some_and(|delay| delay > Duration::from_secs(MAX_BACKOFF_SECONDS)) {
        return None;
    }
    let exponential = BASE_BACKOFF_SECONDS
        .saturating_mul(1_u64 << attempt.saturating_sub(1))
        .min(MAX_BACKOFF_SECONDS);
    Some(
        retry_after
            .unwrap_or_default()
            .max(Duration::from_secs(exponential)),
    )
}

/// The technical lines available only through [`SourceError::details`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceErrorDetails<'a> {
    operation: &'a str,
    technical_cause: &'a str,
    occurred_at: &'a str,
    retry_after: Option<Duration>,
}

impl fmt::Display for SourceErrorDetails<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}\n{}\n{}",
            self.operation, self.technical_cause, self.occurred_at
        )?;
        if let Some(retry_after) = self.retry_after {
            write!(formatter, " · retry in {}", duration_label(retry_after))?;
        }
        Ok(())
    }
}

fn duration_label(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        format!("{seconds} s")
    } else {
        format!("{} min", seconds.div_ceil(60))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{SourceError, SourceErrorKind};

    #[test]
    fn every_source_error_state_has_one_safe_human_sentence() {
        let cases = [
            (
                SourceErrorKind::Unreachable,
                "This source could not be reached. Try again.",
            ),
            (
                SourceErrorKind::RateLimited {
                    retry_after: Some(Duration::from_secs(360)),
                },
                "This source is limiting requests. Reprise will try again.",
            ),
            (
                SourceErrorKind::SourceGone,
                "This source has moved or ended.",
            ),
            (
                SourceErrorKind::HelperOutdated,
                "The source helper needs an update.",
            ),
            (
                SourceErrorKind::Offline,
                "There is no network connection. Try again when you are online.",
            ),
        ];

        for (kind, expected) in cases {
            let error = SourceError::new(kind, "private.example request failed", "HTTP 599");
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn display_never_exposes_the_technical_payload() {
        let error = SourceError::new(
            SourceErrorKind::Unreachable,
            "private.example/path?token=SECRET request failed",
            "HTTP 599 · exception at /home/user/private",
        );

        let displayed = error.to_string();

        for technical in [
            "private.example",
            "token",
            "SECRET",
            "HTTP",
            "599",
            "/home/",
        ] {
            assert!(!displayed.contains(technical), "{displayed}");
        }
    }

    #[test]
    fn debug_never_bypasses_the_explicit_detail_accessor() {
        let error = SourceError::new(
            SourceErrorKind::Unreachable,
            "private.example request failed",
            "HTTP 599 · token=SECRET",
        );

        let debugged = format!("{error:?}");

        assert!(!debugged.contains("private.example"), "{debugged}");
        assert!(!debugged.contains("HTTP 599"), "{debugged}");
        assert!(!debugged.contains("SECRET"), "{debugged}");
    }

    #[test]
    fn details_preserve_the_raw_text_and_use_the_supplied_timestamp() {
        let error = SourceError::new(
            SourceErrorKind::RateLimited {
                retry_after: Some(Duration::from_secs(360)),
            },
            "youtube.com feed request failed",
            "HTTP 429 · too many requests",
        );

        assert_eq!(
            error.details("2026-07-30 14:12").to_string(),
            "youtube.com feed request failed\n\
             HTTP 429 · too many requests\n\
             2026-07-30 14:12 · retry in 6 min"
        );
        assert!(!error.to_string().contains("429"));
    }

    #[test]
    fn shared_retry_after_parser_accepts_delta_seconds_only() {
        assert_eq!(
            super::parse_retry_after(Some(" 360 ")),
            Some(Duration::from_secs(360))
        );
        assert_eq!(super::parse_retry_after(Some("tomorrow")), None);
        assert_eq!(super::parse_retry_after(None), None);
    }

    #[test]
    fn shared_backoff_retries_three_times_and_caps_long_provider_delays() {
        assert_eq!(
            super::source_backoff_delay(1, None),
            Some(Duration::from_secs(2))
        );
        assert_eq!(
            super::source_backoff_delay(2, None),
            Some(Duration::from_secs(4))
        );
        assert_eq!(
            super::source_backoff_delay(3, Some(Duration::from_secs(6))),
            Some(Duration::from_secs(8))
        );
        assert_eq!(super::source_backoff_delay(4, None), None);
        assert_eq!(
            super::source_backoff_delay(1, Some(Duration::from_secs(61))),
            None
        );
    }

    #[test]
    fn feed_and_search_requests_share_the_ten_second_budget() {
        assert_eq!(super::SOURCE_REQUEST_TIMEOUT, Duration::from_secs(10));
    }

    #[test]
    fn net_3d_one_projection_separates_safe_copy_details_and_network_policy() {
        let states = [
            SourceErrorKind::Unreachable,
            SourceErrorKind::RateLimited {
                retry_after: Some(Duration::from_secs(360)),
            },
            SourceErrorKind::SourceGone,
            SourceErrorKind::HelperOutdated,
            SourceErrorKind::Offline,
        ];
        assert_eq!(states.len(), 5);

        let error = SourceError::new(
            states[1].clone(),
            "youtube.com feed request failed",
            "HTTP 429 · too many requests",
        );
        assert!(!error.to_string().contains("429"));
        assert_eq!(
            error.details("2026-07-30 14:12").to_string(),
            "youtube.com feed request failed\n\
             HTTP 429 · too many requests\n\
             2026-07-30 14:12 · retry in 6 min"
        );
        assert_eq!(super::SOURCE_REQUEST_TIMEOUT, Duration::from_secs(10));
        assert_eq!(
            super::source_backoff_delay(3, None),
            Some(Duration::from_secs(8))
        );
    }

    #[test]
    fn net_3_a_cached_failure_uses_one_banner_and_keeps_the_cached_content() {
        let presentation = super::source_failure_presentation(
            super::SourceSurface::Youtube,
            &SourceErrorKind::Unreachable,
            10,
            1,
        );

        assert_eq!(presentation.surface, super::FailureSurface::Banner);
        assert_eq!(
            presentation.headline,
            super::FailureHeadline::CouldNotCheckChannel
        );
        assert_eq!(presentation.actions, vec![super::FailureAction::TryAgain]);
    }

    #[test]
    fn net_3_a_failure_without_cached_content_uses_the_full_area_state() {
        let presentation = super::source_failure_presentation(
            super::SourceSurface::Youtube,
            &SourceErrorKind::Unreachable,
            0,
            1,
        );

        assert_eq!(presentation.surface, super::FailureSurface::FullArea);
        assert_eq!(
            presentation.headline,
            super::FailureHeadline::CouldNotReachYoutube
        );
        assert_eq!(presentation.actions, vec![super::FailureAction::TryAgain]);
    }

    #[test]
    fn source_failure_actions_are_provider_specific_and_never_blame_the_user() {
        let gone = super::source_failure_presentation(
            super::SourceSurface::Podcast,
            &SourceErrorKind::SourceGone,
            4,
            1,
        );
        assert_eq!(gone.headline, super::FailureHeadline::PodcastMovedOrEnded);
        assert_eq!(
            gone.actions,
            vec![
                super::FailureAction::CheckSubscription,
                super::FailureAction::Unsubscribe,
            ]
        );

        let helper = super::source_failure_presentation(
            super::SourceSurface::Youtube,
            &SourceErrorKind::HelperOutdated,
            4,
            1,
        );
        assert_eq!(helper.actions, vec![super::FailureAction::OpenPreferences]);

        let radio = super::source_failure_presentation(
            super::SourceSurface::Radio,
            &SourceErrorKind::Unreachable,
            1,
            1,
        );
        assert_eq!(
            radio.actions,
            vec![
                super::FailureAction::TryAgain,
                super::FailureAction::FindNewUrl,
            ]
        );
    }

    #[test]
    fn net_3_three_or_more_failures_collapse_into_one_collected_notice() {
        let presentation = super::source_failure_presentation(
            super::SourceSurface::Podcast,
            &SourceErrorKind::Unreachable,
            12,
            3,
        );

        assert_eq!(
            presentation.headline,
            super::FailureHeadline::SeveralSourcesCouldNotRefresh { count: 3 }
        );
        assert_eq!(presentation.surface, super::FailureSurface::Banner);
    }
}
