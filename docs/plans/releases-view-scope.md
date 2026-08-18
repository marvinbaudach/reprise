---
slug: releases-view-scope
worktree: ~/Projects/reprise-releases-view-scope
branch: feature/releases-view-scope
phase: shipped
codex_session:
created: 2026-08-07
---

# Releases view: bounded in time, quiet about what you mostly own

Design spec: `docs/superpowers/specs/2026-08-07-releases-view-scope-design.md`
(copied into this worktree — commit it alongside the implementation). Every
decision below was grilled with the user on 2026-08-07 and is final; do not
re-open them, and do not add scope beyond them.

## Goal

The full Releases view is a discography-gap catalog that reaches back decades,
lists albums the listener mostly owns, shows the same work twice when
MusicBrainz types it as both album and EP, and cannot show singles at all.

Measured against the user's real library (185 artists, 1874 tracks, 797 cached
releases) the changes below take the visible table from **665 rows to 168**:
the majority rule removes 64, the five-year window another 433, duplicate
collapsing one — the exact pair from the bug screenshot.

## Decisions

Decisions 1–7 were final for the original 2026-08-07 catalog work. Decision 8
was corrected after a 2026-08-09 screenshot showed historical gaps presented
as new announcements.

1. **Age window** — persistent chip `1 year · 5 years · 10 years · All`,
   default **5 years**. A display filter; nothing is deleted.
2. **Majority ownership** — a released album disappears once local tracks cover
   **more than 50 %** of its official track count.
3. **Types** — three independent toggles `Album · EP · Single`; album and EP on
   by default, single **off**. Singles become durable catalog rows.
4. **Duplicates** — same artist + normalized title + release date collapse to
   one row: album before EP before single.
5. **The `Include Singles` preference is removed entirely**, together with the
   `include_singles` parameter threaded through the parse chain.
6. **Migration v62** resets the per-artist fetch ledger so the catalog
   backfills in ~1.5 days instead of up to seven.
7. **The count line always reads `X of Y gaps`**, Y being the widest scope.
8. **Popover and badge use the announcement window** — future plus the last 90
   days, independently of the full view's age window. Persisted types still
   apply, so the Single chip remains the sole single-announcement control.

## Repo rules that bind this task

- `docs/ux-rules.md` is the binding UX contract. Rule IDs are **append-only**:
  never edit an old rule's text — mark it `[replaced by <NEW-ID>]` and append
  the new one. A rule flips to `[active]` in the same commit that implements
  it. NR-24/NR-25/NR-26/NR-9c landed in the original work; NR-29 supersedes
  NR-9c for the post-ship announcement-scope correction.
- Every `[active]` rule needs a rule-named test (`fn nr_24_…`).
- Every code file created or substantially edited ends **< 800 lines**.
  `crates/reprise-gnome/src/ui/releases/releases_view.rs` is already 721 lines
  — new wiring goes into the filter-bar module or a new small sibling.
- Immutability: build new values, never mutate shared state in place.
- Never run `git stash` — the repo is shared with parallel worktrees.
- Make focused commits. Do not touch files outside this worktree.

## Package 1 — core scope (TDD)

File: `crates/reprise-core/src/artist_news_view.rs` (+ `artist_news_view_tests.rs`).
Keep the module under 400 lines; if it grows past that, split the new scope
logic into `artist_news_scope.rs` and re-export through the `artist_news`
facade so the UI's import path is unchanged. Write the tests first.

### 1.1 `ReleaseWindow`

```rust
pub enum ReleaseWindow { OneYear, FiveYears, TenYears, All }
```

- `cutoff(today: NaiveDate) -> Option<NaiveDate>`; `All` → `None`, so no
  comparison runs at all.
- `Default` is `FiveYears`.
- Setting key `releases.filter.window`, values `1y` / `5y` / `10y` / `all`;
  absent or unparsable → `FiveYears`.

### 1.2 `ReleaseTypeSelection`

Replaces `Option<ReleaseTypeFilter>` in `ReleasesFilter`. Three booleans
(`album`, `ep`, `single`); `Default` is `album: true, ep: true, single: false`.
An **empty** selection means *all types* — a filter row must never be able to
produce a dead end it cannot explain.

