---
slug: stats-artist-images-and-ranking
worktree: /home/marvin/Projects/reprise-stats-artist-images-and-ranking
branch: feature/stats-artist-images-and-ranking
phase: verified
codex_session:
created: 2026-08-13
---
# My Stats: Künstlerbilder und aufklappbare Interpretenrangliste — Umsetzungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Spec:** `docs/superpowers/specs/2026-08-13-stats-artist-images-and-ranking-design.md`
**Baseline:** `origin/dev` @ 79d3a51528

**Goal:** Die Bandreihe in *My Stats* zeigt echte Künstlerporträts statt eines
zufällig gewählten Album-Covers und lässt sich — wie die Songkarte — nach
Abspielungen oder Hörzeit ordnen und auf Rang 20 aufklappen.

**Architecture:** Die Datenschicht wählt den Bild-Repräsentanten künftig je
Album statt je Dateipfad und liefert bis zu drei Kandidaten. Die
Oberflächenschicht bekommt einen gemeinsamen Auflöser (`StatsArtistImage`),
der Porträt → Album-Cover → Initialen durchgeht und dabei den vorhandenen
`CoverLoader` sowie das bestehende, abschaltbare Porträt-Modul benutzt. Die
Bandreihe wird in eine Karte gefasst, die Umschalter und Aufklapper der
Songkarte spiegelt.

**Tech Stack:** Rust, gtk4-rs, libadwaita (`adw::ToggleGroup`, `adw::Avatar`),
rusqlite, glib/gio (`spawn_future_local`, `spawn_blocking`).

## Global Constraints

- **Repo-Sprache ist Englisch.** Code, Kommentare, Testnamen, UI-Strings,
  Commit-Messages und `docs/ux-rules.md` sind englisch. Nur dieser Plan und die
  Spezifikation sind deutsch.
- **`Files:`-Listen sind Startpunkt, kein Zaun.** Angrenzende Dateien dürfen
  minimal mitgeändert werden (Import, Aufrufstelle, Testfixture) — nenne sie in
  der Commit-Message. Anhalten nur, wenn der *Vertrag* einer Aufgabe falsch ist,
  nicht wenn die Dateiliste zu eng ist.
- **Keine Mutation von Nutzerdateien.** Cover- und Porträt-Zwischenspeicher
  liegen ausschließlich unter `~/.cache/reprise/`; die Musikbibliothek wird
  niemals geschrieben.
- **Netzzugriff bleibt abschaltbar.** Porträts dürfen nur geholt werden, wenn
  `ArtistPortraitRuntime::enabled` gesetzt ist.
- **Testbefehle:**
  - Kern: `cargo test -p reprise-core stats_ 2>&1 | tee $SCRATCH/core.log`
  - Oberfläche (ohne Display): `cargo test -p reprise-gnome --bin reprise stats_`
    — `--lib` läuft in `reprise-gnome` ins Leere, es gibt nur `--bin reprise`.
  - Oberfläche (mit Display): `xvfb-run -a cargo test -p reprise-gnome --bin reprise stats_ -- --ignored`
  - Regelwerk: `scripts/check-ux-traceability.sh`
  - Grün heißt: keine Zeile `^test result: FAILED` im Log. `--exact` nicht
    benutzen, es läuft ins Leere.
- **Bekannt rote Tests auf `dev`** sind nicht Schuld dieser Aufgabe. Vor dem
  ersten Commit einmal den Ausgangsstand messen und Abweichungen daran messen.
- **Ausgabe kappen:** lange Läufe nach `$SCRATCH/<name>.log` umleiten und mit
  `grep`/`wc` auswerten, nie ganze Logs zurücklesen.

## Dateiübersicht

| Datei | Rolle |
|---|---|
| `crates/reprise-core/src/library/stats_screen.rs` | `NamedRow`, `RankedGroup`, `artist_rows`, `ranked_groups` — Wahl des Bild-Repräsentanten |
| `crates/reprise-core/src/library/stats_screen_tests.rs` | Tests der Repräsentantenwahl gegen echtes SQL |
| `crates/reprise-core/src/library/stats_snapshot.rs` | `top_artists_sorted`, `artist_share_percent` |
| `crates/reprise-core/src/library/stats_snapshot_tests.rs` | Tests der Sortierung und des Anteils |
| `crates/reprise-gnome/src/ui/cover/cover_loader.rs` | neue Methode für Bilddateien (Porträts) |
| `crates/reprise-gnome/src/ui/now_playing/artist_portrait_worker.rs` | Anfrage-Warteschlange mit Deckel |
| `crates/reprise-gnome/src/ui/stats/stats_artist_image.rs` | **neu** — gemeinsame Bildkette |
| `crates/reprise-gnome/src/ui/stats/stats_artist_image_tests.rs` | **neu** — Tests der Kette |
| `crates/reprise-gnome/src/ui/stats/stats_band_card.rs` | Hero-Karte benutzt die Kette |
| `crates/reprise-gnome/src/ui/stats/stats_band_tile.rs` | Kacheln benutzen die Kette |
| `crates/reprise-gnome/src/ui/stats/stats_bands_card.rs` | **neu** — Kopf, Umschalter, Aufklapper |
| `crates/reprise-gnome/src/ui/stats/stats_bands_more.rs` | **neu** — zweispaltige Fortsetzungszeilen |
| `crates/reprise-gnome/src/ui/stats/stats_bands_row.rs` | liest die sortierte Liste statt `spotlight.also` |
| `crates/reprise-gnome/src/ui/stats/stats_css.rs` | Stil der Fortsetzungszeilen |
| `crates/reprise-gnome/src/ui/stats/stats_view.rs` | Komposition, Porträt-Runtime einhängen |
| `crates/reprise-gnome/src/ui/window/window.rs` | reicht die Porträt-Runtime an die Ansicht |
| `docs/ux-rules.md` | STATS-13 ersetzt, STATS-23 neu |

## Reihenfolge und Parallelität

```
T0 (Regelwerk + Testumbenennung)          ← muss zuerst landen, sonst ist der Gate rot
        │
        ├── T1 (Kern: Repräsentant)   ┐
        ├── T2 (Kern: Sortierung)     ├─ Welle A, parallel
        └── T3 (GTK: Bildkette)       ┘
        │
        ▼  ← Review Welle A (rust-reviewer, Sonnet/high) auf den Diff der Welle
        T4 (Hero + Kacheln benutzen die Kette)
        │
        ▼
        T5 (Bandkarte: Umschalter + Aufklapper)
        │
        ▼  ← Review Welle B
        T6 (Abnahme: Gates + Screenshot)
```

T1, T2 und T3 fassen disjunkte Dateien an und dürfen gleichzeitig in eigenen
Worktrees laufen. T4 und T5 fassen beide `stats_view.rs` und
`stats_bands_row.rs` an — nacheinander, nicht parallel.

---

### Task 0: Regelwerk STATS-23 und Testumbenennung

Der Traceability-Gate lehnt Tests ab, die eine ersetzte Regel nennen, und
verlangt für jede aktive Regel einen gleichnamigen Test. Regeländerung und
Umbenennung müssen deshalb **in einem Commit** landen.

**Files:**
- Modify: `docs/ux-rules.md:2729-2738` (STATS-13 → replaced)
- Modify: `docs/ux-rules.md` (neue Regel STATS-23 hinter STATS-22, ~Zeile 2875)
- Modify: `crates/reprise-core/src/library/stats_snapshot_tests.rs:203`
- Modify: `crates/reprise-gnome/src/ui/stats/stats_band_card.rs:333,356`

**Interfaces:**
- Produces: Regel-ID `STATS-23`, auf die alle folgenden Tests mit dem Präfix
  `stats_23_` verweisen.

- [ ] **Step 1: Ausgangsstand des Gates messen**

```bash
scripts/check-ux-traceability.sh; echo "exit=$?"
```
Expected: `exit=0` (der Gate ist auf `dev` grün — ist er es nicht, den Befund
notieren und trotzdem fortfahren, aber am Ende gegen genau diesen Stand messen).

- [ ] **Step 2: STATS-13 auf ersetzt setzen**

In `docs/ux-rules.md` die Kopfzeile der Regel ändern:

```markdown
- **STATS-13** [replaced by STATS-23] [gtk] — The band card shows the most-listened
```

Der übrige Regeltext bleibt unverändert stehen (so hält es das Dokument bei
allen ersetzten Regeln).

- [ ] **Step 3: STATS-23 einfügen**

Direkt hinter dem Absatz von STATS-22 (vor `## W. Buttons & interaction states`):

