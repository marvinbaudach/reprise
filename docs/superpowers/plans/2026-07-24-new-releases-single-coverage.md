# New Releases: Abdeckung für Artists mit Single-Titeln — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wer die Vorab-Single eines angekündigten Albums besitzt, wird über dieses Album benachrichtigt — und Artists mit wenigen Plays kommen überhaupt an die Reihe.

**Architecture:** Ein neues Fetch-Ledger (`artist_news_fetch`) hält pro Artist fest, wann zuletzt ein Versuch stattfand, unabhängig vom Ergebnis. Darauf bauen zwei Dinge auf: die Frischeprüfung (die heute fälschlich über gefundene Releases läuft) und die Rotationsreihenfolge (die heute nach Play-Count statt nach Fälligkeit sortiert). Getrennt davon wird der Bibliotheks-Filter korrigiert: Bei angekündigten Releases entfällt er ganz, bei erschienenen greift eine Zwei-Track-Schwelle. Das macht ein Drei-Zustands-Präsenzmodell in der UI nötig, damit teilweise vorhandene Alben nicht als „In Bibliothek" beschriftet werden.

**Tech Stack:** Rust, rusqlite (SQLite mit `user_version`-Migrationen), gtk4-rs / libadwaita, chrono. Tests sind `#[test]`-Funktionen in crate-internen Testmodulen, ausgeführt mit `cargo test`.

**Spec:** `docs/superpowers/specs/2026-07-24-new-releases-single-coverage-design.md`

## Global Constraints

- Schema-Version: `user_version` 29 → **30**. Keine andere Migration darf 30 belegen.
- `FETCH_TTL_SECONDS = 7 * 24 * 60 * 60` bleibt unverändert und gilt für alle Ledger-Einträge, auch `failed`.
- `NEWS_WINDOW_DAYS = 90`, `MAX_ITEMS = 20`, `TOP_ARTIST_COUNT = 20` bleiben unverändert.
- `OWNED_ALBUM_MIN_TRACKS = 2` — neue Konstante, ein Album gilt erst ab zwei vorhandenen Tracks als besessen.
- `REST_ARTISTS_PER_RUN = 30` — ersetzt `DAILY_REST_COUNT = 5`, gilt **pro Lauf** und **nur für die Rest-Gruppe**.
- Setting-Key für Singles: exakt `module.new_releases.include_singles`, Default `false`.
- `artist_key` ist überall `lower(trim(artist))` — dieselbe Normalisierung, die `artists_for_fetch` schon für `GROUP BY` benutzt.
- `has_excluded_secondary_type` wird nicht angefasst.
- Der ✦-Badge und `badge.rs` werden nicht angefasst.
- Alle neuen UI-Strings über das `N_!`-Makro in `strings_news.rs`, damit sie übersetzbar bleiben.
- Commit-Format: `<type>: <description>` (feat, fix, refactor, docs, test, chore).

## File Structure

**Neu:**
- `crates/reprise-core/src/db_artist_news_fetch.rs` — Migration v30, legt `artist_news_fetch` an und befüllt sie aus `new_releases` vor.
- `crates/reprise-core/src/db_artist_news_fetch_migration_tests.rs` — Migrationstests, eingebunden per `#[path]` wie bei den anderen Migrationen.
- `crates/reprise-core/src/artist_news_ledger.rs` — Lese-/Schreib-API des Ledgers. Bewusst ein eigenes Modul: `artist_news.rs` hat bereits 763 Zeilen, und die Ledger-Logik hat eine klar abgegrenzte Verantwortung.

**Geändert:**
- `crates/reprise-core/src/lib.rs` — Moduldeklarationen.
- `crates/reprise-core/src/db.rs` — Migrationsaufruf.
- `crates/reprise-core/src/artist_news.rs` — Frischeprüfung, Rotation, Besitz-Schwelle, Präsenz, Singles-Gate.
- `crates/reprise-core/src/artist_news_refresh.rs` — `latest_fetched_at` liest aus dem Ledger.
- `crates/reprise-core/src/artist_news_history.rs` — `HistoryEntry.in_library` → `presence`.
- `crates/reprise-core/src/artist_news_tests.rs` — neue und angepasste Tests.
- `crates/reprise-gnome/src/ui/new_releases/release_row.rs` — Chip und Primäraktion.
- `crates/reprise-gnome/src/ui/new_releases/history_page.rs` — `history_action`.
- `crates/reprise-gnome/src/ui/preferences/preference_new_releases.rs` — Scope-Signatur, Singles-Schalter.
- `crates/reprise-gnome/src/ui/strings_news.rs` — neue Strings.

---

### Task 1: Migration v30 — Tabelle `artist_news_fetch`

**Files:**
- Create: `crates/reprise-core/src/db_artist_news_fetch.rs`
- Create: `crates/reprise-core/src/db_artist_news_fetch_migration_tests.rs`
- Modify: `crates/reprise-core/src/lib.rs`
- Modify: `crates/reprise-core/src/db.rs:676`

**Interfaces:**
- Consumes: nichts.
- Produces: `pub(crate) fn migrate_v30(conn: &Connection) -> Result<(), rusqlite::Error>` und die Tabelle `artist_news_fetch(artist_key TEXT PRIMARY KEY, artist_mbid TEXT, last_attempt_at INTEGER NOT NULL, last_outcome TEXT NOT NULL, releases_found INTEGER NOT NULL DEFAULT 0)`.

- [ ] **Step 1: Write the failing test**

Create `crates/reprise-core/src/db_artist_news_fetch_migration_tests.rs`:

```rust
use rusqlite::Connection;

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get::<_, i64>(0),
    )
    .unwrap()
        > 0
}

#[test]
fn v30_creates_ledger_and_backfills_from_new_releases() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO new_releases (release_group_mbid, artist_name, artist_mbid, title, \
         release_type, first_release_date, fetched_at, fallback_accent, first_seen) \
         VALUES ('rg-1', ' Pink Floyd ', 'mbid-1', 'A', 'Album', '2026-07-01', 500, '#3584E4', 500)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO new_releases (release_group_mbid, artist_name, artist_mbid, title, \
         release_type, first_release_date, fetched_at, fallback_accent, first_seen) \
         VALUES ('rg-2', 'PINK FLOYD', 'mbid-1', 'B', 'Album', '2026-07-02', 900, '#3584E4', 900)",
        [],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 29).unwrap();
    conn.execute("DROP TABLE IF EXISTS artist_news_fetch", []).unwrap();

    crate::db_artist_news_fetch::migrate_v30(&conn).unwrap();

    assert!(table_exists(&conn, "artist_news_fetch"));
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 30);

    let (key, mbid, attempt, outcome, found): (String, Option<String>, i64, String, i64) = conn
        .query_row(
            "SELECT artist_key, artist_mbid, last_attempt_at, last_outcome, releases_found \
             FROM artist_news_fetch",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(key, "pink floyd");
    assert_eq!(mbid.as_deref(), Some("mbid-1"));
    assert_eq!(attempt, 900);
    assert_eq!(outcome, "ok");
    assert_eq!(found, 2);
}

#[test]
fn v30_is_idempotent() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    crate::db_artist_news_fetch::migrate_v30(&conn).unwrap();
    crate::db_artist_news_fetch::migrate_v30(&conn).unwrap();
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 30);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p reprise-core db_artist_news_fetch`
Expected: FAIL — Kompilierfehler, `db_artist_news_fetch` existiert nicht.

- [ ] **Step 3: Write the migration module**

Create `crates/reprise-core/src/db_artist_news_fetch.rs`:

```rust
//! Schema migration adding the per-artist New Releases fetch ledger.
//!
//! Before this table, freshness was judged by `MAX(fetched_at)` over
//! `new_releases` rows — which only exist for artists that actually had
//! news. An artist with nothing to report never got a cache entry and was
//! therefore re-fetched on every single run. The ledger records the
//! *attempt*, not the outcome, so "checked, found nothing" is finally
//! distinguishable from "never checked".
//!
//! The key is the normalized artist name rather than the MBID: artists
//! without a resolved MBID are exactly the ones that need tracking, and the
//! name is what `artists_for_fetch` already groups by.
//!
//! Backfill seeds the ledger from existing `new_releases` rows so an upgrade
//! does not re-fetch the whole library at once. Artists with no rows there
//! deliberately get no entry — they count as "never checked" and are picked
//! up first by the rotation, which is the intended behaviour.

use rusqlite::Connection;

const SCHEMA_V30: &str = r#"
CREATE TABLE IF NOT EXISTS artist_news_fetch (
  artist_key      TEXT PRIMARY KEY,
  artist_mbid     TEXT,
  last_attempt_at INTEGER NOT NULL,
  last_outcome    TEXT NOT NULL,
  releases_found  INTEGER NOT NULL DEFAULT 0
);
INSERT OR IGNORE INTO artist_news_fetch
  (artist_key, artist_mbid, last_attempt_at, last_outcome, releases_found)
SELECT lower(trim(artist_name)), MAX(artist_mbid), MAX(fetched_at), 'ok', COUNT(*)
FROM new_releases
GROUP BY lower(trim(artist_name));
"#;

pub(crate) fn migrate_v30(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 30 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V30)?;
    transaction.pragma_update(None, "user_version", 30)?;
    transaction.commit()
}

#[cfg(test)]
#[path = "db_artist_news_fetch_migration_tests.rs"]
mod tests;
```

