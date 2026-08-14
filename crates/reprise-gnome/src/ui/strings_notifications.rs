//! Translatable copy for update notifications and their preference row.

#![allow(dead_code)]

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::{formatted, plural};

pub const NOTIFY_ABOUT_UPDATES: &str = N_!("Notify about updates");
pub const NOTIFY_UPDATES_OFF: &str = N_!("Off");
pub const NOTIFY_RELEASES_ONLY: &str = N_!("Releases only");
pub const NOTIFY_ALL_UPDATES: &str = N_!("All updates");
pub const NOTIFY_ALL_UPDATES_DESCRIPTION: &str =
    N_!("All updates also announces newly found concerts for your artists.");

pub fn update_release_body(artist: &str, release_type: &str) -> String {
    formatted(
        N_!("{artist} · {type} · out today"),
        &[("artist", artist), ("type", release_type)],
    )
}

pub fn update_releases_title(count: usize) -> String {
    plural(
        N_!("{count} release is out"),
        N_!("{count} releases are out"),
        count,
        &[("count", &count.to_string())],
    )
}

pub fn update_concerts_title(count: usize) -> String {
    plural(
        N_!("{count} new concert"),
        N_!("{count} new concerts"),
        count,
        &[("count", &count.to_string())],
    )
}

pub fn update_concert_body(artist: &str, city: &str, date: &str) -> String {
    formatted(
        N_!("{artist} · {city} · {date}"),
        &[("artist", artist), ("city", city), ("date", date)],
    )
}

#[cfg(test)]
mod tests {
    use crate::ui::strings::{
        update_concert_body, update_concerts_title, update_release_body, update_releases_title,
    };

    #[test]
    fn update_notification_copy_formats_data_without_changing_its_order() {
        assert_eq!(
            update_release_body("Castiel", "EP"),
            "Castiel · EP · out today"
        );
        assert_eq!(update_releases_title(4), "4 releases are out");
        assert_eq!(update_concerts_title(12), "12 new concerts");
        assert_eq!(
            update_concert_body("Castiel", "Zürich", "14.08.2026"),
            "Castiel · Zürich · 14.08.2026"
        );
    }
}
