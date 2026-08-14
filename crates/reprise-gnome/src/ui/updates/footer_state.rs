//! Pure presentation state for the Updates popover footer.

use crate::ui::strings;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct FooterPresentation {
    pub(super) updated: String,
    pub(super) show_cached_failure: bool,
}

pub(super) fn presentation(latest: Option<i64>, now: i64, failed: bool) -> FooterPresentation {
    FooterPresentation {
        updated: latest.map_or_else(
            || strings::text(strings::UPDATED_JUST_NOW),
            |timestamp| strings::new_releases_updated_ago(timestamp, now),
        ),
        show_cached_failure: failed,
    }
}

pub(super) fn oldest_active_feed_timestamp(
    news_active: bool,
    news_latest: Option<i64>,
    concerts_active: bool,
    concerts_latest: Option<i64>,
) -> Option<i64> {
    match (news_active, concerts_active) {
        (false, false) => None,
        (true, false) => news_latest,
        (false, true) => concerts_latest,
        (true, true) => Some(news_latest?.min(concerts_latest?)),
    }
}

pub(super) fn failure_text(news_failed: bool, concerts_failed: bool) -> String {
    match (news_failed, concerts_failed) {
        (false, false) => String::new(),
        (true, false) => strings::text(strings::FETCH_FAILED_INLINE),
        (false, true) => strings::text(strings::UPDATES_CONCERTS_FETCH_FAILED),
        (true, true) => format!(
            "{} · {}",
            strings::text(strings::FETCH_FAILED_INLINE),
            strings::text(strings::UPDATES_CONCERTS_FETCH_FAILED)
        ),
    }
}
