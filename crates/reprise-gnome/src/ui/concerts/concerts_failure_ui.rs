use reprise_core::concerts::ConcertFailure;
use reprise_core::connectivity::Connectivity;
use reprise_core::source_error::{
    source_failure_presentation, FailureAction, FailureHeadline, FailureSurface, SourceError,
    SourceErrorKind, SourceFailurePresentation, SourceSurface,
};

use crate::ui::strings;

pub(super) fn concerts_failure_presentation(
    failure: &ConcertFailure,
    cached_items: usize,
) -> SourceFailurePresentation {
    if failure.is_missing_credentials() {
        return SourceFailurePresentation {
            surface: if cached_items == 0 {
                FailureSurface::FullArea
            } else {
                FailureSurface::Banner
            },
            headline: FailureHeadline::ConcertsNeedsConfiguration,
            actions: vec![FailureAction::OpenPreferences],
            cached_items,
        };
    }
    source_failure_presentation(
        SourceSurface::Concerts,
        failure.source_error().kind(),
        cached_items,
        1,
    )
}

pub(super) fn update_failure_for_connectivity(
    failure: &mut Option<ConcertFailure>,
    connectivity: Connectivity,
    can_fetch: bool,
) {
    let is_offline_notice = failure
        .as_ref()
        .is_some_and(|failure| failure.source_error().kind() == &SourceErrorKind::Offline);
    match connectivity {
        Connectivity::Online if is_offline_notice => *failure = None,
        Connectivity::Offline if !can_fetch && is_offline_notice => *failure = None,
        Connectivity::Offline if can_fetch && failure.is_none() => {
            let offline = SourceError::new(
                SourceErrorKind::Offline,
                "Check Concerts connectivity",
                "NetworkMonitor reports no available connection",
            );
            *failure = Some(ConcertFailure::Source(offline));
        }
        Connectivity::Online | Connectivity::Offline => {}
    }
}

pub(super) const fn row_is_dimmed(connectivity: Connectivity) -> bool {
    matches!(connectivity, Connectivity::Offline)
}

pub(super) fn failure_support(
    failure: &ConcertFailure,
    cached_items: usize,
    updated: &str,
) -> String {
    if failure.is_missing_credentials() {
        strings::text(strings::CONCERTS_CONFIGURATION_DESCRIPTION)
    } else if cached_items == 0 {
        strings::text(strings::CONCERTS_EMPTY_FAILURE_DESCRIPTION)
    } else {
        strings::concerts_cached_failure_description(updated)
    }
}

#[cfg(test)]
mod tests {
    use reprise_core::concerts::{ConcertFailure, ProviderError};
    use reprise_core::connectivity::Connectivity;
    use reprise_core::source_error::{FailureAction, FailureSurface, SourceErrorKind};

    use super::{
        concerts_failure_presentation, failure_support, row_is_dimmed,
        update_failure_for_connectivity,
    };

    #[test]
    fn conc_11_cached_and_empty_failures_choose_the_shared_surfaces() {
        let failure = ConcertFailure::from(ProviderError::HttpStatus(503));

        assert_eq!(
            concerts_failure_presentation(&failure, 4).surface,
            FailureSurface::Banner
        );
        assert_eq!(
            concerts_failure_presentation(&failure, 0).surface,
            FailureSurface::FullArea
        );
    }

    #[test]
    fn conc_11_missing_credentials_open_the_concerts_plugin_instead_of_retrying() {
        let failure = ConcertFailure::from(ProviderError::MissingCredentials);

        assert_eq!(
            concerts_failure_presentation(&failure, 4).actions,
            vec![FailureAction::OpenPreferences]
        );
        assert_eq!(
            failure_support(&failure, 4, "Updated 2 h ago"),
            "Saved concerts stay available. Add credentials in Preferences to refresh them."
        );
    }

    #[test]
    fn conc_11_failure_copy_names_what_still_works_without_raw_text() {
        let failure = ConcertFailure::from(ProviderError::HttpStatus(599));

        assert_eq!(
            failure_support(&failure, 4, "Updated 2 h ago"),
            "Showing saved concerts from Updated 2 h ago. \
             Ticket and event links need a connection."
        );
        assert_eq!(
            failure_support(&failure, 0, "Never updated"),
            "There are no saved concerts to show. Your music is unaffected."
        );
    }

    #[test]
    fn conc_11_going_offline_writing_path_preserves_a_provider_failure() {
        let mut failure = Some(ConcertFailure::from(ProviderError::HttpStatus(404)));

        update_failure_for_connectivity(&mut failure, Connectivity::Offline, true);
        assert_eq!(
            failure
                .as_ref()
                .map(ConcertFailure::source_error)
                .map(reprise_core::source_error::SourceError::kind),
            Some(&SourceErrorKind::Unreachable)
        );

        update_failure_for_connectivity(&mut failure, Connectivity::Online, true);
        assert!(
            failure.is_some(),
            "reconnect must not erase the HTTP failure"
        );

        let mut offline_notice = None;
        update_failure_for_connectivity(&mut offline_notice, Connectivity::Offline, true);
        assert_eq!(
            offline_notice
                .as_ref()
                .map(ConcertFailure::source_error)
                .map(reprise_core::source_error::SourceError::kind),
            Some(&SourceErrorKind::Offline)
        );
        update_failure_for_connectivity(&mut offline_notice, Connectivity::Online, true);
        assert!(offline_notice.is_none());
        assert!(row_is_dimmed(Connectivity::Offline));
        assert!(!row_is_dimmed(Connectivity::Online));
    }
}
