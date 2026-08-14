macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::{formatted, text};

pub const PREFERENCES_LOCATION: &str = N_!("Location");
pub const LOCATION_INTRO: &str =
    N_!("One place, used by everything that asks \"near you\". Set once — no plugin owns it.");
pub const LOCATION_CITY: &str = N_!("City");
pub const LOCATION_NOT_SET: &str = N_!("Not set");
pub const LOCATION_EDIT_CITY: &str = N_!("Edit city");
pub const LOCATION_SET_CITY: &str = N_!("Set city");
pub const LOCATION_USE_CURRENT_LOCATION: &str = N_!("Use current location");
pub const LOCATION_CLEAR_LOCATION: &str = N_!("Clear location");
pub const LOCATION_CURRENT_LOCATION: &str = N_!("Current location");
pub const LOCATION_NOT_FOUND: &str = N_!("Could not find that place");
pub const LOCATION_DEFAULT_RADIUS: &str = N_!("Default radius");
pub const LOCATION_USED_BY: &str = N_!("Used by");
pub const LOCATION_CONCERTS_DESCRIPTION: &str =
    N_!("Upcoming shows within the radius, for artists in your library");
pub const LOCATION_RADIO_NEAR_YOU: &str = N_!("Radio · Near you");
pub const LOCATION_RADIO_DESCRIPTION: &str =
    N_!("Stations from your country and city in Add Station");
pub const LOCATION_PODCASTS_POPULAR_IN: &str = N_!("Podcasts · Popular in {country}");
pub const LOCATION_PODCASTS_DESCRIPTION: &str = N_!("Apple's country chart in Add Podcast");
pub const LOCATION_FOOTNOTE: &str =
    N_!("Clearing the location only stops these three. Switching a plugin off never removes it.");
pub const LOCATION_REFERENCE_NOT_SET: &str = N_!("Location · not set");
pub const LOCATION_SET_LOCATION: &str = N_!("Set location →");
pub const LOCATION_CHANGE_IN_LOCATION: &str = N_!("Change in Location →");

pub fn location_podcasts_popular_in(country: &str) -> String {
    formatted(LOCATION_PODCASTS_POPULAR_IN, &[("country", country)])
}

pub fn location_radius_km(radius: u32) -> String {
    formatted(N_!("{radius} km"), &[("radius", &radius.to_string())])
}

pub fn location_not_set() -> String {
    text(LOCATION_NOT_SET)
}

pub fn location_reference(name: &str, radius_km: u32) -> String {
    formatted(
        N_!("Location · {name}, within {radius} km"),
        &[("name", name), ("radius", &radius_km.to_string())],
    )
}