```markdown
- **STATS-23** [active] [gtk] — Replaces STATS-13, which pinned the band card to
  "the album cover of their most-played track" while the code shipped the
  alphabetically first path, and which knew no ranking past rank 5. **The bands
  row is one card and answers like the songs card.** Every band surface — the
  leader's hero, the four runner-up tiles and the continuation rows — resolves
  its image the same way: the cached artist portrait first, then the cover of
  the artist's most-played album in the period, then the next most-played album
  that actually carries artwork (at most three tried), then an initials tile;
  never an empty surface. A missing portrait is fetched only while the Artist
  portraits module is enabled and only for the ranks on screen; with the module
  off nothing is requested and the album cover stands. The album cover paints as
  soon as it resolves, and a portrait arriving later replaces it. **The "by
  plays / by time" toggle beside the row sorts the whole row** — leader, tiles
  and continuation alike — and the leader's "N % of your artist listening" is
  recomputed for whoever leads under the chosen metric, against the same artist
  population STATS-13 divided by. **"Show more top artists" grows this card and
  never opens a second one:** it reveals ranks 6 to 20 in two columns directly
  below the button, each row carrying its rank, a round portrait, the name, a bar
  relative to rank 1 and the metric the toggle selects. It is offered only when
  there is something past the five on screen, and collapsing returns the card to
  its row. **Its rows answer like the tiles above them:** a focusable target with
  the pointer cursor and the shared hover wash (BTN-1/BTN-4) that opens the
  library filtered to the artist on click and on Enter or Space (regular history
  push). Where a group combines several spellings the unification hint from
  STATS-9 is retained; durations follow the compact format from STATS-11.
```

- [ ] **Step 4: Die drei Tests umbenennen**

`crates/reprise-core/src/library/stats_snapshot_tests.rs:203`:

```rust
fn stats_23_band_card_data_reports_share_and_ranked_artists() {
```

`crates/reprise-gnome/src/ui/stats/stats_band_card.rs:333` und `:356`:

```rust
fn stats_23_missing_cover_falls_back_to_initials() {
```
```rust
fn stats_23_band_card_click_opens_the_artist() {
```

- [ ] **Step 5: Gate laufen lassen**

```bash
scripts/check-ux-traceability.sh; echo "exit=$?"
```
Expected: `exit=0`, keine Zeile mit `STATS-13` oder `STATS-23` in der
Fehlerausgabe.

- [ ] **Step 6: Betroffene Tests laufen lassen**

```bash
cargo test -p reprise-core stats_23 2>&1 | grep -E "^test result"
```
Expected: `test result: ok.` mit mindestens 1 Test.

- [ ] **Step 7: Commit**

```bash
git add docs/ux-rules.md crates/reprise-core/src/library/stats_snapshot_tests.rs \
        crates/reprise-gnome/src/ui/stats/stats_band_card.rs
git commit -m "docs: STATS-23 replaces STATS-13 for the bands row"
```

---

### Task 1: Kern — der Repräsentant ist das meistgehörte Album

**Parallel zu T2 und T3.**

**Files:**
- Modify: `crates/reprise-core/src/library/stats_screen.rs:86-105` (`RankedGroup`, `NamedRow`)
- Modify: `crates/reprise-core/src/library/stats_screen.rs:219-253` (`artist_rows`)
- Modify: `crates/reprise-core/src/library/stats_screen.rs:280-340` (die übrigen `NamedRow`-Konstruktionen)
- Modify: `crates/reprise-core/src/library/stats_screen.rs:480-510` (`ranked_groups`)
- Test: `crates/reprise-core/src/library/stats_screen_tests.rs`

**Interfaces:**
- Consumes: nichts aus anderen Tasks.
- Produces:
  - `pub struct RankedGroup { pub group: Group, pub representative_track_path: String, pub cover_candidates: Vec<String> }`
  - `pub(crate) struct NamedRow { …, pub album: String }`
  - `ranked_groups(rows: &[NamedRow]) -> Vec<RankedGroup>` (Signatur unverändert)

- [ ] **Step 1: Den fehlschlagenden Test schreiben**

Ans Ende von `crates/reprise-core/src/library/stats_screen_tests.rs`:

```rust
use super::{artist_rows, ranked_groups};

/// One artist, two albums: the album that sorts first by path is *not* the one
/// that was listened to. STATS-23 wants the cover of the most-played album.
#[test]
fn stats_23_representative_cover_follows_the_most_played_album() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate_connection(&conn).unwrap();
    insert_album_track(&conn, 1, "/music/Band/A Early/01.flac", "Band", "A Early");
    insert_album_track(&conn, 2, "/music/Band/Z Later/01.flac", "Band", "Z Later");
    play(&conn, 1, 100);
    for at in [200, 300, 400] {
        play(&conn, 2, at);
    }

    let ranked = ranked_groups(&artist_rows(&conn, 0, 1_000).unwrap());

    assert_eq!(ranked.len(), 1);
    assert_eq!(
        ranked[0].representative_track_path,
        "/music/Band/Z Later/01.flac"
    );
    assert_eq!(
        ranked[0].cover_candidates,
        vec![
            "/music/Band/Z Later/01.flac".to_string(),
            "/music/Band/A Early/01.flac".to_string(),
        ],
        "the runner-up album stays available for a cover that does not resolve"
    );
}

/// A cover candidate list of four albums is cut to three: the view walks it
/// synchronously, and a long walk would keep the card blank.
#[test]
fn stats_23_cover_candidates_stop_at_three() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate_connection(&conn).unwrap();
    for (id, album) in [(1, "D"), (2, "C"), (3, "B"), (4, "A")] {
        insert_album_track(
            &conn,
            id,
            &format!("/music/Band/{album}/01.flac"),
            "Band",
            album,
        );
        for play_index in 0..id {
            play(&conn, id, 100 + play_index * 10);
        }
    }

    let ranked = ranked_groups(&artist_rows(&conn, 0, 1_000).unwrap());

    assert_eq!(ranked[0].cover_candidates.len(), 3);
    assert_eq!(ranked[0].cover_candidates[0], "/music/Band/A/01.flac");
}

/// Grouping the query one level finer must not move a single play between
/// artists, and must not change which spelling wins the label.
#[test]
fn stats_23_album_grouping_leaves_plays_and_labels_untouched() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate_connection(&conn).unwrap();
    // "Band" is spelled twice; the dominant spelling has its plays spread over
    // three albums, the other has all of them on one.
    for (id, album) in [(1, "One"), (2, "Two"), (3, "Three")] {
        insert_album_track(&conn, id, &format!("/music/a/{id}.flac"), "Band", album);
        for play_index in 0..2 {
            play(&conn, id, 100 + i64::from(play_index));
        }
    }
    insert_album_track(&conn, 4, "/music/b/4.flac", "band ", "Four");
    for play_index in 0..5 {
        play(&conn, 4, 200 + i64::from(play_index));
    }

    let ranked = ranked_groups(&artist_rows(&conn, 0, 1_000).unwrap());

    assert_eq!(ranked.len(), 1, "both spellings fold into one group");
    assert_eq!(ranked[0].group.plays, 11);
    assert_eq!(
        ranked[0].group.label, "Band",
        "the label follows summed plays per spelling, not per album row"
    );
}

fn insert_album_track(
    conn: &rusqlite::Connection,
    id: i64,
    path: &str,
    artist: &str,
    album: &str,
) {
    conn.execute(
            "INSERT INTO tracks \
             (id, path, title, artist, album, album_artist, genre, duration_ms, \
              play_count, added_at) \
             VALUES (?1, ?2, 'Track', ?3, ?4, '', 'Rock', 100000, 0, 0)",
            rusqlite::params![id, path, artist, album],
        )
        .unwrap();
}

fn play(conn: &rusqlite::Connection, track_id: i64, played_at: i64) {
    conn.execute(
        "INSERT INTO listen_events (track_id, played_at, ms_played) \
         VALUES (?1, ?2, 100000)",
        rusqlite::params![track_id, played_at],
    )
    .unwrap();
}
```

`crate::db::open(None)` liefert hier eine nackte `rusqlite::Connection` (siehe
`db.rs:37`), deshalb `conn.execute(…)` und `artist_rows(&conn, …)` — genau wie
im bestehenden Test derselben Datei.

- [ ] **Step 2: Test laufen lassen, Fehlschlag bestätigen**

```bash
cargo test -p reprise-core stats_23_ 2>&1 | grep -E "^error|^test result" | head
```
Expected: Kompilierfehler `no field `cover_candidates` on type `RankedGroup``
und `no field `album``.

- [ ] **Step 3: `NamedRow` und `RankedGroup` erweitern**

