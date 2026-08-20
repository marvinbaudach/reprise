---
slug: always-download-episodes
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-20
strands: core,ui
merge_order: core,ui
---
# Episoden immer herunterladen statt streamen — Implementierungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eine YouTube-Episode wird nie mehr gestreamt, sondern vor der Wiedergabe heruntergeladen, und jedes Abo hält seine neuesten `keep_downloaded` (Standard 10) Folgen auf der Platte.

**Architecture:** Die vorhandene Zahl `keep_downloaded` wird beidseitig: ein neuer Auffüller in `podcasts::pipeline` lädt, was unter den neuesten N fehlt, während `cleanup_candidates` weiter löscht, was darüber hinausgeht — beide über *eine* gemeinsame Sortiervorschrift, aber über verschiedene Grundmengen. Der Auffüller läuft als eigene Worker-Operation nach dem Refresh, nicht in ihm. Der Wiedergabepfad ersetzt `resolve_youtube` (Stream-Proxy) durch einen Download auf einem `one_shot_task`, an dessen Abschluss die lokale Datei abgespielt wird.

**Tech Stack:** Rust, rusqlite/SQLite, gtk4-rs/libadwaita, glib-Mainloop, `async_channel`.

Bindende Quelle ist
`docs/superpowers/specs/2026-08-20-always-download-episodes-design.md`. Dieser
Plan wiederholt sie nicht, er ergänzt sie um das *Wie*, die Reihenfolge und die
Abnahme. Wo Spec und Plan sich widersprechen, gilt die Spec.

Gelesen gegen `origin/dev` @ `afb839069e`. Jede Zeilenangabe stammt aus diesem
Stand; wer sie nicht wiederfindet, hat eine andere Basis.

## Global Constraints

- `DEFAULT_KEEP_DOWNLOADED` ist nach diesem Plan `10` (vorher `5`).
- `0` bedeutet bei jeder numerischen Mengeneinstellung **unbegrenzt** (`E-9`) —
  beim Auffüller heißt das: alle Episoden des Abos, nicht „keine".
- Die Sortiervorschrift für „neueste Episode zuerst" existiert genau einmal als
  Konstante und wird von Auffüller und Aufräumer benutzt.
- Kein zweiter Download-Executor: jeder Downloadweg geht durch
  `podcasts::pipeline::download_episode_in`.
- Dateien bleiben unter 800 Zeilen (`check-architecture.sh` erzwingt das).
- Chat/Antworten deutsch, alles im Repo (Code, Kommentare, Commit-Botschaften,
  Testnamen) englisch.
- Commit-Format: `<type>: <description>`, Typen `feat|fix|refactor|docs|test|chore|perf|ci`.

## Regeln für den Umsetzer — zuerst lesen

- **Die `Files:`-Liste je Aufgabe ist ein Startpunkt, kein Zaun.** Wenn der
  Vertrag einer Aufgabe eine hier nicht genannte Datei braucht, fass sie an und
  notier es. Halte nur an, wenn der *Vertrag selbst* falsch ist.
- **Jede Aufgabe endet grün und committet.** `cargo test -p <crate>` für den
  berührten Crate, `cargo clippy --all-targets -- -D warnings`.
- **TDD ist bindend:** erst der Test, dann der Lauf, der ihn scheitern sieht,
  dann die Implementierung. Ein Test, der ohne die Implementierung besteht,
  misst nichts und ist zu verwerfen.

---

## Strang `core` — reines Rust, `reprise-core`

### Task 1: Die Zielgröße wird 10

**Files:**
- Modify: `crates/reprise-core/src/podcasts/config.rs:48-53`
- Test: `crates/reprise-core/src/podcasts/config.rs` (dortiges `mod tests`)

**Interfaces:**
- Consumes: nichts.
- Produces: `pub const DEFAULT_KEEP_DOWNLOADED: usize = 10;`

- [ ] **Step 1: Write the failing test**

In `crates/reprise-core/src/podcasts/config.rs`, im vorhandenen `mod tests`:

```rust
#[test]
fn the_keep_downloaded_default_is_ten() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let config = load(&db).unwrap();
    assert_eq!(config.keep_downloaded_default, 10);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p reprise-core the_keep_downloaded_default_is_ten`
Expected: FAIL, `assertion `left == right` failed: left: 5, right: 10`

- [ ] **Step 3: Write minimal implementation**

In `config.rs` den Konstantenwert und seinen Doc-Kommentar ersetzen:

```rust
/// `POD-5` / `O-5`: decided 2026-07-29 — `CleanupPolicy::KeepLast5` kept a
/// hardcoded 5 per show; that hardcoded 5 became this global default, and
/// "keep N" is its generalization. Raised to 10 on 2026-08-20 when the same
/// number became the *fill* target as well: playback no longer streams, so
/// "keep N" and "have N" are one setting. A larger default only ever means
/// more on disk, never a deletion.
pub const DEFAULT_KEEP_DOWNLOADED: usize = 10;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p reprise-core the_keep_downloaded_default_is_ten`
Expected: PASS

- [ ] **Step 5: Check for tests that hardcoded the old default**

Run: `cargo test -p reprise-core podcasts::`
Expected: PASS. Falls ein Test auf `5` festgenagelt war, ist sein Erwartungswert
`DEFAULT_KEEP_DOWNLOADED` — nie erneut eine Literalzahl.

- [ ] **Step 6: Commit**

```bash
git add crates/reprise-core/src/podcasts/config.rs
git commit -m "feat: raise the keep-downloaded default to ten"
```

---

### Task 2: Eine Sortiervorschrift für „neueste zuerst"

**Files:**
- Modify: `crates/reprise-core/src/podcasts/downloads.rs:384-416`
- Test: `crates/reprise-core/src/podcasts/downloads_tests.rs`

**Interfaces:**
- Consumes: nichts.
- Produces: `pub(super) const NEWEST_EPISODE_FIRST: &str` — die `ORDER BY`-Klausel
  ohne das Schlüsselwort `ORDER BY`, mit Tabellenalias `e`.

Diese Aufgabe ändert **kein Verhalten**. Sie zieht die Sortierung aus dem
`KeepLast5`-Zweig heraus, damit Task 4 dieselbe benutzen kann statt eine zweite
zu schreiben, die heute zufällig gleich lautet.

- [ ] **Step 1: Write the failing test**

In `crates/reprise-core/src/podcasts/downloads_tests.rs`:

```rust
#[test]
fn the_newest_first_ordering_puts_undated_episodes_last() {
    // The ordering is shared by the cleanup and the fill-up. This pins its
    // three tie-breakers so a change to either consumer cannot quietly
    // redefine "newest" for the other.
    assert_eq!(
        super::NEWEST_EPISODE_FIRST,
        "e.published_at IS NULL, e.published_at DESC, \
         e.first_seen_at DESC, e.id DESC"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p reprise-core the_newest_first_ordering_puts_undated_episodes_last`
Expected: FAIL, `cannot find value `NEWEST_EPISODE_FIRST` in module `super``

- [ ] **Step 3: Write minimal implementation**

In `downloads.rs`, oberhalb von `cleanup_candidates`:

