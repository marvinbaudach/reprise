---
slug: mystats-optimization
worktree: /home/marvin/Projects/reprise/.worktrees/mystats-optimization
branch: feat/mystats-optimization
phase: shipped
codex_session:
created: 2026-07-19
---
# My Stats (Frame 25a) — editorial rebuild — implementation plan

Language note: repo convention (AGENTS.md) puts design docs in German. This plan
is written in English by explicit request; **all `docs/ux-rules.md` rule texts in
section 6 stay German verbatim**, and all code/tests/comments are English as usual.

Goal: rebuild the existing "My Stats" screen into the editorial Frame-25a layout,
on one consistent play definition and one consistent grouping key, entirely
local, and flip STATS-0..9 from `[geplant]` to `[aktiv]` in the commits that
finish them.

This is the **final** plan. Every decision below was grilled and is binding; a
headless implementer must not re-open any of them and must not ask questions.
Anything not written here is out of scope.

---

## 1. Current state (inventory)

### Core (`crates/reprise-core/src/library/`)

* `stats_screen.rs` (582 lines) — exists and is wired. Provides
  `record_listen_event`, `monthly_listen_timeseries` (hard-coded 12 rolling
  months, UTC), `headline_totals`, `top_artists`, `top_albums`, `top_tracks`,
  `top_genres`, `listening_by_hour`, `distinct_artists_played`,
  `most_active_weekday`, `available_years`.
  **Problem:** two incompatible data sources are mixed. `headline_totals`,
  `top_*` and `distinct_artists_played` aggregate `tracks.play_count` /
  `play_count * duration_ms`; only `monthly_listen_timeseries`,
  `listening_by_hour` and `most_active_weekday` read `listen_events`.
  `play_count` is incremented by the *looser* `stats::should_count_play`
  predicate, `listen_events` rows are written under the *stricter*
  `scrobbling::should_scrobble` (≥ 50 % or ≥ 4 min). That mismatch is exactly
  the "5 h · 1.131 plays" inconsistency STATS-0 kills.
  Also: the period model is `Option<i32> year`, and year filtering on `tracks`
  uses `last_played_at` — i.e. an all-time `play_count` is attributed wholesale
  to the year of the *last* play. Wrong by construction.
* `stats_screen_tests.rs` (573 lines) — largely rewritten with the functions it
  covers.
* `stats.rs` — `should_count_play` / `record_play` (the `tracks.play_count`
  path). **Unchanged**: play counts stay a library concept, they just stop
  feeding the stats screen.
* `remote_stats.rs` (35 lines) + the `listenbrainz.rs` fetch path — remote stats
  types. Not referenced by any UI code today (only by `listenbrainz.rs` itself).
* `settings.rs` (666 lines) — `get_setting`/`set_setting`/`get_bool`/`set_bool`
  plus typed accessors: the pattern to copy for the Customize state.
* `playlists.rs` (547 lines) — `list_smart`, `smart_rules_to_sql` (allowed
  fields include `genre`). There is **no** create function for smart playlists;
  the seeded "Top rated" row is created in the v3 migration SQL. Needed for the
  STATS-4 "Create" CTA.
* `db.rs` — schema at `user_version` **15** (verified: `SCHEMA_V15` is the last
  const, the last migration block writes `user_version = 15`).
  `listen_events(id, track_id, played_at, ms_played)` with
  `idx_listen_events_played_at` (played_at only). `tracks` has
  `genre TEXT NOT NULL DEFAULT ''` and `rating INTEGER NOT NULL DEFAULT 0`.
* `scrobbling.rs:271` — `should_scrobble(position_ms, duration_ms)`, the
  ≥ 50 % OR ≥ 4 min predicate, with edge tests at both thresholds.

### Grouping keys today: three incompatible ones (the STATS-9 problem)

Verified by inspection — this is the second consistency defect in the screen,
independent of the play definition:

| Site | Key |
|---|---|
| `queries/library_views.rs:13` `EFFECTIVE_ALBUM_ARTIST` | `CASE WHEN TRIM(album_artist) <> '' THEN TRIM(album_artist) ELSE TRIM(artist) END`, grouped as `LOWER(…)` |
| `library/stats_screen.rs:218` `top_artists` | bare `GROUP BY artist` — no `TRIM`, no `LOWER`, no album-artist fallback |
| `library/stats_screen.rs:283` `top_albums` | `CASE WHEN album_artist <> '' …` — the fallback but **without** `TRIM`, a third key |
| `library/stats_screen.rs:488` `distinct_artists_played` | `COUNT(DISTINCT artist)`, raw |
| `artist_news.rs:268` `artists_for_fetch` | `GROUP BY lower(trim(artist))` — artist column only |

So "Lorna Shore" / "lorna shore" / `"Lorna Shore "` already split into three rows
in Top Artists while collapsing into one row in the Artists view. Genres are
worse: `top_genres` groups the bare `tracks.genre` column.

* **`tracks.artist_mbid` exists** — `db.rs:305` (`SCHEMA_V12`), with
  `artist_mbid_negative` and `idx_tracks_artist_mbid`. Written **only** by the
  opt-in Artist News network fetch (`artist_news.rs:345`,
  `UPDATE tracks SET artist_mbid = ?1 … WHERE lower(trim(artist)) = lower(trim(?3))`).
  Two consequences that the design must respect: it is **sparsely populated**
  (network feature, off by default), and it is keyed to the raw **`artist`**
  column — it is *not* an album-artist MBID. There is **no** album-artist MBID
  and **no** genre MBID column anywhere in the schema.
* **No multi-artist / split-artist handling exists anywhere.** No code splits an
  artist string on `;`, `/`, `&` or `feat.`; `scanner_meta.rs:105` stores lofty's
  value verbatim. The Artists view's collapse of compilations works purely
  through the `album_artist` tag, not through string parsing.
* **The multi-select tag editor is shipped and merged** (~7 845 lines under
  `ui/tag_edit/` + `library/tag_edit*.rs`). Entry point:
  `TrackList::edit_tags_for_ids(&[i64])` (`track_list/track_list.rs:457`,
  `pub(in crate::ui)`) → `tag_edit_flow::begin_for_ids`. Multi-select is its
  primary axis (`EditorMode::is_multi`, `SessionMode::Multi`). The
  `feat/tag-editor-rework` branch referenced in `docs/` is **stale** — it no
  longer exists (`git branch -a --list "*tag*"` is empty) and is declared merged
  in `docs/plans/ux-rules-motion.md:255`. There is no in-flight rework to
  collide with.

### GTK (`crates/reprise-gnome/src/ui/stats/`)

* `stats_view.rs` (601 lines) — a 680 px `AdwClamp` column: headline label +
  year `DropDown`, a bar-chart card, and three plain "TOP …" boxes built from
  `list_row_with_cover`. Contains three `#[allow(dead_code)]` leftovers
  (`genre_row`, `progress_bar`, `album_strip_item`). No spotlight, no ribbon,
  no genre spectrum, no highlights, no breakpoint, no customize menu, no empty
  state beyond a `"No listening data yet"` label.
* `stats_chart.rs` (169) — cairo bar chart, 12 buckets, month labels. Replaced
  by the ribbon.
* `stats_chart_math.rs` (147) — pure normalization/labels; complemented by the
  new `stats_ribbon_math.rs`.
* `hourly_chart.rs` (179) — 24-bar cairo clock, already close to STATS-4's
  Listening Clock. `#[allow(dead_code)]` in `mod.rs`: **not currently used**.
* `stats_css.rs` (86) — token-driven CSS, `.stats-*` classes.
* Wiring: `window/window.rs:371-383` constructs `StatsView`, calls
  `wire_year_selector`, adds it to `content_stack` as `"stats"`;
  `window/library_shell.rs:296` refreshes on route to `ViewSource::MyStats`.
  `browse/filter_restriction.rs:16` and `track_list_sort.rs` already exclude
  `MyStats` from the filter row and sorting (STATS-8 is largely satisfied — it
  needs a rule-named test, not new code).
* `playback/play_tracking.rs:88-95` — the single writer of `listen_events`,
  gated on `scrobbling::should_scrobble(max_position_ms, duration_ms)`.

### Verdict: rebuild vs. new

| Area | Action |
|---|---|
| `stats_screen.rs` aggregations | **Rewritten** onto `listen_events` only, new period model |
| `stats_screen.rs` `record_listen_event` | **Kept** as is |
| `stats.rs` play-count path | **Untouched** |
| `stats_chart.rs` (12-month bars) | **Replaced** by `stats_ribbon.rs` (area ribbon) |
| `hourly_chart.rs` | **Reused**, un-`allow(dead_code)`d, peak highlighting added |
| `stats_view.rs` | **Rebuilt** as a thin composer; sections extracted to siblings |
| `stats_css.rs` | **Extended** |
| `remote_stats.rs` / `listenbrainz.rs` fetch | **Untouched, explicitly out of scope** (D6) |
| Grouping key for artist / album-artist / genre | **New** `library/group_key.rs`, one shared home (D19) |
| `queries/library_views.rs` `EFFECTIVE_ALBUM_ARTIST` | **Untouched, but adopted** as the stats input expression (D19) |
| Tag editor | **Untouched**; stats only calls `edit_tags_for_ids` (D21) |
| DB schema | One additive index migration (D5) — no MBID migration (D19) |