`stats_screen.rs`, `RankedGroup` (Zeile 86):

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankedGroup {
    pub group: Group,
    pub representative_track_path: String,
    /// Up to three cover candidates, most-played album first. The view walks
    /// them until one resolves to artwork: the most-played album is the right
    /// answer, but an album without a cover must not leave the card blank while
    /// the runner-up carries one (STATS-23).
    pub cover_candidates: Vec<String>,
}
```

`NamedRow` (Zeile 98):

```rust
#[derive(Debug, Clone)]
pub(crate) struct NamedRow {
    pub raw: String,
    pub mbid: Option<String>,
    pub plays: i64,
    pub ms: i64,
    pub last_played_at: i64,
    pub path: String,
    /// The album this aggregate row belongs to, empty where the query cannot
    /// name one. Covers are chosen per album, so a row without one falls into a
    /// single bucket per group — which is what every row did before STATS-23.
    pub album: String,
}
```

- [ ] **Step 4: Die vier Konstruktionsstellen versorgen**

`artist_rows` (Zeile 224): `le.album` in SELECT **und** GROUP BY aufnehmen:

```rust
    let sql = format!(
        "SELECT {RAW_EFFECTIVE_ALBUM_ARTIST} AS raw, le.artist, le.album_artist, \
                NULLIF(TRIM(le.artist_mbid), ''), COUNT(le.id), \
                COALESCE(SUM({CLAMPED_MS}), 0), MAX(le.played_at), MIN(le.path), \
                le.album \
         FROM listen_events le \
         WHERE le.played_at >= ?1 AND le.played_at < ?2 \
           AND TRIM({RAW_EFFECTIVE_ALBUM_ARTIST}) <> '' \
         GROUP BY raw, le.artist, le.album_artist, le.artist_mbid, le.album"
    );
```

und im `query_map` (Zeile 242) `album: row.get(8)?,` ergänzen.

`album_rows` (Zeile 326): `album: row.get(0)?,` — die Spalte steht dort schon
an Position 0 des SELECT.

`genre_artist_rows` (Zeile 293) und `query_named_rows` (Zeile 521):
`album: String::new(),`.

- [ ] **Step 5: `ranked_groups` auf Album-Kandidaten umstellen**

Den `paths`-Block (Zeile 494–503) ersetzen:

```rust
/// How many albums a group offers as cover candidates. The view walks the list
/// until one resolves; three is enough to cover an unillustrated favourite
/// without making the walk visible.
const COVER_CANDIDATE_LIMIT: usize = 3;

#[derive(Clone, Debug)]
struct AlbumCandidate {
    plays: i64,
    ms: i64,
    path: String,
}