```rust
/// The one definition of "newest episode first", shared by the cleanup
/// (`cleanup_candidates`) and the fill-up (`pipeline::fill_downloads_in`).
///
/// The two rank different populations on purpose — the cleanup ranks only
/// downloaded episodes (see the P1 note in `cleanup_candidates`), the fill-up
/// ranks all live ones — so they cannot share a query. What they must share is
/// what "newest" means, or one starts fetching what the other just deleted.
///
/// Uses the table alias `e`; every consumer must alias `podcast_episodes` that
/// way.
pub(super) const NEWEST_EPISODE_FIRST: &str = "e.published_at IS NULL, e.published_at DESC, \
     e.first_seen_at DESC, e.id DESC";
```

Und im `KeepLast5`-Zweig die literale Sortierung durch die Konstante ersetzen —
das SQL wird dazu formatiert statt als Literal geschrieben:

```rust
        CleanupPolicy::KeepLast5 => {
            let sql = format!(
                "SELECT id, downloaded_path, keep_downloaded, episode_rank FROM (
                   SELECT e.id, e.downloaded_path, s.keep_downloaded,
                          ROW_NUMBER() OVER (
                            PARTITION BY e.subscription_id
                            ORDER BY {NEWEST_EPISODE_FIRST}
                          ) AS episode_rank
                   FROM podcast_episodes e
                   JOIN podcast_subscriptions s ON s.id = e.subscription_id
                   WHERE s.removed_at IS NULL
                     AND e.downloaded_path IS NOT NULL
                 )
                 ORDER BY id"
            );
            let mut statement = conn.prepare(&sql)?;
```

Der Rest des Zweigs bleibt unverändert.

- [ ] **Step 4: Run tests to verify nothing moved**

Run: `cargo test -p reprise-core podcasts::downloads`
Expected: PASS — inklusive aller bestehenden `KeepLast5`-Tests. Wenn hier
etwas rot wird, ist die Sortierung beim Herausziehen verändert worden.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-core/src/podcasts/downloads.rs \
        crates/reprise-core/src/podcasts/downloads_tests.rs
git commit -m "refactor: name the shared newest-episode-first ordering"
```

---

### Task 3: Ein Download je Episode, egal wer fragt

**Files:**
- Create: `crates/reprise-core/src/podcasts/download_claims.rs`
- Modify: `crates/reprise-core/src/podcasts.rs` (Modul anmelden)
- Modify: `crates/reprise-core/src/podcasts/pipeline_download.rs:48-70`
- Test: `crates/reprise-core/src/podcasts/download_claims.rs` (eigenes `mod tests`)

**Interfaces:**
- Consumes: nichts.
- Produces:
  - `pub(crate) struct DownloadClaim` — hält den Anspruch, gibt ihn beim `Drop` frei.
  - `pub(crate) fn claim(episode_id: i64) -> Option<DownloadClaim>` — `None`, wenn
    für diese Episode schon ein Download läuft.

Ohne das schreiben zwei gleichzeitige Läufe derselben Episode dieselbe
`.part`-Datei. Ab Task 8 gibt es drei Aufrufer (Knopf, Auffüller, Wiedergabe),
plus MCP als vierten.

- [ ] **Step 1: Write the failing test**

Neue Datei `crates/reprise-core/src/podcasts/download_claims.rs`, Testteil:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_claim_for_the_same_episode_is_refused() {
        let first = claim(4242).expect("first claim");
        assert!(claim(4242).is_none());
        drop(first);
        assert!(claim(4242).is_some(), "the claim is released on drop");
    }

    #[test]
    fn claims_for_different_episodes_coexist() {
        let _one = claim(4243).expect("first claim");
        let _two = claim(4244).expect("second claim");
    }

    #[test]
    fn a_panicking_holder_still_releases_the_claim() {
        // The claim must not survive a panicking download: `Drop` runs while
        // unwinding, a manual `release()` call would not.
        let _ = std::panic::catch_unwind(|| {
            let _held = claim(4245).expect("claim");
            panic!("download exploded");
        });
        assert!(claim(4245).is_some());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p reprise-core download_claims`
Expected: FAIL — das Modul ist noch nicht angemeldet, `cargo` findet die Tests
nicht bzw. der Compiler bricht mit `cannot find function `claim`` ab.

- [ ] **Step 3: Write minimal implementation**

Kopf derselben Datei:

```rust
//! One in-flight download per episode, whatever asks for it.
//!
//! Three callers can start the same episode's download — the download button,
//! the refresh fill-up, and playback — and MCP's `music_manage_episodes` is a
//! fourth. Two concurrent runs write the same `downloads::partial_path`, so
//! they corrupt each other's `.part` file.
//!
//! The guard lives here, next to nothing, rather than in any one caller: a
//! caller-side guard (the podcasts view's `download_states` map) can only see
//! its own dispatches, and a second such map would be the same mistake twice.

use std::collections::BTreeSet;
use std::sync::{Mutex, OnceLock};

fn in_flight() -> &'static Mutex<BTreeSet<i64>> {
    static IN_FLIGHT: OnceLock<Mutex<BTreeSet<i64>>> = OnceLock::new();
    IN_FLIGHT.get_or_init(|| Mutex::new(BTreeSet::new()))
}

/// A held claim on one episode's download. Releasing happens on `Drop`, so an
/// early return or a panic inside the download cannot leak it.
#[derive(Debug)]
pub(crate) struct DownloadClaim {
    episode_id: i64,
}

impl Drop for DownloadClaim {
    fn drop(&mut self) {
        // A poisoned lock means some other holder panicked *while mutating the
        // set*. Recovering is correct here: the set is a plain id collection
        // with no invariant a panic could have broken half-way, and refusing to
        // release would strand this episode for the process's lifetime.
        let mut guard = in_flight().lock().unwrap_or_else(|error| error.into_inner());
        guard.remove(&self.episode_id);
    }
}

/// Claims `episode_id` for a download, or returns `None` if one is already in
/// flight.
pub(crate) fn claim(episode_id: i64) -> Option<DownloadClaim> {
    let mut guard = in_flight().lock().unwrap_or_else(|error| error.into_inner());
    if guard.insert(episode_id) {
        Some(DownloadClaim { episode_id })
    } else {
        None
    }
}
```

In `crates/reprise-core/src/podcasts.rs` zu den übrigen `mod`-Zeilen:

```rust
mod download_claims;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p reprise-core download_claims`
Expected: PASS, 3 Tests.

- [ ] **Step 5: Wire the claim into the one executor**

In `pipeline_download.rs`, in `download_episode_in`, direkt hinter der
Episodenabfrage und **vor** dem ersten Netzzugriff:

```rust
pub(super) fn download_episode_in(
    conn: &Connection,
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    download_root: &Path,
    episode_id: i64,
    on_progress: &mut dyn FnMut(DownloadState),
) -> Result<DownloadState, PipelineError> {
    let episode = store::episode_in(conn, episode_id)?.ok_or(PipelineError::EpisodeNotFound)?;
    if episode.downloaded_path.is_some() {
        let bytes = episode.downloaded_bytes.unwrap_or(0).max(0) as u64;
        let state = DownloadState::Downloaded { bytes };
        on_progress(state.clone());
        return Ok(state);
    }
    // Held for the rest of this call. A concurrent caller gets `AlreadyRunning`
    // rather than a second run over the same `.part` file.
    let Some(_claim) = super::download_claims::claim(episode_id) else {
        return Err(PipelineError::DownloadAlreadyRunning);
    };
```

Neue Variante in `PipelineError` (in derselben Datei, in der das Enum steht —
`grep -n "enum PipelineError" crates/reprise-core/src`):

```rust
    #[error("a download for this episode is already running")]
    DownloadAlreadyRunning,
```