---

## 2. Decisions (binding — no options left open)

**D1 — Play definition, and why no migration is needed.**
`listen_events` rows are *already* only written when
`scrobbling::should_scrobble(max_position_ms, duration_ms)` holds
(`play_tracking.rs:90`), which is exactly "≥ 50 % OR ≥ 4 min". So the ≥ 50 %
criterion is enforced at **write** time and needs neither a new column nor a
backfill. Therefore: **the stats screen reads `listen_events` and nothing else.**
Every aggregate (hero time, plays, top lists, spotlight, genres, clock,
highlights) is a `listen_events` ⋈ `tracks` query. `tracks.play_count` and
`tracks.last_played_at` disappear from `stats_screen.rs` entirely. Time and
count are then two projections of the same row set and cannot disagree.
Formalize the predicate: add
`pub fn counts_as_play(position_ms: i64, duration_ms: i64) -> bool` to
`stats_screen.rs` delegating to `scrobbling::should_scrobble`, and make
`play_tracking.rs` call `stats_screen::counts_as_play` instead of reaching into
`scrobbling` — one named home for the definition.
Hero time uses `MIN(le.ms_played, t.duration_ms)` (clamped) so a bogus
overshooting position cannot inflate hours; `duration_ms <= 0` falls back to raw
`ms_played`.

**D2 — Period model.** Replace `Option<i32> year` with an explicit enum in a new
`library/stats_period.rs`:

```rust
pub enum StatsPeriod { YearToDate(i32), Year(i32), Last30Days, AllTime }

pub struct PeriodRange {
    pub start_unix: i64,
    pub end_unix: i64, // exclusive
    pub granularity: Granularity,
    pub buckets: Vec<Bucket>,
}

pub enum Granularity { Day, Week, Month }

pub struct Bucket {
    pub label: String,
    pub start_unix: i64,
    pub end_unix: i64, // exclusive
    pub open: bool,    // still running (current month / week / day)
}
```

Resolution signature (generic over the time zone — see D3):

```rust
impl StatsPeriod {
    pub fn resolve<Tz: chrono::TimeZone>(
        self,
        now_unix: i64,
        tz: &Tz,
        first_event_unix: Option<i64>,
    ) -> PeriodRange;

    /// Dropdown contents, in display order.
    pub fn available<Tz: chrono::TimeZone>(
        conn: &Connection,
        now_year: i32,
        tz: &Tz,
    ) -> Result<Vec<StatsPeriod>, rusqlite::Error>;
}
```

`AllTime` starts at the first recorded event; with `first_event_unix == None` it
resolves to an empty bucket vector (see "Empty / sparse" in section 7).
Dropdown order is `"<Y> so far"`, then every older local calendar year that
contains at least one `listen_event` (newest first), followed by `"All time" /
"Last 30 days"`; default = `YearToDate(current_year)`. `available` always offers
the current year even on an empty DB. Untimestamped imported
`tracks.play_count` values never create a selectable year.
The bucket list is produced **in core**, not in the widget — that is what makes
`stats_1_ribbon_axis_matches_period` a `[core]` test.

**D3 — Timezone: a `chrono::TimeZone` parameter, bucketing in Rust.**
No offset snapshot is passed into SQL, and SQLite never does `strftime`
bucketing. SQL returns **raw Unix timestamps**; day, hour, week and month
bucketing happens in Rust through a caller-supplied
`Tz: chrono::TimeZone` parameter that is threaded through `resolve` and
`compute`:

* GTK passes `&chrono::Local` — correct across DST, because every event is
  mapped through the zone individually rather than through one offset snapshot.
* Core tests pass `&chrono::Utc` — fully deterministic, no environment
  dependency inside `reprise-core` (see the headless/determinism rule).
* The DST regression test passes a test-local zone (see D3a).

`chrono = { version = "0.4", default-features = false, features = ["clock", …] }`
is **already a dependency of both crates** (`reprise-core/Cargo.toml:48`,
`reprise-gnome/Cargo.toml:34`, chrono 0.4.45 in `Cargo.lock`). No new dependency.

Rationale, and why the cheaper offset snapshot was rejected: the **streak** is
the number users take emotionally seriously. A streak that tears twice a year
because a single offset was snapshotted at refresh time and then applied to the
whole history costs trust in the entire view — and it is the kind of bug nobody
reports as a bug, they just stop believing the screen. The cost of correctness
here is one generic parameter.

Helper, in `stats_period.rs`, used by every day/hour derivation:

```rust
/// Local calendar day and local hour of a Unix timestamp in `tz`.
/// `None` only for timestamps outside chrono's representable range, which
/// `played_at` (seconds, written by `now_unix()`) cannot reach. Callers skip
/// such rows instead of panicking — core never panics on stored data.
pub fn local_parts<Tz: chrono::TimeZone>(tz: &Tz, unix: i64) -> Option<(chrono::NaiveDate, u32)>;
```

Implement it via `tz.timestamp_opt(unix, 0).earliest()`, then `.date_naive()`
and `.hour()`. `earliest()` resolves the (UTC → local) mapping, which is
single-valued for every in-range instant.

**D3a — DST regression test zone.** `chrono-tz` is **not** a dependency and must
not become one. The DST test defines its own zone in the test file:

```rust
/// Test-only zone: +01:00 before 2026-03-29T01:00:00Z, +02:00 from then on —
/// the European spring-forward instant, without pulling in chrono-tz.
/// chrono 0.4.45 requires these five methods.
#[derive(Clone, Copy)]
struct DstZone;

impl DstZone {
    /// 2026-03-29T01:00:00Z — the last Sunday of March 2026, 01:00 UTC.
    const SWITCH_UNIX: i64 = 1_774_746_000;
    fn offset_at(utc: &NaiveDateTime) -> FixedOffset {
        let secs = if utc.and_utc().timestamp() < Self::SWITCH_UNIX { 3600 } else { 7200 };
        FixedOffset::east_opt(secs).expect("valid fixed offset")
    }
}

impl TimeZone for DstZone {
    type Offset = FixedOffset;
    fn from_offset(_: &FixedOffset) -> Self { DstZone }
    fn offset_from_local_date(&self, d: &NaiveDate) -> MappedLocalTime<FixedOffset> { /* Single(offset_at(midnight)) */ }
    fn offset_from_local_datetime(&self, dt: &NaiveDateTime) -> MappedLocalTime<FixedOffset> { /* Single(offset_at(dt)) */ }
    fn offset_from_utc_date(&self, d: &NaiveDate) -> FixedOffset { /* offset_at(midnight) */ }
    fn offset_from_utc_datetime(&self, dt: &NaiveDateTime) -> FixedOffset { /* offset_at(dt) */ }
}
```

**D4 — No cache, no rollup table: direct queries.**
The spec wording is "Aggregationen materialisiert/gecacht". That is overruled by
the project's KISS/YAGNI rule, on the numbers: the real library is **100
`listen_events` rows across 1688 tracks**. Ten years of heavy listening —
50 plays a day, every day — is ~180 000 rows. With D5's index that is a
millisecond-range scan in SQLite, several orders of magnitude below the frame
budget of opening a view.

Therefore, explicitly **not built**: no LRU cache, no `StatsCache`, no
`Watermark` type, no watermark invalidation, no rollup table, no triggers, no
backfill migration. `library/stats_snapshot.rs` exists only as the composition
layer:

```rust
pub struct StatsSnapshot { /* hero, ribbon, spotlight, genres, clock,
                             highlights, top_tracks, flags */ }

pub fn compute<Tz: chrono::TimeZone>(
    conn: &Connection,
    period: StatsPeriod,
    now_unix: i64,
    tz: &Tz,
) -> Result<StatsSnapshot, rusqlite::Error>;

impl StatsSnapshot { pub fn is_empty(&self) -> bool; }
```

`compute` runs eight statements per refresh and returns a plain owned value —
no `Rc`, no `RefCell`, no interior state.

**Reversibility note (deliberate, keep it in mind while writing `compute`):** if
profiling ever shows real latency, a cache drops in *behind* this signature as a
pure intermediate layer — a `get_or_compute(key) -> StatsSnapshot` wrapper in
front of `compute`, with `(period, now-bucket)` as the key. Nothing in the
snapshot type, the view, or the tests changes. That is why `compute` must stay a
**pure function of `(conn, period, now_unix, tz)`** with no hidden state and no
side effects: keeping it pure is what keeps the cache a one-file addition later.

**D5 — Schema v17 (additive, index only).**

```sql
-- Schema v17: index the listen_events → tracks join direction. Every My Stats
-- aggregate joins listen_events to tracks and filters on played_at; the v7
-- index covers played_at alone.
CREATE INDEX IF NOT EXISTS idx_listen_events_track_played
  ON listen_events(track_id, played_at);
```

