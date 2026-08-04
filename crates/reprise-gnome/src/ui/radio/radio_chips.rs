//! `RAD-5`: the one-click radio-browser searches on the radio Add Station
//! dialog — the library's own genre, "Top voted", "Near you" — plus the two
//! pure decisions behind them.
//!
//! "Near you" reuses the app-level, already-consented location (`O-4`)
//! instead of asking for one of its own, and it never fires an unfiltered
//! search pretending to be location-aware.
//!
//! The first chip used to be a hard-coded "Metal in DE", which was only ever
//! right for one library in one country. It now reads the genre this library
//! has spent the most time listening to, and narrows it to the stored
//! country when there is one — so it stays a genuine suggestion for a jazz
//! library in Canada too.

use gtk4::prelude::*;
use reprise_core::library::taste::TopGenre;
use reprise_core::location::AppLocation;
use reprise_core::radio::search::SearchCriteria;

use crate::ui::strings;

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

/// What the library chip reads and what it searches for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LibrarySuggestion {
    pub(super) label: String,
    pub(super) criteria: SearchCriteria,
}

/// Pure, so the whole decision is testable without a display.
///
/// `None` means the library has no evidence for any genre — nothing has been
/// played yet, or nothing played carries one. The chip then disappears
/// rather than proposing a genre out of thin air; "Top voted" and "Near you"
/// still cover the discovery case.
///
/// A stored country narrows the search and shows in the label ("Metal in
/// DE"). Without one the chip keeps the genre and searches worldwide — a
/// country-only chip would just duplicate "Near you".
pub(super) fn library_suggestion(
    genre: Option<TopGenre>,
    location: Option<&AppLocation>,
) -> Option<LibrarySuggestion> {
    let genre = genre?;
    let country_code = location.and_then(|location| location.country_code.clone());
    let label = match country_code.as_deref() {
        Some(country) => strings::radio_chip_genre_in_country(&genre.name, country),
        None => genre.name.clone(),
    };
    Some(LibrarySuggestion {
        label,
        criteria: SearchCriteria {
            tag: Some(genre.tag),
            country_code,
        },
    })
}

pub(super) struct ChipButtons {
    pub(super) root: gtk4::Widget,
    pub(super) library: gtk4::Button,
    pub(super) top_voted: gtk4::Button,
    pub(super) near_you: gtk4::Button,
}

/// Flat pill buttons, visually distinct from the tinted rectangular primary
/// action `SRC-2` reserves for "Add". "Top voted" and "Near you" are always
/// present regardless of whether a location is set — only `near_you`'s
/// *behavior* depends on that (wired by the caller through
/// [`near_you_action`]); that chip never disappears.
///
/// The library chip is the one that can be absent: it is built here but left
/// hidden, and [`apply_library_suggestion`] gives it its label and shows it
/// once the caller knows what this library listens to.
pub(super) fn build() -> ChipButtons {
    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    root.add_css_class("reprise-radio-chips");

    let library = chip_button("");
    library.set_visible(false);
    let top_voted = chip_button(&strings::text(strings::RADIO_CHIP_TOP_VOTED));
    let near_you = chip_button(&strings::text(strings::RADIO_CHIP_NEAR_YOU));
    root.append(&library);
    root.append(&top_voted);
    root.append(&near_you);

    ChipButtons {
        root: root.upcast(),
        library,
        top_voted,
        near_you,
    }
}

/// Label and visibility in one place, so "no suggestion" cannot leave a
/// stale label from a previous library state on a visible chip.
pub(super) fn apply_library_suggestion(
    chip: &gtk4::Button,
    suggestion: Option<&LibrarySuggestion>,
) {
    match suggestion {
        Some(suggestion) => {
            chip.set_label(&suggestion.label);
            chip.set_visible(true);
        }
        None => {
            chip.set_label("");
            chip.set_visible(false);
        }
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

    fn top_genre(name: &str, tag: &str) -> TopGenre {
        TopGenre {
            name: name.into(),
            tag: tag.into(),
        }
    }

    /// `RAD-5`: the chip suggests what this library listens to, narrowed to
    /// the country it already knows — the fact that used to be hard-coded as
    /// "Metal in DE" for every library on earth.
    #[test]
    fn rad_5_the_library_chip_filters_by_the_played_genre_and_the_stored_country() {
        let suggestion = library_suggestion(
            Some(top_genre("Metal", "metal")),
            Some(&location(Some("DE"))),
        )
        .expect("a played genre must produce a chip");

        assert_eq!(suggestion.label, "Metal in DE");
        assert_eq!(
            suggestion.criteria,
            SearchCriteria {
                tag: Some("metal".into()),
                country_code: Some("DE".into()),
            }
        );
    }

    /// Without a country the chip keeps the genre and searches worldwide. It
    /// must not silently become a country-less copy of "Near you", and it
    /// must not claim a country it does not have.
    #[test]
    fn rad_5_without_a_country_the_library_chip_searches_the_genre_worldwide() {
        let suggestion = library_suggestion(Some(top_genre("Jazz", "jazz")), None)
            .expect("a played genre must produce a chip even without a location");

        assert_eq!(suggestion.label, "Jazz");
        assert_eq!(
            suggestion.criteria,
            SearchCriteria {
                tag: Some("jazz".into()),
                country_code: None,
            }
        );
    }

    /// A location without a country code is the XDG-portal case — same
    /// answer as no location at all, never a guessed country.
    #[test]
    fn rad_5_a_countryless_location_does_not_narrow_the_library_chip() {
        let suggestion = library_suggestion(Some(top_genre("Jazz", "jazz")), Some(&location(None)))
            .expect("a played genre must produce a chip");

        assert_eq!(suggestion.label, "Jazz");
        assert_eq!(suggestion.criteria.country_code, None);
    }

    /// No played genre, no chip — "Top voted" and "Near you" carry the
    /// discovery case rather than the dialog inventing a taste.
    #[test]
    fn rad_5_a_library_without_a_played_genre_has_no_chip() {
        assert_eq!(library_suggestion(None, Some(&location(Some("DE")))), None);
        assert_eq!(library_suggestion(None, None), None);
    }
}
