macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use reprise_core::podcasts::PodcastKind;

use super::{formatted, plural};

pub const SOURCE_ADD: &str = N_!("Add");
pub const SOURCE_ADDED: &str = N_!("Added");
pub const SOURCE_SUBSCRIBE_ACCESSIBLE: &str = N_!("Subscribe to {source}");
pub const SOURCE_ADD_ACCESSIBLE: &str = N_!("Add {source}");
pub const SOURCE_ADDED_ACCESSIBLE: &str = N_!("{source} is already in your library");
pub const SOURCE_SUBSCRIBED_DROP_OUT: &str = N_!("Subscribed sources drop out of later searches.");
pub const SOURCE_DETAILS: &str = N_!("Details");
pub const SOURCE_COPY_DETAILS: &str = N_!("Copy");
pub const SOURCE_DISMISS: &str = N_!("Dismiss");
pub const SOURCE_TRY_AGAIN: &str = N_!("Try again");
pub const SOURCE_CHECK_SUBSCRIPTION: &str = N_!("Check subscription");
pub const SOURCE_UNSUBSCRIBE: &str = N_!("Unsubscribe");
pub const SOURCE_OPEN_PREFERENCES: &str = N_!("Open Preferences");
pub const SOURCE_FIND_NEW_URL: &str = N_!("Find a new URL");
pub const SOURCE_COULD_NOT_CHECK_CHANNEL: &str = N_!("Couldn't check this channel for new uploads");
pub const SOURCE_COULD_NOT_CHECK_PODCAST: &str =
    N_!("Couldn't check this podcast for new episodes");
pub const SOURCE_COULD_NOT_REACH_YOUTUBE: &str = N_!("Can't reach YouTube right now");
pub const SOURCE_COULD_NOT_REACH: &str = N_!("Can't reach this source right now");
pub const SOURCE_PODCAST_MOVED: &str = N_!("This podcast has moved or ended");
pub const SOURCE_YOUTUBE_LIMITING: &str =
    N_!("YouTube is limiting requests right now — try again in a few minutes");
pub const SOURCE_YOUTUBE_HELPER_UPDATE: &str = N_!("The YouTube helper needs an update");
pub const SOURCE_OFFLINE: &str = N_!("You're offline");
pub const SOURCE_SEVERAL_FAILED: &str = N_!("Couldn't refresh {count} sources");
pub const SOURCE_COLLECTED_FAILURES_CACHED: &str =
    N_!("Affected: {sources}. Saved episodes and downloads still work.");
pub const SOURCE_COLLECTED_FAILURES_CACHED_MORE: &str =
    N_!("Affected: {sources}, and {count} more. Saved episodes and downloads still work.");
pub const SOURCE_COLLECTED_FAILURES_EMPTY: &str = N_!(
    "Affected: {sources}. Nothing is downloaded from these sources yet; your other sources and music are unaffected."
);
pub const SOURCE_COLLECTED_FAILURES_EMPTY_MORE: &str = N_!(
    "Affected: {sources}, and {count} more. Nothing is downloaded from these sources yet; your other sources and music are unaffected."
);
pub const SOURCE_AGE_JUST_NOW: &str = N_!("just now");
pub const SOURCE_UPDATED_AGO: &str = N_!("Updated {time}");
pub const SOURCE_YOUTUBE_EMPTY_FAILURE_DESCRIPTION: &str = N_!(
    "Nothing is downloaded from this channel yet, so there's nothing to show. Your other channels and your music are unaffected."
);
pub const SOURCE_PODCAST_EMPTY_FAILURE_DESCRIPTION: &str = N_!(
    "Nothing is downloaded from this podcast yet, so there's nothing to show. Your other podcasts and your music are unaffected."
);
pub const SOURCE_OFFLINE_DESCRIPTION: &str =
    N_!("Showing downloaded content. Last checked {time}.");
pub const SOURCE_ACTION_FAILED: &str = N_!("This action couldn't be completed. Try again.");
pub const SOURCE_NOTHING_FOUND: &str =
    N_!("Nothing found for '{query}' — try pasting a feed/channel URL instead");

pub fn source_subscribe_accessible(source: &str) -> String {
    formatted(SOURCE_SUBSCRIBE_ACCESSIBLE, &[("source", source)])
}

pub fn source_add_accessible(source: &str) -> String {
    formatted(SOURCE_ADD_ACCESSIBLE, &[("source", source)])
}

pub fn source_added_accessible(source: &str) -> String {
    formatted(SOURCE_ADDED_ACCESSIBLE, &[("source", source)])
}

pub fn source_several_failed(count: usize) -> String {
    formatted(SOURCE_SEVERAL_FAILED, &[("count", &count.to_string())])
}

pub fn source_collected_failures(
    sources: &str,
    remaining: usize,
    has_cached_items: bool,
) -> String {
    let template = match (has_cached_items, remaining == 0) {
        (true, true) => SOURCE_COLLECTED_FAILURES_CACHED,
        (true, false) => SOURCE_COLLECTED_FAILURES_CACHED_MORE,
        (false, true) => SOURCE_COLLECTED_FAILURES_EMPTY,
        (false, false) => SOURCE_COLLECTED_FAILURES_EMPTY_MORE,
    };
    formatted(
        template,
        &[("sources", sources), ("count", &remaining.to_string())],
    )
}

/// `NET-3`: the cached-content banner names what is on screen (episodes or
/// videos, by kind) and how old it is. `time` is a bare phrase such as
/// "2 minutes ago", never a full "Updated ..." label.
pub fn source_cached_items_still_work(kind: PodcastKind, count: usize, time: &str) -> String {
    let count_text = count.to_string();
    let values = [("count", count_text.as_str()), ("time", time)];
    match kind {
        PodcastKind::Rss => plural(
            "Showing the episode from {time}. Downloads play as usual.",
            "Showing the {count} episodes from {time}. Downloads play as usual.",
            count,
            &values,
        ),
        PodcastKind::Youtube => plural(
            "Showing the video from {time}. Downloads play as usual.",
            "Showing the {count} videos from {time}. Downloads play as usual.",
            count,
            &values,
        ),
    }
}

/// Bare, plural-aware ages such as "2 minutes ago". The literals sit in the
/// `plural` calls themselves so the catalog extractor can see them.
pub fn source_age_minutes(minutes: i64) -> String {
    let count = age_count(minutes);
    plural(
        "{count} minute ago",
        "{count} minutes ago",
        count,
        &[("count", &count.to_string())],
    )
}

pub fn source_age_hours(hours: i64) -> String {
    let count = age_count(hours);
    plural(
        "{count} hour ago",
        "{count} hours ago",
        count,
        &[("count", &count.to_string())],
    )
}

pub fn source_age_days(days: i64) -> String {
    let count = age_count(days);
    plural(
        "{count} day ago",
        "{count} days ago",
        count,
        &[("count", &count.to_string())],
    )
}

fn age_count(amount: i64) -> usize {
    usize::try_from(amount).unwrap_or(0)
}

pub fn source_updated_ago(time: &str) -> String {
    formatted(SOURCE_UPDATED_AGO, &[("time", time)])
}

pub fn source_offline_description(time: &str) -> String {
    formatted(SOURCE_OFFLINE_DESCRIPTION, &[("time", time)])
}

pub fn source_nothing_found(query: &str) -> String {
    formatted(SOURCE_NOTHING_FOUND, &[("query", query)])
}