Add `SCHEMA_V17` next to the existing consts and the `if version < 17 { … }`
block at the end of the migration chain, matching the existing shape exactly
(`tx.execute_batch(SCHEMA_V17)?;` then `tx.pragma_update(None, "user_version", 17)?;`).
No column changes, no data rewrite → old DBs migrate instantly and a downgrade
is harmless. Extend `db_stats_migration_tests.rs` with a v16→v17 case. Main's
v16 network-opt-in grandfathering remains the immediately preceding step.

**D6 — Remote stats / Last.fm.** STATS-0 says local only. `remote_stats.rs` and
the `listenbrainz.rs` *stats fetch* are already not wired into the screen; they
are **not deleted** (scrobbling submission shares that module and deleting would
churn an unrelated area). Action: add one doc-comment paragraph to
`remote_stats.rs` stating that these types never feed the My Stats screen
(STATS-0), and, if `cargo clippy -D warnings` flags newly-dead items, annotate
with `#[allow(dead_code)]` — do not remove code in this branch.

**D7 — "All time" means `listen_events`, and the number is allowed to shrink.**
Plays recorded before schema v7 exist only in `tracks.play_count`, so "All time"
will be smaller than the old screen showed. That is accepted as-is:

* **No backfill.** Synthesising `listen_events` rows from a bare counter would
  invent timestamps, which is worse than a smaller honest number.
* **No "Local history since <Mon YYYY>" caption**, and no other permanent UI
  affordance for it. The repository has **zero git tags** — there has never been
  a release, so there are no existing users whose remembered totals could
  shrink. A caption for a one-time transition that no user will ever experience
  would be permanent ballast in the hero row and a permanent branch in the
  render path.
* No rule text mentions the gap; STATS-6 covers empty/sparse only.

**D8 — Ratings.** `tracks.rating` exists but no Frame-25a element needs it.
Not used in this branch; no rule.

**D9 — Genre spectrum is display-only.** The spec's "Optional klickbar →
gefilterte Trackliste des Genres" is **out of scope for this branch**. The genre
bar is presentation, not navigation: no click handler, no cursor change, no
`ViewSource::Genre(..)` variant.
Rationale: adding a `ViewSource` variant means touching every exhaustive match
on it. There are **17 files** matching on `ViewSource` variants across both
crates (`view_source.rs`, `queries/mod.rs`, `view_session.rs`,
`session_restore.rs`, `sidebar_rebuild.rs`, `track_list_*` ×5, `library_shell.rs`,
`play_origin.rs`, `filter_restriction.rs`, and more) — most of them owned by no
task in this plan. Keeping the variant out keeps this branch's collision surface
for parallel agents inside the files it actually owns.
Recorded as an explicit follow-up in section 8.
Content rules stay: genres come from the single `tracks.genre` column; top 5 by
listening time, the rest folded into `"Other"`; tracks with an empty genre are
excluded from the denominator and surfaced as nothing (not as "Other").

**D10 — Customize (STATS-7): three section toggles, nothing else.**
The Customize menu contains exactly three `CheckButton`s — Clock, Genres,
Highlights — and no spotlight chooser. The spec's "Wahl Spotlight: Artist /
Genre / Track" is dropped **without replacement**: no `stats.spotlight` setting
key, no `SpotlightKind` enum, not even a prepared variant. Frame 25a specifies
only the artist spotlight; the genre and track variants have no design, so an
implementer would have to invent them.

State in the existing settings table, typed accessors in `library/settings.rs`:

```rust
pub struct StatsLayout { pub clock: bool, pub genres: bool, pub highlights: bool }

pub fn get_stats_layout(conn: &Connection) -> StatsLayout;                       // all true by default
pub fn set_stats_layout(conn: &Connection, l: StatsLayout) -> Result<(), rusqlite::Error>;
```

Keys: `stats.section.clock`, `stats.section.genres`, `stats.section.highlights`,
all defaulting to `true`, read/written through the existing
`get_bool`/`set_bool`. Keeping the state in core makes the toggle logic testable
at `[core]` level and leaves the GTK test to assert visibility only.

**D11 — Ribbon rendering: cairo, not SVG.** The repo has no SVG rendering
pipeline; both existing charts are `DrawingArea` + `set_draw_func` (the same
pattern as `player_bar/waveform_seek.rs`). The ribbon is a new `stats_ribbon.rs`
`DrawingArea` filling an area path with an accent gradient, dashed stroke for the
open bucket, a filled peak dot and a hollow open-bucket dot. Hover uses
`gtk4::EventControllerMotion` + `set_tooltip_text` on crossing into a bucket (no
custom popup — TIP-5). All geometry math lives in `stats_ribbon_math.rs` (pure,
unit-tested without a display).

**D12 — Granularity / sparse (STATS-6).** `stats_period.rs` owns

```rust
pub fn granularity_for(span_days: i64, distinct_active_days: i64) -> Granularity
```

Rules, in order: `span_days <= 45 || distinct_active_days < 8` → `Day`;
`span_days <= 120 || distinct_active_days < 24` → `Week`; otherwise `Month`.
Total for `span_days = 0` and for `distinct_active_days = 0`.
`YearToDate` never shows buckets that have not started yet (2026-07-19 →
Jan..Jul). When the period has **zero** events, `StatsSnapshot::is_empty()` is
true and the view renders an `adw::StatusPage` ("Start listening to see your
stats") instead of the sections — never an axis with one lonely bar.

**D13 — Highlights semantics** (all in core, zone-aware per D3):
*Streak* = longest run of consecutive local days (via `local_parts`) with ≥ 1
qualifying play inside the period. *Discovered* = tracks whose **first-ever**
listen event (global `MIN(played_at)` per track, not period-local) falls inside
the period. *Busiest day* = local day with the largest summed clamped
`ms_played`. *On repeat* = track with the highest event count inside the period.

**D14 — Comparison pill (STATS-1).** Previous period = the equally long window
ending at `start_unix`. Delta on total listening ms, rounded to whole percent.
When the previous window has zero ms, the pill is **hidden** (no "▲ ∞ %").
The pill uses the teal app accent (`.stats-pill`), never the cover accent.

**D15 — Spotlight actions.** Play = `player.play_from_view(ids, 0, origin)` with
`play_origin::from_artist(&artist)` and `artist_track_ids(&conn, artist)` —
literally the wiring `window_action_wiring.rs:352` already uses for the artist
hero, so container play semantics stay identical. "Go to artist" = the
`wire_artist_view` push pattern: `nav_history.record_route(&NavPlace::source(
ViewSource::Artist(name), Some(LIBRARY_VIEW_TRACKS)))` then
`track_list.set_source(...)` + `content_stack.set_visible_child_name("library")`
+ `library_stack.set_visible_child_name(LIBRARY_VIEW_TRACKS)`.

**D16 — Breakpoint.** The window-level breakpoint in `library_shell.rs:453` is
about the sidebar. STATS-4's reflow is view-local, so wrap the asymmetric row in
an `adw::BreakpointBin` with `max-width: 720px` flipping the row's `orientation`
to `Vertical` (setter on the `gtk4::Box` `orientation` property). No window
breakpoint is touched.

**D17 — Customize menu placement.** A `gtk4::MenuButton` (`view-more-symbolic`)
inside the stats page itself, immediately right of the period dropdown — not in
the window header. STATS-8 wants the period control to be the only view
regulator in the chrome; putting the ⋮ in-page keeps the shared header untouched
and avoids per-page header swapping.

**D18 — File sizes.** `stats_view.rs` must end < 800 lines (AGENTS.md). It
becomes a composer that owns the period dropdown, refresh orchestration and the
customize menu; every section is built in a sibling module (see task list).

**D19 — Grouping key (STATS-9): two stages, no fuzz, no split, no mutation.**
One shared home, `library/group_key.rs`, used by every stats aggregate over
artist, album-artist and genre. No second path.

```rust
pub enum GroupKind { Artist, AlbumArtist, Genre }

/// Runtime-only grouping key. Never stored, never written back to a tag.
pub fn normalize_group_key(raw: &str) -> String;

/// One raw observation feeding the fold.
pub struct GroupInput<'a> {
    pub raw: &'a str,          // the original spelling, shown to the user
    pub mbid: Option<&'a str>, // stage 1, when the caller has one
    pub plays: i64,
    pub ms: i64,
    pub last_played_at: i64,
}

pub struct Group { pub label: String, pub key: String, pub plays: i64,
                   pub ms: i64, pub variant_count: usize }

/// Folds raw rows into groups, descending by `ms`.
pub fn fold_groups(rows: &[GroupInput<'_>]) -> Vec<Group>;
```

*Stage 1 — MBID.* When `mbid` is `Some(non-empty)`, it is the key; different
spellings under one MBID collapse correctly. Applies to `GroupKind::Artist`
only, because `tracks.artist_mbid` is the only MBID column that exists and it is
keyed to the raw `artist` column. **`GroupKind::AlbumArtist` and
`GroupKind::Genre` always pass `mbid: None`** — the columns do not exist and
this branch invents none. Explicit assumption, recorded: no schema migration for
MBIDs in this round; the signature already carries the stage so a later column
needs no API change. `artist_mbid` is also sparsely populated (opt-in network
feature) — stage 2 is the common case, not the exception, and must be correct on
its own.

*Stage 2 — normalized key*, in exactly this order:
1. `trim()`
2. Unicode lowercase via `str::to_lowercase()` (real Unicode case folding, not
   `to_ascii_lowercase`)
3. whitespace collapse: any run of `char::is_whitespace` → one `' '`
4. diacritics fold: NFKD, then drop `Unicode General_Category = Mn`
   (combining marks)

*Hard limits.* **No fuzzy matching of any kind** — no Levenshtein, no prefix or
substring merging, no token-set comparison. Exact equality after normalization,
nothing else. "Lorna Shore" ≠ "Lorna Shore Band"; "Weezer" ≠ "Weezer (Blue
Album)". Guessing is never acceptable here: a wrong merge silently destroys a
user's numbers and is undetectable from the UI.

