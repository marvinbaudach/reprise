use reprise_core::connectivity::Connectivity;
use reprise_core::source_error::{
    source_failure_presentation, SourceError, SourceErrorKind, SourceFailurePresentation,
    SourceSurface,
};

use crate::ui::strings;

pub(super) fn releases_failure_presentation(
    error: &SourceError,
    cached_items: usize,
) -> SourceFailurePresentation {
    source_failure_presentation(SourceSurface::NewReleases, error.kind(), cached_items, 1)
}

pub(super) fn update_failure_for_connectivity(
    failure: &mut Option<SourceError>,
    connectivity: Connectivity,
) {
    let is_offline_notice = failure
        .as_ref()
        .is_some_and(|error| error.kind() == &SourceErrorKind::Offline);
    match connectivity {
        Connectivity::Online if is_offline_notice => *failure = None,
        Connectivity::Offline if failure.is_none() => {
            *failure = Some(SourceError::new(
                SourceErrorKind::Offline,
                "Check New Releases connectivity",
                "NetworkMonitor reports no available connection",
            ));
        }
        Connectivity::Online | Connectivity::Offline => {}
    }
}

pub(super) const fn row_is_dimmed(connectivity: Connectivity) -> bool {
    matches!(connectivity, Connectivity::Offline)
}

pub(super) fn failure_support(cached_items: usize, updated: Option<&str>) -> String {
    if cached_items == 0 {
        return strings::text(strings::RELEASES_EMPTY_FAILURE_DESCRIPTION);
    }
    let updated = updated.map_or_else(
        || strings::text(strings::RELEASES_SAVED_CACHE_TIME),
        str::to_owned,
    );
    strings::releases_cached_failure_description(&updated)
}

#[cfg(test)]
mod tests {
    use reprise_core::connectivity::Connectivity;
    use reprise_core::source_error::{FailureSurface, SourceError, SourceErrorKind};

    use super::{
        failure_support, releases_failure_presentation, row_is_dimmed,
        update_failure_for_connectivity,
    };

    #[test]
    fn nr_21a_cached_and_empty_failures_choose_the_shared_surfaces() {
        let error = SourceError::new(SourceErrorKind::Unreachable, "refresh", "HTTP 503");

        assert_eq!(
            releases_failure_presentation(&error, 4).surface,
            FailureSurface::Banner
        );
        assert_eq!(
            releases_failure_presentation(&error, 0).surface,
            FailureSurface::FullArea
        );
        assert_eq!(
            failure_support(4, Some("2026-07-13")),
            "Showing saved releases from 2026-07-13. Announcement links need a connection."
        );
        assert_eq!(
            failure_support(0, None),
            "There are no saved releases to show. Your library is unaffected."
        );
    }

    #[test]
    fn nr_21a_going_offline_writing_path_preserves_a_provider_failure() {
        let mut failure = Some(SourceError::new(
            SourceErrorKind::Unreachable,
            "refresh",
            "HTTP 404",
        ));

        update_failure_for_connectivity(&mut failure, Connectivity::Offline);
        assert_eq!(
            failure.as_ref().map(SourceError::kind),
            Some(&SourceErrorKind::Unreachable)
        );

        update_failure_for_connectivity(&mut failure, Connectivity::Online);
        assert!(
            failure.is_some(),
            "reconnect must not erase the HTTP failure"
        );

        let mut offline_notice = None;
        update_failure_for_connectivity(&mut offline_notice, Connectivity::Offline);
        assert_eq!(
            offline_notice.as_ref().map(SourceError::kind),
            Some(&SourceErrorKind::Offline)
        );
        update_failure_for_connectivity(&mut offline_notice, Connectivity::Online);
        assert!(offline_notice.is_none());
        assert!(row_is_dimmed(Connectivity::Offline));
        assert!(!row_is_dimmed(Connectivity::Online));
    }
}