- [ ] **Step 6: Write the failing integration test**

In `crates/reprise-core/src/podcasts/downloads_tests.rs` — dort stehen `conn()`,
`add_show()` und `add_undownloaded_episode()` schon, also braucht es keine neue
Datei:

```rust
#[test]
fn a_concurrent_download_of_the_same_episode_is_refused() {
    let db = conn();
    let root = tempfile::tempdir().unwrap();
    let subscription_id = add_show(db.conn());
    let episode_id = add_undownloaded_episode(db.conn(), subscription_id, 1);
    // Stand in for another caller that already holds this episode.
    let held = crate::podcasts::download_claims::claim(episode_id).expect("claim");
    let error = crate::podcasts::pipeline::download_episode(
        &db,
        &crate::podcasts::pipeline_refresh_tests::FakeFeed,
        &crate::podcasts::pipeline_refresh_tests::FakeYoutube,
        root.path(),
        episode_id,
        &mut |_| {},
    )
    .expect_err("a claimed episode must not download twice");
    assert!(matches!(
        error,
        crate::podcasts::pipeline::PipelineError::DownloadAlreadyRunning
    ));
    drop(held);
}
```

Das setzt voraus, dass `FakeFeed`/`FakeYoutube` `pub(super)` sind — dieselbe
Sichtbarkeitsanhebung, die Task 4 ohnehin braucht. Zieh sie hier vor.

- [ ] **Step 7: Run the tests**

Run: `cargo test -p reprise-core podcasts::pipeline_download`
Expected: PASS

- [ ] **Step 8: Prove the guard is load-bearing**

Kommentiere die `let Some(_claim) = ... else` -Zeilen aus, lauf den Test erneut,
sieh ihn scheitern, mach die Änderung rückgängig. Ein Test, der ohne den Guard
besteht, misst nichts.

Run: `cargo test -p reprise-core a_concurrent_download_of_the_same_episode_is_refused`
Expected (mit auskommentiertem Guard): FAIL

- [ ] **Step 9: Commit**

```bash
git add crates/reprise-core/src/podcasts/download_claims.rs \
        crates/reprise-core/src/podcasts.rs \
        crates/reprise-core/src/podcasts/pipeline_download.rs \
        crates/reprise-core/src/podcasts/pipeline_download_tests.rs
git commit -m "feat: allow one in-flight download per episode"
```

---

### Task 4: Der Auffüller

**Files:**
- Create: `crates/reprise-core/src/podcasts/fill_downloads.rs`
- Modify: `crates/reprise-core/src/podcasts.rs` (Modul anmelden und re-exportieren)
- Modify: `crates/reprise-core/src/podcasts/pipeline.rs:547-581` (Auto-Download-Zweig entfernen)
- Test: `crates/reprise-core/src/podcasts/fill_downloads_tests.rs`

**Interfaces:**
- Consumes: `downloads::NEWEST_EPISODE_FIRST` (Task 2),
  `downloads::resolve_keep_downloaded(default_keep: usize, channel_override: Option<i64>) -> usize`,
  `pipeline::download_episode_in` (Task 3).
- Produces:
  ```rust
  pub struct FillSummary { pub downloaded: usize, pub failed: usize }
  pub fn fill_downloads(
      db: &Db,
      feed_fetcher: &dyn FeedFetcher,
      youtube_fetcher: &dyn YoutubeFetcher,
      download_root: &Path,
      on_progress: &mut dyn FnMut(i64, DownloadState),
  ) -> Result<FillSummary, PipelineError>
  ```
  und, modul-intern sichtbar für die Tests,
  ```rust
  pub(crate) fn missing_episode_ids_in(
      conn: &Connection,
      default_keep_downloaded: usize,
  ) -> Result<Vec<i64>, rusqlite::Error>
  ```

- [ ] **Step 1: Write the failing test for the selection**

Neue Datei `crates/reprise-core/src/podcasts/fill_downloads_tests.rs`:

Die Bausteine gibt es schon in `downloads_tests.rs` — `conn()` (Zeile 9),
`add_show()` (15), `add_download()` (35) und `add_undownloaded_episode()` (327).
Sie sind dort dateiprivat; heb genau diese vier auf `pub(super)` und benutz sie
hier, statt ein zweites Gerüst zu bauen.

```rust
use super::download_state::DownloadState;
use super::downloads_tests::{add_download, add_show, add_undownloaded_episode, conn};
use super::fill_downloads::{fill_downloads, missing_episode_ids_in, FillSummary};

/// One show with `count` episodes, newest first: episode number 1 is the
/// newest, `count` the oldest. Returns their ids in that order.
fn show_with_episodes(connection: &rusqlite::Connection, count: i64) -> (i64, Vec<i64>) {
    let subscription_id = add_show(connection);
    let ids = (1..=count)
        .map(|number| add_undownloaded_episode(connection, subscription_id, count - number + 1))
        .collect();
    (subscription_id, ids)
}

fn mark_played(connection: &rusqlite::Connection, episode_id: i64) {
    connection
        .execute(
            "UPDATE podcast_episodes SET played_at = 1 WHERE id = ?1",
            [episode_id],
        )
        .unwrap();
}

#[test]
fn the_fill_up_takes_the_newest_missing_episodes() {
    let db = conn();
    let (_, ids) = show_with_episodes(db.conn(), 20);
    let missing = missing_episode_ids_in(db.conn(), 10).unwrap();
    assert_eq!(missing.len(), 10, "exactly the newest ten are missing");
    assert!(missing.contains(&ids[0]), "the newest is among them");
    assert!(!missing.contains(&ids[10]), "the eleventh is not");
}

#[test]
fn the_fill_up_ignores_episodes_that_are_already_downloaded() {
    let db = conn();
    let subscription_id = add_show(db.conn());
    let downloaded = add_download(db.conn(), subscription_id, "newest", 1);
    for number in 2..=20 {
        add_undownloaded_episode(db.conn(), subscription_id, 21 - number);
    }
    let missing = missing_episode_ids_in(db.conn(), 10).unwrap();
    assert_eq!(missing.len(), 9);
    assert!(!missing.contains(&downloaded));
}

#[test]
fn the_fill_up_skips_played_episodes_instead_of_sliding_past_them() {
    // Sliding would pull the eleventh episode into the download set, which the
    // cleanup ranks outside the newest ten — the two would then fight forever.
    let db = conn();
    let (_, ids) = show_with_episodes(db.conn(), 20);
    mark_played(db.conn(), ids[0]);
    mark_played(db.conn(), ids[1]);
    let missing = missing_episode_ids_in(db.conn(), 10).unwrap();
    assert_eq!(missing.len(), 8);
    assert!(!missing.contains(&ids[10]), "the window does not slide");
}

#[test]
fn a_keep_of_zero_means_unlimited() {
    let db = conn();
    show_with_episodes(db.conn(), 20);
    let missing = missing_episode_ids_in(db.conn(), 0).unwrap();
    assert_eq!(missing.len(), 20);
}
```

`add_download`s und `add_undownloaded_episode`s echte Signaturen prüfen und die
Aufrufe daran anpassen — die Argumentreihenfolge oben ist aus den Zeilennummern
abgeleitet, nicht abgeschrieben.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p reprise-core fill_downloads`
Expected: FAIL, `unresolved import `super::fill_downloads``

- [ ] **Step 3: Write the selection**

Neue Datei `crates/reprise-core/src/podcasts/fill_downloads.rs`:

```rust
//! Keeping the newest N episodes of every subscription on disk.
//!
//! The mirror image of `downloads::cleanup_candidates`: that one deletes what
//! ranks beyond N, this one fetches what is missing within N. Both read the
//! same `keep_downloaded` and the same `downloads::NEWEST_EPISODE_FIRST`
//! ordering, over deliberately different populations — see that constant's
//! comment.