- [ ] **Step 4: Wire the module into the crate**

In `crates/reprise-core/src/lib.rs`, neben die anderen `db_*`-Deklarationen (alphabetisch vor `mod db_ai_jobs;` in Zeile 28):

```rust
mod db_artist_news_fetch;
```

In `crates/reprise-core/src/db.rs` direkt nach dem Aufruf in Zeile 676:

```rust
    crate::db_ai_jobs::migrate_v29(conn)?;
    crate::db_artist_news_fetch::migrate_v30(conn)?;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p reprise-core db_artist_news_fetch`
Expected: PASS, 2 Tests.

- [ ] **Step 6: Run the full core suite to catch version collisions**

Run: `cargo test -p reprise-core`
Expected: PASS. Schlägt ein anderer Migrationstest fehl, belegt eine zweite Migration die 30 — dann diese hier auf die nächste freie Nummer heben und die Konstante in „Global Constraints" mitziehen.

- [ ] **Step 7: Commit**

```bash
git add crates/reprise-core/src/db_artist_news_fetch.rs \
        crates/reprise-core/src/db_artist_news_fetch_migration_tests.rs \
        crates/reprise-core/src/lib.rs crates/reprise-core/src/db.rs
git commit -m "feat: Fetch-Ledger-Tabelle für New Releases (Schema v30)"
```

---

### Task 2: Ledger-API

**Files:**
- Create: `crates/reprise-core/src/artist_news_ledger.rs`
- Modify: `crates/reprise-core/src/lib.rs`

**Interfaces:**
- Consumes: Tabelle `artist_news_fetch` aus Task 1.
- Produces:
  - `pub(crate) enum FetchOutcome { Ok, Unmatched, Failed }` mit `pub(crate) fn as_str(&self) -> &'static str`
  - `pub(crate) fn record_attempt(conn: &Connection, artist_key: &str, artist_mbid: Option<&str>, now: i64, outcome: FetchOutcome, releases_found: usize) -> Result<(), rusqlite::Error>`
  - `pub(crate) fn last_attempt_at(conn: &Connection, artist_key: &str) -> Result<Option<i64>, rusqlite::Error>`
  - `pub(crate) fn latest_attempt(conn: &Connection) -> Result<Option<i64>, rusqlite::Error>`

- [ ] **Step 1: Write the failing test**

Am Ende von `crates/reprise-core/src/artist_news_ledger.rs` (die Datei entsteht in Step 3; Test zuerst schreiben, dann Implementierung darüber setzen):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn record_attempt_inserts_then_updates_same_key() {
        let conn = conn();
        record_attempt(&conn, "pink floyd", Some("mbid-1"), 100, FetchOutcome::Ok, 3).unwrap();
        assert_eq!(last_attempt_at(&conn, "pink floyd").unwrap(), Some(100));

        record_attempt(&conn, "pink floyd", None, 200, FetchOutcome::Failed, 0).unwrap();
        assert_eq!(last_attempt_at(&conn, "pink floyd").unwrap(), Some(200));

        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM artist_news_fetch", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 1, "same key must not create a second row");
    }

    #[test]
    fn record_attempt_keeps_known_mbid_when_later_attempt_has_none() {
        let conn = conn();
        record_attempt(&conn, "pink floyd", Some("mbid-1"), 100, FetchOutcome::Ok, 1).unwrap();
        record_attempt(&conn, "pink floyd", None, 200, FetchOutcome::Failed, 0).unwrap();
        let mbid: Option<String> = conn
            .query_row(
                "SELECT artist_mbid FROM artist_news_fetch WHERE artist_key = 'pink floyd'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mbid.as_deref(), Some("mbid-1"));
    }

    #[test]
    fn unknown_key_has_no_attempt() {
        let conn = conn();
        assert_eq!(last_attempt_at(&conn, "nobody").unwrap(), None);
    }

    #[test]
    fn latest_attempt_reports_newest_across_all_artists() {
        let conn = conn();
        assert_eq!(latest_attempt(&conn).unwrap(), None);
        record_attempt(&conn, "a", None, 100, FetchOutcome::Unmatched, 0).unwrap();
        record_attempt(&conn, "b", None, 400, FetchOutcome::Ok, 2).unwrap();
        record_attempt(&conn, "c", None, 250, FetchOutcome::Failed, 0).unwrap();
        assert_eq!(latest_attempt(&conn).unwrap(), Some(400));
    }

    #[test]
    fn outcomes_serialize_to_stable_strings() {
        assert_eq!(FetchOutcome::Ok.as_str(), "ok");
        assert_eq!(FetchOutcome::Unmatched.as_str(), "unmatched");
        assert_eq!(FetchOutcome::Failed.as_str(), "failed");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p reprise-core artist_news_ledger`
Expected: FAIL — Modul existiert nicht.

- [ ] **Step 3: Write the implementation**

Oberhalb des Testmoduls in `crates/reprise-core/src/artist_news_ledger.rs`:

```rust
//! Read/write access to the per-artist New Releases fetch ledger.
//!
//! Every refresh attempt lands here — success, unmatched artist, and network
//! failure alike. That is the whole point: freshness must not depend on
//! whether the artist happened to have news, or artists with nothing to
//! report get re-fetched forever (see `db_artist_news_fetch`).

use rusqlite::Connection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FetchOutcome {
    Ok,
    Unmatched,
    Failed,
}

impl FetchOutcome {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            FetchOutcome::Ok => "ok",
            FetchOutcome::Unmatched => "unmatched",
            FetchOutcome::Failed => "failed",
        }
    }
}

/// Records one attempt. A later attempt that could not resolve an MBID keeps
/// the previously known one via `COALESCE` — losing a resolved MBID because
/// of one failed run would cost an extra search request on every future run.
pub(crate) fn record_attempt(
    conn: &Connection,
    artist_key: &str,
    artist_mbid: Option<&str>,
    now: i64,
    outcome: FetchOutcome,
    releases_found: usize,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO artist_news_fetch
           (artist_key, artist_mbid, last_attempt_at, last_outcome, releases_found)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(artist_key) DO UPDATE SET
           artist_mbid     = COALESCE(excluded.artist_mbid, artist_news_fetch.artist_mbid),
           last_attempt_at = excluded.last_attempt_at,
           last_outcome    = excluded.last_outcome,
           releases_found  = excluded.releases_found",
        rusqlite::params![
            artist_key,
            artist_mbid,
            now,
            outcome.as_str(),
            i64::try_from(releases_found).unwrap_or(i64::MAX),
        ],
    )?;
    Ok(())
}