*No mutation.* Stats is read-only. `normalize_group_key` produces a value that
lives in a `String` for the duration of one `compute` call. No column is added,
no tag is rewritten, no `UPDATE` is issued on any path this plan touches. Test
`dedup_does_not_mutate_tags` pins it.

*Input expression.* SQL delivers **raw** rows; the fold happens in Rust (D4 —
same reason: no cache, and normalization is not expressible in SQLite without a
custom function). The artist aggregates select
`queries::library_views::EFFECTIVE_ALBUM_ARTIST` — the *existing* Artists-view
expression — so Top Artists and the Artists view finally agree. This replaces
all three divergent keys listed in section 1.

*Split artists: deliberately NOT implemented.* The brief asked for
"`;`/`/`-separated multi-artists assigned to the first artist, consistent with
the existing Artists-view rule". **That existing rule does not exist**: verified,
no code anywhere splits an artist string, and the Artists view collapses
compilations purely through the `album_artist` tag
(`library_views.rs:13`, `:84-115`). Adding a split *in stats only* would make
Top Artists disagree with the Artists view — the exact divergence STATS-9
exists to remove, and the "second path" the brief forbids. Splitting is
therefore out of scope here and recorded as a follow-up (section 8) that must
change both views in one branch.

**D20 — Display label: dominant raw spelling, three-level deterministic tiebreak.**
The group is labelled with an **original** spelling, never the normalized form —
`"lorna shore"` must never become the label. Selection, applied in order until
one wins:
1. highest number of plays for that raw spelling;
2. then the most recent `last_played_at`;
3. then the lexicographically smallest raw string (`Ord` on `&str`).

Level 3 is mandatory, not decorative: two spellings can tie on both plays and
timestamp (a library imported in one batch has identical `last_played_at`
values), and without a total order the label would flicker between refreshes.
`fold_groups` must be a total order end to end — group ordering itself is
`ms DESC`, then `plays DESC`, then `label` ascending, for the same reason.
`Group::variant_count` carries how many raw spellings the group absorbed; it
is `1` for a clean group and feeds D21.

**D21 — Fix path: hint always, click wired.** When `variant_count >= 2`, the
list row carries a discreet secondary-tone hint plus a tooltip
("3 Schreibweisen zusammengefasst — im Tag-Editor vereinheitlichen?"). It is a
**suggestion, never an automatic merge**: no tag is written without the user
going through the tag editor.

The click path is wired in this branch. It is safe to do so — verified: the
multi-select tag editor is merged, its entry point is stable
(`TrackList::edit_tags_for_ids(&[i64])`, `track_list.rs:457`), and the
`feat/tag-editor-rework` branch that `docs/` still mentions no longer exists.
The wiring copies the album precedent verbatim
(`window_action_wiring.rs:224-230`):

```rust
let track_list = Rc::downgrade(track_list);
stats_view.set_on_unify_spellings(move |ids| {
    if let Some(track_list) = track_list.upgrade() { track_list.edit_tags_for_ids(&ids); }
});
```

The ids come from a core helper that the spotlight's Play action needs anyway —
one function, two callers:

```rust
/// Track ids whose normalized group key equals `key`. Used by the spotlight
/// Play action and by the "unify spellings" hint.
pub fn group_track_ids(conn: &Connection, kind: GroupKind, key: &str)
    -> Result<Vec<i64>, rusqlite::Error>;
```

This matters beyond the hint: D15's spotlight Play uses `artist_track_ids`,
which matches one exact name `COLLATE NOCASE` and would therefore miss the very
rows STATS-9 just merged. **The spotlight plays `group_track_ids`, not
`artist_track_ids`** — otherwise the screen shows 94 plays and plays 61 of them.

**D22 — One new dependency: `unicode-normalization`.** NFKD is not reachable
from the current tree. `icu_normalizer` appears in `Cargo.lock` but only
transitively (`url` → `idna` → `idna_adapter`), and a transitive dependency is
not usable without declaring it; `icu_normalizer`'s API is also far heavier than
this needs. Add to `crates/reprise-core/Cargo.toml`, verbatim:

```toml
# Unicode NFKD for the runtime-only stats grouping key (STATS-9): decompose,
# then drop combining marks so "Bjork" and "Björk" fold together. Pure Rust,
# one dependency (tinyvec), no platform or GUI deps.
unicode-normalization = "0.1"
```

Version 0.1.25 and its single dependency `tinyvec` are already in the local
cargo registry cache, so the build stays offline-capable. The
`cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` gate is
unaffected — both crates are `no_std`-capable and pull in nothing platform-bound.
Rust's `str::to_lowercase` is already full Unicode lowercase, so **only** the
NFKD step needs the crate; do not reimplement case folding on top of it.

---

## 3. Tasks, ownership, order

Sequencing: **T1 → T2 → (T3 ‖ T5) → T4 → (T6 ‖ T7) → T8 → T9 → T10.**
The grouping key (T3) lands before the aggregations that consume it (T4), and
core aggregations land before any GTK layout work. T5 (settings) depends on
nothing in T3/T4 and runs alongside T3.

Files listed under "owns" are exclusive to that task; no other task may edit
them. Two files are shared and need a discipline instead:

* `stats/mod.rs` and `library/mod.rs` are **append-only registries** — each task
  adds only its own `mod` line and rebases rather than reformats.
* `docs/ux-rules.md` is **line-scoped**: T1 lands the whole section V with every
  rule `[geplant]`; afterwards a task may only change the status token of the
  rules assigned to it below. No two tasks that can run in parallel own the same
  rule line.

Rule → flipping task (a rule flips to `[aktiv]` in the commit of the task that
**finishes** it, which is not always the task that writes its test):

| Rule | Flipped by |
|---|---|
| STATS-0, STATS-9 | T4 |
| STATS-1, STATS-3, STATS-5, STATS-6, STATS-7 | T8 |
| STATS-2, STATS-4, STATS-8 | T9 |

STATS-9 flips in T4, not T3: T3 delivers the pure key function and its fold,
but the rule promises that the *aggregates* group by it and that tags stay
untouched, which is only true once `compute` uses it.

**Rule ID note.** The rule is `STATS-9`, not `STATS-DEDUP`. Verified against
`scripts/check-ux-traceability.sh`: the document parser is
`^- \*\*[A-Z]+-[0-9]+[a-z]?\*\*`, which requires digits after the dash — a
`STATS-DEDUP` bullet parses to nothing, lands in neither `status_of` nor
`level_of`, and the rule would silently escape the coverage gate entirely (no
error, no coverage). `STATS-9` parses and is enforced. Every other ID in
`docs/ux-rules.md` is numeric; the "dedup" mnemonic is kept in the rule text.

### T1 — Rulebook section + release checklist
Owns: `docs/ux-rules.md` (creates section V, all rules `[geplant]`),
`RELEASING.md`.
First commit of the branch. Lands the verbatim block from section 6 after
the existing section U and the "Manual GNOME QA" bullet. Must come first:
`check-ux-traceability.sh` errors on a `RELEASING.md` reference to a rule the
document does not contain.

### T2 — Schema v17 index
Owns: `crates/reprise-core/src/db.rs`,
`crates/reprise-core/src/db_stats_migration_tests.rs`.
Adds `SCHEMA_V17` + migration block per D5. Nothing else.

### T3 — Grouping key (STATS-9 core)
Owns: `library/group_key.rs` (new), `library/group_key_tests.rs` (new),
`crates/reprise-core/Cargo.toml` (one dependency line, D22).
Its own module pair precisely so it can run parallel to everything else and so
no other task edits it. Delivers `normalize_group_key`, `GroupKind`,
`GroupInput`, `Group` and `fold_groups` per D19/D20 — pure functions over plain
input, no `Connection`, no SQL, no I/O. `group_track_ids` (D21) belongs to T4
because it needs a `Connection`.
Nothing in this task touches the stats screen; it is a self-contained library
primitive with four named tests.

