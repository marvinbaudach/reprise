//! `RAD-5`: three one-click radio-browser searches on the radio Add Station
//! dialog — "Metal in DE", "Top voted", "Near you" — plus the pure decision
//! behind "Near you": it reuses the app-level, already-consented location
//! (`O-4`) instead of asking for one of its own, and it never fires an
//! unfiltered search pretending to be location-aware.

use gtk4::prelude::*;
use reprise_core::location::AppLocation;
use reprise_core::radio::search::SearchCriteria;

use crate::ui::strings;

const METAL_TAG: &str = "metal";
const METAL_COUNTRY: &str = "DE";

/// What activating "Near you" does, given the current app-level location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum NearYouAction {
    Search(SearchCriteria),
    OpenLocationSettings,
}

/// Pure so it is testable without a display. `O-4`'s only permitted country
/// source is Nominatim's `addressdetails` enrichment of the existing
/// forward-geocode request behind city search — never a new
/// reverse-geocoding call — so a location set via the XDG portal ("Use
/// current location") carries no country and, like no location at all,
/// resolves to [`NearYouAction::OpenLocationSettings`] rather than a search
/// that silently ignores "near you" and returns the whole catalog.
pub(super) fn near_you_action(location: Option<&AppLocation>) -> NearYouAction {
    match location.and_then(|location| location.country_code.clone()) {
        Some(country_code) => NearYouAction::Search(SearchCriteria {
            tag: None,
            country_code: Some(country_code),
        }),
        None => NearYouAction::OpenLocationSettings,
    }
}

#[must_use]
pub(super) fn metal_in_germany_criteria() -> SearchCriteria {
    SearchCriteria {
        tag: Some(METAL_TAG.to_owned()),
        country_code: Some(METAL_COUNTRY.to_owned()),
    }
}

pub(super) struct ChipButtons {
    pub(super) root: gtk4::Widget,
    pub(super) metal: gtk4::Button,
    pub(super) top_voted: gtk4::Button,
    pub(super) near_you: gtk4::Button,
}

/// Three flat pill buttons, visually distinct from the tinted rectangular
/// primary action `SRC-2` reserves for "Add" — always present regardless of
/// whether a location is set. Only `near_you`'s *behavior* depends on that
/// (wired by the caller through [`near_you_action`]); the chip itself never
/// disappears.
pub(super) fn build() -> ChipButtons {
    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    root.add_css_class("reprise-radio-chips");

    let metal = chip_button(&strings::text(strings::RADIO_CHIP_METAL_DE));
    let top_voted = chip_button(&strings::text(strings::RADIO_CHIP_TOP_VOTED));
    let near_you = chip_button(&strings::text(strings::RADIO_CHIP_NEAR_YOU));
    root.append(&metal);
    root.append(&top_voted);
    root.append(&near_you);

    ChipButtons {
        root: root.upcast(),
        metal,
        top_voted,
        near_you,
    }
}

fn chip_button(label: &str) -> gtk4::Button {
    let button = gtk4::Button::with_label(label);
    button.add_css_class("pill");
    button.add_css_class("reprise-radio-chip");
    button
}

#[cfg(test)]
mod tests {
    use super::*;

    fn location(country_code: Option<&str>) -> AppLocation {
        AppLocation {
            latitude: 52.52,
            longitude: 13.405,
            name: "Berlin, Deutschland".into(),
            country_code: country_code.map(str::to_owned),
        }
    }

    /// `RAD-5`: the required "present with a location" half — activating
    /// "Near you" with a country-taggable location runs a genuine
    /// country-filtered search, never an unfiltered one.
    #[test]
    fn rad_5_near_you_with_a_location_runs_a_country_filtered_search() {
        assert_eq!(
            near_you_action(Some(&location(Some("DE")))),
            NearYouAction::Search(SearchCriteria {
                tag: None,
                country_code: Some("DE".into()),
            })
        );
    }

    /// `RAD-5`: the required "absent without a location" half — activating
    /// "Near you" with no stored location opens the location setting and
    /// starts no search.
    #[test]
    fn rad_5_near_you_without_a_location_opens_the_location_setting_and_starts_no_search() {
        assert_eq!(near_you_action(None), NearYouAction::OpenLocationSettings);
    }

    /// `RAD-5`: a location that exists but has no derivable country — the
    /// XDG-portal "Use current location" path, which returns coordinates
    /// only — must resolve exactly like no location at all. Never a silent
    /// unfiltered search dressed up as "near you".
    #[test]
    fn rad_5_a_location_without_a_country_also_opens_the_location_setting() {
        assert_eq!(
            near_you_action(Some(&location(None))),
            NearYouAction::OpenLocationSettings
        );
    }

    #[test]
    fn rad_5_metal_in_de_chip_filters_by_tag_and_country() {
        assert_eq!(
            metal_in_germany_criteria(),
            SearchCriteria {
                tag: Some("metal".into()),
                country_code: Some("DE".into()),
            }
        );
    }
}
