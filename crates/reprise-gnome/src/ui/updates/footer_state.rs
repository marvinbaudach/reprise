//! Pure presentation state for the Updates popover footer.

use crate::ui::feed_footer::FeedFooterState;
#[cfg(test)]
use crate::ui::strings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ActiveFeed {
    pub active: bool,
    pub latest: Option<i64>,
    pub loaded_this_visit: bool,
}

pub(super) fn aggregate(
    news: ActiveFeed,
    concerts: ActiveFeed,
    network_enabled: bool,
    fetching: bool,
    failed: bool,
    no_credentials: bool,
) -> FeedFooterState {
    if !news.active && !concerts.active && !no_credentials {
        FeedFooterState::ModuleOff
    } else if !network_enabled {
        FeedFooterState::NetworkOff
    } else if no_credentials && !news.active {
        FeedFooterState::NoCredentials
    } else if fetching {
        FeedFooterState::Fetching {
            checked: 0,
            total: 0,
        }
    } else {
        let latest = oldest_active_feed_timestamp(
            news.active,
            news.latest,
            concerts.active,
            concerts.latest,
        );
        let Some(at) = latest else {
            return FeedFooterState::NeverFetched;
        };
        if failed {
            FeedFooterState::Failed { latest: at }
        } else if active_feeds_loaded_this_visit(news, concerts) {
            FeedFooterState::Loaded { at }
        } else {
            FeedFooterState::Cached { at }
        }
    }
}

fn active_feeds_loaded_this_visit(news: ActiveFeed, concerts: ActiveFeed) -> bool {
    (!news.active || news.loaded_this_visit) && (!concerts.active || concerts.loaded_this_visit)
}

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
pub(super) struct FooterPresentation {
    pub(super) updated: String,
    pub(super) show_cached_failure: bool,
}

#[cfg(test)]
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

#[cfg(test)]
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