Persistence keeps the existing key `releases.filter.type` and stays backward
compatible:

| stored | selection |
|---|---|
| absent | album + ep |
| `album` | album only |
| `ep` | ep only |
| `album,ep`, `single`, `album,single`, `album,ep,single`, … | as written |

Unparsable → the default. No schema migration for this.

### 1.3 `counts_as_owned`

The single ownership definition, shared by table, badge, and popover. It must
work for both `HistoryEntry` and `StoredRelease` — give it the five fields it
needs (`presence`, `release_type`, `first_release_date`, `track_count`,
`local_track_count`) and add thin wrappers, or a small trait. Do not duplicate
the logic.

- `presence == LibraryPresence::Complete` → owned.
- status `Upcoming` (release date in the future) → **never** owned. This
  preserves NR-16's intent that advance singles are not ownership.
- single-typed row → owned when `local_track_count > 0` (see 1.4).
- otherwise → owned when `track_count` is known, `>= 2`, and
  `local_track_count * 2 > track_count`.
- `track_count IS NULL` → not owned; the row stays visible exactly as today.

`LibraryPresence::Complete` keeps its strict meaning — do **not** loosen
`presence_for`. The Updates popover's "Show in library" action and the
`X of Y tracks` status label depend on it. (The gap catalog's own
"Show in library" promise died with `NR-13`; `NR-28` replaced it, because a
row that could offer it is a row the filter already removed.)

### 1.4 Single ownership needs a track-title map

`local_album_track_counts` in `artist_news_query.rs` keys the library by
*(normalized album artist, normalized album)*. A single whose song sits on its
later album matches nothing there, and the schema forbids `track_count = 1`
(`CHECK (track_count IS NULL OR track_count >= 2)`), so a one-track single can
never reach `Complete`. Without a second lookup, switching the Single chip on
lists hundreds of songs the library demonstrably holds.

Add `local_track_titles(conn) -> HashSet<(String, String)>` keyed by
*(normalized release artist, normalized track title)* over the same
`removed_at IS NULL AND missing_since IS NULL` tracks, with the same
`album_artist`-falls-back-to-`artist` rule. For single-typed release groups,
`local_track_count` comes from that map (1 when present, 0 otherwise).

### 1.5 `filter_rows` order

1. hidden state (unchanged)
2. ownership — drop everything `counts_as_owned`
3. catalog type gate: `Album | Ep | Single` (was `Album | Ep`)
4. selected types
5. window — drop rows whose parsed release date is before the cutoff; a row
   whose date does not parse **survives every window**
6. collapse duplicates (last, so it only sees rows that survived)

### 1.6 Duplicate collapsing

Group key `(normalize(artist_name), normalize(title), first_release_date)` —
the date must match, so a re-recording years later is not a duplicate. Winner:
highest type rank `Album > Ep > Single`; tie-break first a known `track_count`,
then the lexicographically smallest `release_group_mbid`. It must be
deterministic, because the badge counts the same set.

### 1.7 One pass, two numbers

`query_releases_view_in` loads the whole history and builds a map over every
library track. The UI needs three numbers per render (visible rows, the widest
-scope total, the badge count). Do **not** call the query three times. Provide
one entry point that returns the filtered rows together with the widest-scope
total (all three types, window `All`, same hidden state) from a single load of
history + library maps. This view already carries known switch-time
regressions; do not triple its cost.

## Package 2 — singles become durable catalog data (TDD)

### 2.1 `release_kind` (`artist_news_parsing.rs`)

```rust
"single" if date_text.len() == 10 && delta > 0 => Some(NewsKind::Upcoming),
"single" => Some(NewsKind::Catalog),
```

No network change: `release_groups_page_url` already requests
`type=album%7Cep%7Csingle`, so singles arrive in the same payload and are
merely discarded while parsing today.