use std::path::Path;

use rusqlite::Connection;

use super::download_state::DownloadState;
use super::downloads::{self, NEWEST_EPISODE_FIRST};
use super::pipeline::{FeedFetcher, PipelineError, YoutubeFetcher};
use crate::db::Db;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FillSummary {
    pub downloaded: usize,
    pub failed: usize,
}

/// The episodes that ought to be on disk and are not.
///
/// Ranks over *all* live episodes, not only downloaded ones — the opposite of
/// the cleanup, and for the opposite reason: the job here is to find what is
/// missing, so a missing episode must occupy its rank position.
///
/// Played episodes are excluded rather than replaced. Replacing them would pull
/// the (N+1)th episode into the download set while the cleanup still ranks it
/// outside — with `CleanupPolicy::DeletePlayedAfter7Days` the two would then
/// delete and re-fetch the same episode forever.
pub(crate) fn missing_episode_ids_in(
    conn: &Connection,
    default_keep_downloaded: usize,
) -> Result<Vec<i64>, rusqlite::Error> {
    let sql = format!(
        "SELECT id, keep_downloaded, episode_rank, downloaded_path, played_at FROM (
           SELECT e.id, s.keep_downloaded, e.downloaded_path, e.played_at,
                  ROW_NUMBER() OVER (
                    PARTITION BY e.subscription_id
                    ORDER BY {NEWEST_EPISODE_FIRST}
                  ) AS episode_rank
           FROM podcast_episodes e
           JOIN podcast_subscriptions s ON s.id = e.subscription_id
           WHERE s.removed_at IS NULL
             AND e.removed_at IS NULL
         )
         ORDER BY episode_rank, id"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<i64>>(4)?,
        ))
    })?;
    let mut missing = Vec::new();
    for row in rows {
        let (episode_id, keep_override, episode_rank, downloaded_path, played_at) = row?;
        let keep = downloads::resolve_keep_downloaded(default_keep_downloaded, keep_override);
        // `0` is unlimited (`E-9`), never "keep none".
        if keep != 0 && episode_rank > keep as i64 {
            continue;
        }
        if downloaded_path.is_some() || played_at.is_some() {
            continue;
        }
        missing.push(episode_id);
    }
    Ok(missing)
}
```

- [ ] **Step 4: Run the selection tests**

Run: `cargo test -p reprise-core fill_downloads`
Expected: PASS, 4 Tests.

- [ ] **Step 5: Commit the selection**

```bash
git add crates/reprise-core/src/podcasts/fill_downloads.rs \
        crates/reprise-core/src/podcasts/fill_downloads_tests.rs \
        crates/reprise-core/src/podcasts.rs
git commit -m "feat: select the newest missing episodes per subscription"
```

- [ ] **Step 6: Write the failing test for the executor**

In `fill_downloads_tests.rs` ergänzen:

Die Attrappen dafür existieren als `FakeFeed` (`pipeline_refresh_tests.rs:12`)
und `FakeYoutube` (Zeile 26) — genau die, mit denen der jetzt entfallende
Auto-Download-Zweig getestet wurde. Heb beide auf `pub(super)` und benutz sie
hier; sie wandern damit von einem Test, der gleich gestrichen wird, zu dem, der
seinen Vertrag erbt.

```rust
use super::pipeline_refresh_tests::{FakeFeed, FakeYoutube};

#[test]
fn the_fill_up_downloads_every_missing_episode_and_reports_each() {
    let db = conn();
    let root = tempfile::tempdir().unwrap();
    show_with_episodes(db.conn(), 12);
    let mut seen: Vec<(i64, DownloadState)> = Vec::new();
    let summary = fill_downloads(
        &db,
        &FakeFeed,
        &FakeYoutube,
        root.path(),
        &mut |episode_id, state| seen.push((episode_id, state)),
    )
    .unwrap();
    assert_eq!(summary.downloaded, 10);
    assert_eq!(summary.failed, 0);
    assert!(seen
        .iter()
        .any(|(_, state)| matches!(state, DownloadState::Downloaded { .. })));
}

#[test]
fn a_second_fill_up_run_downloads_nothing() {
    // Convergence: this is the property that rules out a fill/delete loop.
    let db = conn();
    let root = tempfile::tempdir().unwrap();
    show_with_episodes(db.conn(), 12);
    let mut ignore = |_: i64, _: DownloadState| {};
    let first = fill_downloads(&db, &FakeFeed, &FakeYoutube, root.path(), &mut ignore).unwrap();
    let second = fill_downloads(&db, &FakeFeed, &FakeYoutube, root.path(), &mut ignore).unwrap();
    assert_eq!(first.downloaded, 10);
    assert_eq!(second, FillSummary::default());
}

#[test]
fn the_fill_up_and_the_cleanup_agree_on_the_newest_ten() {
    // The load-bearing test: both halves run, and the cleanup must find
    // nothing among what the fill-up just fetched.
    let db = conn();
    let root = tempfile::tempdir().unwrap();
    show_with_episodes(db.conn(), 12);
    let mut ignore = |_: i64, _: DownloadState| {};
    fill_downloads(&db, &FakeFeed, &FakeYoutube, root.path(), &mut ignore).unwrap();
    let summary = super::downloads::enforce_cleanup(
        &db,
        root.path(),
        super::config::CleanupPolicy::KeepLast5,
        10,
        0,
    )
    .unwrap();
    assert_eq!(
        summary,
        super::downloads::CleanupSummary::default(),
        "the cleanup must not delete what the fill-up just fetched"
    );
}
```

`CleanupSummary`s Felder prüfen — wenn es kein `PartialEq`/`Default` ableitet,
statt des Ganzvergleichs sein Löschzählerfeld auf `0` prüfen, und `PartialEq`
nicht eigens dafür hinzufügen.

- [ ] **Step 7: Run it to verify it fails**

Run: `cargo test -p reprise-core the_fill_up_downloads_every_missing_episode_and_reports_each`
Expected: FAIL, `cannot find function `fill_downloads``

- [ ] **Step 8: Write the executor**

Ans Ende von `fill_downloads.rs`:

```rust
/// Downloads everything `missing_episode_ids_in` reports, one episode at a
/// time, reporting each state change through `on_progress`.
///
/// Runs to completion rather than under a per-run cap: a cap would leave the
/// target unreached until the next refresh hours later, and the caller runs
/// this off the refresh precisely so a long run costs nobody anything.
pub fn fill_downloads(
    db: &Db,
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    download_root: &Path,
    on_progress: &mut dyn FnMut(i64, DownloadState),
) -> Result<FillSummary, PipelineError> {
    let config = super::config::load(db)?;
    let episode_ids = {
        let conn = db.conn();
        missing_episode_ids_in(conn, config.keep_downloaded_default)?
    };
    let mut summary = FillSummary::default();
    for episode_id in episode_ids {
        let mut report = |state: DownloadState| on_progress(episode_id, state);
        let outcome = super::pipeline::download_episode(
            db,
            feed_fetcher,
            youtube_fetcher,
            download_root,
            episode_id,
            &mut report,
        );
        match outcome {
            Ok(DownloadState::Downloaded { .. }) => summary.downloaded += 1,
            Ok(DownloadState::Failed { .. }) => summary.failed += 1,
            // Another caller — the download button, or playback — already has
            // this episode in flight. Not this run's job and not a failure.
            Err(PipelineError::DownloadAlreadyRunning) => {}
            Err(error) => return Err(error),
            Ok(_) => {}
        }
    }
    Ok(summary)
}
```

- [ ] **Step 9: Run the executor tests**

Run: `cargo test -p reprise-core fill_downloads`
Expected: PASS, 6 Tests.

- [ ] **Step 10: Remove the old auto-download branch**

In `pipeline.rs` den gesamten Block `if subscription.auto_download { ... }`
(Zeilen 547–581) ersatzlos löschen, samt der jetzt unbenutzten Konstante
`MAX_AUTO_DOWNLOADS_PER_SUBSCRIPTION` (Zeile 24). Der `enforce_cleanup_in`-Aufruf
darunter bleibt.

- [ ] **Step 11: Run the refresh tests**

Run: `cargo test -p reprise-core podcasts::`
Expected: Die Tests in `pipeline_refresh_tests.rs`, die das Auto-Downloaden des
Refreshs prüfen, schlagen fehl — das ist richtig, das Verhalten ist weg. Streich
sie, statt sie zu retten; ihr Vertrag ist auf `fill_downloads_tests.rs`
übergegangen. Alle übrigen müssen grün bleiben.

- [ ] **Step 12: Commit**

```bash
git add crates/reprise-core/src/podcasts/fill_downloads.rs \
        crates/reprise-core/src/podcasts/fill_downloads_tests.rs \
        crates/reprise-core/src/podcasts/pipeline.rs \
        crates/reprise-core/src/podcasts/pipeline_refresh_tests.rs
