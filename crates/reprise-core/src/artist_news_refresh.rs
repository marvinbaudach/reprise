//! Staleness policy for the New Releases background refresh: when a refresh
//! is due, the deterministic per-install jitter that spreads refreshes across
//! installations, and the most recent fetch timestamp the policy is judged
//! against.

const REFRESH_INTERVAL_SECONDS: i64 = 6 * 60 * 60;
const REFRESH_JITTER_MAX_SECONDS: i64 = 45 * 60;

/// Is a background refresh due? Never fetched (`None`) is always due.
/// Otherwise due once `now - last_fetch_at` reaches the base interval plus
/// jitter. A clock that moved backwards (negative elapsed time) is never due
/// — only the "never fetched" case forces an immediate refresh.
pub fn refresh_due(last_fetch_at: Option<i64>, now: i64, jitter: i64) -> bool {
    let Some(last) = last_fetch_at else {
        return true;
    };
    let elapsed = now.saturating_sub(last);
    if elapsed < 0 {
        return false;
    }
    let jitter = jitter.clamp(0, REFRESH_JITTER_MAX_SECONDS);
    elapsed >= REFRESH_INTERVAL_SECONDS + jitter
}

/// Deterministic jitter in `[0, REFRESH_JITTER_MAX_SECONDS]` derived from a
/// seed (e.g. the database path), so different installations do not all
/// refresh at the same wall-clock moment. Uses a hand-rolled FNV-1a hash
/// rather than `std::collections::hash_map::DefaultHasher`: `DefaultHasher`'s
/// algorithm is an unspecified implementation detail of the standard library
/// and is not guaranteed stable across Rust versions, so the same seed could
/// yield a different jitter after a toolchain upgrade. FNV-1a's definition is
/// fixed, so the same seed always yields the same jitter everywhere.
pub fn jitter_seconds(seed: &str) -> i64 {
    let hash = fnv1a_64(seed.as_bytes());
    (hash % (REFRESH_JITTER_MAX_SECONDS as u64 + 1)) as i64
}

/// FNV-1a (64-bit): a fixed, non-cryptographic hash whose definition never
/// changes, so the same bytes always produce the same value across Rust
/// versions, platforms, and process runs — unlike `DefaultHasher`.
pub(crate) fn fnv1a_64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes.iter().fold(OFFSET_BASIS, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(PRIME)
    })
}

/// The most recent attempt across all artists, or `None` if no artist has
/// ever been attempted. Reads the ledger rather than `new_releases`: a
/// library whose artists simply have no news would otherwise look like it
/// had never refreshed, and `refresh_due` would fire on every timer tick.
pub fn latest_fetched_at(db: &crate::db::Db) -> Result<Option<i64>, rusqlite::Error> {
    let conn = db.conn();
    crate::artist_news_ledger::latest_attempt(conn)
}

#[cfg(test)]
mod tests {
    use super::{jitter_seconds, latest_fetched_at, refresh_due};

    fn migrated_conn() -> crate::db::Db {
        crate::db::Db::open_in_memory().unwrap()
    }

    fn record_ledger_attempt(db: &crate::db::Db, artist_key: &str, attempted_at: i64) {
        crate::artist_news_ledger::record_attempt(
            db.conn(),
            artist_key,
            None,
            attempted_at,
            crate::artist_news_ledger::FetchOutcome::Ok,
            0,
        )
        .unwrap();
    }

    #[test]
    fn refresh_due_is_true_when_never_fetched() {
        assert!(refresh_due(None, 1_000_000, 0));
    }

    #[test]
    fn refresh_due_is_false_just_below_the_base_interval() {
        let now = 1_000_000;
        assert!(!refresh_due(Some(now - (6 * 3600 - 1)), now, 0));
    }

    #[test]
    fn refresh_due_is_true_at_and_above_the_interval_plus_jitter() {
        let now = 1_000_000;
        let jitter = 900;
        assert!(refresh_due(Some(now - (6 * 3600 + jitter)), now, jitter));
        assert!(refresh_due(
            Some(now - (6 * 3600 + jitter + 1)),
            now,
            jitter
        ));
    }

    #[test]
    fn refresh_due_is_false_when_the_clock_moved_backwards() {
        let now = 1_000_000;
        assert!(!refresh_due(Some(now + 10), now, 0));
    }

    #[test]
    fn jitter_seconds_is_deterministic_and_bounded() {
        let max = 45 * 60;
        for seed in ["x", "/home/user/.local/share/reprise/library.db", ""] {
            let value = jitter_seconds(seed);
            assert_eq!(value, jitter_seconds(seed), "same seed, same jitter");
            assert!((0..=max).contains(&value), "{seed} jitter out of range");
        }
    }

    #[test]
    fn jitter_seconds_differs_across_seeds() {
        assert_ne!(jitter_seconds("install-a"), jitter_seconds("install-b"));
    }

    #[test]
    fn latest_fetched_at_is_none_for_an_empty_table() {
        let conn = migrated_conn();
        assert_eq!(latest_fetched_at(&conn).unwrap(), None);
    }

    #[test]
    fn latest_fetched_at_returns_the_maximum_across_rows() {
        // `latest_fetched_at` now delegates to the ledger (Task 3), so this
        // must drive attempts through it rather than inserting into
        // `new_releases` directly — an artist ledger entry with no release
        // found is exactly the case the old `new_releases`-based query got
        // wrong.
        let conn = migrated_conn();
        record_ledger_attempt(&conn, "artist one", 1_000);
        record_ledger_attempt(&conn, "artist two", 5_000);
        record_ledger_attempt(&conn, "artist three", 2_500);
        assert_eq!(latest_fetched_at(&conn).unwrap(), Some(5_000));
    }
}