pub(crate) fn last_attempt_at(
    conn: &Connection,
    artist_key: &str,
) -> Result<Option<i64>, rusqlite::Error> {
    conn.query_row(
        "SELECT last_attempt_at FROM artist_news_fetch WHERE artist_key = ?1",
        [artist_key],
        |row| row.get::<_, Option<i64>>(0),
    )
    .or_else(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
}

/// The newest attempt across every artist — the clock `refresh_due` is
/// judged against.
pub(crate) fn latest_attempt(conn: &Connection) -> Result<Option<i64>, rusqlite::Error> {
    conn.query_row("SELECT MAX(last_attempt_at) FROM artist_news_fetch", [], |row| {
        row.get(0)
    })
}
```

In `crates/reprise-core/src/lib.rs` neben die anderen `artist_news_*`-Deklarationen:

```rust
mod artist_news_ledger;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p reprise-core artist_news_ledger`
Expected: PASS, 5 Tests.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-core/src/artist_news_ledger.rs crates/reprise-core/src/lib.rs
git commit -m "feat: Lese-/Schreib-API für das New-Releases-Fetch-Ledger"
```

---

### Task 3: Frischeprüfung auf das Ledger umstellen

Das ist der Regressionsfix für U2: Ein Artist ohne Fundstellen gilt heute bei jedem Lauf als veraltet.

**Files:**
- Modify: `crates/reprise-core/src/artist_news.rs:228-271` (`refresh_with`), `:362-374` (`artist_cache_is_fresh`)
- Modify: `crates/reprise-core/src/artist_news_refresh.rs:54-60` (`latest_fetched_at`)
- Modify: `crates/reprise-core/src/artist_news_tests.rs`

**Interfaces:**
- Consumes: `artist_news_ledger::{record_attempt, last_attempt_at, latest_attempt, FetchOutcome}` aus Task 2.
- Produces: `artist_cache_is_fresh(conn: &Connection, artist_key: &str, now: i64) -> Result<bool, rusqlite::Error>` — nimmt jetzt den **Artist-Key**, nicht mehr die MBID.

- [ ] **Step 1: Write the failing test**

Ans Ende von `crates/reprise-core/src/artist_news_tests.rs`:

```rust
#[test]
fn ledger_marks_artist_without_news_fresh_and_second_run_skips_it() {
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, artist_mbid, album, play_count, added_at) \
         VALUES ('/music/one.flac', 'One', 'Pink Floyd', ?1, 'Local Album', 20, 0)",
        [ARTIST_ID],
    )
    .unwrap();
    // No release groups at all — the artist has nothing to report.
    let empty = r#"{"release-groups":[]}"#;
    let mut calls = 0;
    let mut fetch = |_url: &str| {
        calls += 1;
        Ok(empty.to_string())
    };

    let first = refresh_with(
        &conn,
        date(),
        1_000,
        FetchScope::TopArtists,
        false,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();
    assert_eq!(first.artists_fetched, 1);
    let after_first = calls;
    assert!(after_first > 0, "first run must hit the network");

    let second = refresh_with(
        &conn,
        date(),
        2_000, // well inside FETCH_TTL_SECONDS
        FetchScope::TopArtists,
        false,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();
    assert_eq!(
        second.artists_fetched, 0,
        "artist with no news must count as fresh, not be re-fetched"
    );
    assert_eq!(calls, after_first, "second run must issue no requests");
}

#[test]
fn ledger_records_unmatched_artist_and_skips_it_while_fresh() {
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
         VALUES ('/music/two.flac', 'Two', 'Nobody At All', 'Local Album', 5, 0)",
        [],
    )
    .unwrap();
    let mut calls = 0;
    let mut fetch = |_url: &str| {
        calls += 1;
        Ok(r#"{"artists":[]}"#.to_string())
    };

    let first = refresh_with(
        &conn,
        date(),
        1_000,
        FetchScope::TopArtists,
        false,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();
    assert_eq!(first.unmatched, 1);
    let after_first = calls;

    refresh_with(
        &conn,
        date(),
        2_000,
        FetchScope::TopArtists,
        false,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();
    assert_eq!(
        calls, after_first,
        "an unmatched artist must not be searched again while fresh"
    );
}

#[test]
fn latest_fetched_at_reads_the_ledger_not_found_releases() {
    let conn = migrated_conn();
    assert_eq!(
        crate::artist_news::latest_fetched_at(&conn).unwrap(),
        None,
        "empty ledger means never fetched"
    );
    crate::artist_news_ledger::record_attempt(
        &conn,
        "pink floyd",
        None,
        4_242,
        crate::artist_news_ledger::FetchOutcome::Ok,
        0,
    )
    .unwrap();
    assert_eq!(
        crate::artist_news::latest_fetched_at(&conn).unwrap(),
        Some(4_242),
        "an attempt without any found release must still count"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p reprise-core ledger_ latest_fetched_at_reads`
Expected: FAIL — `ledger_marks_artist_without_news_fresh_and_second_run_skips_it` scheitert mit `second.artists_fetched == 1`, `latest_fetched_at_reads_the_ledger_not_found_releases` mit `None` statt `Some(4242)`.

- [ ] **Step 3: Rewrite `artist_cache_is_fresh` to read the ledger**

In `crates/reprise-core/src/artist_news.rs` die Funktion ab Zeile 362 vollständig ersetzen:

```rust
/// Freshness is judged by the last *attempt* recorded in the ledger, not by
/// the newest release we happened to store. An artist with nothing to report
/// stores no release — judging by releases meant re-fetching them forever.
fn artist_cache_is_fresh(
    conn: &Connection,
    artist_key: &str,
    now: i64,
) -> Result<bool, rusqlite::Error> {
    let last_attempt = crate::artist_news_ledger::last_attempt_at(conn, artist_key)?;
    Ok(last_attempt
        .is_some_and(|attempt| now.saturating_sub(attempt).max(0) <= FETCH_TTL_SECONDS))
}
```

- [ ] **Step 4: Move the freshness check ahead of MBID resolution and record every branch**

In `crates/reprise-core/src/artist_news.rs` die Schleife in `refresh_with` (Zeilen 246-268) vollständig ersetzen:

```rust
    for candidate in candidates {
        let artist_key = normalize(&candidate.name);
        // Checked before resolving the MBID: a fresh artist must cost zero
        // requests, and the search request would otherwise be spent before
        // we ever consult the cache.
        if !force && artist_cache_is_fresh(conn, &artist_key, now).map_err(database_error)? {
            continue;
        }
        let mbid = match resolve_artist_mbid(conn, &candidate, fetch, &mut report)? {
            Some(mbid) => mbid,
            None => {
                crate::artist_news_ledger::record_attempt(
                    conn,
                    &artist_key,
                    None,
                    now,
                    crate::artist_news_ledger::FetchOutcome::Unmatched,
                    0,
                )
                .map_err(database_error)?;
                continue;
            }
        };
        let body = match fetch(&release_groups_url(&mbid)) {
            Ok(body) if release_payload_valid(&body) => body,
            Ok(_) | Err(_) => {
                report.failed += 1;
                crate::artist_news_ledger::record_attempt(
                    conn,
                    &artist_key,
                    Some(&mbid),
                    now,
                    crate::artist_news_ledger::FetchOutcome::Failed,
                    0,
                )
                .map_err(database_error)?;
                continue;
            }
        };
        let local_albums = local_albums(conn, &candidate.name).map_err(database_error)?;
        let items = parse_release_groups(&body, &local_albums, today);
        let accent = normalize_fallback_accent(fallback_accent(conn, &candidate.name));
        upsert_releases(conn, &candidate.name, &mbid, now, &accent, &items)
            .map_err(database_error)?;
        crate::artist_news_ledger::record_attempt(
            conn,
            &artist_key,
            Some(&mbid),
            now,
            crate::artist_news_ledger::FetchOutcome::Ok,
            items.len(),
        )
        .map_err(database_error)?;
        report.artists_fetched += 1;
        report.releases_upserted += items.len();
    }
```

Beachten: `resolve_artist_mbid` zählt `report.failed` bzw. `report.unmatched` bereits selbst hoch — hier wird nur der Ledger-Eintrag ergänzt, keine Zählung dupliziert.

- [ ] **Step 5: Point `latest_fetched_at` at the ledger**

In `crates/reprise-core/src/artist_news_refresh.rs` die Funktion ab Zeile 54 ersetzen (Doc-Kommentar in Zeile 52-53 mit anpassen):

```rust
/// The most recent attempt across all artists, or `None` if no artist has
/// ever been attempted. Reads the ledger rather than `new_releases`: a
/// library whose artists simply have no news would otherwise look like it
/// had never refreshed, and `refresh_due` would fire on every timer tick.
pub fn latest_fetched_at(conn: &Connection) -> Result<Option<i64>, rusqlite::Error> {
    crate::artist_news_ledger::latest_attempt(conn)
}
```

- [ ] **Step 6: Run the new tests**

Run: `cargo test -p reprise-core ledger_ latest_fetched_at_reads`
Expected: PASS, 3 Tests.

- [ ] **Step 7: Run the whole core suite**

Run: `cargo test -p reprise-core`
Expected: PASS. Bestehende Tests, die zweimal `refresh_with` mit nah beieinander liegenden `now`-Werten aufrufen, können jetzt am zweiten Lauf scheitern — das ist die gewollte neue Semantik. Solche Tests mit `force = true` oder einem `now` jenseits von `FETCH_TTL_SECONDS` anpassen, **nicht** die Produktionslogik aufweichen.

- [ ] **Step 8: Commit**

```bash
git add crates/reprise-core/src/artist_news.rs \
        crates/reprise-core/src/artist_news_refresh.rs \
        crates/reprise-core/src/artist_news_tests.rs
git commit -m "fix: Frische über das Fetch-Ledger statt über gefundene Releases"
```

---

### Task 4: Rotation nach „am längsten nicht geprüft"

**Files:**
- Modify: `crates/reprise-core/src/artist_news.rs:14-19` (Konstanten), `:62-79` (`FetchScope`, `configured_fetch_scope`), `:273-312` (`artists_for_fetch`)
- Modify: `crates/reprise-gnome/src/ui/preferences/preference_new_releases.rs:12-22`
- Modify: `crates/reprise-core/src/artist_news_tests.rs`

**Interfaces:**
- Consumes: Tabelle `artist_news_fetch` aus Task 1.
- Produces:
  - `pub enum FetchScope { TopArtists, AllArtists }` — die Variante `AllArtists` verliert `day_index`.
  - `pub fn configured_fetch_scope(conn: &Connection) -> Result<FetchScope, rusqlite::Error>` — verliert den `today`-Parameter.
  - `const REST_ARTISTS_PER_RUN: usize = 30` ersetzt `DAILY_REST_COUNT`.

- [ ] **Step 1: Write the failing test**

Ans Ende von `crates/reprise-core/src/artist_news_tests.rs`:

```rust
#[test]
fn rotation_prefers_never_checked_artists_over_play_count() {
    let conn = migrated_conn();
    // 22 artists so the rest group is non-empty (TOP_ARTIST_COUNT = 20).
    // Play counts descend, so "artist-21" and "artist-22" are the tail.
    for index in 1..=22 {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
             VALUES (?1, 'T', ?2, 'Album', ?3, 0)",
            rusqlite::params![
                format!("/music/{index}.flac"),
                format!("artist-{index:02}"),
                100 - index,
            ],
        )
        .unwrap();
    }
    // The very last artist by plays was checked long ago; the second-to-last
    // was checked just now. Only the stale one may come up.
    crate::artist_news_ledger::record_attempt(
        &conn,
        "artist-21",
        None,
        9_000,
        crate::artist_news_ledger::FetchOutcome::Ok,
        0,
    )
    .unwrap();

    let candidates = artists_for_fetch(&conn, FetchScope::AllArtists).unwrap();
    let names = candidates
        .iter()
        .map(|candidate| candidate.name.as_str())
        .collect::<Vec<_>>();

    assert_eq!(names.len(), 22, "top 20 plus the rest group");
    assert_eq!(
        names[20], "artist-22",
        "never-checked artist must come before a recently checked one"
    );
    assert_eq!(names[21], "artist-21");
}

#[test]
fn top_artists_scope_ignores_the_rest_group_entirely() {
    let conn = migrated_conn();
    for index in 1..=22 {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
             VALUES (?1, 'T', ?2, 'Album', ?3, 0)",
            rusqlite::params![
                format!("/music/{index}.flac"),
                format!("artist-{index:02}"),
                100 - index,
            ],
        )
        .unwrap();
    }
    let candidates = artists_for_fetch(&conn, FetchScope::TopArtists).unwrap();
    assert_eq!(candidates.len(), 20);
}

#[test]
fn configured_scope_round_trips_without_a_date() {
    let conn = migrated_conn();
    assert_eq!(
        configured_fetch_scope(&conn).unwrap(),
        FetchScope::TopArtists
    );
    set_fetch_all_artists(&conn, true).unwrap();
    assert_eq!(
        configured_fetch_scope(&conn).unwrap(),
        FetchScope::AllArtists
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p reprise-core rotation_prefers top_artists_scope configured_scope_round_trips`
Expected: FAIL — Kompilierfehler, `FetchScope::AllArtists` erwartet noch das Feld `day_index`, `configured_fetch_scope` noch zwei Argumente.

- [ ] **Step 3: Simplify the scope type**

In `crates/reprise-core/src/artist_news.rs` Zeile 17 ersetzen:

```rust
const REST_ARTISTS_PER_RUN: usize = 30;
```

`DAILY_REST_COUNT` ersatzlos entfernen. Dann Zeilen 62-79 ersetzen:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchScope {
    TopArtists,
    AllArtists,
}

pub fn configured_fetch_scope(conn: &Connection) -> Result<FetchScope, rusqlite::Error> {
    if crate::library::settings::get_bool(conn, FETCH_ALL_ARTISTS_KEY, false)? {
        Ok(FetchScope::AllArtists)
    } else {
        Ok(FetchScope::TopArtists)
    }
}
```

Der Import von `Datelike` in Zeile 7 wird damit unbenutzt — auf `use chrono::NaiveDate;` reduzieren.

- [ ] **Step 4: Order the rest group by staleness**

In `crates/reprise-core/src/artist_news.rs` `artists_for_fetch` (Zeilen 273-312) vollständig ersetzen:

```rust
/// Candidates for this run: the `TOP_ARTIST_COUNT` most-played artists
/// always, plus — in `AllArtists` scope — the `REST_ARTISTS_PER_RUN` artists
/// that have gone longest without an attempt, never-checked ones first.
///
/// Ordering the tail by staleness rather than by a date-derived rotation
/// window is what lets an artist you own a single track of ever come up at
/// all: play count decides who is *preferred*, not who is *reachable*. A run
/// that never happens costs nothing now — the skipped artists are simply the
/// oldest next time.
pub(crate) fn artists_for_fetch(
    conn: &Connection,
    scope: FetchScope,
) -> Result<Vec<ArtistCandidate>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT MIN(trim(artist)), MAX(artist_mbid), SUM(play_count) AS plays
         FROM tracks
         WHERE removed_at IS NULL AND missing_since IS NULL AND trim(artist) <> ''
         GROUP BY lower(trim(artist))
         HAVING MAX(artist_mbid) IS NOT NULL OR MAX(artist_mbid_negative) = 0
         ORDER BY plays DESC, lower(MIN(trim(artist))) ASC",
    )?;
    let mut candidates = statement
        .query_map([], |row| {
            Ok(ArtistCandidate {
                name: row.get(0)?,
                mbid: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if candidates.len() <= TOP_ARTIST_COUNT {
        return Ok(candidates);
    }
    match scope {
        FetchScope::TopArtists => {
            candidates.truncate(TOP_ARTIST_COUNT);
            Ok(candidates)
        }
        FetchScope::AllArtists => {
            let mut rest = candidates.split_off(TOP_ARTIST_COUNT);
            let mut keyed = Vec::with_capacity(rest.len());
            for candidate in rest.drain(..) {
                let last_attempt = crate::artist_news_ledger::last_attempt_at(
                    conn,
                    &normalize(&candidate.name),
                )?;
                keyed.push((last_attempt, candidate));
            }
            // `None` sorts before `Some` — never-checked artists come first.
            keyed.sort_by(|(left, _), (right, _)| left.cmp(right));
            candidates.extend(
                keyed
                    .into_iter()
                    .take(REST_ARTISTS_PER_RUN)
                    .map(|(_, candidate)| candidate),
            );
            Ok(candidates)
        }
    }
}
```

- [ ] **Step 5: Update the preferences call site**

In `crates/reprise-gnome/src/ui/preferences/preference_new_releases.rs` die Zeilen 13-22 ersetzen:

```rust
    let selected = reprise_core::artist_news::configured_fetch_scope(&conn.borrow()).map_or(
        0,
        |scope| {
            u32::from(matches!(
                scope,
                reprise_core::artist_news::FetchScope::AllArtists
            ))
        },
    );
```

Die Zeile `let today = chrono::Local::now().date_naive();` entfällt ersatzlos.

- [ ] **Step 6: Run the new tests**

Run: `cargo test -p reprise-core rotation_prefers top_artists_scope configured_scope_round_trips`
Expected: PASS, 3 Tests.

- [ ] **Step 7: Build and test both crates**

Run: `cargo test -p reprise-core && cargo build -p reprise-gnome`
Expected: PASS bzw. erfolgreicher Build. Bestehende Tests, die `FetchScope::AllArtists { day_index: … }` konstruieren, auf die parameterlose Variante umstellen.

- [ ] **Step 8: Commit**

```bash
git add crates/reprise-core/src/artist_news.rs \
        crates/reprise-core/src/artist_news_tests.rs \
        crates/reprise-gnome/src/ui/preferences/preference_new_releases.rs
git commit -m "feat: Rest-Artists nach Fälligkeit statt nach Tagesrotation abfragen"
```

---

### Task 5: Besitz messen statt Titel vergleichen

Der Kern des Vorab-Single-Falls.

**Files:**
- Modify: `crates/reprise-core/src/artist_news.rs:15` (Konstante), `:178-205` (`parse_release_groups`), `:376-385` (`local_albums`), `:659-696` (`parse_release_group`)
- Modify: `crates/reprise-core/src/artist_news_tests.rs`

**Interfaces:**
- Consumes: nichts aus früheren Tasks.
- Produces:
  - `const OWNED_ALBUM_MIN_TRACKS: i64 = 2`
  - `local_albums` liefert weiterhin `Vec<String>`, enthält aber nur Alben mit `>= OWNED_ALBUM_MIN_TRACKS` Tracks.
  - `pub fn parse_release_groups(json: &str, local_albums: &[String], today: NaiveDate) -> Vec<AlbumNews>` — Signatur unverändert (der Singles-Schalter kommt erst in Task 7 dazu).

- [ ] **Step 1: Write the failing test**

Ans Ende von `crates/reprise-core/src/artist_news_tests.rs`:

```rust
#[test]
fn upcoming_album_survives_a_local_title_match() {
    // The lead single is tagged with the forthcoming album's name. An album
    // that has not been released yet cannot be owned, so the match must be
    // ignored entirely — this is the case the whole change exists for.
    let json = r#"{"release-groups":[
      {"id":"1","title":"Eclipse","first-release-date":"2026-09-01","primary-type":"Album","secondary-types":[]}
    ]}"#;
    let items = parse_release_groups(json, &["Eclipse".into()], date());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Eclipse");
    assert_eq!(items[0].kind, NewsKind::Upcoming);
}

#[test]
fn released_album_is_filtered_only_when_the_local_album_is_really_owned() {
    let conn = migrated_conn();
    // Two tracks under "Owned Album" — that counts as owned.
    for index in 1..=2 {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
             VALUES (?1, 'T', 'Pink Floyd', 'Owned Album', 1, 0)",
            [format!("/music/owned-{index}.flac")],
        )
        .unwrap();
    }
    // One track under "Single Only" — a single, not the album.
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
         VALUES ('/music/single.flac', 'S', 'Pink Floyd', 'Single Only', 1, 0)",
        [],
    )
    .unwrap();

    let owned = crate::artist_news::local_albums_for_test(&conn, "Pink Floyd").unwrap();
    assert!(owned.iter().any(|album| album == "Owned Album"));
    assert!(
        !owned.iter().any(|album| album == "Single Only"),
        "one track must not make the whole album count as owned"
    );

    let json = r#"{"release-groups":[
      {"id":"1","title":"Owned Album","first-release-date":"2026-07-01","primary-type":"Album","secondary-types":[]},
      {"id":"2","title":"Single Only","first-release-date":"2026-07-01","primary-type":"Album","secondary-types":[]}
    ]}"#;
    let items = parse_release_groups(json, &owned, date());
    let titles = items
        .iter()
        .map(|item| item.title.as_str())
        .collect::<Vec<_>>();
    assert_eq!(titles, ["Single Only"]);
}
```

Damit der erste Teil des zweiten Tests kompiliert, braucht `local_albums` einen Test-Zugang. In `crates/reprise-core/src/artist_news.rs` direkt unter `local_albums` ergänzen:

```rust
#[cfg(test)]
pub(crate) fn local_albums_for_test(
    conn: &Connection,
    artist: &str,
) -> Result<Vec<String>, rusqlite::Error> {
    local_albums(conn, artist)
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p reprise-core upcoming_album_survives released_album_is_filtered`
Expected: FAIL — `upcoming_album_survives_a_local_title_match` liefert 0 Items, `local_albums_for_test` enthält noch „Single Only".

- [ ] **Step 3: Add the ownership threshold**

In `crates/reprise-core/src/artist_news.rs` neben die anderen Konstanten (nach Zeile 15):

```rust
/// How many tracks of an album must be present before the album counts as
/// owned. One track is a single, not an album — treating it as ownership is
/// what used to suppress the very album the single announces.
const OWNED_ALBUM_MIN_TRACKS: i64 = 2;
```

`local_albums` (Zeilen 376-385) ersetzen:

```rust
fn local_albums(conn: &Connection, artist: &str) -> Result<Vec<String>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT album FROM tracks
         WHERE lower(trim(artist)) = lower(trim(?1)) AND trim(album) <> ''
           AND removed_at IS NULL AND missing_since IS NULL
         GROUP BY lower(trim(album))
         HAVING COUNT(*) >= ?2",
    )?;
    let albums = statement
        .query_map(rusqlite::params![artist, OWNED_ALBUM_MIN_TRACKS], |row| {
            row.get(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(albums)
}
```

- [ ] **Step 4: Skip the library check for upcoming releases**

In `crates/reprise-core/src/artist_news.rs` `parse_release_group` (Zeilen 659-696) ersetzen. Entscheidend ist die Reihenfolge: Der `NewsKind` muss **vor** dem Bibliotheks-Abgleich feststehen, weil er darüber entscheidet, ob der Abgleich überhaupt stattfindet.

```rust
fn parse_release_group(
    group: &serde_json::Value,
    local: &std::collections::HashSet<String>,
    today: NaiveDate,
) -> Option<(AlbumNews, NaiveDate)> {
    let mbid = group.get("id")?.as_str()?.to_string();
    let title = group.get("title")?.as_str()?.trim().to_string();
    let date_text = group.get("first-release-date")?.as_str()?.to_string();
    let release_date = parse_partial_date(&date_text)?;
    let primary_type = group.get("primary-type")?.as_str()?.to_string();
    let primary_type_normalized = primary_type.to_ascii_lowercase();
    if !matches!(primary_type_normalized.as_str(), "album" | "ep" | "single")
        || title.is_empty()
        || has_excluded_secondary_type(group)
    {
        return None;
    }
    let delta = release_date.signed_duration_since(today).num_days();
    let kind = match primary_type_normalized.as_str() {
        "single" if date_text.len() == 10 && delta > 0 => NewsKind::Upcoming,
        "single" => return None,
        _ if delta >= 0 => NewsKind::Upcoming,
        _ if delta >= -NEWS_WINDOW_DAYS => NewsKind::New,
        _ => return None,
    };
    // An unreleased album cannot be owned. A title match here is by
    // definition a mis-tagged pre-release track — typically the lead single
    // tagged with the forthcoming album's name — so the library check is
    // skipped outright rather than merely relaxed.
    if kind == NewsKind::New && local.contains(&normalize(&title)) {
        return None;
    }
    Some((
        AlbumNews {
            release_group_mbid: mbid,
            title,
            first_release_date: date_text,
            primary_type,
            kind,
            announce_url: crate::artist_news_links::parse_announce_url(group),
        },
        release_date,
    ))
}
```

- [ ] **Step 5: Run the new tests**

Run: `cargo test -p reprise-core upcoming_album_survives released_album_is_filtered`
Expected: PASS, 2 Tests.

- [ ] **Step 6: Run the whole core suite**

Run: `cargo test -p reprise-core`
Expected: PASS. `release_parser_keeps_regular_albums_and_eps_but_not_local_albums` (Zeile 66) filtert eine `New`-EP und bleibt gültig.

- [ ] **Step 7: Commit**

```bash
git add crates/reprise-core/src/artist_news.rs crates/reprise-core/src/artist_news_tests.rs
git commit -m "fix: Vorab-Single unterdrückt nicht mehr das angekündigte Album"
```

---

### Task 6: Präsenz als Drei-Zustand — Core und UI in einem Commit

> Core- und UI-Teil gehören in **einen** Commit: Sobald `StoredRelease.in_library` verschwindet, kompiliert `reprise-gnome` erst wieder, wenn `release_row.rs` und `history_page.rs` nachgezogen sind. Getrennt committen hieße einen nicht baubaren Zwischenstand in der Historie.

#### Teil A — Core

**Files:**
- Modify: `crates/reprise-core/src/artist_news.rs:46-60` (`StoredRelease`), `:439-456` (`local_album_set`), `:458-495` (`query_releases`)
- Modify: `crates/reprise-core/src/artist_news_history.rs:38-50` (`HistoryEntry`), `:110-125` (`query_history`)
- Modify: `crates/reprise-core/src/artist_news_tests.rs`

**Interfaces:**
- Consumes: `OWNED_ALBUM_MIN_TRACKS` aus Task 5.
- Produces:
  - `pub enum LibraryPresence { Absent, Partial, Complete }`
  - `StoredRelease.presence: LibraryPresence` ersetzt `in_library: bool`
  - `HistoryEntry.presence: LibraryPresence` ersetzt `in_library: bool`
  - `pub(crate) fn local_album_track_counts(conn: &Connection) -> Result<HashMap<(String, String), i64>, rusqlite::Error>` ersetzt `local_album_set`
  - `pub(crate) fn presence_for(counts: &HashMap<(String, String), i64>, artist: &str, title: &str) -> LibraryPresence`

- [ ] **Step 1: Write the failing test**

Ans Ende von `crates/reprise-core/src/artist_news_tests.rs`:

```rust
#[test]
fn presence_distinguishes_absent_partial_and_complete() {
    use crate::artist_news::{presence_for, LibraryPresence};

    let mut counts = std::collections::HashMap::new();
    counts.insert(("pink floyd".to_string(), "owned album".to_string()), 2);
    counts.insert(("pink floyd".to_string(), "just a single".to_string()), 1);

    assert_eq!(
        presence_for(&counts, "Pink Floyd", "Owned Album"),
        LibraryPresence::Complete
    );
    assert_eq!(
        presence_for(&counts, " PINK   FLOYD ", " just a single "),
        LibraryPresence::Partial,
        "normalization must match query_releases' own"
    );
    assert_eq!(
        presence_for(&counts, "Pink Floyd", "Never Heard Of It"),
        LibraryPresence::Absent
    );
}

#[test]
fn query_releases_reports_partial_ownership_for_a_single_track() {
    use crate::artist_news::LibraryPresence;

    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
         VALUES ('/music/lead.flac', 'Lead Single', 'Pink Floyd', 'Eclipse', 1, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO new_releases (release_group_mbid, artist_name, artist_mbid, title, \
         release_type, first_release_date, fetched_at, fallback_accent, first_seen) \
         VALUES ('rg-1', 'Pink Floyd', 'mbid-1', 'Eclipse', 'Album', '2026-09-01', 100, \
         '#3584E4', 100)",
        [],
    )
    .unwrap();

    let releases = query_releases(&conn, false, date()).unwrap();
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].presence, LibraryPresence::Partial);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p reprise-core presence_distinguishes query_releases_reports_partial`
Expected: FAIL — Kompilierfehler, `LibraryPresence` und `presence_for` existieren nicht.

- [ ] **Step 3: Introduce the presence type**

In `crates/reprise-core/src/artist_news.rs` vor `StoredRelease` einfügen:

```rust
/// How much of a release the local library already holds. A `bool` cannot
/// express the case this feature exists for: you own the lead single, so the
/// album is *relevant* to you — but calling that "in library" would send you
/// to the library instead of to the announcement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryPresence {
    Absent,
    Partial,
    Complete,
}
```

In `StoredRelease` (Zeile 58) das Feld ersetzen:

```rust
    pub presence: LibraryPresence,
