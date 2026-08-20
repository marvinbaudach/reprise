---
slug: always-download-episodes-core
worktree: /home/marvin/Projects/reprise-always-download-episodes-core
branch: feature/always-download-episodes-core
phase: planned
codex_session:
created: 2026-08-20
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

## Datei-Eigentum dieses Strangs

Dieser Strang gehört `crates/reprise-core/**` und dieser Plandatei. Er fasst
**nichts** unter `crates/reprise-gnome/**` an — der `ui`-Strang baut auf den hier
entstehenden Schnittstellen auf und wird erst nach diesem gebaut.

Fällt beim Umbau auf, dass ein Aufrufer in `reprise-gnome` bricht: **nicht dort
reparieren.** Notier es im Abschlussbericht; der `ui`-Strang zieht nach. Der Lauf
`cargo test -p reprise-core` und `cargo clippy -p reprise-core --all-targets --
-D warnings` ist das bindende grüne Ziel dieses Strangs, nicht der ganze
Workspace.

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