git commit -m "feat: fill each subscription up to its keep-downloaded target"
```

---

### Task 5: Der Fehler sagt, was zu tun ist

**Files:**
- Modify: `crates/reprise-core/src/podcasts.rs:190-216`
- Test: `crates/reprise-core/src/podcasts.rs` (dortiges `mod tests`)

**Interfaces:**
- Consumes: `ytdlp::YtDlpFailureKind::user_message()` (existiert, `ytdlp_failure.rs:72`).
- Produces: unverändertes `PodcastError::classify() -> &'static str`, aber mit
  fallabhängigem Ergebnis für `YtDlpFailure`.

Ab Task 8 ist ein fehlgeschlagener Download der Grund, warum gar nichts
abspielt. Der heutige Einheitssatz „YouTube source could not be read with
yt-dlp" verschweigt dann die einzige Information, die weiterhilft.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn a_ytdlp_failure_classifies_to_its_own_kind_message() {
    let outdated = PodcastError::YtDlpFailure {
        kind: ytdlp::YtDlpFailureKind::ExtractorOutdated,
        message: "irrelevant".into(),
    };
    assert_eq!(
        outdated.classify(),
        "YouTube changed its response — update yt-dlp and try again"
    );

    let refused = PodcastError::YtDlpFailure {
        kind: ytdlp::YtDlpFailureKind::AccessRefused,
        message: "irrelevant".into(),
    };
    assert_ne!(
        refused.classify(),
        outdated.classify(),
        "two kinds must not collapse onto one sentence"
    );
}
```

Die Felder von `PodcastError::YtDlpFailure` prüfen und den Ausdruck daran
anpassen: `grep -n "YtDlpFailure {" crates/reprise-core/src/podcasts.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p reprise-core a_ytdlp_failure_classifies_to_its_own_kind_message`
Expected: FAIL, linke Seite ist „YouTube source could not be read with yt-dlp"

- [ ] **Step 3: Write minimal implementation**

In `classify()` den sammelnden Arm

```rust
            (
                SourceErrorKind::Unreachable
                | SourceErrorKind::RateLimited { .. }
                | SourceErrorKind::HelperOutdated,
                PodcastError::YtDlpFailure { .. },
            ) => "YouTube source could not be read with yt-dlp",
```

ersetzen durch

```rust
            // Every kind carries its own repair instruction in
            // `ytdlp_failure.rs`; folding them onto one sentence threw away the
            // only actionable part. It matters more since playback fails when a
            // download does — the message is now the reason nothing plays.
            (
                SourceErrorKind::Unreachable
                | SourceErrorKind::RateLimited { .. }
                | SourceErrorKind::HelperOutdated,
                PodcastError::YtDlpFailure { kind, .. },
            ) => kind.user_message(),
```

Der `VerificationRequired`-Arm darüber bleibt, weil er bewusst eine andere,
längere Wiederherstellungs-Anleitung zeigt. Der Auffang-Arm ganz unten
(`(SourceErrorKind::HelperOutdated, _)`) bleibt ebenfalls; er ist nur noch für
Nicht-`YtDlpFailure`-Fehler erreichbar.

`YtDlpFailureKind::user_message` ist heute `pub(crate)` — ob das für die
Aufrufstelle reicht, zeigt der Compiler; falls nicht, auf `pub` heben.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p reprise-core a_ytdlp_failure_classifies_to_its_own_kind_message`
Expected: PASS

- [ ] **Step 5: Fix the tests that asserted the single sentence**

Run: `cargo test -p reprise-core`
Erwartet rot: `podcasts/pipeline_youtube_handle_tests.rs:193` und
`crates/reprise-gnome/src/ui/podcasts/add_dialog_tests.rs:660` nageln den alten
Satz fest. Beide auf die Meldung der jeweiligen Fehlerart umstellen — nicht auf
eine neue Literalkonstante, sondern auf `kind.user_message()`.

- [ ] **Step 6: Commit**

```bash
git add crates/reprise-core/src/podcasts.rs \
        crates/reprise-core/src/podcasts/pipeline_youtube_handle_tests.rs \
        crates/reprise-gnome/src/ui/podcasts/add_dialog_tests.rs
git commit -m "fix: let each yt-dlp failure keep its own repair instruction"
```

---

### Task 6: Die Dauer kommt aus der Listung, nicht aus der Wiedergabe

**Files:**
- Modify: `crates/reprise-core/src/podcasts/youtube.rs` (Projektion der Listung)
- Modify: `crates/reprise-core/src/podcasts/store.rs` (Ingest der Episoden)
- Test: `crates/reprise-core/src/podcasts/pipeline_youtube_projection_tests.rs`

**Interfaces:**
- Consumes: `ytdlp::YtDlpVideo { duration_secs: Option<i64>, .. }` (`ytdlp.rs:71`).
- Produces: `podcast_episodes.duration_secs` ist nach einem Refresh gesetzt, ohne
  dass je ein `resolve` lief.

Heute schreibt `save_youtube_resolution` die Dauer im Wiedergabepfad
(`external_media.rs:329`) aus `ResolvedAudio`. Task 8 entfernt diesen Pfad. Die
Listung liefert `duration_secs` bereits mit.

- [ ] **Step 1: Write the failing test**

In `pipeline_youtube_projection_tests.rs`:

```rust
#[test]
fn a_listed_video_carries_its_duration_into_the_episode() {
    let playlist = YtDlpPlaylist {
        title: Some("Channel".into()),
        channel: Some("Channel".into()),
        source_url: Some("https://www.youtube.com/@channel".into()),
        image_url: None,
        entries: vec![YtDlpVideo {
            id: "abc123".into(),
            title: "Track".into(),
            duration_secs: Some(225),
            timestamp: Some(1_700_000_000),
            upload_date: None,
            image_url: None,
        }],
    };
    let listing = super::super::youtube::project_playlist(playlist);
    assert_eq!(listing.episodes[0].duration_secs, Some(225));
}
```