```

- [ ] **Step 4: Return track counts instead of a set**

In `crates/reprise-core/src/artist_news.rs` `local_album_set` (Zeilen 439-456) ersetzen:

```rust
/// `(normalized artist, normalized album) → track count` for the local
/// library. Shared by `query_releases`' presence annotation and
/// `query_history`'s identical need. Deliberately threshold-free: this
/// describes the library, it does not filter — the threshold lives in
/// `presence_for`.
pub(crate) fn local_album_track_counts(
    conn: &Connection,
) -> Result<std::collections::HashMap<(String, String), i64>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT artist, album, COUNT(*) FROM tracks
         WHERE removed_at IS NULL AND missing_since IS NULL AND trim(album) <> ''
         GROUP BY lower(trim(artist)), lower(trim(album))",
    )?;
    let counts = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .map(|row| row.map(|(artist, album, count)| ((normalize(&artist), normalize(&album)), count)))
        .collect::<Result<std::collections::HashMap<_, _>, _>>()?;
    Ok(counts)
}

/// Maps a track count onto the presence states. `OWNED_ALBUM_MIN_TRACKS` is
/// the same threshold `local_albums` filters by, so "counts as owned" means
/// the same thing on both sides.
pub(crate) fn presence_for(
    counts: &std::collections::HashMap<(String, String), i64>,
    artist: &str,
    title: &str,
) -> LibraryPresence {
    match counts
        .get(&(normalize(artist), normalize(title)))
        .copied()
        .unwrap_or(0)
    {
        0 => LibraryPresence::Absent,
        count if count < OWNED_ALBUM_MIN_TRACKS => LibraryPresence::Partial,
        _ => LibraryPresence::Complete,
    }
}
```

- [ ] **Step 5: Update both query sites**

In `crates/reprise-core/src/artist_news.rs` `query_releases`: Zeile 484 (`in_library: false`) wird zu `presence: LibraryPresence::Absent`, und die Schleife in Zeilen 488-492 wird zu:

```rust
    let counts = local_album_track_counts(conn)?;
    for release in &mut releases {
        release.presence = presence_for(&counts, &release.artist_name, &release.title);
    }
