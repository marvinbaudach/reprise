#![allow(dead_code)]

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::formatted;

pub const PLUGIN_GROUP_LOCAL: &str = N_!("Local");
pub const PLUGIN_GROUP_ONLINE_CONTENT: &str = N_!("Online content");
pub const PLUGIN_GROUP_CONNECTED_SERVICES: &str = N_!("Connected services");
pub const ONLINE_CONTENT_MASTER_DESCRIPTION: &str = N_!(
    "Use online sources — off makes this a local player: nothing below runs, no requests, sidebar entries hidden."
);
pub const ONLINE_CONTENT_MASTER_ACCESSIBLE: &str = N_!("Use online sources");
pub const ONLINE_CONTENT_SHOW_SOURCES: &str = N_!("Show the {count} sources");
pub const SCROBBLING_NEEDS_ONLINE_SOURCES: &str = N_!("Scrobbling · needs online sources");

pub fn online_content_show_sources(count: usize) -> String {
    formatted(
        ONLINE_CONTENT_SHOW_SOURCES,
        &[("count", &count.to_string())],
    )
}

pub const PREFERENCES_ONLINE_SOURCES: &str = N_!("Online sources");
pub const ONLINE_SOURCES_MASTER_TITLE: &str = N_!("Use online sources");
pub const ONLINE_SOURCES_MASTER_BODY: &str = N_!(
    "Off makes this a local player only: no requests, no downloads, nothing hidden — the three entries disappear from the sidebar."
);
pub const ONLINE_SOURCES_FOOTER: &str = N_!(
    "Each block is self-contained: turning one off hides its sidebar entry and stops its requests; subscriptions and favorites are kept, not deleted."
);
pub const ONLINE_SOURCES_YOUTUBE_SUBTITLE: &str =
    N_!("Channels as audio episodes · channel feeds, audio via yt-dlp");
pub const ONLINE_SOURCES_PODCASTS_SUBTITLE: &str =
    N_!("Shows as audio episodes · RSS feeds, search via Apple Podcasts");
pub const ONLINE_SOURCES_RADIO_SUBTITLE: &str =
    N_!("Stations and live streams · radio-browser.info directory");
pub const SOURCE_IMAGES: &str = N_!("Source Images");
pub const SOURCE_IMAGES_DESCRIPTION: &str = N_!(
    "Show source artwork · contacts YouTube, Apple Podcasts, feed publishers, radio-browser.info, and image hosts"
);
pub const ONLINE_SOURCES_USE_YOUTUBE: &str = N_!("Use YouTube");
pub const ONLINE_SOURCES_USE_PODCASTS: &str = N_!("Use Podcasts");
pub const ONLINE_SOURCES_USE_RADIO: &str = N_!("Use Radio");
pub const ONLINE_DISCOVERY_BANNER_BODY: &str = N_!(
    "Reprise can now follow podcasts, YouTube channels, radio and concerts — all off by default."
);
pub const ONLINE_DISCOVERY_REVIEW: &str = N_!("Review in Preferences");
pub const ONLINE_DISCOVERY_NOT_NOW: &str = N_!("Not now");

/// `NET-1a`: shown wherever a surface refuses to reach the network because the
/// global switch — or the source's own switch — is off. The page promises "no
/// requests, no downloads, nothing hidden", so the refusal has to be visible.
pub const ONLINE_SOURCES_TURNED_OFF: &str =
    N_!("Online sources are turned off — enable them in Preferences");