### T4 — Core period model + snapshot (the heart)
Owns: `library/stats_period.rs` (new), `library/stats_period_tests.rs` (new),
`library/stats_snapshot.rs` (new), `library/stats_snapshot_tests.rs` (new),
`library/stats_screen.rs` (rewrite), `library/stats_screen_tests.rs` (rewrite),
`library/remote_stats.rs` (doc note only, D6),
`ui/playback/play_tracking.rs` (one call site: `scrobbling::should_scrobble` →
`stats_screen::counts_as_play`), `docs/ux-rules.md` (STATS-0 + STATS-9 status
tokens only).
Consumes T3's `fold_groups` for every artist / album-artist / genre aggregate
and adds `group_track_ids` (D21).

Split so no file passes 800 lines: `stats_screen.rs` keeps the row types +
`record_listen_event` + `counts_as_play` + the primitive queries;
`stats_period.rs` owns `StatsPeriod`/`PeriodRange`/`Granularity`/`Bucket`/
`granularity_for`/`local_parts`; `stats_snapshot.rs` composes them into
`StatsSnapshot`.

Deletes `headline_totals`'s `tracks` path, `available_years` (replaced by
`StatsPeriod::available`), `monthly_listen_timeseries` (replaced by the ribbon
series inside `compute`), `most_active_weekday` (no Frame-25a element uses it),
and the `Option<i32> year` parameter everywhere.

Query shape (D3 + D4): exactly **one** row-level query feeds every zone-aware
derivation —

```rust
struct ListenRow { played_at: i64, ms: i64 } // ms already clamped in SQL
fn listen_rows(conn: &Connection, start_unix: i64, end_unix: i64)
    -> Result<Vec<ListenRow>, rusqlite::Error>;
```

Ribbon series, listening clock, streak and busiest day are all derived from that
one `Vec` in Rust.

Name-keyed aggregates (top artists, album-artists, genres, spotlight, distinct
artists) select **raw spellings pre-aggregated per raw value** and are folded by
T3's `fold_groups` in Rust — SQLite cannot express the D19 normalization without
a custom function, and the fold is where the MBID stage and the D20 label choice
live. The artist queries select `EFFECTIVE_ALBUM_ARTIST` (D19), carry
`MAX(t.artist_mbid)` for `GroupKind::Artist`, and pass `mbid: None` for
album-artist and genre. Only aggregates with no name key stay pure SQL `GROUP
BY` (headline totals, top tracks, discovered, on repeat) — eight statements
total per `compute`.

### T5 — Layout settings + smart-playlist creation
Owns: `library/settings.rs` (+ its tests file), `library/playlists.rs`
(+ its tests file).
Adds the `StatsLayout` accessors (D10 — three booleans, no spotlight key) and

```rust
pub fn create_smart(conn: &Connection, name: &str, rules_json: &str,
                    sort_field: &str, sort_dir: &str, limit_count: Option<i64>)
    -> Result<i64, rusqlite::Error>;
```

an `INSERT INTO smart_playlists` mirroring the v3 seed row, validating
`rules_json` through `smart_rules_to_sql` before writing.

### T6 — Ribbon widget
Owns: `ui/stats/stats_ribbon.rs` (new), `ui/stats/stats_ribbon_math.rs` (new).
Consumes `PeriodRange`/`Bucket` from T4; renders per D11. `stats_chart.rs` is
left in place until T8 removes its last use.

### T7 — Section widgets
Owns: `ui/stats/stats_spotlight.rs` (new), `ui/stats/stats_genre_bar.rs` (new),
`ui/stats/stats_highlights.rs` (new), `ui/stats/hourly_chart.rs` (peak
highlighting + caption, drop `allow(dead_code)`).
Each exposes `new()` + `set_data(&…Section)` + `widget()`; callbacks are
`Rc<RefCell<Option<Rc<dyn Fn(...)>>>>` setters like `artist_detail_pane.rs`.
The genre bar is display-only per D9: no click controller, no cursor change.
Rows whose `Group::variant_count >= 2` render the D21 hint + tooltip and expose
an `on_unify` callback setter; the widgets never call the tag editor themselves.

### T8 — View recomposition
Owns: `ui/stats/stats_view.rs` (rebuild), `ui/stats/stats_customize.rs` (new),
`ui/stats/stats_css.rs`, `ui/stats/mod.rs`, `docs/ux-rules.md` (STATS-1, -3, -5,
-6, -7 status tokens only).
Vertical flow inside `adw::Clamp(1120)`: hero row (time + pill + subline +
period dropdown + ⋮), ribbon, spotlight, genre spectrum, asymmetric row
(`BreakpointBin`, 1.35fr/1fr via `Box` with `hexpand` + `size_request`), top
tracks with the plays/time sort toggle. Deletes `stats_chart.rs` usage and the
three dead helpers (`genre_row`, `progress_bar`, `album_strip_item`).
Passes `&chrono::Local` into `compute` (D3). Empty path per D12. Exposes
`set_on_unify_spellings(impl Fn(Vec<i64>))` for T9 to wire (D21).

### T9 — Wiring
Owns: `ui/window/window_action_wiring.rs`, `ui/window/window.rs`,
`ui/window/library_shell.rs` (stats-related lines only),
`ui/browse/filter_restriction.rs` (test only), `docs/ux-rules.md` (STATS-2, -4,
-8 status tokens only).
Spotlight Play / Go to artist per D15 — Play resolves ids through
`group_track_ids`, **not** `artist_track_ids` (D21); the Smart Mix "Create" CTA
calls `playlists::create_smart` with a genre-rule JSON built from the snapshot's
top genres, then refreshes the sidebar and routes to the new
`ViewSource::Smart(id)` (an existing variant — no new one is introduced);
the "unify spellings" callback is wired to `TrackList::edit_tags_for_ids`
following the album precedent at `window_action_wiring.rs:224-230` (D21).

### T10 — Close-out
Owns: `.superpowers/sdd/progress.md`.
Records the branch in the progress ledger and runs the full gate sweep from
section 4 one last time on the merged branch state. No code changes.

---

## 4. TDD — test first, per task

Run `cargo test --workspace` (never bare `cargo test`). Display tests carry
`#[ignore = "requires a display; run via xvfb-run"]` and run via
`scripts/check-display-tests.sh`. Every rule-named test must be a real `#[test]`
fn, or the traceability gate does not see it.

**Traceability gate — checked, no script change needed.**
`scripts/check-ux-traceability.sh` derives its ID prefixes from
`docs/ux-rules.md` itself (`prefixes=$(printf '%s\n' "${!status_of[@]}" | sed …)`),
so section V is gated automatically once T1 lands — no edit to the script.
Two properties matter for this plan and both hold:
1. the exact marker `#[ignore = "requires a display; run via xvfb-run"]` counts
   as coverage on **every** rule status, so `stats_7_customize_toggles_sections`
   may be a display test and still green the gate;
2. only fn names matching `stats_<digit>…` are read as rule references, so the
   non-rule test names below (`stats_layout_…`, `stats_streak_…`,
   `stats_view_…`, `stats_css_…`) cannot accidentally claim a rule.

All ten rules are covered: STATS-0..6 and STATS-9 by `[core]` tests in T4,
STATS-7 by the display test in T8, STATS-8 by the `[gtk]` test in T9.

**STATS-9 needs a `stats_9_*` test in addition to the four mandated names.**
Verified against the gate: its reference regex is `fn (stats)_[0-9]+[a-z]?_`, so
`dedup_casing_whitespace_merges_one_artist`, `dedup_mbid_beats_name`,
`dedup_no_fuzzy` and `dedup_does_not_mutate_tags` are **invisible** to it — all
four are kept verbatim as required, and the rule-named aggregate-level test
`stats_9_group_key_dedups_top_artists_and_genres` is added alongside them.
Without it the build fails the moment STATS-9 flips to `[aktiv]`.

**T1**
No Rust test. Verification is `scripts/check-ux-traceability.sh` passing with
ten new `[geplant]` rules and the RELEASING bullet in place.

**T2**
1. `migrating_a_v16_database_adds_the_listen_events_track_index` — open a DB at
   `user_version = 16`, migrate, assert `PRAGMA index_list(listen_events)`
   contains `idx_listen_events_track_played` and `user_version = 17`.

**T3** (pure functions over plain input — no DB, no display, no time zone)
1. **`dedup_casing_whitespace_merges_one_artist`** `[core]` — `"Lorna Shore"`,
   `"lorna shore "` and `"Lorna\tShore"` fold into **one** group; its `label` is
   the most-played raw spelling (`"Lorna Shore"`), never the normalized form;
   `plays`/`ms` are the sums; `variant_count == 3`.
2. **`dedup_mbid_beats_name`** `[core]` — two rows with different names but the
   same `Some(mbid)` produce one group; a third row with a different mbid and an
   identical name stays separate. Proves stage 1 outranks stage 2 in both
   directions.
3. **`dedup_no_fuzzy`** `[core]` — `"Lorna Shore"` vs `"Lorna Shore Band"` stay
   two groups, and `"Weezer"` vs `"Weezer (Blue Album)"` stay two groups. Pins
   D19's hard limit against any later "helpful" prefix or distance matching.