```

In `crates/reprise-core/src/artist_news_history.rs`: Feld `in_library: bool` (Zeile 48) wird zu `pub presence: crate::artist_news::LibraryPresence`, Zeile 114 zu `presence: crate::artist_news::LibraryPresence::Absent`, und Zeilen 119-121 zu:

```rust
    let counts = crate::artist_news::local_album_track_counts(conn)?;
    for entry in &mut entries {
        entry.presence =
            crate::artist_news::presence_for(&counts, &entry.artist_name, &entry.title);
    }
```

Der bestehende Code dort lautet:

```rust
    let local_albums = crate::artist_news::local_album_set(conn)?;
    for entry in &mut entries {
        entry.in_library = local_albums.contains(&(
            // … normalisierte (artist, title)-Paarbildung
        ));
    }
```

Die Paarbildung entfällt ersatzlos — `presence_for` normalisiert selbst. Nach der Änderung darf `local_album_set` nirgends mehr aufgerufen werden; `rg "local_album_set" crates/` muss leer sein.

- [ ] **Step 6: Run the new tests**

Run: `cargo test -p reprise-core presence_distinguishes query_releases_reports_partial`
Expected: PASS, 2 Tests.

- [ ] **Step 7: Run the core suite**

Run: `cargo test -p reprise-core`
Expected: PASS. Testfixtures, die `in_library: false` oder `in_library = true` setzen, auf `presence: LibraryPresence::Absent` bzw. `LibraryPresence::Complete` umstellen. `reprise-gnome` kompiliert an dieser Stelle noch nicht — das kommt in Teil B. **Hier noch nicht committen.**

- [ ] **Step 8: Noch nicht committen — weiter mit Teil B**

Der Core-Teil allein hinterlässt ein nicht baubares `reprise-gnome`. Erst nach Teil B wird committed, in einem gemeinsamen Commit.

---

#### Teil B — UI

**Files:**
- Modify: `crates/reprise-gnome/src/ui/new_releases/release_row.rs:30-42` (Enums), `:105-133` (`chip_presentation`, `primary_action`), `:290-310` (Chip-Rendering)
- Modify: `crates/reprise-gnome/src/ui/new_releases/history_page.rs:43-54` (`history_action`)
- Modify: `crates/reprise-gnome/src/ui/strings_news.rs`
- Modify: `crates/reprise-gnome/src/ui/new_releases/css.rs`

**Interfaces:**
- Consumes: `LibraryPresence` aus Teil A dieses Tasks.
- Produces: `ChipPresentation::PartiallyOwned` als neue Variante neben `Upcoming`, `Released`, `InLibrary`.

- [ ] **Step 1: Write the failing test**

Ans Ende des `mod tests` in `crates/reprise-gnome/src/ui/new_releases/release_row.rs`:

```rust
#[test]
fn partial_ownership_gets_its_own_chip_and_opens_the_announcement() {
    use reprise_core::artist_news::LibraryPresence;

    let mut release = stored_release("rg-partial");
    release.first_release_date = "2026-07-01".to_string();
    release.presence = LibraryPresence::Partial;

    assert_eq!(
        chip_presentation(&release, today()),
        ChipPresentation::PartiallyOwned
    );
    assert!(
        matches!(
            primary_action(&release, today()),
            PrimaryAction::OpenAnnouncement(_)
        ),
        "owning one track means you want the rest, not a jump into the library"
    );
}

