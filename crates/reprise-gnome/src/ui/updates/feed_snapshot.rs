//! Cache-only snapshots consumed while rendering the Updates popover.

use chrono::NaiveDate;
use reprise_core::artist_news::StoredRelease;
use reprise_core::concerts::{ConcertFilter, ConcertRow};
use reprise_core::db::Db;
use reprise_core::updates::{delta_batch, DeltaBatch};

pub(super) const RELEASES_DELTA_CAP: usize = 5;
pub(super) const CONCERTS_DELTA_CAP: usize = 3;

pub(super) struct ReleasesSnapshot {
    pub delta: DeltaBatch<StoredRelease>,
    pub unseen_ids: Vec<String>,
}

pub(super) struct ConcertsSnapshot {
    pub credentials: bool,
    pub filter: ConcertFilter,
    pub delta: DeltaBatch<ConcertRow>,
}

pub(super) fn unseen_release_ids(candidates: &[StoredRelease]) -> Vec<String> {
    candidates
        .iter()
        .filter(|release| release.seen_at.is_none())
        .map(|release| release.release_group_mbid.clone())
        .collect()
}

pub(super) fn releases(db: &Db, enabled: bool, today: NaiveDate) -> ReleasesSnapshot {
    let candidates = if enabled {
        reprise_core::artist_news::delta_candidates(db, today).unwrap_or_else(|error| {
            tracing::warn!(%error, "could not query New Releases delta candidates");
            Vec::new()
        })
    } else {
        Vec::new()
    };
    let unseen_ids = unseen_release_ids(&candidates);
    let delta = delta_batch(candidates, |release| release.seen_at, RELEASES_DELTA_CAP);
    ReleasesSnapshot { delta, unseen_ids }
}

pub(super) fn concerts(db: &Db, enabled: bool, today: NaiveDate) -> ConcertsSnapshot {
    let credentials = reprise_core::concerts::config::credentials(db)
        .is_ok_and(|credentials| !credentials.is_empty());
    let filter = reprise_core::concerts::config::persisted_filter(db).unwrap_or_default();
    let location = reprise_core::concerts::config::location(db).ok().flatten();
    let rows = if enabled && credentials {
        reprise_core::concerts::query_scope_with_seen(db, &filter, location.as_ref(), today)
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "could not query Concerts delta candidates");
                Vec::new()
            })
    } else {
        Vec::new()
    };
    let delta = delta_batch(rows, |(_, seen_at)| *seen_at, CONCERTS_DELTA_CAP);
    let delta = DeltaBatch {
        shown: delta.shown.into_iter().map(|(row, _seen_at)| row).collect(),
        total: delta.total,
        unseen: delta.unseen,
    };
    ConcertsSnapshot {
        credentials,
        filter,
        delta,
    }
}
