use reprise_core::connectivity::Connectivity;
use reprise_core::podcasts::{PodcastKind, SubscriptionRow};

pub(in crate::ui) const TAB_OPEN_STALE_SECONDS: i64 = 15 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum RefreshWindow {
    /// Schedule: `sources.refresh_hours` plus database jitter.
    Hours {
        refresh_hours: i64,
        jitter_seconds: i64,
    },
    /// Tab opening: exact seconds without jitter.
    Seconds(i64),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) struct ScopeStatus {
    pub count: usize,
    pub due: bool,
}

#[must_use]
pub(in crate::ui) fn scope_status(
    subscriptions: &[SubscriptionRow],
    kind: Option<PodcastKind>,
    window: RefreshWindow,
    now: i64,
) -> ScopeStatus {
    let mut status = ScopeStatus::default();
    for subscription in subscriptions
        .iter()
        .filter(|subscription| kind.is_none_or(|kind| subscription.kind == kind))
    {
        status.count += 1;
        if status.due {
            continue;
        }
        status.due = match window {
            RefreshWindow::Hours {
                refresh_hours,
                jitter_seconds,
            } => reprise_core::podcasts::refresh::refresh_due_with_hours(
                subscription.last_fetch_at,
                now,
                refresh_hours,
                jitter_seconds,
            ),
            RefreshWindow::Seconds(seconds) => {
                reprise_core::podcasts::refresh::refresh_due_after_seconds(
                    subscription.last_fetch_at,
                    now,
                    seconds,
                )
            }
        };
    }
    status
}

#[must_use]
pub(in crate::ui) fn tab_open_refresh_allowed(
    network_allowed: bool,
    connectivity: Connectivity,
    metered: bool,
    refresh_running: bool,
    status: ScopeStatus,
) -> bool {
    connectivity == Connectivity::Online
        && !refresh_running
        && super::podcasts_worker::automatic_refresh_allowed(
            network_allowed,
            status.count,
            metered,
            status.due,
        )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum RefreshButtonState {
    Idle,
    Busy,
}

#[must_use]
pub(in crate::ui) const fn refresh_button_state(in_flight: usize) -> RefreshButtonState {
    if in_flight == 0 {
        RefreshButtonState::Idle
    } else {
        RefreshButtonState::Busy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subscription(id: i64, kind: PodcastKind, last_fetch_at: Option<i64>) -> SubscriptionRow {
        SubscriptionRow {
            id,
            kind,
            feed_url: format!("https://example.test/{id}"),
            title: format!("Source {id}"),
            author: None,
            image_url: None,
            etag: None,
            last_modified: None,
            last_fetch_at,
            last_outcome: None,
            auto_download: false,
            keep_downloaded: None,
            added_at: 1,
            removed_at: None,
        }
    }

    #[test]
    fn scope_status_counts_only_the_requested_kind() {
        let subscriptions = [
            subscription(1, PodcastKind::Rss, None),
            subscription(2, PodcastKind::Rss, None),
            subscription(3, PodcastKind::Youtube, None),
        ];

        assert_eq!(
            scope_status(
                &subscriptions,
                Some(PodcastKind::Rss),
                RefreshWindow::Seconds(900),
                100_000,
            )
            .count,
            2
        );
        assert_eq!(
            scope_status(
                &subscriptions,
                Some(PodcastKind::Youtube),
                RefreshWindow::Seconds(900),
                100_000,
            )
            .count,
            1
        );
        assert_eq!(
            scope_status(&subscriptions, None, RefreshWindow::Seconds(900), 100_000,).count,
            3
        );
    }

    #[test]
    fn scope_status_is_not_due_when_every_subscription_in_scope_was_just_fetched() {
        let subscriptions = [
            subscription(1, PodcastKind::Rss, Some(99_940)),
            subscription(2, PodcastKind::Rss, Some(99_940)),
        ];

        assert!(
            !scope_status(
                &subscriptions,
                Some(PodcastKind::Rss),
                RefreshWindow::Seconds(900),
                100_000,
            )
            .due
        );
    }

    #[test]
    fn scope_status_is_due_when_one_subscription_in_scope_is_stale() {
        let subscriptions = [
            subscription(1, PodcastKind::Rss, Some(99_940)),
            subscription(2, PodcastKind::Rss, Some(99_099)),
        ];

        assert!(
            scope_status(
                &subscriptions,
                Some(PodcastKind::Rss),
                RefreshWindow::Seconds(900),
                100_000,
            )
            .due
        );
    }

    #[test]
    fn scope_status_ignores_a_stale_subscription_of_another_kind() {
        let subscriptions = [
            subscription(1, PodcastKind::Rss, Some(99_940)),
            subscription(2, PodcastKind::Youtube, Some(1)),
        ];

        assert!(
            !scope_status(
                &subscriptions,
                Some(PodcastKind::Rss),
                RefreshWindow::Seconds(900),
                100_000,
            )
            .due
        );
    }

    #[test]
    fn scope_status_measures_the_schedule_in_hours_plus_jitter() {
        let not_due = [subscription(1, PodcastKind::Rss, Some(74_801))];
        let due = [subscription(1, PodcastKind::Rss, Some(74_800))];
        let window = RefreshWindow::Hours {
            refresh_hours: 6,
            jitter_seconds: 3_600,
        };

        assert!(!scope_status(&not_due, None, window, 100_000).due);
        assert!(scope_status(&due, None, window, 100_000).due);
    }

    #[test]
    fn tab_open_refuses_offline_metered_disabled_empty_fresh_and_already_running() {
        let due = ScopeStatus {
            count: 1,
            due: true,
        };
        let empty = ScopeStatus {
            count: 0,
            due: true,
        };
        let fresh = ScopeStatus {
            count: 1,
            due: false,
        };

        assert!(!tab_open_refresh_allowed(
            true,
            Connectivity::Offline,
            false,
            false,
            due,
        ));
        assert!(!tab_open_refresh_allowed(
            true,
            Connectivity::Online,
            true,
            false,
            due,
        ));
        assert!(!tab_open_refresh_allowed(
            false,
            Connectivity::Online,
            false,
            false,
            due,
        ));
        assert!(!tab_open_refresh_allowed(
            true,
            Connectivity::Online,
            false,
            false,
            empty,
        ));
        assert!(!tab_open_refresh_allowed(
            true,
            Connectivity::Online,
            false,
            false,
            fresh,
        ));
        assert!(!tab_open_refresh_allowed(
            true,
            Connectivity::Online,
            false,
            true,
            due,
        ));
        assert!(tab_open_refresh_allowed(
            true,
            Connectivity::Online,
            false,
            false,
            due,
        ));
    }

    #[test]
    fn refresh_button_stays_busy_until_the_last_refresh_finished() {
        assert_eq!(refresh_button_state(0), RefreshButtonState::Idle);
        assert_eq!(refresh_button_state(1), RefreshButtonState::Busy);
        assert_eq!(refresh_button_state(2), RefreshButtonState::Busy);
    }
}
