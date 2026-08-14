#![allow(dead_code)]

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::formatted;

pub const CONCERTS: &str = N_!("Concerts");
pub const CONCERTS_DESCRIPTION: &str =
    N_!("Show upcoming concerts for library artists · contacts event providers");
pub const CONCERTS_DATE: &str = N_!("Date");
pub const CONCERTS_ARTIST: &str = N_!("Artist");
pub const CONCERTS_CITY: &str = N_!("City");
pub const CONCERTS_VENUE: &str = N_!("Venue");
pub const CONCERTS_DISTANCE: &str = N_!("Distance");
pub const CONCERTS_TICKETS: &str = N_!("Tickets");
pub const CONCERTS_ON_SALE: &str = N_!("On sale");
pub const CONCERTS_OFF_SALE: &str = N_!("Off sale");
pub const CONCERTS_UNKNOWN: &str = N_!("Unknown");
pub const CONCERTS_OFF_SALE_TOOLTIP: &str =
    N_!("The ticket source reports no active sale. This can mean sold out, or not on sale yet.");
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
    N_!("Saved concerts stay available. Ticket and event links need a connection.");
pub const CONCERTS_EMPTY_FAILURE_DESCRIPTION: &str =
    N_!("There are no saved concerts to show. Your music is unaffected.");
pub const CONCERTS_CONFIGURATION_DESCRIPTION: &str =
    N_!("Saved concerts stay available. Add credentials in Preferences to refresh them.");
pub const CONCERTS_NO_LINK: &str = N_!("No ticket or event link available");
pub const CONCERTS_BANDSINTOWN_APP_ID: &str = N_!("Bandsintown app_id");
pub const CONCERTS_CREDENTIAL_SAVED: &str = N_!("Saved");
pub const CONCERTS_CREDENTIAL_CHECKING: &str = N_!("Checking key…");
pub const CONCERTS_CREDENTIAL_VALID: &str = N_!("Key works");
pub const CONCERTS_CREDENTIAL_REJECTED: &str = N_!("Key was rejected");
pub const CONCERTS_CREDENTIAL_UNVERIFIED: &str = N_!("Could not verify");
pub const CONCERTS_LOCATION: &str = N_!("Location");
pub const CONCERTS_CITY_ENTRY: &str = N_!("City");
pub const CONCERTS_USE_CURRENT_LOCATION: &str = N_!("Use current location");
pub const CONCERTS_CLEAR_LOCATION: &str = N_!("Clear location");
pub const CONCERTS_CURRENT_LOCATION: &str = N_!("Current location");
pub const CONCERTS_LOCATION_NOT_FOUND: &str = N_!("Could not find that place");
pub const CONCERTS_DEFAULT_RADIUS: &str = N_!("Default radius");
pub const CONCERTS_PLAY_WINDOW: &str = N_!("Consider artists played in the last N days");
pub const CONCERTS_SIMILAR_ENABLED: &str = N_!("Include similar artists");
pub const CONCERTS_SIMILAR_COUNT: &str = N_!("Similar artists per top artist");
pub const CONCERTS_OFF: &str = N_!("Off");
pub const CONCERTS_NO_LOCATION_DESCRIPTION: &str =
    N_!("Distance and the radius filter stay switched off until a city is known.");
pub const FEED_LOADED_AT: &str = N_!("Up to date — loaded at {time}");
pub const FEED_CHECKED_AT: &str = N_!("Up to date — checked {time}");
pub const CONCERTS_UPDATING: &str = N_!("Updating concerts …");
pub const FEED_NOT_LOADED: &str = N_!("Not loaded yet");
pub const FEED_NETWORK_OFF: &str = N_!("Online sources are off");
pub const FEED_RELOAD: &str = N_!("Reload");
pub const CONCERTS_UPDATE_FAILED: &str = N_!("Update failed — showing saved concerts from {time}");
pub const CONCERTS_OFFLINE: &str = N_!("Offline — showing saved concerts from {time}");

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

pub fn concerts_opens_source(source: &str) -> String {
    formatted(N_!("Opens {source}"), &[("source", source)])
}

pub fn feed_loaded_at(time: &str) -> String {
    formatted(FEED_LOADED_AT, &[("time", time)])
}

pub fn feed_checked_at(time: &str) -> String {
    formatted(FEED_CHECKED_AT, &[("time", time)])
}

pub fn concerts_update_failed(time: &str) -> String {
    formatted(CONCERTS_UPDATE_FAILED, &[("time", time)])
}

pub fn concerts_offline(time: &str) -> String {
    formatted(CONCERTS_OFFLINE, &[("time", time)])
}

pub(in crate::ui) fn concerts_feed_footer_copy() -> crate::ui::feed_footer::FeedFooterCopy {
    crate::ui::feed_footer::FeedFooterCopy {
        updating: CONCERTS_UPDATING,
        no_credentials: CONCERTS_NEEDS_CONFIGURATION,
        failed: concerts_update_failed,
        offline: concerts_offline,
    }
}

pub fn show_all_concerts(total: usize) -> String {
    formatted(
        N_!("Show all {total} concerts"),
        &[("total", &total.to_string())],
    )
}

pub fn concerts_end_of_radius(hidden: usize, radius: f64, city: Option<&str>) -> String {
    let radius = if radius.fract() == 0.0 {
        format!("{radius:.0}")
    } else {
        radius.to_string()
    };
    let hidden = hidden.to_string();
    city.map_or_else(
        || {
            formatted(
                N_!("End of results — {hidden} concerts hidden by the {radius} km radius"),
                &[("hidden", &hidden), ("radius", &radius)],
            )
        },
        |city| {
            formatted(
                N_!(
                    "End of results — {hidden} concerts hidden by the {radius} km radius around {city}"
                ),
                &[("hidden", &hidden), ("radius", &radius), ("city", city)],
            )
        },
    )
}

pub fn concerts_radius_km(radius: u32) -> String {
    formatted(N_!("{radius} km"), &[("radius", &radius.to_string())])
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
