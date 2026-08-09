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

/// `SRC-19`: the country the chip names and the text search below it share.
/// The stored code is third-party data (`O-4`: Nominatim's `addressdetails`)
/// and reaches a URL path segment in `itunes_charts::chart_url`, so it has to
/// clear the same storefront check `locale_country` applies to a locale
/// territory — anything else falls through to the locale rather than being
/// passed on uppercased.
pub(super) fn dialog_country(location: Option<&AppLocation>, locale: &str) -> String {
    location
        .and_then(|location| location.country_code.as_deref())
        .map(str::trim)
        .filter(|country| podcasts::itunes::is_country_code(country))
        .map_or_else(
            || podcasts::itunes::locale_country(locale),
            str::to_ascii_uppercase,
        )
}

/// `network_allowed` is `podcasts::config::source_network_allowed` for this
/// dialog's kind, read by the caller — `SRC-19`'s chip is a network action, so
/// it needs `NET-1a`'s consent as well as `NET-3`'s reachability, and
/// `Connectivity::Online` carries only the latter. Passing the fact in keeps
/// this decision pure, and therefore testable without a display or a DB.
pub(super) fn chip_for(
    kind: PodcastKind,
    connectivity: Connectivity,
    network_allowed: bool,
    country: &str,
    library_genre: Option<&str>,
) -> Option<AddDialogChip> {
    match kind {
        PodcastKind::Rss if connectivity == Connectivity::Online && network_allowed => {
            Some(AddDialogChip::Charts {
                country: country.to_owned(),
            })
        }
        PodcastKind::Rss => None,
        // `SRC-15a`: the library chip fills the entry and submits, so a
        // refused source answers through `submit_refusal` — it names the
        // reason instead of silently issuing the request the chart chip would.
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

    /// `SRC-19`: the stored code is third-party data — Nominatim's
    /// `addressdetails` enrichment, read back out of the settings DB — and it
    /// ends up in a URL *path* segment in `itunes_charts::chart_url`. Anything
    /// that is not a storefront code falls back to the locale, the same way
    /// `locale_country` falls back to `US`.
    #[test]
    fn src_19_a_country_code_that_is_not_a_storefront_falls_back_to_the_locale() {
        for stored in ["Deutschland", "D", "D3", "  ", "de/podcasts", "d\u{e9}"] {
            assert_eq!(
                dialog_country(Some(&location(Some(stored))), "en_GB.UTF-8"),
                "GB",
                "{stored:?} must not reach a chart URL"
            );
        }
        assert_eq!(
            dialog_country(Some(&location(Some("  ca  "))), "en_GB.UTF-8"),
            "CA",
            "surrounding whitespace is still a usable storefront"
        );
    }

    #[test]
    fn src_19_no_location_at_all_still_produces_a_country() {
        assert_eq!(dialog_country(None, "en-CA"), "CA");
        assert_eq!(dialog_country(None, "broken"), "US");
    }

    #[test]
    fn src_19_the_apple_dialog_offers_the_charts_chip_and_the_youtube_dialog_the_genre() {
        assert_eq!(
            chip_for(
                PodcastKind::Rss,
                Connectivity::Online,
                true,
                "DE",
                Some("Metal")
            ),
            Some(AddDialogChip::Charts {
                country: "DE".to_owned()
            })
        );
        assert_eq!(
            chip_for(
                PodcastKind::Youtube,
                Connectivity::Online,
                true,
                "DE",
                Some("Metal")
            ),
            Some(AddDialogChip::LibraryGenre {
                genre: "Metal".to_owned()
            })
        );
        assert_eq!(
            chip_for(PodcastKind::Youtube, Connectivity::Online, true, "DE", None),
            None
        );
    }

    /// `SRC-19` / `NET-1a`: reachability is not consent. With podcast online
    /// sources switched off the chip must be absent for the same reason it is
    /// absent offline — activating it would issue two requests to Apple that
    /// the user has refused. `Connectivity::Online` says only that a network
    /// exists; the caller supplies the consent half.
    #[test]
    fn src_19_the_charts_chip_is_absent_without_network_consent() {
        assert_eq!(
            chip_for(
                PodcastKind::Rss,
                Connectivity::Online,
                false,
                "DE",
                Some("Metal")
            ),
            None
        );
    }

    #[test]
    fn src_19_the_charts_chip_is_absent_offline() {
        assert_eq!(
            chip_for(
                PodcastKind::Rss,
                Connectivity::Offline,
                true,
                "DE",
                Some("Metal")
            ),
            None
        );
        assert_eq!(
            chip_for(
                PodcastKind::Youtube,
                Connectivity::Offline,
                true,
                "DE",
                Some("Metal")
            ),
            Some(AddDialogChip::LibraryGenre {
                genre: "Metal".to_owned()
            })
        );
    }
}
