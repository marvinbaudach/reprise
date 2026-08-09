//! The one place that decides how Reprise writes a date.
//!
//! `i18n::init` initialises the system locale at startup, so the C library
//! already knows the user's `LC_TIME` — nobody was reading it. This module
//! reads it exactly once and hands every call site the same value, which is
//! what makes STYLE-11 enforceable rather than aspirational.

use std::sync::OnceLock;

use reprise_core::format::{
    system_date_pattern, system_time_pattern, ClockConvention, DatePattern, DateTimeFormat,
};

/// Pins the date pattern for tests and screenshots. Changing the process
/// locale would be the honest alternative, but `setlocale` is global and the
/// test harness runs cases concurrently.
pub(in crate::ui) const PATTERN_ENV: &str = "REPRISE_DATE_PATTERN";

static FORMAT: OnceLock<DateTimeFormat> = OnceLock::new();

/// The display format for this process. Cheap after the first call.
pub(in crate::ui) fn current() -> &'static DateTimeFormat {
    FORMAT.get_or_init(|| DateTimeFormat {
        date: pattern_from(std::env::var(PATTERN_ENV).ok(), system_date_pattern),
        clock: ClockConvention::from_platform(&system_time_pattern()),
    })
}

/// Warms the cache. Call once, directly after `i18n::init`, so the pattern is
/// read after `setlocale` and never from a half-initialised locale.
pub(crate) fn init() {
    let format = current();
    tracing::info!(
        date = ?format.date,
        clock = ?format.clock,
        "date display format resolved"
    );
}

fn pattern_from(override_value: Option<String>, platform: impl FnOnce() -> String) -> DatePattern {
    let raw = override_value.unwrap_or_else(platform);
    DatePattern::from_platform(&raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// STYLE-11: the environment override exists so a display test can pin a
    /// locale shape without mutating the process locale, which `setlocale`
    /// makes global and racy across the test harness.
    #[test]
    fn style_11_environment_override_wins_over_the_platform() {
        let pattern = pattern_from(Some("%d.%m.%Y".to_owned()), || "%m/%d/%y".to_owned());
        assert_eq!(pattern.render(Some(2026), Some(5), Some(29)), "29.05.2026");
    }

    #[test]
    fn style_11_platform_pattern_is_used_when_no_override_is_set() {
        let pattern = pattern_from(None, || "%m/%d/%y".to_owned());
        assert_eq!(pattern.render(Some(2026), Some(5), Some(29)), "05/29/2026");
    }

    #[test]
    fn style_11_unreadable_platform_pattern_falls_back_to_iso() {
        let pattern = pattern_from(None, String::new);
        assert_eq!(pattern.render(Some(2026), Some(5), Some(29)), "2026-05-29");
    }
}
