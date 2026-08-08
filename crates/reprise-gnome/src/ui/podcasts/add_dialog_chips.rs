//! Pure chip decisions for the podcast and YouTube add dialogs.

use reprise_core::connectivity::Connectivity;
use reprise_core::location::AppLocation;
use reprise_core::podcasts::{self, PodcastKind};

use crate::ui::strings;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AddDialogChip {
    Charts { country: String },
    LibraryGenre { genre: String },
}

impl AddDialogChip {
    pub(super) fn label(&self) -> String {
        match self {
            Self::Charts { country } => strings::podcast_chip_popular_in_country(country),
            Self::LibraryGenre { genre } => strings::youtube_chip_genre(genre),
        }
    }
}

pub(super) fn dialog_country(location: Option<&AppLocation>, locale: &str) -> String {
    location
        .and_then(|location| location.country_code.as_deref())
        .map(str::trim)
        .filter(|country| !country.is_empty())
        .map_or_else(
            || podcasts::itunes::locale_country(locale),
            str::to_ascii_uppercase,
        )
}

pub(super) fn chip_for(
    kind: PodcastKind,
    connectivity: Connectivity,
    country: &str,
    library_genre: Option<&str>,
) -> Option<AddDialogChip> {
    match kind {
        PodcastKind::Rss if connectivity == Connectivity::Online => Some(AddDialogChip::Charts {
            country: country.to_owned(),
        }),
        PodcastKind::Rss => None,
        PodcastKind::Youtube => library_genre.map(|genre| AddDialogChip::LibraryGenre {
            genre: genre.to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn location(country_code: Option<&str>) -> AppLocation {
        AppLocation {
            latitude: 47.0,
            longitude: 8.0,
            name: "Somewhere".to_owned(),
            country_code: country_code.map(str::to_owned),
        }
    }

    #[test]
    fn src_19_the_country_prefers_the_stored_location_over_the_locale() {
        assert_eq!(
            dialog_country(Some(&location(Some("ca"))), "de_DE.UTF-8"),
            "CA"
        );
    }

    #[test]
    fn src_19_a_location_without_a_country_falls_through_to_the_locale() {
        assert_eq!(dialog_country(Some(&location(None)), "de_DE.UTF-8"), "DE");
    }

    #[test]
    fn src_19_no_location_at_all_still_produces_a_country() {
        assert_eq!(dialog_country(None, "en-CA"), "CA");
        assert_eq!(dialog_country(None, "broken"), "US");
    }

    #[test]
    fn src_19_the_apple_dialog_offers_the_charts_chip_and_the_youtube_dialog_the_genre() {
        assert_eq!(
            chip_for(PodcastKind::Rss, Connectivity::Online, "DE", Some("Metal")),
            Some(AddDialogChip::Charts {
                country: "DE".to_owned()
            })
        );
        assert_eq!(
            chip_for(
                PodcastKind::Youtube,
                Connectivity::Online,
                "DE",
                Some("Metal")
            ),
            Some(AddDialogChip::LibraryGenre {
                genre: "Metal".to_owned()
            })
        );
        assert_eq!(
            chip_for(PodcastKind::Youtube, Connectivity::Online, "DE", None),
            None
        );
    }

    #[test]
    fn src_19_the_charts_chip_is_absent_offline() {
        assert_eq!(
            chip_for(PodcastKind::Rss, Connectivity::Offline, "DE", Some("Metal")),
            None
        );
        assert_eq!(
            chip_for(
                PodcastKind::Youtube,
                Connectivity::Offline,
                "DE",
                Some("Metal")
            ),
            Some(AddDialogChip::LibraryGenre {
                genre: "Metal".to_owned()
            })
        );
    }
}
