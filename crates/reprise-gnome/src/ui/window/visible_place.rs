//! The place the user is looking at, read from the content stack.
//!
//! The track list always holds *a* place, even while a section page covers it,
//! so asking it where the user is answers "the last list they saw". Every
//! navigation that records an origin asks here instead (`BROWSE-4a`).

use reprise_core::browser::BrowserPlace;

use super::library_shell::{active_content_target, ActiveContentTarget};
use crate::ui::nav_history::Origin;
use crate::ui::track_list::TrackList;

/// Classifies a visible content-stack page. Only a page that names a section
/// of its own produces a `Section` origin; the track list's page and the pages
/// with no place of their own (the device card, anything unrecognised) hand
/// over the track list's place, which refreshes the router but never enters
/// history.
pub(super) fn origin_for_page(
    page: Option<&str>,
    track_place: impl FnOnce() -> BrowserPlace,
) -> Origin {
    match active_content_target(page) {
        Some(ActiveContentTarget::Stats) => Origin::Section(BrowserPlace::MyStats),
        Some(ActiveContentTarget::Concerts) => Origin::Section(BrowserPlace::Concerts),
        Some(ActiveContentTarget::Releases) => Origin::Section(BrowserPlace::Releases),
        Some(ActiveContentTarget::Podcasts) => Origin::Section(BrowserPlace::Podcasts),
        Some(ActiveContentTarget::Youtube) => Origin::Section(BrowserPlace::Youtube),
        Some(ActiveContentTarget::Radio) => Origin::Section(BrowserPlace::Radio),
        Some(ActiveContentTarget::LibraryDoctor) => Origin::Section(BrowserPlace::LibraryDoctor),
        Some(ActiveContentTarget::Tracks) => Origin::TrackList(track_place()),
        // The device page is placeless like the Doctor, but it is one page for
        // every device: no `BrowserPlace` can say which one, so it keeps the
        // pre-`BROWSE-4a` behaviour instead of guessing.
        None => Origin::Unknown(track_place()),
    }
}

/// The origin to hand to `NavHistory`'s `*_from` methods.
pub(in crate::ui) fn origin(content_stack: &gtk4::Stack, track_list: &TrackList) -> Origin {
    origin_for_page(content_stack.visible_child_name().as_deref(), || {
        track_list.browser_place()
    })
}

#[cfg(test)]
mod tests {
    use reprise_core::view_source::ViewSource;

    use super::super::content_stack::{DEVICE_SYNC_PAGE, LIBRARY_DOCTOR_PAGE};
    use super::*;

    fn lagging_track_list() -> BrowserPlace {
        BrowserPlace::from(ViewSource::Queue)
    }

    #[test]
    fn browse_4a_every_section_page_names_its_own_place() {
        let pages = [
            ("stats", BrowserPlace::MyStats),
            ("concerts", BrowserPlace::Concerts),
            ("releases", BrowserPlace::Releases),
            ("podcasts", BrowserPlace::Podcasts),
            ("youtube", BrowserPlace::Youtube),
            ("radio", BrowserPlace::Radio),
            (LIBRARY_DOCTOR_PAGE, BrowserPlace::LibraryDoctor),
        ];
        for (page, place) in pages {
            assert_eq!(
                origin_for_page(Some(page), lagging_track_list),
                Origin::Section(place),
                "page {page}"
            );
        }
    }

    /// `ImportErrors` and `Conversions` are trackless places hosted on the
    /// library page: the page decides, not the shape of the place.
    #[test]
    fn browse_4a_the_library_page_is_a_track_list_origin_even_for_a_trackless_place() {
        for place in [BrowserPlace::ImportErrors, BrowserPlace::Conversions] {
            assert_eq!(
                origin_for_page(Some("library"), || place.clone()),
                Origin::TrackList(place)
            );
        }
    }

    #[test]
    fn browse_4a_the_device_page_and_unknown_pages_are_not_sections() {
        for page in [Some(DEVICE_SYNC_PAGE), Some("no-such-page"), None] {
            assert_eq!(
                origin_for_page(page, lagging_track_list),
                Origin::Unknown(lagging_track_list()),
                "page {page:?}"
            );
        }
    }

    #[test]
    fn browse_4a_a_section_page_never_consults_the_track_list() {
        let origin = origin_for_page(Some("podcasts"), || {
            panic!("the track list lags behind a section page")
        });
        assert_eq!(origin, Origin::Section(BrowserPlace::Podcasts));
    }
}