**Remove the `include_singles` parameter entirely** from `release_kind`,
`parse_release_group`, `parse_release_groups`, `parse_release_group_page*`,
`fetch_release_discography`, and the pipeline call sites. Remove
`artist_news::include_singles` / `set_include_singles`,
`INCLUDE_SINGLES_KEY` in `artist_news_candidates.rs`, the `singles_row` in
`crates/reprise-gnome/src/ui/preferences/preference_new_releases.rs`, and the
strings `NEW_RELEASES_INCLUDE_SINGLES` / `_DESCRIPTION` in `strings_news.rs`.
Adjust the tests that pin the old behaviour rather than deleting their intent.

The catalog is where singles live; the Single chip is the only control.

### 2.2 `enforce_retention` (`artist_news_history.rs`)

The purge treats album and EP as durable catalog data and everything else as
transient (six months by `first_seen`, or beyond the 200 newest). **Singles
join the durable set**, otherwise the Single chip shows a set that silently
churns. Update the doc comment: afterwards only primary types the fetch no
longer requests can still be reaped (legacy rows).

### 2.3 Migration v62

New migration file beside the existing ones, `PRAGMA user_version = 62`, and
raise the supported-version constant. It does two things:

```sql
UPDATE artist_news_fetch SET last_attempt_at = 0;
DELETE FROM settings WHERE key = 'module.new_releases.include_singles';
```

`artist_mbid` must be preserved — losing it costs an extra search request per
artist. Document *why* the reset is correct even though
`db_artist_news_fetch.rs` deliberately backfills the ledger to avoid a
full re-fetch: the definition of what the cache must contain has changed, so
every artist's cached answer is now incomplete. The existing 30-artists-per-run
batching keeps the load bounded; the user sees the catalog fill within about a
day and a half, or immediately via "Fetch now".

The design originally reserved v40, but the branch point already supported
schema v60, so the implementation took v61. `origin/dev` then landed its own
v61 (`db_mobile_sync`) while this branch was being written, and two migrations
sharing a number is not a merge conflict — it is a silent skip, because the
second one checks `version >= 61`, finds it true and returns. The merge
therefore renumbered this one to **v62**. Migrations are monotonic; only the
number changed, never the contents or the intent.

## Package 3 — GTK surface

- `releases_filter_bar.rs`: window chip + three type toggles + hidden chip, all
  persistent. Keep the module under 800 lines.
- `releases_view.rs` (721 lines — do not grow it materially): `render_cache`
  uses the single-pass entry point from 1.7. The count line always renders
  `release_count_line(shown, total)` with `total` = widest scope; on defaults
  that reads "168 of 601 gaps", and at the widest scope it reads, for example,
  "601 of 601 gaps". `shown <= total` must hold by construction.
- `releases_empty_state_for`'s "is filtered" flag compares against the widest
  scope, not `default()` — otherwise a library whose only gaps predate the
  window claims "No missing albums or EPs" while hundreds of rows sit behind
  the chip.
- The zero-result "Show all" step clears type, window, and hidden together.
- Sidebar badge: follows `count_releases_view` automatically; prove it with a
  test.
- New strings in `strings_releases.rs`; regenerate `po/reprise.pot`.

## Package 4 — popover and badge announcement scope

`delta_candidates` in `artist_news_query.rs` feeds both the popover
(`updates/feed_snapshot.rs`) and `unseen_release_count`, i.e. the badge. It
applies:

