#![allow(dead_code)]

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub const PREFERENCES_ONLINE_SOURCES: &str = N_!("Online sources");
pub const ONLINE_SOURCES_MASTER_TITLE: &str = N_!("Use online sources");
pub const ONLINE_SOURCES_MASTER_BODY: &str = N_!(
    "Off makes this a local player only: no requests, no downloads, nothing hidden — the three entries disappear from the sidebar."
);
pub const ONLINE_SOURCES_FOOTER: &str = N_!(
    "Each block is self-contained: turning one off hides its sidebar entry and stops its requests; subscriptions and favorites are kept, not deleted."
);
pub const ONLINE_SOURCES_YOUTUBE_SUBTITLE: &str = N_!("Channel feeds, audio via yt-dlp");
pub const ONLINE_SOURCES_PODCASTS_SUBTITLE: &str = N_!("RSS feeds, search via Apple Podcasts");
pub const ONLINE_SOURCES_RADIO_SUBTITLE: &str = N_!("Directory: radio-browser.info");
pub const ONLINE_SOURCES_USE_YOUTUBE: &str = N_!("Use YouTube");
pub const ONLINE_SOURCES_USE_PODCASTS: &str = N_!("Use Podcasts");
pub const ONLINE_SOURCES_USE_RADIO: &str = N_!("Use Radio");