#[test]
fn complete_ownership_still_navigates_into_the_library() {
    use reprise_core::artist_news::LibraryPresence;

    let mut release = stored_release("rg-complete");
    release.first_release_date = "2026-07-01".to_string();
    release.presence = LibraryPresence::Complete;

    assert_eq!(chip_presentation(&release, today()), ChipPresentation::InLibrary);
    assert_eq!(primary_action(&release, today()), PrimaryAction::ShowInLibrary);
}

#[test]
fn upcoming_still_outranks_every_presence_state() {
    use reprise_core::artist_news::LibraryPresence;

    for presence in [
        LibraryPresence::Absent,
        LibraryPresence::Partial,
        LibraryPresence::Complete,
    ] {
        let mut release = stored_release("rg-upcoming");
        release.first_release_date = "2026-09-01".to_string();
        release.presence = presence;
        assert!(matches!(
            chip_presentation(&release, today()),
            ChipPresentation::Upcoming(_)
        ));
        assert!(matches!(
            primary_action(&release, today()),
            PrimaryAction::OpenAnnouncement(_)
        ));
    }
}
```

Die Helfer `stored_release(id)` und `today()` existieren im Testmodul der Datei bereits. Falls der vorhandene Helfer anders heißt, den vorhandenen benutzen statt einen neuen anzulegen.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p reprise-gnome partial_ownership complete_ownership upcoming_still_outranks`
Expected: FAIL — `ChipPresentation::PartiallyOwned` existiert nicht.

