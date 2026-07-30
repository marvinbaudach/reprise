//! One safe projection for failures from every network-backed source.
//!
//! [`SourceError`] deliberately separates its human-facing [`Display`]
//! implementation from the technical payload exposed by [`SourceError::details`].

use std::fmt;
use std::time::Duration;

/// The complete set of failure states a source surface may render.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceErrorKind {
    Unreachable,
    RateLimited { retry_after: Option<Duration> },
    SourceGone,
    HelperOutdated,
    Offline,
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
}
