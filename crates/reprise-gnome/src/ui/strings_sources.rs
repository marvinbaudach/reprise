#![allow(dead_code)]

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::formatted;

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
pub const SOURCE_CACHED_EPISODES_STILL_WORK: &str =
    N_!("Showing the {count} episodes from {time}. Downloads play as usual.");
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

pub fn source_cached_episodes_still_work(count: usize, time: &str) -> String {
    formatted(
        SOURCE_CACHED_EPISODES_STILL_WORK,
        &[("count", &count.to_string()), ("time", time)],
    )
}

pub fn source_offline_description(time: &str) -> String {
    formatted(SOURCE_OFFLINE_DESCRIPTION, &[("time", time)])
}

pub fn source_nothing_found(query: &str) -> String {
    formatted(SOURCE_NOTHING_FOUND, &[("query", query)])
}
