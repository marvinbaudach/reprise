//! Small data projections used by the grouped source view.

use reprise_core::db::Db;
use reprise_core::podcasts;

pub(super) fn unique(mut values: Vec<String>) -> Vec<String> {
    values.sort_by_key(|value| value.to_lowercase());
    values.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    values
}

pub(super) fn last_updated_text(conn: &Db) -> String {
    let last = podcasts::store::active_subscriptions(conn)
        .ok()
        .and_then(|rows| rows.into_iter().filter_map(|row| row.last_fetch_at).max());
    super::podcasts_presentation::updated_ago(last, chrono::Utc::now().timestamp())
}