4. `dedup_folds_diacritics_via_nfkd` `[core]` — `"Björk"`, `"Bjo\u{308}rk"`
   (combining diaeresis) and `"bjork"` fold into one group; covers the NFKD step
   that `str::to_lowercase` alone does not provide (D22).
5. `dedup_label_tiebreak_is_total_order` `[core]` — two spellings with identical
   `plays` **and** identical `last_played_at` still yield a stable label across
   repeated `fold_groups` calls and across input permutations, resolved
   lexicographically (D20 level 3). Assert by shuffling the input order and
   comparing results.
6. `normalize_group_key_is_idempotent` `[core]` —
   `normalize_group_key(normalize_group_key(x)) == normalize_group_key(x)` over a
   fixture list including empty, whitespace-only and combining-mark strings.

**T4** (write all of these red first; all pass `&Utc` unless stated)
1. **`stats_0_play_definition_consistent_time_and_count`** `[core]` — seed 3
   tracks; insert listen events plus `tracks.play_count` values that
   deliberately disagree (e.g. `play_count = 40` with 3 events). Assert
   `snapshot.hero.total_ms == Σ MIN(ms_played, duration_ms)` of exactly those
   events and `snapshot.hero.plays == events.len()`, i.e. the `play_count` noise
   is invisible; additionally assert `hero.plays ==
   snapshot.top_tracks.iter().map(|t| t.plays).sum()` when the top list is
   unlimited — time and count come from one row set.
2. **`stats_1_ribbon_axis_matches_period`** `[core]` — with `now = 2026-07-19`,
   `StatsPeriod::YearToDate(2026).resolve(now, &Utc, Some(..))` yields exactly 7
   buckets `Jan..Jul`, `buckets.last().open == true`, all others `open == false`;
   `StatsPeriod::Year(2025)` yields 12 buckets, none open; `Last30Days` yields 30
   `Day` buckets ending today.
3. **`stats_6_sparse_uses_finer_granularity`** `[core]` — a library with 5 events
   across 4 days in `YearToDate` resolves to `Granularity::Day`, not 7
   mostly-empty months; a library with events spread over 200 days resolves to
   `Month`; a period with zero events yields `snapshot.is_empty() == true` and an
   **empty** bucket vector (no axis).
4. `stats_2_spotlight_reports_share_and_top_tracks` `[core]` — #1 artist, their
   plays/ms, `share_percent` relative to period total, exactly 3 top-track chips,
   ranks 2–5 in `also`.
5. `stats_3_genre_spectrum_buckets_other` `[core]` — 7 genres → 5 segments plus
   `"Other"`; shares sum to 100 (±1 rounding); blank genres excluded from both
   the segments and the denominator.
6. `stats_4_highlights_streak_and_discovered` `[core]` — run with
   `&FixedOffset::east_opt(3600).unwrap()`: an event at 23:30 UTC belongs to the
   next local day, so the streak counts consecutive **local** days; `discovered`
   counts only tracks whose first-ever event falls inside the period.
7. `stats_5_top_tracks_sort_toggle_orders_by_time` `[core]` — a short track
   played often outranks a long track played rarely under `SortBy::Plays` and the
   order inverts under `SortBy::Time`.
8. **`stats_streak_survives_dst_change`** `[core]` — the D3/D3a regression guard.
   Using `DstZone` (spring-forward at `2026-03-29T01:00:00Z` = `1_774_746_000`),
   insert one event per local day at 00:30 local time, given as **fixed UTC
   timestamps**:

   | `played_at` | UTC instant | offset | local day |
   |---|---|---|---|
   | `1_774_567_800` | 2026-03-26 23:30Z | +01:00 | Mar 27 |
   | `1_774_654_200` | 2026-03-27 23:30Z | +01:00 | Mar 28 |
   | `1_774_740_600` | 2026-03-28 23:30Z | +01:00 | Mar 29 |
   | `1_774_823_400` | 2026-03-29 22:30Z | +02:00 | Mar 30 |
   | `1_774_909_800` | 2026-03-30 22:30Z | +02:00 | Mar 31 |

   Assert the streak is **5**. This fails under a single snapshotted offset:
   applying +01:00 to the two post-switch events lands them on Mar 29 and Mar 30
   instead of Mar 30 and Mar 31, collapsing the run to 4. Then assert the same
   row set also yields streak 5 under `&Utc` (UTC days Mar 26..Mar 30) — the
   control that proves the input is a genuine five-day run and the DstZone
   result of 5 is not an artifact of a coincidental collision.
9. `counts_as_play_matches_the_scrobble_threshold` — parity with
   `scrobbling::should_scrobble` at the 50 % and 4 min edges (mirror the existing
   assertions in `scrobbling.rs:588-603`).
10. `compute_is_pure_and_repeatable` — calling `compute` twice with identical
    arguments on an unchanged DB returns equal snapshots. Guards D4's purity
    requirement, which is what keeps a later cache a drop-in.
11. **`stats_9_group_key_dedups_top_artists_and_genres`** `[core]` — the
    rule-named test. Seed tracks tagged `"Lorna Shore"`, `"lorna shore"` and
    `"Lorna Shore "` with listen events on each, plus genres `"Deathcore"`,
    `"deathcore"` and `"Death core"`. Assert Top Artists has **one** row whose
    plays and ms are the full sums and whose label is `"Lorna Shore"`; assert
    `"Death core"` stays a **separate** genre from `"Deathcore"`/`"deathcore"`
    (no fuzzy merge — the space is a real difference after normalization);
    assert the merged artist row reports `variant_count == 3`; assert
    `group_track_ids` for that group returns all seeded track ids, so the
    spotlight would play every merged row (D21).
12. **`dedup_does_not_mutate_tags`** `[core]` — snapshot every
    `tracks.(artist, album_artist, genre, artist_mbid)` value before `compute`,
    run `compute` for every `StatsPeriod` variant, and assert the rows are
    byte-identical afterwards. Also assert the DB's `total_changes()` is
    unchanged across the call, so a stray `UPDATE` cannot hide behind an
    equal-value write.

**T5**
1. `stats_layout_defaults_to_all_sections_visible`
2. `stats_layout_roundtrips_through_settings`
3. `create_smart_inserts_a_playlist_that_list_smart_returns`
4. `create_smart_rejects_invalid_rules_json`

**T6** (pure math, no display)
1. `ribbon_area_path_spans_every_bucket`
2. `ribbon_marks_the_open_bucket_and_the_peak`
3. `ribbon_with_all_zero_values_draws_a_flat_baseline`
4. `ribbon_hover_maps_x_to_the_bucket_under_the_cursor`

**T7**
1. `spotlight_shows_rank_badge_name_and_three_chips` `[gtk, display]`
2. `genre_bar_renders_one_segment_per_share_plus_legend` `[gtk, display]`
3. `genre_bar_has_no_click_controller` `[gtk, display]` — D9 is a guarantee, not
   an omission: assert the segment widget carries no `GestureClick`.
4. `highlights_grid_renders_four_tiles` `[gtk, display]`
5. `hourly_chart_highlights_peak_hours` (math-level, no display)
6. `unify_hint_appears_only_for_multi_variant_groups` `[gtk, display]` — a row
   with `variant_count == 1` shows no hint and no tooltip; a row with
   `variant_count == 3` shows both (D21).

**T8**
1. **`stats_7_customize_toggles_sections`** `[gtk, display]` — build the view,
   assert clock/genres/highlights widgets are `is_visible()`; call the customize
   handler with a `StatsLayout` that disables the clock; assert the clock widget
   is hidden while the others stay visible and section **order** is unchanged;
   assert the choice round-trips through `settings`. The menu is also asserted to
   contain exactly three check items (D10 — no spotlight chooser).
2. `stats_view_empty_history_shows_the_status_page_and_no_ribbon` `[gtk, display]`
3. `stats_css_defines_the_ribbon_pill_and_spotlight_classes`
4. `stats_view_narrow_width_stacks_the_asymmetric_row` `[gtk, display]` — set the
   `BreakpointBin` narrow and assert the row's orientation is `Vertical`.

**T9**
1. **`stats_8_my_stats_source_hides_the_track_filter_row`** `[gtk]` — in
   `filter_restriction.rs`, a rule-named test asserting
   `!is_track_source(&ViewSource::MyStats)` (renames/extends the existing
   assertion at line 53 so the gate sees STATS-8).
2. `spotlight_play_uses_the_group_track_ids` — with a library containing two
   spellings of the #1 artist, assert the resolved id list equals
   `group_track_ids` for the group and is a **strict superset** of
   `artist_track_ids` for the label alone (D21).
3. `smart_mix_cta_creates_a_genre_smart_playlist` — after invoking the CTA
   handler, `list_smart` contains a playlist whose rules reference the top genres.
4. `unify_spellings_callback_opens_the_tag_editor_for_the_group_ids` — assert the
   callback forwards exactly `group_track_ids`' output to
   `TrackList::edit_tags_for_ids`, and that no tag write happens on the callback
   itself (D21 — suggestion, never auto-merge).