- [ ] **Step 3: Add the chip variant and the strings**

In `crates/reprise-gnome/src/ui/new_releases/release_row.rs` das Enum (Zeilen 30-35) ersetzen:

```rust
#[derive(Debug, PartialEq, Eq)]
pub(in crate::ui) enum ChipPresentation {
    Upcoming(String),
    Released,
    PartiallyOwned,
    InLibrary,
}
```

In `crates/reprise-gnome/src/ui/strings_news.rs` neben `NEWS_UPCOMING`:

```rust
pub const NEW_RELEASES_PARTIALLY_OWNED: &str = N_!("Single in library");
```

- [ ] **Step 4: Map presence onto chip and action**

In `crates/reprise-gnome/src/ui/new_releases/release_row.rs` `chip_presentation` und `primary_action` (Zeilen 105-133) ersetzen:

```rust
pub(in crate::ui) fn chip_presentation(
    release: &StoredRelease,
    today: NaiveDate,
) -> ChipPresentation {
    if let Some(date) = parse_release_date(&release.first_release_date) {
        if date > today {
            let days_until = (date - today).num_days();
            return ChipPresentation::Upcoming(strings::new_releases_days_until(days_until));
        }
    }
    match release.presence {
        LibraryPresence::Complete => ChipPresentation::InLibrary,
        LibraryPresence::Partial => ChipPresentation::PartiallyOwned,
        LibraryPresence::Absent => ChipPresentation::Released,
    }
}

pub(in crate::ui) fn primary_action(release: &StoredRelease, today: NaiveDate) -> PrimaryAction {
    // Only full ownership navigates into the library. Owning the lead single
    // means the album is still something to go read about, not something to
    // go listen to.
    if release.presence == LibraryPresence::Complete && !is_upcoming(release, today) {
        return PrimaryAction::ShowInLibrary;
    }
    PrimaryAction::OpenAnnouncement(reprise_core::artist_news_links::announce_url_or_fallback(
        release.announce_url.as_deref(),
        &release.release_group_mbid,
    ))
}
```

Den Import ergänzen: `use reprise_core::artist_news::LibraryPresence;`.

- [ ] **Step 5: Render the new chip**

In `crates/reprise-gnome/src/ui/new_releases/release_row.rs` im Chip-Rendering (um Zeile 300) den bestehenden `match` um den Arm erweitern, direkt neben `ChipPresentation::InLibrary`:

```rust
        ChipPresentation::PartiallyOwned => {
            chip.set_label(&strings::text(strings::NEW_RELEASES_PARTIALLY_OWNED));
            chip.add_css_class("new-release-chip-partial");
        }
```

In `crates/reprise-gnome/src/ui/new_releases/css.rs` direkt nach der Regel `.new-release-chip-neutral` (endet um Zeile 85) einfügen — beachten, dass die Datei ein escaptes String-Literal ist, jede Zeile endet also auf `\`:

```rust
    /* Partial-ownership chip: you hold the lead single, not the album. \
       Sits between the neutral "released" chip and the accent "upcoming" \
       one — a dimmed accent outline says "related to you" without \
       claiming the album is yours. */\
    .new-release-chip-partial {\
        border: 1px solid alpha(@accent_bg_color, 0.30);\
        color: alpha(@accent_color, 0.85);\
        background-color: transparent;\
        border-radius: 999px;\
        padding: 2px 8px;\
        font-size: 11px;\
    }\
```

Im Test `css_covers_every_new_release_class` (Zeile 149) den Eintrag `".new-release-chip-partial",` neben `".new-release-chip-neutral",` in die Liste aufnehmen — sonst schlägt dieser bestehende Test fehl.

- [ ] **Step 6: Update the history action**

In `crates/reprise-gnome/src/ui/new_releases/history_page.rs` `history_action` (Zeilen 43-54):

```rust
    if entry.presence == reprise_core::artist_news::LibraryPresence::Complete
        && !is_upcoming(entry, today)
    {
        return HistoryAction::ShowInLibrary;
    }
```

Den Doc-Kommentar darüber (Zeilen 32-42) entsprechend anpassen: Er beschreibt `in_library` als Namens-Match; jetzt ist die Bedingung vollständiger Besitz.

- [ ] **Step 7: Run the tests**

Run: `cargo test -p reprise-gnome partial_ownership complete_ownership upcoming_still_outranks`
Expected: PASS, 3 Tests.

- [ ] **Step 8: Build and test everything**

Run: `cargo test`
Expected: PASS über beide Crates. Fixtures in `history_page.rs`, die `in_library` setzen, auf `presence` umstellen.

- [ ] **Step 9: Commit**

Ein gemeinsamer Commit für Teil A und Teil B — der Core-Umbau allein wäre nicht baubar.

```bash
git add crates/reprise-core/src/artist_news.rs \
        crates/reprise-core/src/artist_news_history.rs \
        crates/reprise-core/src/artist_news_tests.rs \
        crates/reprise-gnome/src/ui/new_releases/release_row.rs \
        crates/reprise-gnome/src/ui/new_releases/history_page.rs \
        crates/reprise-gnome/src/ui/new_releases/css.rs \
        crates/reprise-gnome/src/ui/strings_news.rs
git commit -m "feat: LibraryPresence als Drei-Zustand statt in_library-Bool"
```

---

### Task 7: Singles-Schalter

**Files:**
- Modify: `crates/reprise-core/src/artist_news.rs:19` (Setting-Key), `:178-205` (`parse_release_groups`), `:228-271` (`refresh_with`), `:659-696` (`parse_release_group`)
- Modify: `crates/reprise-gnome/src/ui/preferences/preference_new_releases.rs`
- Modify: `crates/reprise-gnome/src/ui/strings_news.rs`
- Modify: `crates/reprise-core/src/artist_news_tests.rs`

**Interfaces:**
- Consumes: `parse_release_group` in der Form aus Task 5.
- Produces:
  - `pub fn include_singles(conn: &Connection) -> Result<bool, rusqlite::Error>`
  - `pub fn set_include_singles(conn: &Connection, include: bool) -> Result<(), rusqlite::Error>`
  - `pub fn parse_release_groups(json: &str, local_albums: &[String], today: NaiveDate, include_singles: bool) -> Vec<AlbumNews>` — der Parameter kommt hier dazu.
  - `pub(in crate::ui) fn singles_row(conn: &Rc<RefCell<Connection>>, enabled: bool) -> adw::SwitchRow`

- [ ] **Step 1: Write the failing test**

Ans Ende von `crates/reprise-core/src/artist_news_tests.rs`:

```rust
const SINGLES: &str = r#"{"release-groups":[
  {"id":"1","title":"Released Single","first-release-date":"2026-07-01","primary-type":"Single","secondary-types":[]},
  {"id":"2","title":"Announced Single","first-release-date":"2026-08-20","primary-type":"Single","secondary-types":[]},
  {"id":"3","title":"Old Single","first-release-date":"2025-01-01","primary-type":"Single","secondary-types":[]}
]}"#;

#[test]
fn released_singles_are_dropped_while_the_switch_is_off() {
    let items = parse_release_groups(SINGLES, &[], date(), false);
    let titles = items
        .iter()
        .map(|item| item.title.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        titles,
        ["Announced Single"],
        "an announced single with an exact date passes regardless of the switch"
    );
}

#[test]
fn released_singles_pass_within_the_window_while_the_switch_is_on() {
    let items = parse_release_groups(SINGLES, &[], date(), true);
    let mut titles = items
        .iter()
        .map(|item| item.title.as_str())
        .collect::<Vec<_>>();
    titles.sort_unstable();
    assert_eq!(
        titles,
        ["Announced Single", "Released Single"],
        "'Old Single' is outside NEWS_WINDOW_DAYS and stays out"
    );
}