Feldnamen der Projektion prüfen:
`grep -n "duration_secs" crates/reprise-core/src/podcasts/youtube.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p reprise-core a_listed_video_carries_its_duration_into_the_episode`
Expected: FAIL — entweder `left: None, right: Some(225)`, oder das Feld gibt es
in der Projektion noch nicht. **Wenn der Test sofort besteht**, trägt die
Projektion die Dauer bereits; dann entfällt Step 3, und die Aufgabe reduziert
sich darauf, in Step 4 zu belegen, dass sie auch persistiert wird.

- [ ] **Step 3: Carry the duration through the projection**

`duration_secs` von `YtDlpVideo` in die projizierte Episode übernehmen und im
Ingest-`INSERT`/`UPDATE` von `store.rs` mitschreiben. Bestehende Werte dürfen
nicht mit `NULL` überschrieben werden — beim Aktualisieren gilt
`duration_secs = COALESCE(?n, duration_secs)`.

- [ ] **Step 4: Write the persistence test**

Der Refresh-Prüfstand dafür ist `pipeline_refresh_tests.rs` mit `FakeFeed` und
`FakeYoutube` (Zeilen 12 und 26). `FakeYoutube` muss dazu ein `YtDlpVideo` mit
gesetztem `duration_secs` liefern — falls es das heute nicht tut, ergänz das
Feld dort, statt eine zweite Attrappe zu bauen.

```rust
#[test]
fn a_refresh_persists_the_duration_without_any_resolve() {
    // `save_youtube_resolution` is the only writer today, and Task 8 removes
    // its caller. If this fails, a played episode has no duration at all.
    let db = conn();
    let subscription_id = add_youtube_show(db.conn());
    refresh_once(&db, &FakeFeed, &FakeYoutube);
    let episodes = crate::podcasts::store::episodes(&db, subscription_id).unwrap();
    assert_eq!(episodes[0].duration_secs, Some(225));
}
```

`add_youtube_show` und `refresh_once` sind die in `pipeline_refresh_tests.rs`
vorhandenen Entsprechungen — nimm ihre echten Namen
(`grep -n "^fn \|^    fn " crates/reprise-core/src/podcasts/pipeline_refresh_tests.rs | head -20`).

- [ ] **Step 5: Run the tests**

Run: `cargo test -p reprise-core podcasts::`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/reprise-core/src/podcasts/youtube.rs \
        crates/reprise-core/src/podcasts/store.rs \
        crates/reprise-core/src/podcasts/pipeline_youtube_projection_tests.rs
git commit -m "feat: persist a youtube episode's duration at ingest"
```

---

## Strang `ui` — `reprise-gnome`

### Task 7: Der Auffüller als Worker-Operation

**Files:**
- Modify: `crates/reprise-gnome/src/ui/podcasts/podcasts_worker.rs:10-35, 92-102, 250-302`
- Modify: `crates/reprise-gnome/src/ui/podcasts/podcasts_view.rs` (Anstoß nach dem Refresh)
- Test: `crates/reprise-gnome/src/ui/podcasts/podcasts_worker_tests.rs`

**Interfaces:**
- Consumes: `podcasts::fill_downloads::fill_downloads` (Task 4).
- Produces: `PodcastsOperation::FillDownloads`, `PodcastsWorkerResult::Filled(FillSummary)`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn a_fill_downloads_request_does_not_cancel_a_running_refresh() {
    // Same non-cancelling treatment `Download` has: the fill-up runs for
    // minutes and must never invalidate a refresh, nor be invalidated by one.
    let current = 7;
    assert_eq!(
        request_generation(current, PodcastsOperation::FillDownloads),
        current
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p reprise-gnome a_fill_downloads_request_does_not_cancel_a_running_refresh`
Expected: FAIL, `no variant named `FillDownloads``

- [ ] **Step 3: Add the operation**

In `podcasts_worker.rs`:

```rust
pub(in crate::ui) enum PodcastsOperation {
    Refresh {
        policy: podcasts::refresh::RefreshPolicy,
        kind: Option<podcasts::PodcastKind>,
    },
    LoadMore {
        subscription_id: i64,
        end: usize,
    },
    Download {
        episode_id: i64,
    },
    /// Brings every subscription up to its `keep_downloaded` target. Runs
    /// after a refresh rather than inside it: the first run after this feature
    /// lands has a whole library's backlog to fetch, and a refresh that blocks
    /// for that long looks hung.
    FillDownloads,
}
```

im `request_generation`-`match`:

```rust
        PodcastsOperation::Download { .. } | PodcastsOperation::FillDownloads => current,
```

im `PodcastsWorkerResult`:

```rust
    Filled(podcasts::fill_downloads::FillSummary),
```

in `send_response`s `terminal`-Berechnung wird `Filled` wie `Refreshed`
behandelt:

```rust
    let terminal = match &result {
        Err(_)
        | Ok(
            PodcastsWorkerResult::Refreshed(_)
            | PodcastsWorkerResult::LoadedMore { .. }
            | PodcastsWorkerResult::Filled(_),
        ) => true,
```

und im Operations-`match`:

```rust
        PodcastsOperation::FillDownloads => {
            let result = podcasts::config::load(conn)
                .map_err(|error| error.to_string())
                .and_then(|config| {
                    let ytdlp = podcasts::ytdlp::YtDlp::discover_with_browser(
                        config.ytdlp_path.as_deref(),
                        config.youtube_browser,
                    );
                    podcasts::fill_downloads::fill_downloads(
                        conn,
                        &podcasts::pipeline::HttpFeedFetcher,
                        &ytdlp,
                        &podcasts::downloads::default_download_root(),
                        &mut |episode_id, state| {
                            send_response(
                                request,
                                Ok(PodcastsWorkerResult::DownloadState { episode_id, state }),
                            );
                        },
                    )
                    .map(PodcastsWorkerResult::Filled)
                    .map_err(|error| error.to_string())
                });
            send_response(request, result);
        }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p reprise-gnome a_fill_downloads_request_does_not_cancel_a_running_refresh`
Expected: PASS

- [ ] **Step 5: Dispatch it after a refresh**

In `podcasts_view.rs` dort, wo `PodcastsWorkerResult::Refreshed(_)` verarbeitet
wird, im Anschluss `PodcastsOperation::FillDownloads` in Auftrag geben — über
denselben Weg, den `dispatch_download` benutzt (`podcasts_view.rs:544`). Die
eintreffenden `DownloadState`-Antworten fließen in dieselbe
`download_states`-Karte wie beim Knopf-Download, damit die Zeilen ihren
Fortschritt zeigen.

Ein zweiter Auffüller darf nicht neben einem laufenden starten: dieselbe
`Cell<bool>`-Wache, die die Ansicht für andere laufende Operationen benutzt,
oder eine neue mit demselben Muster.

- [ ] **Step 6: Run the crate's tests**

Run: `cargo test -p reprise-gnome podcasts`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/reprise-gnome/src/ui/podcasts/podcasts_worker.rs \
        crates/reprise-gnome/src/ui/podcasts/podcasts_view.rs \
        crates/reprise-gnome/src/ui/podcasts/podcasts_worker_tests.rs