**T10**
No new tests; runs the gate sweep.

Gates before every commit: `cargo fmt --check`,
`cargo clippy --all-targets --workspace -- -D warnings`,
`cargo test --workspace`, `cargo audit`,
`cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` (empty),
`scripts/check-ux-traceability.sh`, `scripts/check-display-tests.sh`.

---

## 5. Migration / schema summary

* `user_version` 16 → **17**; content: one `CREATE INDEX` (D5). Additive only.
* No column added to `listen_events`; the ≥ 50 % criterion is a write-time
  predicate that already holds for every existing row (D1).
* No rollup table, no triggers, no backfill (D4, D7).
* Backward compatibility: a v17 DB opened by older code is unaffected (an index
  is invisible to those queries). Nothing is dropped or renamed.
* Behavioural (not schema) change: totals shown for "All time" now cover only
  recorded `listen_events`, so pre-v7 plays stop being counted and the number
  gets smaller. Accepted without mitigation and without a UI caption — there is
  no released build and therefore no user with a remembered total (D7).

---

## 6. `docs/ux-rules.md` — rule texts verbatim

Append a new section **after the existing section U** and before the
closing `---` / "Wenn beim Testen ein Fall auftaucht …" paragraph, keeping the
exact bullet shape the traceability gate parses
(`- **ID** [status] [ebene] — …`). Land this block in T1 with every rule
`[geplant]`; the tasks named in section 3 flip their own rules to `[aktiv]`.

```markdown
## V. My Stats

- **STATS-0** [geplant] [core] — Ein „play" ist überall dieselbe Sache:
  mindestens 50 % des Tracks oder mindestens vier Minuten gehört. Genau diese
  Ereignisse stehen in `listen_events`, und die My-Stats-Ansicht rechnet
  ausschließlich aus ihnen — Hero-Zeit, Plays, Top-Listen, Spotlight, Genres,
  Clock und Highlights sind Projektionen derselben Zeilenmenge. Der laufende
  Zähler `tracks.play_count` speist die Ansicht nie; Zeit und Anzahl können
  daher nicht auseinanderlaufen. Tages- und Stundengrenzen entstehen nicht in
  SQL: die Kernfunktionen nehmen eine Zeitzone als Parameter und bucketen jedes
  Ereignis einzeln durch sie hindurch, damit Sommer-/Winterzeit-Wechsel keine
  Grenze verschieben. Alles ist lokal: kein Netz, keine Cloud, keine
  Fremdquelle wird eingemischt.
- **STATS-1** [geplant] [core] — Der Kopf zeigt die Gesamt-Hörzeit groß, eine
  Vergleichs-Pill „▲ N % vs <Vorperiode>" im teal App-Akzent (nie im
  Cover-Akzent) und die Subzeile „N plays · Ø X min/day · N artists" auf
  Sekundär-Ton. Rechts steht das Zeitraum-Dropdown („<Jahr> so far / <Vorjahr> /
  All time / Last 30 days"). Darunter läuft ein schlankes Area-Ribbon der
  Hörzeit, dessen Achse **exakt dem gewählten Zeitraum** folgt — „2026 so far"
  zeigt Jan–Jul, nie ein rollendes 12-Monats-Fenster. Der laufende Bucket ist
  offen markiert (gestrichelt, hohler Punkt), der Peak gesetzt; Hover nennt den
  exakten Wert. Fehlt eine Vorperiode mit Hörzeit, entfällt die Pill.
- **STATS-2** [geplant] [core] — Das Artist-Spotlight ist das Herzstück:
  #1-Artist mit großem Cover und Rang-Badge, Eyebrow „YOUR #1 ARTIST", Name,
  Zeile „N plays · N h · N % of your listening", drei Top-Track-Chips sowie die
  Aktionen Play (Container-Play über die Trackliste des Artists) und
  „Go to artist" (regulärer NAV-Push mit Back-Historie). Hinter dem Cover liegt
  ein dezenter Cover-Akzent-Glow — der Cover-Akzent bleibt Playback-Elementen
  vorbehalten. Darunter nennt eine Ghost-Zeile die Ränge 2–5.
- **STATS-3** [geplant] [core] — Das Genre-Spektrum ist **eine** horizontale
  Segment-Leiste in Teal-Abstufungen mit Legende (Punkt · Name · %), gespeist
  aus den Genre-Tags der Bibliothek. Die fünf stärksten Genres bilden eigene
  Segmente, der Rest wird zu „Other" gebündelt; Tracks ohne Genre zählen weder
  als Segment noch als „Other". Die Leiste ist reine Anzeige und keine
  Navigation: Segmente und Legende sind nicht klickbar.
- **STATS-4** [geplant] [core] — Unter dem Spektrum steht eine asymmetrische
  Reihe (1.35fr / 1fr): links die Listening Clock als 24-Stunden-Histogramm aus
  den Timestamps mit teal hervorgehobenen Peak-Stunden und Caption
  („Peak 11 PM–1 AM · night owl"), rechts vier Highlight-Kacheln — Streak
  (längste Folge aufeinanderfolgender lokaler Tage mit ≥ 1 play), Discovered
  (im Zeitraum erstmals gespielte Tracks), Busiest day, On repeat (höchste
  Play-Zahl) — plus der CTA „Smart Mix aus Top-Genres? · Create", der eine
  echte Smart Playlist anlegt. Tages- und Stundengrenzen folgen der lokalen
  Zeit des Nutzers, nicht UTC. Im schmalen Fenster klappt die Reihe per
  AdwBreakpoint einspaltig, ohne dass sich die Reihenfolge ändert.
- **STATS-5** [geplant] [core] — Top Tracks steht über die volle Breite:
  nummerierte Liste mit Cover, Titel und Artist, relativem Play-Balken und
  Play-Count, mit Sort-Toggle „by plays / by time". Der Balken ist relativ zum
  Spitzenreiter der Liste, nie zu einem absoluten Maximum.
- **STATS-6** [geplant] [core] — Leere und dünne Datenlagen werden nie als
  leere Diagramme gezeigt. Ohne Hörhistorie im Zeitraum erscheint ein
  freundlicher Leerzustand („Start listening to see your stats") statt Achsen
  mit einem einsamen Balken. Bei dünner Datenlage wird die Granularität feiner
  (Tage bzw. Wochen statt größtenteils leerer Monate).
- **STATS-7** [geplant] [gtk] — My Stats ist kuratiert, nicht frei editierbar:
  kein Drag-and-Drop-Widget-Board. Ein ⋮-Menü „Customize" blendet die Sektionen
  Clock, Genres und Highlights per CheckButton ein und aus; die Auswahl bleibt
  über Sitzungen erhalten. Mehr enthält das Menü nicht — das Spotlight ist
  fest das Artist-Spotlight. Die Reihenfolge der Sektionen ist fix, Größen sind
  nicht manuell veränderbar — Anpassung an die Fensterbreite geschieht
  ausschließlich per AdwBreakpoint.
- **STATS-8** [geplant] [gtk] — In My Stats gibt es keine Filter-Zeile und
  keine Suche der Trackliste — das ist eine andere Ansicht. Die rechte
  Now-Playing-Spalte verhält sich wie überall. Das Zeitraum-Dropdown ist der
  einzige Ansichts-Regler dieser Ansicht.
- **STATS-9** [geplant] [core] — **Dedup:** Unsaubere Tags dürfen Zahlen nicht
  zersplittern. Top Artists, Top Genres, Album-Artist-Aggregate und das
  Spotlight gruppieren über einen zweistufigen Schlüssel: liegt eine MBID vor,
  gilt sie; sonst ein normalisierter Schlüssel aus Trim, Unicode-Casefold
  (nicht nur ASCII), Whitespace-Kollaps und Diakritika-Faltung (NFKD ohne
  Combining Marks). „Lorna Shore", „lorna shore" und „Lorna Shore " sind damit
  ein Eintrag mit einer Summe. Der Schlüssel existiert nur zur Laufzeit: keine
  gespeicherte Spalte, und die Ansicht schreibt **niemals** Tags zurück —
  Statistik ist lesend. Angezeigt wird stets eine echte Original-Schreibweise
  der Gruppe (die häufigste; bei Gleichstand die zuletzt gespielte, dann
  alphabetisch), nie die normalisierte Form. **Geraten wird nie:**
  zusammengefasst wird ausschließlich, was nach Normalisierung exakt gleich ist
  — kein Fuzzy-Matching, keine Levenshtein-Distanz, kein Präfix-Merge, also
  bleibt „Lorna Shore Band" von „Lorna Shore" getrennt. Fasst eine Gruppe
  mindestens zwei Schreibweisen zusammen, weist ein dezenter Hinweis am
  Listeneintrag darauf hin und führt in den Mehrfach-Tag-Editor der betroffenen
  Tracks; das Vereinheitlichen bleibt eine Einladung, nie ein automatischer
  Schreibvorgang.
```