- ownership (`counts_as_owned`, replacing today's `presence != Complete`),
- the type selection — so with the Single chip off, no single ever badges, and
  with it on singles count like everything else,
- a dedicated announcement window — parsable future dates and dates no more
  than 90 days old; the full view's 1/5/10-year/All window cannot widen it,
- duplicate collapsing, the same way the table does it.

Hidden entries are already excluded. Because badge and popover read the same
function, they cannot disagree — which is the incoherence NR-23 exists to
prevent.

## Package 5 — rules (`docs/ux-rules.md`, section R)

Mark `NR-16`, `NR-17`, `NR-18`, `NR-9b` as `[replaced by …]` (text otherwise
untouched) and append, `[active]`:

- **NR-24** `[core]` `[gtk]` — catalog scope: albums, EPs, and singles for
  artists currently in the library; secondary types never enter. A release
  counts as owned, and therefore does not appear, when its distinct local track
  identities cover at least the smallest official MusicBrainz edition, **or**
  more than half the official track count of an already-released release,
  **or**, for a single, when the library holds any track by that artist under
  that title. Unknown official counts and not-yet-released titles never count
  as owned. Entries sharing artist, normalized title, and release date collapse
  to one row: album ahead of EP ahead of single. Catalog rows of all three
  types are durable and exempt from time-based cache retention.
- **NR-25** `[gtk]` — the table and its filter row: independent Album, EP, and
  Single toggles (album and EP on by default, single off), a persistent window
  `1 year · 5 years · 10 years · All` defaulting to five years, and the Hidden
  chip. An empty type selection shows every type; a release without a parsable
  date survives every window. The count line always names shown and total, the
  total being the widest scope. Zero results offer exactly one "Show all" step
  clearing type, window, and hidden together.
- **NR-26** `[core]` `[gtk]` — the sidebar badge equals the number of gaps
  visible under the persistent type, window, and hidden filters; 0 renders no
  badge.
- **NR-29** `[core]` `[gtk]` — replaces NR-9c. The delta popover and badge use
  future plus 90-day announcements independently of the full view's age
  window. Releases owned under NR-24, filtered out by type, or already hidden
  do not enter; duplicates, batches, stamping, and badge consistency remain.

Keep `docs/plans/ux-rules-acceptance-tests.md` in sync if it lists the replaced
rules.

## Package 6 — tests

Rule-named, in `artist_news_view_tests.rs`, `artist_news_parsing_tests.rs`,
the test module of `artist_news_history.rs`, and the existing display-test
neighbours:

- `nr_24_majority_coverage_hides_a_released_album` (7/12 gone, 6/12 stays)
- `nr_24_unknown_official_count_never_counts_as_owned`
- `nr_24_upcoming_release_is_never_owned_by_advance_singles`
- `nr_24_single_is_owned_through_a_matching_track_title`
- `nr_24_single_survives_cache_retention`
- `nr_24_duplicate_album_and_ep_collapse_to_the_album` — fixture: *By the
  Thousands — Visions of Inner Depth*, `2018-05-11`, once `Album` once `EP`,
  both missing; exactly the album row survives
- `nr_24_same_title_on_a_different_date_is_not_a_duplicate`
- `nr_25_default_window_hides_releases_older_than_five_years`
- `nr_25_singles_are_absent_until_their_chip_is_on`
- `nr_25_undated_release_survives_every_window`
- `nr_25_window_all_shows_the_full_catalog`
- `nr_25_empty_type_selection_shows_every_type`
- `nr_25_all_selected_types_with_all_window_is_the_widest_scope`
- `nr_25_count_line_never_exceeds_its_total`
- `nr_25_gaps_beyond_the_window_offer_show_all`
- `nr_26_badge_follows_the_window_filter`
- `nr_29_owned_release_does_not_enter_the_popover`
- `nr_29_single_badges_only_when_its_chip_is_on`
- `nr_29_ancient_discovery_does_not_badge`
- `nr_29_updates_popover_keeps_catalog_history_out_when_the_full_view_shows_all`
- `nr_29_announcement_window_includes_day_ninety_and_excludes_day_ninety_one`
- `nr_29_future_single_requires_an_exact_date_in_the_delta`
- migration test: v62 zeroes `last_attempt_at`, keeps `artist_mbid`, drops the
  dead setting key, and is idempotent

## Verification

- `cargo test -p reprise-core` green, `cargo test -p reprise-core artist_news`
  in particular.
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt`.
- GTK display tests need an X server and will not run in this sandbox. Do not
  fake them, do not delete them, and do not report them as passing — state
  plainly in the summary which suites ran and which could not.
- `scripts/check-architecture.sh` and `scripts/check-ux-traceability.sh` if
  they run without a display.
- Check a suite's **pass count**, not just its exit status: a filter that
  matches nothing still prints `ok`.

## Out of scope

- No change to `NEWS_WINDOW_DAYS`, to the per-artist news cap (`NR-1a`, now `NR-27`), or to
  what the fetch requests.
- No deletion of catalog rows by the window — it is a filter.
- No new network requests, no new provider, no new UI beyond the filter row.
