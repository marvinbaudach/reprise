#![allow(dead_code)]

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::{formatted, text};

pub const CONCERTS: &str = N_!("Concerts");
pub const CONCERTS_DESCRIPTION: &str =
    N_!("Show upcoming concerts for library artists · contacts event providers");
pub const CONCERTS_DATE: &str = N_!("Date");
pub const CONCERTS_ARTIST: &str = N_!("Artist");
pub const CONCERTS_CITY: &str = N_!("City");
pub const CONCERTS_VENUE: &str = N_!("Venue");
pub const CONCERTS_DISTANCE: &str = N_!("Distance");
pub const CONCERTS_TICKETS: &str = N_!("Tickets");
pub const CONCERTS_ADD_FILTER: &str = N_!("+ Add filter");
pub const CONCERTS_CLEAR_ALL: &str = N_!("Clear all ×");
pub const CONCERTS_RADIUS: &str = N_!("Radius");
pub const CONCERTS_COUNTRY: &str = N_!("Country");
pub const CONCERTS_DATE_RANGE: &str = N_!("Date range");
pub const CONCERTS_SOURCE: &str = N_!("Source");
pub const CONCERTS_ALL_UPCOMING: &str = N_!("All upcoming");
pub const CONCERTS_NEXT_30_DAYS: &str = N_!("Next 30 days");
pub const CONCERTS_NEXT_3_MONTHS: &str = N_!("Next 3 months");
pub const CONCERTS_NEXT_6_MONTHS: &str = N_!("Next 6 months");
pub const CONCERTS_LIBRARY_ARTISTS_ONLY: &str = N_!("Library artists only");
pub const CONCERTS_INCLUDE_SIMILAR: &str = N_!("Include similar artists");
pub const CONCERTS_SET_LOCATION_TOOLTIP: &str = N_!("Set a location in Preferences");
pub const CONCERTS_NO_DATA_TITLE: &str = N_!("No concert data yet");
pub const CONCERTS_NO_UPCOMING_TITLE: &str = N_!("No upcoming concerts for your artists");
pub const CONCERTS_FETCH_FAILED: &str = N_!("Concerts fetch failed · showing saved concerts");
pub const CONCERTS_COULD_NOT_REFRESH: &str = N_!("Couldn't refresh concerts");
pub const CONCERTS_NEEDS_CONFIGURATION: &str = N_!("Concerts needs provider credentials");
pub const CONCERTS_CACHED_FAILURE_DESCRIPTION: &str =
    N_!("Showing saved concerts from {time}. Ticket and event links need a connection.");
pub const CONCERTS_EMPTY_FAILURE_DESCRIPTION: &str =
    N_!("There are no saved concerts to show. Your music is unaffected.");
pub const CONCERTS_CONFIGURATION_DESCRIPTION: &str =
    N_!("Saved concerts stay available. Add credentials in Preferences to refresh them.");
pub const CONCERTS_UPDATED_NEVER: &str = N_!("Never updated");
pub const CONCERTS_NO_LINK: &str = N_!("No ticket or event link available");
pub const CONCERTS_BANDSINTOWN_APP_ID: &str = N_!("Bandsintown app_id");
pub const CONCERTS_CREDENTIAL_SAVED: &str = N_!("Saved");
pub const CONCERTS_CREDENTIAL_CHECKING: &str = N_!("Checking key…");
pub const CONCERTS_CREDENTIAL_VALID: &str = N_!("Key works");
pub const CONCERTS_CREDENTIAL_REJECTED: &str = N_!("Key was rejected");
pub const CONCERTS_CREDENTIAL_UNVERIFIED: &str = N_!("Could not verify");
pub const CONCERTS_PLAY_WINDOW: &str = N_!("Consider artists played in the last N days");
pub const CONCERTS_SIMILAR_ENABLED: &str = N_!("Include similar artists");
pub const CONCERTS_SIMILAR_COUNT: &str = N_!("Similar artists per top artist");
pub const CONCERTS_OFF: &str = N_!("Off");
pub const CONCERTS_NO_LOCATION_DESCRIPTION: &str =
    N_!("Distance and the radius filter stay switched off until a city is known.");

pub fn concert_count_line(shown: usize, total: usize) -> String {
    concert_count(&shown.to_string(), total)
}

/// FIL-2: the same line with the shown number accented. The bold goes in as
/// the *argument*, not as a substring search over the rendered sentence — a
/// translation that puts the total first would otherwise bold the wrong
/// number, silently.
pub fn concert_count_line_markup(shown: usize, total: usize) -> String {
    concert_count(&format!("<b>{shown}</b>"), total)
}

fn concert_count(shown: &str, total: usize) -> String {
    formatted(
        N_!("{shown} of {total} concerts"),
        &[("shown", shown), ("total", &total.to_string())],
    )
}

pub fn concert_total_line(total: usize) -> String {
    formatted(N_!("{total} concerts"), &[("total", &total.to_string())])
}

pub fn concerts_location_radius(city: &str, radius: u32) -> String {
    formatted(
        N_!("{city} · {radius} km"),
        &[("city", city), ("radius", &radius.to_string())],
    )
}

pub fn concerts_radius_off(radius: u32) -> String {
    formatted(N_!("{radius} km · off"), &[("radius", &radius.to_string())])
}

pub fn concerts_no_location_title(total: usize) -> String {
    formatted(
        N_!("No location set — showing all {total} concerts worldwide"),
        &[("total", &total.to_string())],
    )
}

pub fn concert_similar_caption(artist: &str) -> String {
    formatted(N_!("similar to {artist}"), &[("artist", artist)])
}

pub fn show_all_concerts(total: usize) -> String {
    formatted(
        N_!("Show all {total} concerts"),
        &[("total", &total.to_string())],
    )
}

pub fn concerts_radius_km(radius: u32) -> String {
    formatted(N_!("{radius} km"), &[("radius", &radius.to_string())])
}

pub fn concerts_updated_ago(timestamp: i64, now: i64) -> String {
    let age = now.saturating_sub(timestamp);
    if age < 60 {
        return text(N_!("Updated just now"));
    }
    if age < 60 * 60 {
        return formatted(
            N_!("Updated {age} min ago"),
            &[("age", &(age / 60).to_string())],
        );
    }
    if age < 24 * 60 * 60 {
        return formatted(
            N_!("Updated {age} h ago"),
            &[("age", &(age / (60 * 60)).to_string())],
        );
    }
    formatted(
        N_!("Updated {age} d ago"),
        &[("age", &(age / (24 * 60 * 60)).to_string())],
    )
}

pub fn concerts_cached_failure_description(time: &str) -> String {
    formatted(CONCERTS_CACHED_FAILURE_DESCRIPTION, &[("time", time)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concert_count_line_formats_shown_and_total() {
        assert_eq!(concert_count_line(5, 23), "5 of 23 concerts");
    }

    #[test]
    fn concert_similar_caption_formats_seed_artist() {
        assert_eq!(
            concert_similar_caption("Lorna Shore"),
            "similar to Lorna Shore"
        );
    }

    #[test]
    fn show_all_concerts_formats_count() {
        assert_eq!(show_all_concerts(14), "Show all 14 concerts");
    }
}