`RELEASING.md`, "Manual GNOME QA" (line 109), add one bullet (German prose is
not used there — match the file's English):

```markdown
- My Stats editorial pass (UX STATS-1, STATS-2, STATS-3, STATS-4): open My Stats
  on a populated library. Hero time and play count must agree with the top-track
  list; the ribbon axis must match the selected period with the running bucket
  drawn open and the peak marked; hover must name an exact value. Play the
  spotlight artist and follow "Go to artist", then use Back. Check that axis
  labels, eyebrows and sublines stay readable against the view background in all
  three dark themes, and narrow the window until the clock/highlights row stacks.
- My Stats grouping (UX STATS-9): on a library with a deliberately mis-tagged
  artist ("Lorna Shore" / "lorna shore" / "Lorna Shore "), Top Artists must show
  one entry with the summed plays and hours, labelled in the clean spelling, and
  the spotlight Play must queue every merged track. Two genuinely different
  artists must never merge. Follow the "unify spellings" hint into the tag editor
  and cancel it, then confirm with a tag dump that the files and DB rows are
  unchanged by merely opening My Stats.
```

---

## 7. Risks and pitfalls

**gtk4-rs**
* *RefCell re-entrancy.* The refresh path borrows `Rc<RefCell<Connection>>`.
  Never hold that borrow across a widget call that can re-enter (dropdown
  `selected_notify` fires during `set_selected`). Pattern: compute the snapshot
  into a local `StatsSnapshot` in its own statement, `drop` the borrow, then
  touch widgets. The existing `wire_year_selector` already borrows `conn` inside
  the callback — the rebuilt version must not nest a second borrow. (With D4
  there is no second `RefCell` in play, which removes the worst version of this
  trap; do not reintroduce one.)
* *Dropdown callback storms.* `populate_year_model` calls `set_selected(1)`,
  which fires `selected_notify` and triggers a refresh during construction.
  Wire the callback *after* populating the model.
* *Cover loading.* Reuse the generation-token pattern (`cover_loader.rs`) for the
  spotlight cover and every top-track row — a stale period's cover must not land
  in a new row. One generation `Cell` per list, bumped on refresh.
* *No ListView.* Top Tracks is a `Box` of at most ~10 rows; do **not** introduce
  a `ListView`/factory here — the recycling bugs are not worth it at this size.
* *Cairo, not SVG.* See D11. Text in cairo (`show_text`) uses the toy API, as in
  `stats_chart.rs`; keep labels short and left-anchored, and never rely on
  `text_extents` succeeding (the existing code `continue`s on error).
* *Contrast.* Axis labels, eyebrows and sublines must use the secondary tone
  (`alpha(@window_fg_color, 0.55–0.6)`), not the 0.4–0.45 dim step currently used
  by `.stats-rank` / `.stats-play-count`. Raise those two in `stats_css.rs` while
  rebuilding — this is the CONTRAST requirement, easy to forget.
* *Breakpoint.* `adw::BreakpointBin` needs its own `width-request`/
  `height-request` or it warns at runtime; set both before adding setters.

**Time zones (D3)**
* Thread the `Tz` through — do **not** collapse it to an offset anywhere in the
  chain, not even "just for the ribbon labels". The moment one derivation uses a
  snapshotted offset, `stats_streak_survives_dst_change` is the only thing
  standing between that and a silently wrong streak.
* `local_parts` returns `Option`; handle `None` by skipping the row, never by
  `unwrap`. `reprise-core` must not panic on stored data.
* Do not add `chrono-tz`. `Utc`, `FixedOffset` and the test-local `DstZone` cover
  every test case; GTK covers production with `Local`.
* `reprise-core` tests must never read the `TZ` environment variable or call
  `Local::now()` — they pass their zone explicitly.

**Performance**
* Every aggregate joins `listen_events` to `tracks`; without D5's index this is a
  scan per section. If a section ever feels slow, confirm with
  `EXPLAIN QUERY PLAN` before changing anything.
* `compute` runs eight statements, one `Vec` pass and the group fold per
  refresh, and it runs on every route to `ViewSource::MyStats`
  (`library_shell.rs:296`). That is deliberate (D4). The escape hatch, if it is
  ever needed, is the wrapper described in D4 — not a rollup table.
* The group fold is `O(rows)` with one `String` allocation per raw variant, not
  per event: fold over **pre-aggregated** rows (one per raw spelling), never over
  the raw event stream. On the real library that is a handful of allocations.
* Cover thumbnails for the spotlight are 150 px: request a `ThumbnailSize` large
  enough, do not upscale a `List` thumbnail.

**Grouping (STATS-9)**
* *The label is not the key.* Every lookup that starts from a displayed label
  must go back through `group_track_ids`, never through a name equality. The
  spotlight Play action is the trap: `artist_track_ids` matches one exact
  spelling `COLLATE NOCASE` and would silently play a subset of what the row
  claims (D21).
* *Do not push normalization into SQL.* A `lower(trim(x))` in a query looks like
  it does the same job and does not — it misses NFKD and whitespace collapse,
  and it re-creates a second key. The only normalization site is
  `normalize_group_key`.
* *Do not "improve" the matcher.* Prefix, substring or edit-distance merging is
  explicitly forbidden (D19). A wrong merge is invisible in the UI and destroys
  the user's numbers; `dedup_no_fuzzy` exists to fail such a change.
* *`artist_mbid` is sparse and artist-scoped.* It is `NULL` for most rows and it
  belongs to the raw `artist` column, not `album_artist`. Never pass it as the
  MBID for `GroupKind::AlbumArtist` or `GroupKind::Genre`, and never assume the
  MBID stage will carry the common case.
* *Empty keys.* A blank or whitespace-only artist/genre normalizes to `""`;
  those rows are excluded before folding, exactly as `EFFECTIVE_ALBUM_ARTIST`
  and D9's genre rule already require. Do not let them collapse into one giant
  `""` group.

**Empty / sparse**
* Zero events, one event, and one-track-only histories must all render: guard
  every division (`share_percent`, ribbon normalization, relative bars) against a
  zero denominator, and make `granularity_for` total for `span_days = 0`.
* `AllTime` on an empty DB has no `first_event_unix`; `resolve` must return an
  empty bucket vector rather than a range starting at epoch 0 (which would try to
  draw 56 years of months).
* The period dropdown must still offer the current year when there are no events
  at all.

**Process**
* The traceability gate fails the build if a rule is `[aktiv]` without a
  rule-named `#[test]`. Flip each rule only in the task listed in section 3's
  mapping table, never earlier — a rule that is `[aktiv]` before its UI exists is
  a false claim even when the gate is green.
* `stats_view.rs` and `stats_screen.rs` both approach the 800-line limit —
  extract siblings as planned, do not trim doc comments to fit.
* T1 must land before anything else: `RELEASING.md` referencing STATS-1..4 while
  section V is absent makes `check-ux-traceability.sh` exit 1.

---

## 8. Deliberate follow-ups (not this branch)

Recorded so they are not re-litigated mid-implementation and not silently lost.

1. **Clickable genre spectrum → filtered track list.** The spec's optional
   STATS-3 interaction (D9). Needs a `ViewSource::Genre(String)` variant plus its
   `queries` branch, sidebar/session-restore handling and exhaustive-match
   updates across the ~17 files that match on `ViewSource`. Own branch, own rule
   amendment (STATS-3 gains a sentence, or a new STATS-3a) — it is a navigation
   feature, not a stats feature.
2. **Snapshot cache, if and only if profiling shows latency.** D4's wrapper in
   front of `compute`, keyed on `(period, now-bucket)`. `compute` is kept pure
   and repeatable (test `compute_is_pure_and_repeatable`) precisely so this stays
   a single-file addition with no change to the snapshot type, the view, or any
   existing test. Trigger for revisiting: a measured refresh above ~50 ms on a
   real library, not a hypothetical row count.
3. **Spotlight variants (Genre / Track).** Only if Frame 25a is extended with
   designs for them. Until a design exists there is nothing to implement, and the
   Customize menu stays at three checkboxes (D10).
4. **Split multi-artist tags ("A; B" → first artist).** Requested in the
   STATS-9 brief, deliberately deferred (D19). It cannot be a stats-only change:
   the Artists view, artist detail, browse facets and `artist_context` all key
   off `EFFECTIVE_ALBUM_ARTIST` (`queries/library_views.rs:13`), so splitting in
   stats alone would re-create the divergence STATS-9 removes. Own branch,
   touching `library_views.rs`, `artist_detail.rs`, `queries/browse.rs` and
   `queries/artist_context.rs` together, with a rule of its own — and a decision
   the user has to make first, because separator handling is a genuine judgement
   call ("AC/DC" and "Simon & Garfunkel" are single artists containing
   separators).
5. **Adopt `normalize_group_key` in the Artists and Genre views.** The brief
   asks for the same bracket there "later". Once stats proves the key, the same
   function should replace `LOWER(EFFECTIVE_ALBUM_ARTIST)` grouping in
   `query_artists` and the browse facets, and the ad-hoc
   `lower(trim(artist))` in `artist_news.rs:268` / `:570`. Deferred here only to
   keep this branch's collision surface at the files it owns.