git commit -m "feat: run the download fill-up after every refresh"
```

---

### Task 8: Wiedergabe lädt herunter statt zu streamen

**Files:**
- Modify: `crates/reprise-gnome/src/ui/playback/external_media.rs:280-376`
- Modify: `crates/reprise-gnome/src/ui/playback/external_media_state.rs:36-42`
- Test: `crates/reprise-gnome/src/ui/playback/external_media_state_tests.rs`

**Interfaces:**
- Consumes: `podcasts::pipeline::download_episode` (Task 3),
  `one_shot_task::spawn_with_progress`, `store::episode`.
- Produces: `resolve_youtube` heißt `fetch_youtube` und spielt eine lokale Datei
  ab; `stream_proxy` wird vom Wiedergabepfad nicht mehr betreten. Neu und rein:
  ```rust
  pub(super) enum FetchOutcome { Play(String), Fail(String) }
  pub(super) fn fetch_outcome(
      result: Result<(), String>,
      downloaded_path: Option<String>,
  ) -> FetchOutcome
  ```

**Zur Testbarkeit — vorher lesen.** `reprise-gnome` hat **keinen** Fake-Player
und keinen `PlayerController`-Test; die vorhandenen Wiedergabetests
(`external_media_state_tests.rs`, `external_media_state_queue_tests.rs`) prüfen
reine Zustandsfunktionen. Einen Controller-Prüfstand zu bauen ist ein eigener
Umbau und gehört nicht in diese Aufgabe. Deshalb wird die **Entscheidung** in
eine reine Funktion gezogen und dort getestet; dass die glib-Verdrahtung darum
herum stimmt, belegt Task 10 an der laufenden Anwendung. Wer hier einen
Controller-Test erzwingt, baut das größere Ding — halt an und sag Bescheid.

- [ ] **Step 1: Write the failing test**

In `external_media_state_tests.rs`:

```rust
use super::external_media_state::{fetch_outcome, FetchOutcome};

#[test]
fn a_finished_fetch_plays_the_file_the_download_wrote() {
    let outcome = fetch_outcome(Ok(()), Some("/music/episode.opus".into()));
    assert_eq!(outcome, FetchOutcome::Play("/music/episode.opus".into()));
}

#[test]
fn a_failed_fetch_carries_the_downloads_own_message() {
    // Task 5 makes this message specific; the playback path must not replace
    // it with one of its own, because it is now the reason nothing plays.
    let outcome = fetch_outcome(
        Err("YouTube changed its response — update yt-dlp and try again".into()),
        None,
    );
    assert_eq!(
        outcome,
        FetchOutcome::Fail("YouTube changed its response — update yt-dlp and try again".into())
    );
}

#[test]
fn a_finished_fetch_without_a_file_fails_rather_than_playing_nothing() {
    // `persist_download` writing the row is what makes an episode locally
    // available. A success with no row is a bug, not a playable state.
    let outcome = fetch_outcome(Ok(()), None);
    assert!(matches!(outcome, FetchOutcome::Fail(_)));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p reprise-gnome fetch_outcome`
Expected: FAIL, `cannot find function `fetch_outcome``

- [ ] **Step 2b: Write the pure decision**

In `external_media_state.rs`, neben `podcast_source_requires_resolution`:

```rust
/// What to do once a fetch finishes.
///
/// Pure on purpose: the glib wiring around it cannot be unit-tested in this
/// crate (no fake player exists), so the decision it makes is kept where a
/// test can reach it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum FetchOutcome {
    Play(String),
    Fail(String),
}

pub(super) fn fetch_outcome(
    result: Result<(), String>,
    downloaded_path: Option<String>,
) -> FetchOutcome {
    match (result, downloaded_path) {
        (Ok(()), Some(path)) => FetchOutcome::Play(path),
        (Ok(()), None) => {
            FetchOutcome::Fail("the episode reported a finished download but no file".into())
        }
        (Err(message), _) => FetchOutcome::Fail(message),
    }
}
```

Run: `cargo test -p reprise-gnome fetch_outcome`
Expected: PASS, 3 Tests.

- [ ] **Step 3: Replace the resolve with a fetch**

In `external_media.rs` `resolve_youtube` (Zeile 284) durch `fetch_youtube`
ersetzen. Der Rumpf, an derselben Stelle in `begin_podcast` aufgerufen:

```rust
    /// Fetches the episode, then plays it from disk.
    ///
    /// Replaces the streaming path: an episode is played from a local file or
    /// not at all. The download runs on a named background thread the same way
    /// the resolve used to, and its progress drives the session's phase, so the
    /// player bar shows "fetching" rather than a dead zero.
    fn fetch_youtube(
        self: &Rc<Self>,
        generation: u64,
        episode_id: i64,
        resume_ms: i64,
    ) {
        let db = self.conn.clone();
        let task = crate::ui::one_shot_task::spawn_with_progress(
            "reprise-youtube-fetch",
            move |publish| {
                let config = reprise_core::podcasts::config::load(&db)
                    .map_err(|error| error.to_string())?;
                let ytdlp = reprise_core::podcasts::ytdlp::YtDlp::discover_with_browser(
                    config.ytdlp_path.as_deref(),
                    config.youtube_browser,
                );
                reprise_core::podcasts::pipeline::download_episode(
                    &db,
                    &reprise_core::podcasts::pipeline::HttpFeedFetcher,
                    &ytdlp,
                    &reprise_core::podcasts::downloads::default_download_root(),
                    episode_id,
                    &mut |state| publish(state),
                )
                .map_err(|error| error.to_string())
            },
        );
        let (progress, result) = match task {
            Ok(pair) => pair,
            Err(error) => {
                self.fail_podcast(generation, &error.to_string());
                return;
            }
        };
        let weak = Rc::downgrade(self);
        gtk4::glib::spawn_future_local(async move {
            while let Ok(state) = progress.recv().await {
                let Some(controller) = weak.upgrade() else {
                    return;
                };
                if !controller.external_generation_matches_podcast(generation) {
                    return;
                }
                controller.update_podcast_fetch_progress(generation, &state);
            }
        });
        let weak = Rc::downgrade(self);
        gtk4::glib::spawn_future_local(async move {
            let Ok(result) = result.recv().await else {
                return;
            };
            let Some(controller) = weak.upgrade() else {
                return;
            };
            if !controller.external_generation_matches_podcast(generation) {
                return;
            }
            // The path comes from the row the download just wrote, not from the
            // download's return value: `persist_download` is what makes an
            // episode locally available, so reading the row is what proves it.
            let path = reprise_core::podcasts::store::episode(&controller.conn, episode_id)
                .ok()
                .flatten()
                .and_then(|episode| episode.downloaded_path);
            match fetch_outcome(result.map(|_| ()), path) {
                FetchOutcome::Play(path) => {
                    let _ = controller.start_podcast_source(
                        generation,
                        episode_id,
                        EpisodeSource::File(path),
                        resume_ms,
                    );
                }
                FetchOutcome::Fail(message) => controller.fail_podcast(generation, &message),
            }
        });
    }
```

`update_podcast_fetch_progress` ist neu und klein: es hält die Sitzung in
`PodcastPhase::Resolving` und reicht `DownloadState::Downloading { received, total }`
an die vorhandene Fortschrittsanzeige weiter. Wenn die Wiedergabeleiste heute
keinen Ladefortschritt kennt, genügt für diese Aufgabe, dass die Phase steht —
dann ist die Funktion ein `tracing::debug!` und ein `notify_external_changed()`.

In `begin_podcast` (Zeile 275) den Aufruf ersetzen:

```rust
        if needs_ytdlp {
            self.fetch_youtube(generation, episode_id, resume_ms);
            return Ok(());
        }