#[test]
fn include_singles_setting_defaults_to_off_and_round_trips() {
    let conn = migrated_conn();
    assert!(!crate::artist_news::include_singles(&conn).unwrap());
    crate::artist_news::set_include_singles(&conn, true).unwrap();
    assert!(crate::artist_news::include_singles(&conn).unwrap());
    crate::artist_news::set_include_singles(&conn, false).unwrap();
    assert!(!crate::artist_news::include_singles(&conn).unwrap());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p reprise-core released_singles include_singles_setting`
Expected: FAIL — `parse_release_groups` nimmt drei Argumente, `include_singles` existiert nicht.

- [ ] **Step 3: Add the setting**

In `crates/reprise-core/src/artist_news.rs` neben `FETCH_ALL_ARTISTS_KEY` (Zeile 19):

```rust
const INCLUDE_SINGLES_KEY: &str = "module.new_releases.include_singles";
```

Neben `set_fetch_all_artists` (nach Zeile 83):

```rust
/// Whether already-released singles count as news. Off by default: singles
/// are the most common release type, so switching this on noticeably
/// increases how much the badge reports.
pub fn include_singles(conn: &Connection) -> Result<bool, rusqlite::Error> {
    crate::library::settings::get_bool(conn, INCLUDE_SINGLES_KEY, false)
}

pub fn set_include_singles(conn: &Connection, include: bool) -> Result<(), rusqlite::Error> {
    crate::library::settings::set_bool(conn, INCLUDE_SINGLES_KEY, include)
}
```

- [ ] **Step 4: Thread the flag through the parser**

`parse_release_groups` (Zeilen 178-205) — Signatur und Weitergabe:

```rust
pub fn parse_release_groups(
    json: &str,
    local_albums: &[String],
    today: NaiveDate,
    include_singles: bool,
) -> Vec<AlbumNews> {
```

und im Filter-Aufruf:

```rust
        .filter_map(|group| parse_release_group(group, &local, today, include_singles))
```

In `parse_release_group` Signatur und `kind`-Arm anpassen:

```rust
fn parse_release_group(
    group: &serde_json::Value,
    local: &std::collections::HashSet<String>,
    today: NaiveDate,
    include_singles: bool,
) -> Option<(AlbumNews, NaiveDate)> {
```

```rust
    let kind = match primary_type_normalized.as_str() {
        // An announced single needs an exact date to be trustworthy; that
        // rule predates the switch and stays on unconditionally, so turning
        // the switch off never shows *less* than before.
        "single" if date_text.len() == 10 && delta > 0 => NewsKind::Upcoming,
        "single" if !include_singles => return None,
        "single" if delta >= -NEWS_WINDOW_DAYS => NewsKind::New,
        "single" => return None,
        _ if delta >= 0 => NewsKind::Upcoming,
        _ if delta >= -NEWS_WINDOW_DAYS => NewsKind::New,
        _ => return None,
    };
```

In `refresh_with` den Schalter einmal pro Lauf lesen — direkt vor der `for candidate`-Schleife:

```rust
    let include_singles = include_singles(conn).map_err(database_error)?;
```

und den Parse-Aufruf anpassen:

```rust
        let items = parse_release_groups(&body, &local_albums, today, include_singles);
```

- [ ] **Step 5: Run the new tests**

Run: `cargo test -p reprise-core released_singles include_singles_setting`
Expected: PASS, 3 Tests.

- [ ] **Step 6: Add the preferences switch**

In `crates/reprise-gnome/src/ui/strings_news.rs`:

```rust
pub const NEW_RELEASES_INCLUDE_SINGLES: &str = N_!("Include Singles");
pub const NEW_RELEASES_INCLUDE_SINGLES_DESCRIPTION: &str =
    N_!("Also report singles that have already been released");
```

In `crates/reprise-gnome/src/ui/preferences/preference_new_releases.rs` nach `scope_row` ergänzen:

```rust
pub(in crate::ui) fn singles_row(conn: &Rc<RefCell<Connection>>, enabled: bool) -> adw::SwitchRow {
    let active = reprise_core::artist_news::include_singles(&conn.borrow()).unwrap_or(false);
    let row = adw::SwitchRow::builder()
        .title(strings::text(strings::NEW_RELEASES_INCLUDE_SINGLES))
        .subtitle(strings::text(strings::NEW_RELEASES_INCLUDE_SINGLES_DESCRIPTION))
        .active(active)
        .sensitive(enabled)
        .build();
    let conn = conn.clone();
    row.connect_active_notify(move |row| {
        if let Err(error) =
            reprise_core::artist_news::set_include_singles(&conn.borrow(), row.is_active())
        {
            tracing::warn!(%error, "could not save New Releases singles setting");
        }
    });
    row
}
```

Am Aufrufort von `scope_row` in `crates/reprise-gnome/src/ui/preferences/` (per `rg "scope_row" crates/reprise-gnome/src` finden) `singles_row` direkt darunter in dieselbe Gruppe hängen, mit demselben `enabled`-Wert.

- [ ] **Step 7: Build and test everything**

Run: `cargo test && cargo build -p reprise-gnome`
Expected: PASS. Bestehende Aufrufe von `parse_release_groups` mit drei Argumenten bekommen `false` als viertes, außer der Test prüft gezielt Singles.

- [ ] **Step 8: Commit**

```bash
git add crates/reprise-core/src/artist_news.rs \
        crates/reprise-core/src/artist_news_tests.rs \
        crates/reprise-gnome/src/ui/preferences/preference_new_releases.rs \
        crates/reprise-gnome/src/ui/strings_news.rs
git commit -m "feat: Schalter für erschienene Singles in den New Releases"
```

---

### Task 8: Abschluss — Lint, Übersetzungen, manuelle Prüfung

**Files:**
- Modify: `po/` (nur falls das Projekt eine Template-Regeneration vorsieht)

**Interfaces:**
- Consumes: alle vorherigen Tasks.
- Produces: nichts Neues.

- [ ] **Step 1: Clippy über beide Crates**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: keine Warnungen. Typische Rückstände aus diesem Plan: der entfernte `Datelike`-Import in `artist_news.rs`, ein ungenutztes `today`-Argument in `preference_new_releases.rs`, `DAILY_REST_COUNT`-Reste.

- [ ] **Step 2: Formatierung**

Run: `cargo fmt --all -- --check`
Expected: keine Ausgabe. Bei Abweichungen `cargo fmt --all` laufen lassen.

- [ ] **Step 3: Volle Testsuite**

Run: `cargo test`
Expected: PASS über `reprise-core` und `reprise-gnome`.

- [ ] **Step 4: Übersetzungs-Template prüfen**

Run: `rg "NEW_RELEASES_PARTIALLY_OWNED|NEW_RELEASES_INCLUDE_SINGLES" po/ || echo "kein po-Eintrag nötig"`
Erwartung: Wenn das Projekt `po/*.pot` eingecheckt hat, nach dem projektüblichen Verfahren regenerieren (siehe `RELEASING.md`). Andernfalls ohne Änderung weiter.

- [ ] **Step 5: Manuelle Prüfung an einer Kopie der echten Bibliothek**

Nicht gegen die Live-Datenbank arbeiten. Kopie ziehen, migrieren, und den Zielfall verifizieren:

```bash
cp ~/.local/share/reprise/reprise.db /tmp/reprise-verify.db
sqlite3 /tmp/reprise-verify.db "PRAGMA user_version;"
```

Erwartung nach dem ersten App-Start gegen die Kopie: `user_version` ist 30, `artist_news_fetch` ist aus `new_releases` vorbefüllt, und Artists ohne Ledger-Eintrag stehen als Erste in der Rotation.

Gegenprobe für den Kern des Features:

```bash
sqlite3 /tmp/reprise-verify.db "
SELECT trim(artist), trim(album), COUNT(*) AS tracks
FROM tracks
WHERE removed_at IS NULL AND missing_since IS NULL AND trim(album) <> ''
GROUP BY lower(trim(artist)), lower(trim(album))
HAVING COUNT(*) = 1
LIMIT 10;"
```

Diese Alben dürfen künftig **nicht** mehr als Besitz gelten und deren Artists nicht mehr aus der Rotation fallen.

- [ ] **Step 6: Commit any lint or formatting fixes**

```bash
git add -A
git commit -m "chore: Lint- und Formatierungsnachlauf für die New-Releases-Abdeckung"
```

---

## Spec-Abdeckung

| Spec-Abschnitt | Task |
|---|---|
| A — Fetch-Ledger (Tabelle) | 1 |
| A — Ledger-API | 2 |
| A — Frischeprüfung, Reihenfolge, `latest_fetched_at` | 3 |
| B — Rotation, `FetchScope`, `REST_ARTISTS_PER_RUN` | 4 |
| D — Upcoming-Ausnahme und Besitz-Schwelle | 5 |
| D2 — `LibraryPresence` im Core | 6, Teil A |
| D2 — Chip und Primäraktion in der UI | 6, Teil B |
| E — Singles-Schalter | 7 |
| Migration inkl. Vorbefüllung | 1 |
| Fehlerbehandlung (`failed` im Ledger, TTL, kein Backoff) | 3 |
| Tests | in jedem Task |