pub(crate) fn ranked_groups(rows: &[NamedRow]) -> Vec<RankedGroup> {
    let inputs = rows
        .iter()
        .filter(|row| !normalize_group_key(&row.raw).is_empty())
        .map(|row| GroupInput {
            raw: &row.raw,
            mbid: row.mbid.as_deref(),
            plays: row.plays,
            ms: row.ms,
            last_played_at: row.last_played_at,
        })
        .collect::<Vec<_>>();
    let resolver = key_resolver(rows);

    // Covers are chosen per album, not per aggregate row: one album can arrive
    // split across several spellings or MBIDs, and the cover question is about
    // the album, not about the spelling (STATS-23).
    let mut albums = HashMap::<(String, String), AlbumCandidate>::new();
    for row in rows
        .iter()
        .filter(|row| !normalize_group_key(&row.raw).is_empty())
    {
        let entry = albums
            .entry((resolver.key_for(&row.raw), row.album.clone()))
            .or_insert_with(|| AlbumCandidate {
                plays: 0,
                ms: 0,
                path: row.path.clone(),
            });
        entry.plays += row.plays;
        entry.ms += row.ms;
        if row.path < entry.path {
            entry.path = row.path.clone();
        }
    }

    let mut by_key = HashMap::<String, Vec<AlbumCandidate>>::new();
    for ((key, _album), candidate) in albums {
        by_key.entry(key).or_default().push(candidate);
    }
    for candidates in by_key.values_mut() {
        // Most played first; ties fall to listening time and then to the path,
        // so the same library always shows the same cover.
        candidates.sort_by(|left, right| {
            right
                .plays
                .cmp(&left.plays)
                .then_with(|| right.ms.cmp(&left.ms))
                .then_with(|| left.path.cmp(&right.path))
        });
        candidates.truncate(COVER_CANDIDATE_LIMIT);
    }

    fold_groups(&inputs)
        .into_iter()
        .map(|group| {
            let paths = by_key
                .get(&group.key)
                .map(|candidates| {
                    candidates
                        .iter()
                        .map(|candidate| candidate.path.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            RankedGroup {
                representative_track_path: paths.first().cloned().unwrap_or_default(),
                cover_candidates: paths,
                group,
            }
        })
        .collect()
}
```

- [ ] **Step 6: Übrige `RankedGroup`-Literale nachziehen**

```bash
grep -rn "RankedGroup {" --include='*.rs' crates | grep -v stats_screen.rs
```
Jede Fundstelle (Testfixtures in `stats_band_card.rs`, `stats_genre_card.rs`)
bekommt `cover_candidates: vec![…]` mit demselben Pfad wie
`representative_track_path`.

- [ ] **Step 7: Tests laufen lassen**

```bash
cargo test -p reprise-core stats_ 2>&1 | tee $SCRATCH/t1-core.log | grep -E "^test result"
grep -c "^test result: FAILED" $SCRATCH/t1-core.log
```
Expected: `test result: ok.`, Zähler `0`.

- [ ] **Step 8: Commit**

```bash
git add crates/reprise-core/src/library/stats_screen.rs \
        crates/reprise-core/src/library/stats_screen_tests.rs
git commit -m "fix: pick the stats cover from the most-played album (STATS-23)"
```

---

### Task 2: Kern — Rangliste nach Metrik sortieren

**Parallel zu T1 und T3.**

**Files:**
- Modify: `crates/reprise-core/src/library/stats_snapshot.rs:126-150` (neben `top_tracks_sorted`)
- Test: `crates/reprise-core/src/library/stats_snapshot_tests.rs`

**Interfaces:**
- Consumes: `RankedGroup` aus T1 — greift aber nur auf `group.plays`,
  `group.ms` und `group.label` zu und kompiliert deshalb auch ohne T1.
- Produces:
  - `StatsSnapshot::top_artists_sorted(&self, sort_by: SortBy) -> Vec<RankedGroup>`
  - `StatsSnapshot::artist_share_percent(&self, artist: &RankedGroup) -> i64`

- [ ] **Step 1: Den fehlschlagenden Test schreiben**

Ans Ende von `stats_snapshot_tests.rs`:

```rust
/// The toggle changes the ranking, not just its labels: a short-track band with
/// many plays leads by plays and trails by time.
#[test]
fn stats_23_top_artists_sorted_follows_the_chosen_metric() {
    let conn = migrated_conn();
    // "Sprinter": six short plays. "Marathon": two long ones.
    insert_track(&conn, 1, "Short", "Sprinter", "", "Rock", 60_000, 0, None);
    insert_track(&conn, 2, "Long", "Marathon", "", "Rock", 600_000, 0, None);
    for play in 0..6 {
        insert_event(&conn, 1, timestamp(2026, 2, 1, 12, play), 60_000);
    }
    for play in 0..2 {
        insert_event(&conn, 2, timestamp(2026, 2, 2, 12, play), 600_000);
    }

    let snapshot = compute(&conn, StatsPeriod::AllTime, NOW_2026_07_19, &Utc).unwrap();

    let by_plays = snapshot.top_artists_sorted(SortBy::Plays);
    let by_time = snapshot.top_artists_sorted(SortBy::Time);
    assert_eq!(by_plays[0].group.label, "Sprinter");
    assert_eq!(by_time[0].group.label, "Marathon");
    assert_eq!(by_plays.len(), by_time.len(), "sorting drops no artist");
}

/// The share is the leader's share of all artist listening — so it has to be
/// recomputed when the toggle hands the lead to somebody else.
#[test]
fn stats_23_artist_share_follows_whoever_leads() {
    let conn = migrated_conn();
    insert_track(&conn, 1, "Short", "Sprinter", "", "Rock", 60_000, 0, None);
    insert_track(&conn, 2, "Long", "Marathon", "", "Rock", 600_000, 0, None);
    for play in 0..6 {
        insert_event(&conn, 1, timestamp(2026, 2, 1, 12, play), 60_000);
    }
    for play in 0..2 {
        insert_event(&conn, 2, timestamp(2026, 2, 2, 12, play), 600_000);
    }

    let snapshot = compute(&conn, StatsPeriod::AllTime, NOW_2026_07_19, &Utc).unwrap();

    let sprinter = &snapshot.top_artists_sorted(SortBy::Plays)[0];
    let marathon = &snapshot.top_artists_sorted(SortBy::Time)[0];
    // 360 s of 1560 s, and 1200 s of 1560 s.
    assert_eq!(snapshot.artist_share_percent(sprinter), 23);
    assert_eq!(snapshot.artist_share_percent(marathon), 77);
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag bestätigen**

```bash
cargo test -p reprise-core stats_23_ 2>&1 | grep -E "^error\[|^test result" | head
```
Expected: `no method named `top_artists_sorted` found`.

- [ ] **Step 3: Die beiden Methoden schreiben**

In `stats_snapshot.rs`, direkt hinter `top_tracks_sorted`:

```rust
    /// The artist ranking under the chosen metric. The bands row reads this
    /// rather than `spotlight.also`, which stops after four runners-up
    /// (STATS-23).
    pub fn top_artists_sorted(&self, sort_by: SortBy) -> Vec<RankedGroup> {
        let mut artists = self.top_artists.clone();
        artists.sort_by(|left, right| {
            match sort_by {
                SortBy::Plays => right
                    .group
                    .plays
                    .cmp(&left.group.plays)
                    .then_with(|| right.group.ms.cmp(&left.group.ms)),
                SortBy::Time => right
                    .group
                    .ms
                    .cmp(&left.group.ms)
                    .then_with(|| right.group.plays.cmp(&left.group.plays)),
            }
            // The label breaks the last tie so the row order is stable across
            // renders — a ranking that reshuffles on redraw reads as broken.
            .then_with(|| left.group.label.cmp(&right.group.label))
        });
        artists
    }

    /// One artist's share of all artist listening, against the same population
    /// `spotlight` divides by: tracks whose artist tag is empty are no artist
    /// and must not shrink everyone else's share.
    pub fn artist_share_percent(&self, artist: &RankedGroup) -> i64 {
        let denominator = self
            .top_artists
            .iter()
            .map(|entry| entry.group.ms)
            .sum::<i64>();
        percent(artist.group.ms, denominator)
    }
```

`RankedGroup` ist in dieser Datei zu importieren, falls noch nicht geschehen
(`use crate::library::stats_screen::RankedGroup;`).

- [ ] **Step 4: Tests laufen lassen**

```bash
cargo test -p reprise-core stats_ 2>&1 | tee $SCRATCH/t2-core.log | grep -E "^test result"
grep -c "^test result: FAILED" $SCRATCH/t2-core.log
```
Expected: `test result: ok.`, Zähler `0`.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-core/src/library/stats_snapshot.rs \
        crates/reprise-core/src/library/stats_snapshot_tests.rs
git commit -m "feat: sort the stats artist ranking by plays or time (STATS-23)"
```

---

### Task 3: GTK — die Bildkette

**Parallel zu T1 und T2.**

**Files:**
- Modify: `crates/reprise-gnome/src/ui/cover/cover_loader.rs:133-146`
- Modify: `crates/reprise-gnome/src/ui/now_playing/artist_portrait_worker.rs`
- Create: `crates/reprise-gnome/src/ui/stats/stats_artist_image.rs`
- Create: `crates/reprise-gnome/src/ui/stats/stats_artist_image_tests.rs`
- Modify: `crates/reprise-gnome/src/ui/stats/mod.rs` (Modul anmelden)

**Interfaces:**
- Consumes: `CoverLoader` (`load_into_picture`), `ArtistPortraitRuntime`.
- Produces:
  - `CoverLoader::load_image_into_picture(picture, image_path, size, token, current, on_loaded: impl Fn(bool) + 'static)`
  - `ArtistPortraitRuntime::is_enabled(&self) -> bool`
  - `ArtistPortraitRuntime::request(&self, name: String, on_ready: impl Fn(Option<PathBuf>) + 'static)`
  - `StatsArtistImage::new(cover_loader: Rc<CoverLoader>) -> Rc<Self>`
  - `StatsArtistImage::set_portrait_runtime(&self, runtime: Rc<ArtistPortraitRuntime>)`
  - `StatsArtistImage::load(&self, picture: &gtk4::Picture, request: ArtistImageRequest)`
  - `pub(super) struct ArtistImageRequest { pub artist: String, pub candidates: Vec<String>, pub size: ThumbnailSize, pub token: u64, pub generation: Rc<Cell<u64>>, pub on_loaded: Rc<dyn Fn(bool)> }`
  - `pub(super) fn next_candidate(candidates: &[String], tried: usize) -> Option<&str>`

- [ ] **Step 1: Den fehlschlagenden Test schreiben**

`crates/reprise-gnome/src/ui/stats/stats_artist_image_tests.rs`:

```rust
use super::*;

/// The walk down the candidate list is plain arithmetic and must hold without a
/// display: the first candidate is tried first, each failure advances by one,
/// and a list that runs out ends the walk instead of wrapping.
#[test]
fn stats_23_the_candidate_walk_advances_once_per_failure() {
    let candidates = vec![
        "/music/one.flac".to_string(),
        "/music/two.flac".to_string(),
    ];

    assert_eq!(next_candidate(&candidates, 0), Some("/music/one.flac"));
    assert_eq!(next_candidate(&candidates, 1), Some("/music/two.flac"));
    assert_eq!(next_candidate(&candidates, 2), None);
    assert_eq!(next_candidate(&[], 0), None);
}

/// With the module off no name may reach the fetch queue — the setting is the
/// only thing standing between the stats page and a request per artist.
#[test]
fn stats_23_a_disabled_module_queues_no_portrait_request() {
    let runtime = Rc::new(ArtistPortraitRuntime {
        enabled: Rc::new(std::cell::Cell::new(false)),
    });

    assert!(!runtime.is_enabled());
    assert!(
        !runtime.request_would_run("Lorna Shore"),
        "a disabled module answers from the cache alone"
    );
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag bestätigen**

```bash
cargo test -p reprise-gnome --bin reprise stats_23_ 2>&1 | grep -E "^error|^test result" | head
```
Expected: `unresolved module` / `cannot find function `next_candidate``.

- [ ] **Step 3: Den Porträt-Zugang bauen**

`artist_portrait_worker.rs` ergänzen (bestehendes `enabled`-Feld bleibt):

```rust
use std::collections::VecDeque;
use std::path::PathBuf;

/// At most this many portrait fetches are in flight at once. Twenty ranks come
/// on screen together; firing twenty Deezer requests at once would be rude and
/// would flood the blocking pool for the covers beside them.
const MAX_IN_FLIGHT: usize = 3;

pub(in crate::ui) struct ArtistPortraitRuntime {
    pub enabled: Rc<Cell<bool>>,
    in_flight: Rc<Cell<usize>>,
    queue: Rc<RefCell<VecDeque<(String, Rc<dyn Fn(Option<PathBuf>)>)>>>,
}

impl ArtistPortraitRuntime {
    pub(in crate::ui) fn is_enabled(&self) -> bool {
        self.enabled.get()
    }

    /// Whether a fetch for `name` would leave this process. Pure — the tests
    /// ask it instead of watching the network.
    pub(in crate::ui) fn request_would_run(&self, name: &str) -> bool {
        self.is_enabled() && !name.trim().is_empty()
    }

    /// Queues a portrait fetch. `on_ready` runs on the main context with the
    /// portrait's path, or `None` when the artist has none. Never runs when the
    /// module is off.
    pub(in crate::ui) fn request(&self, name: String, on_ready: impl Fn(Option<PathBuf>) + 'static) {
        if !self.request_would_run(&name) {
            on_ready(None);
            return;
        }
        self.queue
            .borrow_mut()
            .push_back((name, Rc::new(on_ready)));
        self.pump();
    }

    fn pump(&self) {
        while self.in_flight.get() < MAX_IN_FLIGHT {
            let Some((name, on_ready)) = self.queue.borrow_mut().pop_front() else {
                return;
            };
            self.in_flight.set(self.in_flight.get() + 1);
            let in_flight = self.in_flight.clone();
            let queue = self.queue.clone();
            let enabled = self.enabled.clone();
            glib::spawn_future_local(async move {
                // `load_or_fetch` blocks: it answers from the cache when fresh
                // and talks to Deezer otherwise. Never on the main loop.
                let fetch_name = name.clone();
                let found = gio::spawn_blocking(move || {
                    match reprise_core::artist_portrait::load_or_fetch(&fetch_name) {
                        Ok(reprise_core::artist_portrait::PortraitOutcome::Found(path)) => {
                            Some(path)
                        }
                        _ => None,
                    }
                })
                .await
                .ok()
                .flatten();
                in_flight.set(in_flight.get().saturating_sub(1));
                on_ready(found);
                // Whatever is still queued moves up a slot.
                let next = Self {
                    enabled,
                    in_flight,
                    queue,
                };
                next.pump();
            });
        }
    }
}
```

`setup` und `set_enabled` behalten ihr Verhalten und füllen die beiden neuen
Felder mit `Rc::new(Cell::new(0))` bzw. `Rc::new(RefCell::new(VecDeque::new()))`.
Den überholten Kommentar in Zeile 3–5 durch einen Satz ersetzen, der die neue
Fläche nennt (My Stats, STATS-23).

- [ ] **Step 4: Den Bildlader für Bilddateien ergänzen**

In `cover_loader.rs` neben `load_into_picture`:

```rust
    /// Loads an image file (not a track) into a picture — the artist portraits
    /// in My Stats are plain files in the portrait cache, and they go through
    /// the same thumbnail cache as every cover so a 500 px JPEG never lands
    /// behind a 32 px avatar.
    pub fn load_image_into_picture(
        self: &Rc<Self>,
        picture: &gtk4::Picture,
        image_path: &std::path::Path,
        size: ThumbnailSize,
        token: u64,
        current: &Rc<Cell<u64>>,
        on_loaded: impl Fn(bool) + 'static,
    ) {
        if current.get() != token {
            return;
        }
        let key = format!("{}|{}", image_path.to_string_lossy(), size.pixels());
        if let Some(cached) = self.cache_get(&key) {
            picture.set_paintable(Some(&cached.texture));
            on_loaded(true);
            return;
        }
        let this = self.clone();
        let picture = picture.clone();
        let current = current.clone();
        let source = image_path.to_path_buf();
        glib::spawn_future_local(async move {
            let thumbnail_path = gio::spawn_blocking(move || {
                thumbnail(
                    &reprise_core::cover::CoverSource::FolderImage(source),
                    size,
                )
                .ok()
            })
            .await
            .ok()
            .flatten();
            if current.get() != token {
                return;
            }
            let Some(path) = thumbnail_path else {
                on_loaded(false);
                return;
            };
            match gdk::Texture::from_filename(&path) {
                Ok(texture) => {
                    picture.set_paintable(Some(&texture));
                    this.cache_put(key, CachedCover { texture, path });
                    on_loaded(true);
                }
                Err(_) => on_loaded(false),
            }
        });
    }
```

- [ ] **Step 5: Die Kette schreiben**

`crates/reprise-gnome/src/ui/stats/stats_artist_image.rs`:

```rust
//! One image chain for every band surface in My Stats (STATS-23): the cached
//! artist portrait first, the artist's most-played album cover while none is
//! there, and an initials tile when neither answers.
//!
//! The album cover paints as soon as it resolves and a portrait arriving from
//! the network replaces it — a card that waits for the network stays empty for
//! as long as the network takes.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::cover::ThumbnailSize;

use crate::ui::artist_portrait_worker::ArtistPortraitRuntime;
use crate::ui::cover_loader::CoverLoader;

pub(super) struct ArtistImageRequest {
    pub artist: String,
    pub candidates: Vec<String>,
    pub size: ThumbnailSize,
    pub token: u64,
    pub generation: Rc<Cell<u64>>,
    pub on_loaded: Rc<dyn Fn(bool)>,
}

/// The next album to try after `tried` failures, or `None` when the list is
/// spent.
pub(super) fn next_candidate(candidates: &[String], tried: usize) -> Option<&str> {
    candidates.get(tried).map(String::as_str)
}

#[derive(Clone)]
pub(super) struct StatsArtistImage {
    cover_loader: Rc<CoverLoader>,
    portrait: Rc<RefCell<Option<Rc<ArtistPortraitRuntime>>>>,
}

impl StatsArtistImage {
    pub(super) fn new(cover_loader: Rc<CoverLoader>) -> Rc<Self> {
        Rc::new(Self {
            cover_loader,
            portrait: Rc::new(RefCell::new(None)),
        })
    }

    pub(super) fn set_portrait_runtime(&self, runtime: Rc<ArtistPortraitRuntime>) {
        *self.portrait.borrow_mut() = Some(runtime);
    }

    pub(super) fn load(self: &Rc<Self>, picture: &gtk4::Picture, request: ArtistImageRequest) {
        let this = self.clone();
        let picture = picture.clone();
        glib::spawn_future_local(async move {
            let name = request.artist.clone();
            let cached = gio::spawn_blocking(move || {
                match reprise_core::artist_portrait::load_cached(&name) {
                    reprise_core::artist_portrait::PortraitOutcome::Found(path) => Some(path),
                    reprise_core::artist_portrait::PortraitOutcome::NotFound => None,
                }
            })
            .await
            .ok()
            .flatten();
            if request.generation.get() != request.token {
                return;
            }
            if let Some(path) = cached {
                this.show_portrait(&picture, &path, &request);
                return;
            }
            // Nothing cached: paint the album cover now, ask the network after.
            this.walk_candidates(&picture, &request, 0);
            this.fetch_portrait(&picture, &request);
        });
    }

    fn show_portrait(&self, picture: &gtk4::Picture, path: &PathBuf, request: &ArtistImageRequest) {
        let on_loaded = request.on_loaded.clone();
        let fallback = (self.clone(), picture.clone());
        let mirrored = mirror_request(request);
        self.cover_loader.load_image_into_picture(
            picture,
            path,
            request.size,
            request.token,
            &request.generation,
            move |loaded| {
                if loaded {
                    on_loaded(true);
                } else {
                    // A portrait file that will not decode is no reason to show
                    // nothing: fall through to the album covers.
                    let (this, picture) = &fallback;
                    this.walk_candidates(picture, &mirrored, 0);
                }
            },
        );
    }

    fn walk_candidates(
        self: &Rc<Self>,
        picture: &gtk4::Picture,
        request: &ArtistImageRequest,
        tried: usize,
    ) {
        let Some(candidate) = next_candidate(&request.candidates, tried) else {
            (request.on_loaded)(false);
            return;
        };
        let this = self.clone();
        let picture_for_retry = picture.clone();
        let mirrored = mirror_request(request);
        let on_loaded = request.on_loaded.clone();
        self.cover_loader.load_into_picture(
            picture,
            candidate,
            request.size,
            request.token,
            &request.generation,
            move |loaded| {
                if loaded {
                    on_loaded(true);
                } else {
                    this.walk_candidates(&picture_for_retry, &mirrored, tried + 1);
                }
            },
        );
    }

    fn fetch_portrait(self: &Rc<Self>, picture: &gtk4::Picture, request: &ArtistImageRequest) {
        let Some(runtime) = self.portrait.borrow().clone() else {
            return;
        };
        if !runtime.request_would_run(&request.artist) {
            return;
        }
        let this = self.clone();
        let picture = picture.clone();
        let mirrored = mirror_request(request);
        runtime.request(request.artist.clone(), move |found| {
            let Some(path) = found else {
                return;
            };
            if mirrored.generation.get() != mirrored.token {
                return;
            }
            this.show_portrait(&picture, &path, &mirrored);
        });
    }
}

/// A second handle on the same request — the callbacks above outlive the
/// original, and `ArtistImageRequest` is deliberately not `Clone` so nobody
/// copies one into a widget by accident.
fn mirror_request(request: &ArtistImageRequest) -> ArtistImageRequest {
    ArtistImageRequest {
        artist: request.artist.clone(),
        candidates: request.candidates.clone(),
        size: request.size,
        token: request.token,
        generation: request.generation.clone(),
        on_loaded: request.on_loaded.clone(),
    }
}

#[cfg(test)]
#[path = "stats_artist_image_tests.rs"]
mod tests;
```

In `crates/reprise-gnome/src/ui/stats/mod.rs` das Modul anmelden:

```rust
mod stats_artist_image;
```

- [ ] **Step 6: Tests laufen lassen**

```bash
cargo test -p reprise-gnome --bin reprise stats_23_ 2>&1 | tee $SCRATCH/t3.log | grep -E "^test result"
grep -c "^test result: FAILED" $SCRATCH/t3.log
```
Expected: `test result: ok.` mit 2 Tests, Zähler `0`.

- [ ] **Step 7: Commit**

```bash
git add crates/reprise-gnome/src/ui/cover/cover_loader.rs \
        crates/reprise-gnome/src/ui/now_playing/artist_portrait_worker.rs \
        crates/reprise-gnome/src/ui/stats/stats_artist_image.rs \
        crates/reprise-gnome/src/ui/stats/stats_artist_image_tests.rs \
        crates/reprise-gnome/src/ui/stats/mod.rs
git commit -m "feat: resolve band images portrait-first in My Stats (STATS-23)"
```

---

**→ Review Welle A:** `rust-reviewer` auf `git diff <basis>...HEAD` der drei
Tasks ansetzen, bevor T4 beginnt. Kritische Befunde sofort einarbeiten,
Kleinigkeiten sammeln.

---

### Task 4: Hero-Karte und Kacheln benutzen die Kette

**Files:**
- Modify: `crates/reprise-gnome/src/ui/stats/stats_band_card.rs:160-235`
- Modify: `crates/reprise-gnome/src/ui/stats/stats_band_tile.rs:170-200`
- Modify: `crates/reprise-gnome/src/ui/stats/stats_bands_row.rs:70-80`
- Modify: `crates/reprise-gnome/src/ui/stats/stats_view.rs:88-105`
- Modify: `crates/reprise-gnome/src/ui/window/window.rs:344`

**Interfaces:**
- Consumes: `StatsArtistImage`, `ArtistImageRequest` (T3); `RankedGroup::cover_candidates` (T1).
- Produces:
  - `StatsBandCard::set_artist_image(&self, image: Rc<StatsArtistImage>)`
  - `StatsBandTile::set_artist_image(&self, image: Rc<StatsArtistImage>)`
  - `StatsBandsRow::set_artist_image(&self, image: &Rc<StatsArtistImage>)`
  - `StatsView::set_portrait_runtime(&self, runtime: Rc<ArtistPortraitRuntime>)`

- [ ] **Step 1: Den fehlschlagenden Test schreiben**

In `crates/reprise-gnome/src/ui/stats/stats_band_card.rs`, im vorhandenen
`mod tests`:

```rust
/// STATS-23: the card asks for the artist by name and hands over every album
/// candidate — not just the first — so a coverless favourite cannot blank it.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_23_the_card_requests_the_artist_and_all_candidates() {
    gtk4::init().unwrap();
    let card = StatsBandCard::new();
    let mut section = section_fixture("Lorna Shore");
    section.artist.cover_candidates = vec![
        "/music/first.flac".to_string(),
        "/music/second.flac".to_string(),
    ];

    card.set_data(&section);

    assert_eq!(&*card.current_artist.borrow(), "Lorna Shore");
    assert_eq!(
        *card.current_candidates.borrow(),
        vec![
            "/music/first.flac".to_string(),
            "/music/second.flac".to_string()
        ]
    );
}
```

`section_fixture` ist der vorhandene Fixture-Helfer der Datei (siehe Zeile
~300); falls er anders heißt, den bestehenden Namen benutzen.

- [ ] **Step 2: Test laufen lassen, Fehlschlag bestätigen**

```bash
xvfb-run -a cargo test -p reprise-gnome --bin reprise stats_23_the_card_requests -- --ignored 2>&1 | grep -E "^error|^test result" | head
```
Expected: `no field `current_candidates``.

- [ ] **Step 3: Die Hero-Karte umstellen**

In `stats_band_card.rs` das Feld `cover_loader` durch die Kette ersetzen und
die Kandidaten mitführen:

```rust
    artist_image: Rc<RefCell<Option<Rc<StatsArtistImage>>>>,
    current_candidates: Rc<RefCell<Vec<String>>>,
```

`set_data` (Zeile 164) merkt sich die Kandidaten und ruft die Kette:

```rust
        *self.current_candidates.borrow_mut() = section.artist.cover_candidates.clone();
        self.load_image(&leader.label, &section.artist.cover_candidates);
```

`load_cover` wird zu `load_image`:

```rust
    fn load_image(&self, artist: &str, candidates: &[String]) {
        let token = self.cover_generation.get().wrapping_add(1);
        self.cover_generation.set(token);
        self.picture.set_paintable(gtk4::gdk::Paintable::NONE);
        let Some(image) = self.artist_image.borrow().clone() else {
            return;
        };
        let picture = self.picture.clone();
        let fallback = self.fallback.clone();
        let generation = self.cover_generation.clone();
        image.load(
            &self.picture,
            ArtistImageRequest {
                artist: artist.to_string(),
                candidates: candidates.to_vec(),
                size: ThumbnailSize::Portrait,
                token,
                generation: generation.clone(),
                on_loaded: Rc::new(move |loaded| {
                    if generation.get() != token {
                        return;
                    }
                    picture.set_visible(loaded);
                    fallback.set_visible(!loaded);
                }),
            },
        );
    }

    pub(in crate::ui) fn set_artist_image(&self, image: Rc<StatsArtistImage>) {
        *self.artist_image.borrow_mut() = Some(image);
    }
```

`clear_data` leert zusätzlich `current_candidates`.

- [ ] **Step 4: Die Kacheln gleich umstellen**

`stats_band_tile.rs` bekommt dieselben zwei Felder, dieselbe `load_image`
(mit `ThumbnailSize::Portrait`) und `set_artist_image`. `set_data` (Zeile 179)
ruft `self.load_image(&ranked.group.label, &ranked.cover_candidates)`.

- [ ] **Step 5: Durchreichen**

`stats_bands_row.rs`: `set_cover_loader` wird zu

```rust
    pub(in crate::ui) fn set_artist_image(&self, image: &Rc<StatsArtistImage>) {
        self.leader.set_artist_image(image.clone());
        for tile in &self.tiles {
            tile.set_artist_image(image.clone());
        }
    }
```

`stats_view.rs` (Zeile 88): die Kette einmal bauen und beides versorgen:

```rust
        let artist_image = StatsArtistImage::new(cover_loader.clone());
        bands_row.set_artist_image(&artist_image);
```

und die Runtime nachreichbar machen — die Testhelfer bauen `StatsView::new`
ohne Datenbankmodul, deshalb ein Setter statt eines neuen Konstruktorarguments:

```rust
    pub(in crate::ui) fn set_portrait_runtime(&self, runtime: Rc<ArtistPortraitRuntime>) {
        self.render.artist_image.set_portrait_runtime(runtime);
    }
```

`window.rs:344`: direkt hinter der Konstruktion

```rust
    stats_view.set_portrait_runtime(artist_portrait.clone());
```

Die Runtime entsteht bereits in Zeile 157, also vor dieser Stelle.

- [ ] **Step 6: Tests laufen lassen**

```bash
cargo test -p reprise-gnome --bin reprise stats_ 2>&1 | tee $SCRATCH/t4.log | grep -E "^test result"
xvfb-run -a cargo test -p reprise-gnome --bin reprise stats_ -- --ignored 2>&1 | tee -a $SCRATCH/t4.log | grep -E "^test result"
grep -c "^test result: FAILED" $SCRATCH/t4.log
```
Expected: Zähler `0` (bekannt rote Display-Tests vom Ausgangsstand abziehen).

- [ ] **Step 7: Commit**

```bash
git add crates/reprise-gnome/src/ui/stats crates/reprise-gnome/src/ui/window/window.rs
git commit -m "feat: band card and tiles show artist portraits (STATS-23)"
```

---

### Task 5: Die Bandkarte — Umschalter und Aufklapper

**Files:**
- Create: `crates/reprise-gnome/src/ui/stats/stats_bands_card.rs`
- Create: `crates/reprise-gnome/src/ui/stats/stats_bands_more.rs`
- Create: `crates/reprise-gnome/src/ui/stats/stats_bands_card_tests.rs`
- Modify: `crates/reprise-gnome/src/ui/stats/stats_bands_row.rs:85-100` (`set_data`)
- Modify: `crates/reprise-gnome/src/ui/stats/stats_view.rs:105-120` (Komposition), `:390-400` (Abschnittserkennung)
- Modify: `crates/reprise-gnome/src/ui/stats/stats_css.rs:217-222`

**Interfaces:**
- Consumes: `top_artists_sorted`, `artist_share_percent` (T2); `StatsArtistImage` (T3);
  `StatsBandsRow::set_data` (T4).
- Produces:
  - `StatsBandsCard::new(artist_image: Rc<StatsArtistImage>) -> Self`
  - `StatsBandsCard::set_data(&self, snapshot: &StatsSnapshot)`
  - `StatsBandsCard::widget(&self) -> &gtk4::Box`
  - `StatsBandsCard::set_on_open_artist(&self, callback: impl Fn(String) + 'static)`
  - `const ARTIST_ROW_EXTRA: usize = 15`
  - `StatsBandsRow::set_data(&self, artists: &[RankedGroup], share_percent: i64, sort_by: SortBy)`

- [ ] **Step 1: Den fehlschlagenden Test schreiben**

`crates/reprise-gnome/src/ui/stats/stats_bands_card_tests.rs`:

```rust
use super::*;

/// The continuation continues the ranking: five surfaces on screen, fifteen
/// more behind the button, no rank shown twice.
#[test]
fn stats_23_the_continuation_starts_at_rank_six() {
    assert_eq!(RUNNER_UP_COUNT, 4);
    assert_eq!(ARTIST_ROW_EXTRA, 15);
    assert_eq!(first_continuation_rank(), 6);
}

/// The button is offered only when it would open onto something.
#[test]
fn stats_23_the_expander_is_offered_only_past_the_five_on_screen() {
    assert!(!has_continuation(5));
    assert!(has_continuation(6));
    assert!(has_continuation(150));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_23_the_toggle_reorders_the_whole_row() {
    gtk4::init().unwrap();
    let (card, snapshot) = card_and_snapshot();

    card.set_data(&snapshot);
    let by_time = card.leader_label();
    card.sort_toggle.set_active_name(Some("plays"));
    let by_plays = card.leader_label();

    assert_eq!(by_time, "Marathon");
    assert_eq!(by_plays, "Sprinter");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_23_show_more_reveals_the_continuation_rows() {
    gtk4::init().unwrap();
    let (card, snapshot) = card_and_snapshot_with(9);

    card.set_data(&snapshot);
    assert!(!card.revealer.reveals_child());
    assert_eq!(card.reveal_button.label().unwrap(), "Show more top artists");

    card.reveal_button.emit_clicked();

    assert!(card.revealer.reveals_child());
    assert_eq!(card.reveal_button.label().unwrap(), "Hide more top artists");
    assert_eq!(card.continuation_rows(), 4, "ranks 6 to 9");
}
```

Die Helfer `card_and_snapshot` / `card_and_snapshot_with(artists)` bauen — wie
`stats_songs_card_tests.rs:33` — eine Testdatenbank über `crate::test_db::open()`,
schreiben je Interpret Tracks und Abspielungen und rufen
`reprise_core::library::stats_snapshot::compute`. Für den Umschaltertest zwei
Interpreten anlegen: „Sprinter" mit sechs kurzen Abspielungen (60 s),
„Marathon" mit zwei langen (600 s).

- [ ] **Step 2: Test laufen lassen, Fehlschlag bestätigen**

```bash
cargo test -p reprise-gnome --bin reprise stats_23_the_continuation 2>&1 | grep -E "^error|^test result" | head
```
Expected: `cannot find function `first_continuation_rank``.

- [ ] **Step 3: Die Fortsetzungszeilen bauen**

`stats_bands_more.rs`:

```rust
//! The continuation of the bands ranking (STATS-23): ranks 6 and up in two
//! columns, each row a focusable button that opens its artist.

use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::format::format_thousands;
use reprise_core::library::stats_screen::RankedGroup;
use reprise_core::library::stats_snapshot::SortBy;

use super::stats_artist_image::{ArtistImageRequest, StatsArtistImage};

/// Avatar edge in a continuation row. Big enough to recognise a band, small
/// enough that fifteen rows stay a list rather than a second tile grid.
const AVATAR_SIZE: i32 = 32;

pub(super) struct ContinuationRow {
    pub(super) root: gtk4::Button,
    pub(super) avatar: adw::Avatar,
    pub(super) bar: gtk4::LevelBar,
}

pub(super) fn build_row(
    rank: usize,
    artist: &RankedGroup,
    leader_metric: i64,
    sort_by: SortBy,
    image: &Rc<StatsArtistImage>,
    generation: &Rc<std::cell::Cell<u64>>,
    on_open_artist: Rc<dyn Fn(String)>,
) -> ContinuationRow {
    // A button, not a box with a gesture: the row inherits focus, Enter/Space
    // and the platform's pressed state, exactly like the tiles above it.
    let root = gtk4::Button::new();
    root.add_css_class("flat");
    root.add_css_class("stats-artist-row");
    crate::ui::style::buttons::arm_cursor(&root);

    let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    let rank_label = gtk4::Label::new(Some(&rank.to_string()));
    rank_label.add_css_class("stats-ghost-rank");
    rank_label.set_size_request(24, -1);
    rank_label.set_xalign(1.0);
    line.append(&rank_label);

    let avatar = adw::Avatar::new(AVATAR_SIZE, Some(&artist.group.label), true);
    line.append(&avatar);

    let name = gtk4::Label::new(Some(&artist.group.label));
    name.add_css_class("stats-item-title");
    name.set_xalign(0.0);
    name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    name.set_hexpand(true);
    line.append(&name);

    let bar = gtk4::LevelBar::new();
    bar.add_css_class("stats-song-bar");
    bar.set_min_value(0.0);
    bar.set_max_value(1.0);
    let metric = match sort_by {
        SortBy::Plays => artist.group.plays,
        SortBy::Time => artist.group.ms,
    };
    bar.set_value(share_of_leader(metric, leader_metric));
    bar.set_size_request(90, -1);
    line.append(&bar);

    let value = gtk4::Label::new(Some(&match sort_by {
        SortBy::Plays => format!("{} plays", format_thousands(artist.group.plays)),
        // The same compact duration the tiles above use (STATS-11).
        SortBy::Time => crate::ui::strings::stats_duration(artist.group.ms),
    }));
    value.add_css_class("stats-ghost-rank");
    value.set_xalign(1.0);
    line.append(&value);

    root.set_child(Some(&line));
    let label = artist.group.label.clone();
    root.connect_clicked(move |_| on_open_artist(label.clone()));

    let token = generation.get();
    let picture = gtk4::Picture::new();
    // The avatar takes a paintable, so the picture is only the sink the loader
    // knows how to fill; it never joins the widget tree.
    let avatar_for_image = avatar.clone();
    let picture_for_image = picture.clone();
    image.load(
        &picture,
        ArtistImageRequest {
            artist: artist.group.label.clone(),
            candidates: artist.cover_candidates.clone(),
            size: reprise_core::cover::ThumbnailSize::List,
            token,
            generation: generation.clone(),
            on_loaded: Rc::new(move |loaded| {
                avatar_for_image.set_custom_image(
                    loaded
                        .then(|| picture_for_image.paintable())
                        .flatten()
                        .as_ref(),
                );
            }),
        },
    );

    ContinuationRow { root, avatar, bar }
}

/// A bar relative to rank 1, clamped so a zero leader cannot produce NaN.
fn share_of_leader(metric: i64, leader: i64) -> f64 {
    if leader <= 0 {
        return 0.0;
    }
    (metric as f64 / leader as f64).clamp(0.0, 1.0)
}
```

`strings::stats_duration` ist dieselbe Funktion, die `stats_band_tile.rs:170`
für seine Zeile benutzt — die Zeilen der Fortsetzung dürfen kein zweites
Dauerformat einführen.

- [ ] **Step 4: Die Karte bauen**

`stats_bands_card.rs` — Aufbau eins zu eins nach `stats_songs_card.rs:95-205`:

```rust
//! The bands ranking as one card (STATS-23): the 2:1:1:1:1 row, a
//! "by plays / by time" toggle that orders the whole card, and a continuation
//! that grows the same card instead of opening a second one.

/// Ranks the expander adds below the five on screen — the songs card offers
/// fifteen past its ten, and the two sections read as one page.
pub(super) const ARTIST_ROW_EXTRA: usize = 15;

/// The first rank the continuation shows: the hero plus four tiles are already
/// on screen.
pub(super) fn first_continuation_rank() -> usize {
    RUNNER_UP_COUNT + 2
}

/// Whether the expander would open onto anything.
pub(super) fn has_continuation(artists: usize) -> bool {
    artists > RUNNER_UP_COUNT + 1
}
```

Der Rest der Datei:

1. `root`: `gtk4::Box` (vertikal, Abstand 12), CSS-Klasse `stats-bands-card`.
2. `header`: horizontaler Kasten, links ein leerer `hexpand`-Platzhalter (der
   Kicker steht im Hero-Bild), rechts die `adw::ToggleGroup` mit
   `adw::Toggle` `plays` / `time`, `set_active_name(Some("time"))` — die Reihe
   ist heute nach Hörzeit geordnet, also startet der Umschalter dort;
   `update_property(&[gtk4::accessible::Property::Label("Sort top artists")])`.
3. `bands_row.widget()`.
4. `reveal_button`: `gtk4::Button::with_label("Show more top artists")`,
   Klassen `flat` und `stats-songs-reveal`, `halign = Start`.
5. `revealer`: `gtk4::Revealer` mit einem zweispaltigen `gtk4::Box`
   (horizontal, homogen, Abstand 24, je Spalte ein vertikaler Kasten mit
   Abstand 2), `set_visible(false)` und derselbe
   `connect_child_revealed_notify`-Kniff wie in `stats_songs_card.rs:153`.

`set_data`:

```rust
    pub(in crate::ui) fn set_data(&self, snapshot: &StatsSnapshot) {
        *self.snapshot.borrow_mut() = Some(snapshot.clone());
        self.render();
    }

    fn render(&self) {
        let Some(snapshot) = self.snapshot.borrow().clone() else {
            return;
        };
        let sort_by = self.sort_by.get();
        let artists = snapshot.top_artists_sorted(sort_by);
        let share = artists
            .first()
            .map_or(0, |leader| snapshot.artist_share_percent(leader));
        self.bands_row.set_data(&artists, share, sort_by);

        let offer = has_continuation(artists.len());
        self.reveal_button.set_visible(offer);
        if !offer {
            self.revealer.set_reveal_child(false);
            self.reveal_button.set_label("Show more top artists");
        }
        self.render_continuation(&artists, sort_by);
    }
```

`render_continuation` leert beide Spalten, nimmt
`artists.iter().skip(RUNNER_UP_COUNT + 1).take(ARTIST_ROW_EXTRA)`, baut je
Eintrag eine Zeile über `stats_bands_more::build_row` (Rang =
`index + first_continuation_rank()`, `leader_metric` aus `artists[0]`) und hängt
die erste Hälfte in Spalte 0, die zweite in Spalte 1 — dieselbe Aufteilung, die
`stats_songs_card.rs` für seine zehn benutzt.

`connect_clicked` des Knopfes und `connect_active_name_notify` des Umschalters
folgen exakt `stats_songs_card.rs:180-224`; der Umschalter setzt `sort_by` und
ruft `render()`.

- [ ] **Step 5: Die Reihe auf die sortierte Liste umstellen**

`stats_bands_row.rs`, `set_data` bekommt die neue Signatur:

```rust
    pub(in crate::ui) fn set_data(
        &self,
        artists: &[RankedGroup],
        share_percent: i64,
        sort_by: SortBy,
    ) {
        let Some(leader) = artists.first() else {
            self.clear_data();
            return;
        };
        self.leader.set_data(leader, share_percent, sort_by);
        let leader_metric = match sort_by {
            SortBy::Plays => leader.group.plays,
            SortBy::Time => leader.group.ms,
        };
        for (index, tile) in self.tiles.iter().enumerate() {
            match artists.get(index + 1) {
                Some(ranked) => tile.set_data(index + 2, ranked, leader_metric),
                None => tile.clear_data(),
            }
        }
    }
```

`StatsBandCard::set_data` nimmt entsprechend `&RankedGroup`, den Anteil und die
Metrik statt `&SpotlightSection`; die drei Textzeilen der Karte bleiben, nur die
Prozentzahl kommt jetzt als Argument. Der Unify-Hinweis liest weiter
`ranked.group.variant_count`.

Damit ändert sich der Vertrag, den die Tests aus T4 aufrufen: der Fixture-Helfer
in `stats_band_card.rs` baut künftig einen `RankedGroup` statt einer
`SpotlightSection`, und `stats_23_the_card_requests_the_artist_and_all_candidates`
ruft `card.set_data(&ranked, 11, SortBy::Time)`. Diese Anpassung gehört in
diesen Commit — ein Test, den der eigene Umbau brechen lässt, ist kein Befund,
sondern eine offene Baustelle.

- [ ] **Step 6: In die Ansicht einhängen**

`stats_view.rs`: `StatsBandsRow` wird von `StatsBandsCard` umschlossen. In
`SECTION_ORDER` bleibt `"bands"`; die Abschnittserkennung (Zeile ~390)
vergleicht künftig gegen `bands_card.widget()` statt gegen
`bands_row.widget()`. Die Aufrufstelle von `set_data` in `refresh` reicht den
ganzen `snapshot` an `bands_card.set_data(&snapshot)`.

- [ ] **Step 7: Stil ergänzen**

`stats_css.rs`, hinter `.stats-top-track-row`:

```rust
         .stats-artist-row {{ \
           padding: 5px; \
           transition: background-color {transition}; }}\n\
         .stats-artist-row:hover {{ \
           background-color: alpha(currentColor, {hover_alpha}); }}\n\
         .stats-artist-row:focus-visible {{ outline: 2px solid @accent_color; }}\n\
         .stats-bands-card {{ padding: 8px; }}
```

- [ ] **Step 8: Tests laufen lassen**

```bash
cargo test -p reprise-gnome --bin reprise stats_ 2>&1 | tee $SCRATCH/t5.log | grep -E "^test result"
xvfb-run -a cargo test -p reprise-gnome --bin reprise stats_ -- --ignored 2>&1 | tee -a $SCRATCH/t5.log | grep -E "^test result"
grep -c "^test result: FAILED" $SCRATCH/t5.log
```
Expected: Zähler `0` gegenüber dem Ausgangsstand.

- [ ] **Step 9: Commit**

```bash
git add crates/reprise-gnome/src/ui/stats
git commit -m "feat: sort and expand the My Stats bands ranking (STATS-23)"
```

---

**→ Review Welle B:** `rust-reviewer` auf den Diff von T4 und T5.

---

### Task 6: Abnahme

Grüne Tests beweisen keine Oberfläche. Diese Aufgabe belegt sie.

**Files:**
- Keine Produktionsänderung; nur Belege und, falls nötig, Nachbesserungen.

- [ ] **Step 1: Gates vollständig laufen lassen**

```bash
scripts/check-ux-traceability.sh; echo "traceability=$?"
cargo clippy --workspace --all-targets 2>&1 | tee $SCRATCH/clippy.log | grep -cE "^error"
cargo test --workspace 2>&1 | tee $SCRATCH/all.log | grep -cE "^test result: FAILED"
```
Expected: `traceability=0`, Clippy-Zähler `0`, Testzähler auf dem Stand von
`dev` (bekannt rote Display-Tests bleiben rot und sind nicht Sache dieser
Aufgabe — belege den Vergleich).

- [ ] **Step 2: Die Seite ansehen**

Die Anwendung headless starten und *My Stats* aufnehmen — nach dem Verfahren
in `docs/` zur Screenshot-Abnahme (Xvfb + Fensterverwalter + `import`, niemals
ein Fenster auf dem echten Desktop). Belegt werden müssen:

1. Die fünf Bandflächen tragen Bilder, nicht den Platzhalter.
2. Der Umschalter steht rechts über der Reihe und ordnet sie um.
3. `Show more top artists` öffnet fünfzehn Zeilen in zwei Spalten mit runden
   Porträts.

- [ ] **Step 3: Ergebnis festhalten**

Screenshot-Pfade und die drei Beobachtungen in die Abschlussmeldung schreiben.
Weicht das Bild vom Entwurf ab (fehlende Porträts, gequetschte Zeilen), ist das
ein Befund für eine Nachbesserung, keine Abnahme.

---

## Selbstprüfung des Plans

**Abdeckung gegen die Spezifikation**

| Spezifikation | Aufgabe |
|---|---|
| Bildkette Porträt → Album → Initialen | T3 (Kette), T4 (Hero/Kacheln), T5 (Zeilen) |
| Nachladen nur bei aktivem Modul, Deckel | T3 |
| Porträts durch die Thumbnail-Pipeline | T3 (`load_image_into_picture`) |
| Repräsentant = meistgehörtes Album | T1 |
| Bis zu drei Cover-Kandidaten | T1, benutzt in T3/T4/T5 |
| `top_artists_sorted`, Anteil je Spitzenreiter | T2 |
| Umschalter ordnet die ganze Reihe | T5 |
| Aufklapper Rang 6–20, zweispaltig, rundes Porträt | T5 |
| Zeilen öffnen den Interpreten, Fokus/Hover | T5 (`build_row`, CSS) |
| STATS-23 ersetzt STATS-13, Tests umbenannt | T0 |
| Genre-Karte erbt nur die bessere Albumwahl | T1 (ohne eigene Aufgabe — beabsichtigt) |

**Ein Prüfpunkt bleibt am Code** — nur dort entscheidbar, nicht am Schreibtisch:
ob `adw::Avatar::set_custom_image` in der gepinnten libadwaita-Fassung ein
`Option<&impl IsA<gdk::Paintable>>` nimmt. Weicht die Signatur ab, das
Texture-Handle direkt setzen statt über `gtk4::Picture::paintable` — die
Bildkette bleibt davon unberührt.