```

Das `source`-Argument entfällt: der Download braucht die `episode_id`, nicht die
Video-URL. Die `use`-Zeilen für `stream_proxy` und `YtDlp` in dieser Datei
fallen weg, sofern sie nichts anderes mehr benutzt — der Compiler sagt es.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p reprise-gnome playback`
Expected: PASS

- [ ] **Step 5: Prove the tests are load-bearing**

Dreh in `fetch_outcome` den `(Ok(()), None)`-Arm auf
`FetchOutcome::Play(String::new())`, lauf
`a_finished_fetch_without_a_file_fails_rather_than_playing_nothing`, sieh ihn
scheitern, mach es rückgängig.

- [ ] **Step 5b: Prove the streaming path is gone**

Run: `grep -n "stream_proxy" crates/reprise-gnome/src/ui/playback/external_media.rs`
Expected: keine Treffer. Solange dort noch einer steht, kann die Wiedergabe
weiterhin streamen, und kein Test in diesem Crate würde es merken.

- [ ] **Step 6: Check the file's length**

Run: `wc -l crates/reprise-gnome/src/ui/playback/external_media.rs`
Expected: unter 800. Wenn nicht, wandert `fetch_youtube` in ein neues
`external_media_fetch.rs` — die Datei ist ohnehin die richtige Grenze für diese
Verantwortung.

- [ ] **Step 7: Commit**

```bash
git add crates/reprise-gnome/src/ui/playback/
git commit -m "feat: download a youtube episode before playing it"
```

---

### Task 9: Die Schalter, die nichts mehr tun, verschwinden

**Files:**
- Modify: `crates/reprise-gnome/src/ui/podcasts/add_dialog.rs` (Auto-Download-Zeile)
- Modify: `crates/reprise-gnome/src/ui/podcasts/add_dialog_subscription.rs:27-30`
- Modify: `crates/reprise-gnome/src/ui/preferences/preference_podcasts.rs`
- Modify: `docs/ux-rules.md`
- Test: `crates/reprise-gnome/src/ui/podcasts/add_dialog_tests.rs`

**Interfaces:**
- Consumes: nichts.
- Produces: keine UI mehr für `auto_download` / `auto_download_default`.

Die Spalte `podcast_subscriptions.auto_download` **bleibt**; sie wird nur von
niemandem mehr gelesen. Das Fallenlassen der Spalte ist laut Spec ausdrücklich
außerhalb dieses Plans.

- [ ] **Step 1: Find every switch**

Run: `grep -rn "auto_download" crates/reprise-gnome/src --include='*.rs' | grep -v _tests`
Expected: die Zeilen im Abo-Dialog und in den Einstellungen. Jede davon ist eine
Bedienfläche, die nach diesem Plan nichts mehr bewirkt.

- [ ] **Step 2: Write the failing test**

In `add_dialog_tests.rs`:

```rust
#[test]
fn the_add_dialog_no_longer_offers_an_auto_download_switch() {
    let rows = add_dialog_row_titles();
    assert!(
        !rows.iter().any(|title| title.contains("utomatisch")
            || title.to_lowercase().contains("download")),
        "a switch that changes nothing must not be shown: {rows:?}"
    );
}
```

Die vorhandene Abfrage der Dialogzeilen benutzen:
`grep -n "fn add_dialog_row_titles\|row_titles" crates/reprise-gnome/src/ui/podcasts/add_dialog_tests.rs`.
Gibt es sie nicht, prüf stattdessen, dass die Konstruktionsfunktion des Dialogs
keine `adw::SwitchRow` für Auto-Download mehr baut.

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p reprise-gnome the_add_dialog_no_longer_offers_an_auto_download_switch`
Expected: FAIL

- [ ] **Step 4: Remove the switches**

Die Zeile aus dem Abo-Dialog und aus den Podcast-Einstellungen entfernen, samt
`configured_auto_download_default()` und dem Schreiben von
`AUTO_DOWNLOAD_DEFAULT_KEY`. Neue Abos setzen die Spalte auf ihren
Vorgabewert; nichts liest ihn.

- [ ] **Step 5: Update the UX rules**

`docs/ux-rules.md` nach Auto-Download durchsuchen und die verwaiste Regel
streichen oder auf die neue Regel umschreiben: „Die neuesten `keep_downloaded`
Folgen jedes Abos liegen auf der Platte; Wiedergabe erfolgt immer lokal."

Run: `grep -rn -i "auto.download\|automatisch herunterladen" docs/ux-rules.md`
Expected: nach der Änderung keine Treffer, die den entfernten Schalter meinen.

- [ ] **Step 6: Run the full suite**

Run: `cargo test -p reprise-gnome && cargo clippy --all-targets -- -D warnings`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/reprise-gnome/src/ui/podcasts/ \
        crates/reprise-gnome/src/ui/preferences/preference_podcasts.rs \
        docs/ux-rules.md
git commit -m "refactor: drop the auto-download switches the pipeline no longer reads"
```

---

### Task 10: Abnahme am laufenden Programm

**Files:**
- Modify: keine — dies ist der Nachweis, nicht der Umbau.

**Interfaces:**
- Consumes: alles Vorstehende.
- Produces: das Protokoll, das an den Plan gehängt wird.

Ein grüner Testlauf belegt hier nicht genug: die tragende Behauptung ist, dass
Auffüller und Aufräumer **gemeinsam** konvergieren, und dass eine Episode
tatsächlich von der Platte spielt.

- [ ] **Step 1: Run the whole suite**

Run: `cargo test --workspace 2>&1 | tail -30`
Expected: PASS. Ausgabe nach `$SCRATCH/suite.log` umleiten und nur die
Zusammenfassung lesen.

- [ ] **Step 2: Verify convergence against a real database**

Auf einer **Kopie** der echten Datenbank (samt `-wal`, sonst fehlen die
jüngsten Schreibvorgänge) zweimal hintereinander auffüllen und aufräumen. Beim
zweiten Lauf muss `FillSummary::default()` herauskommen und
`cleanup_candidates` leer sein.

- [ ] **Step 3: Verify playback comes from disk**

Die Anwendung starten, eine YouTube-Episode ohne Datei abspielen. Erwartet:
die Zeile zeigt Ladefortschritt, danach beginnt die Wiedergabe, und
`podcast_episodes.downloaded_path` ist für diese Episode gesetzt. Belegen mit
der DB-Abfrage, nicht mit einem Screenshot.

- [ ] **Step 4: Verify the failure message is specific**

`REPRISE_YTDLP_BIN` auf ein veraltetes yt-dlp zeigen lassen und dieselbe
Episode abspielen. Erwartet: die Zeile nennt die Reparatur („update yt-dlp"),
nicht mehr „YouTube source could not be read with yt-dlp".

- [ ] **Step 5: Record the evidence**

Die vier Ergebnisse als Abschnitt „Abnahme" an diese Plandatei anhängen, mit
den tatsächlichen Zahlen und Abfragen. `phase:` im Frontmatter auf `verified`.

- [ ] **Step 6: Commit**

```bash
git add docs/plans/always-download-episodes.md
git commit -m "docs: record the acceptance run for always-download episodes"
```
