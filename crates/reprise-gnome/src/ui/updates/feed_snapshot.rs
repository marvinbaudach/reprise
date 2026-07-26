//! Cache-only snapshots consumed while rendering the Updates popover.

use chrono::NaiveDate;
use reprise_core::concerts::{ConcertFilter, ConcertRow};
use rusqlite::Connection;

pub(super) struct ConcertsSnapshot {
    pub credentials: bool,
    pub filter: ConcertFilter,
    pub unseen: Vec<ConcertRow>,
    pub count: usize,
}

pub(super) fn concerts(conn: &Connection, enabled: bool, today: NaiveDate) -> ConcertsSnapshot {
    let credentials = reprise_core::concerts::config::credentials(conn)
        .is_ok_and(|credentials| !credentials.is_empty());
    let filter = reprise_core::concerts::config::persisted_filter(conn).unwrap_or_default();
    let location = reprise_core::concerts::config::location(conn)
        .ok()
        .flatten();
    let unseen = if enabled && credentials {
        reprise_core::concerts::query_unseen(conn, &filter, location.as_ref(), today, 3)
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "could not query unseen Concerts updates");
                Vec::new()
            })
    } else {
        Vec::new()
    };
    let count = reprise_core::concerts::count_upcoming(conn, &filter, location.as_ref(), today)
        .map_or_else(
            |error| {
                tracing::warn!(%error, "could not count upcoming Concerts");
                0
            },
            |count| usize::try_from(count).unwrap_or_default(),
        );
    ConcertsSnapshot {
        credentials,
        filter,
        unseen,
        count,
    }
}

pub(super) fn releases_count(conn: &Connection, today: NaiveDate) -> usize {
    reprise_core::artist_news::persisted_releases_filter(conn)
        .and_then(|filter| reprise_core::artist_news::count_releases_view(conn, &filter, today))
        .map_or_else(
            |error| {
                tracing::warn!(%error, "could not count Releases view rows");
                0
            },
            |count| usize::try_from(count).unwrap_or_default(),
        )
}
