//! Cache-only snapshots consumed while rendering the Updates popover.

use chrono::NaiveDate;
use reprise_core::concerts::{ConcertFilter, ConcertRow};
use reprise_core::db::Db;

pub(super) struct ConcertsSnapshot {
    pub credentials: bool,
    pub filter: ConcertFilter,
    pub unseen: Vec<ConcertRow>,
    pub count: usize,
}

pub(super) fn concerts(db: &Db, enabled: bool, today: NaiveDate) -> ConcertsSnapshot {
    let credentials = reprise_core::concerts::config::credentials(db)
        .is_ok_and(|credentials| !credentials.is_empty());
    let filter = reprise_core::concerts::config::persisted_filter(db).unwrap_or_default();
    let location = reprise_core::concerts::config::location(db).ok().flatten();
    let unseen = if enabled && credentials {
        reprise_core::concerts::query_unseen(db, &filter, location.as_ref(), today, 3)
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "could not query unseen Concerts updates");
                Vec::new()
            })
    } else {
        Vec::new()
    };
    let count = reprise_core::concerts::count_upcoming(db, &filter, location.as_ref(), today)
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

pub(super) fn releases_count(db: &Db, today: NaiveDate) -> usize {
    reprise_core::artist_news::persisted_releases_filter(db)
        .and_then(|filter| reprise_core::artist_news::count_releases_view(db, &filter, today))
        .map_or_else(
            |error| {
                tracing::warn!(%error, "could not count Releases view rows");
                0
            },
            |count| usize::try_from(count).unwrap_or_default(),
        )
}
