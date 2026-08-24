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
pub const ONLINE_CONTENT_SHOW_SOURCES: &str = N_!("Show the {count} sources");
pub const SCROBBLING_NEEDS_ONLINE_SOURCES: &str = N_!("Scrobbling · needs online sources");

pub fn online_content_show_sources(count: usize) -> String {
    formatted(
        ONLINE_CONTENT_SHOW_SOURCES,
        &[("count", &count.to_string())],
    )
}

pub const PREFERENCES_ONLINE_SOURCES: &str = N_!("Online sources");
pub const ONBOARDING_ONLINE_SOURCES_BODY: &str = N_!(
    "Three sources may reach the network. Off makes this a local player: no requests, no downloads, and their sidebar entries stay hidden."
);
pub const ONBOARDING_ONLINE_SOURCES_FOOTER: &str =
    N_!("You can change this any time in Preferences · Plugins.");
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
pub const ARTWORK: &str = N_!("Artwork");
pub const ARTWORK_DESCRIPTION: &str = N_!(
    "Album covers, artist portraits and source artwork · contacts MusicBrainz, coverartarchive.org, Deezer, YouTube, Apple Podcasts and image hosts"
);
pub const ONLINE_SOURCES_USE_YOUTUBE: &str = N_!("Use YouTube");
pub const ONLINE_SOURCES_USE_PODCASTS: &str = N_!("Use Podcasts");
pub const ONLINE_SOURCES_USE_RADIO: &str = N_!("Use Radio");
pub const ONLINE_DISCOVERY_BANNER_BODY: &str = N_!(
    "Reprise can now follow podcasts, YouTube channels, radio and concerts — all off by default."
);
pub const ONLINE_DISCOVERY_REVIEW: &str = N_!("Review in Preferences");
pub const ONLINE_DISCOVERY_NOT_NOW: &str = N_!("Not now");
pub const ARTWORK_CONSENT_MERGE_NOTICE_BODY: &str = N_!(
    "Reprise merged the separate image modules into Artwork. It now loads album covers, artist portraits, and images for podcasts, YouTube, and radio."
);
pub const ARTWORK_CONSENT_MERGE_NOTICE_REVIEW: &str = N_!("Review Artwork Settings");
pub const ARTWORK_CONSENT_MERGE_NOTICE_DISMISS: &str = N_!("Dismiss");

/// `NET-1a`: shown wherever a surface refuses to reach the network because the
/// global switch — or the source's own switch — is off. The page promises "no
/// requests, no downloads, nothing hidden", so the refusal has to be visible.
pub const ONLINE_SOURCES_TURNED_OFF: &str =
    N_!("Online sources are turned off — enable them in Preferences");

// --- Plugins page: the online-content master and the background-activity bar
// (`docs/plans/plugins-online-content-master-hierarchy.md`, third draft). ---

/// State badge next to the master title while the gate is on. The draft writes
/// it as "5 of 5 plugins on"; the counts are filled in from the real module
/// list, which is longer than the mock's.
pub const ONLINE_CONTENT_PLUGINS_ON: &str = N_!("{on} of {total} plugins on");
/// State badge while the gate is off.
pub const ONLINE_CONTENT_PLUGINS_OFF: &str = N_!("all {total} plugins off");
/// The hint below the children card while the gate is off. `{names}` lists the
/// sidebar entries that disappear with it.
pub const ONLINE_CONTENT_PAUSED_HINT: &str =
    N_!("{count} plugins paused · no requests · {names} hidden from the sidebar");
/// Joins the last two entries of a human-readable name list.
pub const NAME_LIST_LAST_PAIR: &str = N_!("{first} and {second}");
/// Joins every earlier pair of a human-readable name list.
pub const NAME_LIST_SEPARATOR: &str = N_!("{first}, {second}");

/// Section label of the dialog's footer bar.
pub const BACKGROUND_ACTIVITY: &str = N_!("Background activity");
/// Description column of the Artwork job.
pub const BACKGROUND_JOB_ALBUM_COVERS: &str = N_!("Album covers · {done} of {total}");
/// Description column of the Online Lyrics job.
pub const BACKGROUND_JOB_MISSING_LYRICS: &str = N_!("Missing lyrics · {done} of {total}");
/// Shown instead of job rows while the gate is off.
pub const BACKGROUND_NO_ONLINE_JOBS: &str = N_!("No online jobs — Online content is off");
/// Accessible name and tooltip of a row's cancel button.
pub const BACKGROUND_JOB_CANCEL: &str = N_!("Cancel {job}");

pub fn online_content_plugins_on(on: usize, total: usize) -> String {
    formatted(
        ONLINE_CONTENT_PLUGINS_ON,
        &[("on", &on.to_string()), ("total", &total.to_string())],
    )
}

pub fn online_content_plugins_off(total: usize) -> String {
    formatted(ONLINE_CONTENT_PLUGINS_OFF, &[("total", &total.to_string())])
}

pub fn online_content_paused_hint(count: usize, names: &str) -> String {
    formatted(
        ONLINE_CONTENT_PAUSED_HINT,
        &[("count", &count.to_string()), ("names", names)],
    )
}

/// "A", "A and B", "A, B and C" — the grammar the paused hint reads in.
pub fn joined_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [only] => only.clone(),
        [rest @ .., last] => {
            let head = rest
                .iter()
                .cloned()
                .reduce(|left, right| {
                    formatted(NAME_LIST_SEPARATOR, &[("first", &left), ("second", &right)])
                })
                .unwrap_or_default();
            formatted(NAME_LIST_LAST_PAIR, &[("first", &head), ("second", last)])
        }
    }
}

pub fn background_job_album_covers(done: usize, total: usize) -> String {
    formatted(
        BACKGROUND_JOB_ALBUM_COVERS,
        &[("done", &done.to_string()), ("total", &total.to_string())],
    )
}

pub fn background_job_missing_lyrics(done: usize, total: usize) -> String {
    formatted(
        BACKGROUND_JOB_MISSING_LYRICS,
        &[("done", &done.to_string()), ("total", &total.to_string())],
    )
}

pub fn background_job_cancel(job: &str) -> String {
    formatted(BACKGROUND_JOB_CANCEL, &[("job", job)])
}

#[cfg(test)]
mod background_activity_tests {
    use super::*;

    #[test]
    fn a_name_list_reads_as_a_sentence_at_every_length() {
        let names = |items: &[&str]| {
            joined_names(
                &items
                    .iter()
                    .map(|item| (*item).to_owned())
                    .collect::<Vec<_>>(),
            )
        };

        assert_eq!(names(&[]), "");
        assert_eq!(names(&["Concerts"]), "Concerts");
        assert_eq!(names(&["Concerts", "YouTube"]), "Concerts and YouTube");
        assert_eq!(
            names(&["Concerts", "New Releases", "YouTube"]),
            "Concerts, New Releases and YouTube"
        );
    }

    #[test]
    fn the_master_badge_names_both_sides_of_the_count() {
        assert_eq!(online_content_plugins_on(5, 7), "5 of 7 plugins on");
        assert_eq!(online_content_plugins_off(7), "all 7 plugins off");
    }

    #[test]
    fn a_job_description_carries_its_own_counts() {
        assert_eq!(
            background_job_album_covers(1942, 2132),
            "Album covers · 1942 of 2132"
        );
        assert_eq!(
            background_job_missing_lyrics(261, 2132),
            "Missing lyrics · 261 of 2132"
        );
    }
}
