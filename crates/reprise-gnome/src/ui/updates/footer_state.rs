//! Pure presentation state for the Updates popover footer.

use crate::ui::feed_footer::FeedFooterState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ActiveFeed {
    pub active: bool,
    pub latest: Option<i64>,
    pub loaded_this_visit: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct FeedProgress {
    pub news: (usize, usize),
    pub concerts: (usize, usize),
}

pub(super) fn aggregate(
    news: ActiveFeed,
    concerts: ActiveFeed,
    network_enabled: bool,
    fetching: Option<FeedProgress>,
    failed: bool,
    no_credentials: bool,
) -> FeedFooterState {
    if !news.active && !concerts.active && !no_credentials {
        FeedFooterState::ModuleOff
    } else if !network_enabled {
        FeedFooterState::NetworkOff
    } else if no_credentials && !news.active {
        FeedFooterState::NoCredentials
    } else if let Some(progress) = fetching {
        FeedFooterState::Fetching {
            checked: progress.news.0 + progress.concerts.0,
            total: progress.news.1 + progress.concerts.1,
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

fn oldest_active_feed_timestamp(
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
