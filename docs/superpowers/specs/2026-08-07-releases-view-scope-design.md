# Releases view: bounded in time, quiet about what you mostly own

Date: 2026-08-07
Status: implemented on `feature/releases-view-scope`; visual review pending

## Problem

The Releases view is a discography-gap catalog. `NR-16` states its scope
literally: regular albums and EPs "regardless of age", and "album and EP
catalog rows are not subject to any time-based retention". For a library with
long-lived artists that means the table reaches back decades — a listener
scrolls past gaps from the 1970s to find what came out this year. The
catalog answers "what is missing from this artist's life's work", but the
question a listener actually asks the view is "what came out that I don't
have".

Three further defects sharpen the same complaint:

1. **Mostly-owned albums stay in the list.** Ownership is binary today:
   `presence_for` returns `Complete` only when the local track count reaches
   the official one, so an album where 9 of 12 tracks are already in the
   library still occupies a row as `Incomplete`. That is a gap on paper and
   noise in practice.
2. **The same release appears twice.** Observed 2026-08-07: *By the
   Thousands — Visions of Inner Depth*, 11 May 18, listed once as `Album`
   and once as `EP`, both `Missing`. MusicBrainz carries the work as two
   release groups with different primary types; the view shows both.
3. **The type filter hides its own capability, and singles are missing
   entirely.** `ReleasesFilter.release_type` is an
   `Option<ReleaseTypeFilter>`: no chip means both, `Album` means albums
   only, `Ep` means EPs only. Nothing on screen says both types are shown.
   Singles cannot be chosen at all — `release_kind` drops them past the
   90-day news window and `filter_rows` gates the table to `Album | Ep`.

## Decisions

| # | Decision | Value |
|---|---|---|
| 1 | Age bound | Persistent window chip `1 year · 5 years · 10 years · All`, default **5 years**. Display filter only — nothing is deleted. |
| 2 | Ownership | A row disappears once local tracks cover **more than 50 %** of the official track count. `LibraryPresence::Complete` keeps its strict meaning. |
| 3 | Types | Three independent toggles `Album · EP · Single`. Album and EP on by default, **Single off by default** — singles do enter the catalog, so everyone can decide for themselves. |
| 4 | Duplicates | Same artist + same normalized title + same release date collapse to one row: **Album before EP before Single**. |

### Non-goals

- No extra network traffic. `release_groups_page_url` already requests
  `type=album|ep|single`; singles arrive in the same payload today and are
  discarded while parsing. Admitting them changes storage, not requests.
- No deletion by the window. It is a filter, not retention; `All` must
  always be able to show what the cache holds, and the fetch-window
  protection in `artist_news_history.rs` stays untouched.
- `NEWS_WINDOW_DAYS` and the per-artist news cap from `NR-1a` (now `NR-27`) stay as they
  are. The delta popover's candidate scope follows the persisted Releases
  filters, while its visit-batch and stamping semantics remain unchanged.

## Design

### Core: one scope definition, five filters

Everything lives beside the existing decisions in
`crates/reprise-core/src/artist_news_view.rs`, which the sidebar badge and
the visible table already share through `query_releases_view_in`. Extending
that one function keeps count and content identical by construction.

`ReleasesFilter` grows from two fields to three:

```rust
pub struct ReleasesFilter {
    pub release_types: ReleaseTypeSelection, // was Option<ReleaseTypeFilter>
    pub window: ReleaseWindow,               // new
    pub hidden: bool,
}
```

`ReleaseWindow` is a closed enum (`OneYear`, `FiveYears`, `TenYears`, `All`)
with a `cutoff(today) -> Option<NaiveDate>` method; `All` yields `None` and
therefore no comparison at all. `ReleaseTypeSelection` holds three booleans
rather than a set, because the catalog has exactly three types and an empty
`HashSet` would need the same special case anyway. Its `Default` is
`album: true, ep: true, single: false`.

`filter_rows` gains three steps and keeps its existing ones, in this order:

1. hidden state (unchanged)
2. **ownership** — drop `Complete`, and drop anything the majority or single
   rule covers (below)
3. catalog type gate, now `Album | Ep | Single`
4. selected types (three toggles instead of one either-or)
5. **window** — drop rows whose parsed release date is before the cutoff
6. **collapse duplicates** — last, so it only ever sees rows that survived

### Core: what counts as owned

One predicate, used by the view, the badge, and the popover:

```rust
pub fn counts_as_owned(entry: &HistoryEntry, today: NaiveDate) -> bool {
    if entry.presence == LibraryPresence::Complete {
        return true;
    }
    if release_status(entry, today) == ReleaseStatus::Upcoming {
        return false;
    }
    if is_single(&entry.release_type) {
        return entry.local_track_count > 0;
    }
    entry.track_count.is_some_and(|official| {
        official >= 2 && entry.local_track_count * 2 > official
    })
}
```

`HistoryEntry` already carries both `track_count: Option<i64>` and
`local_track_count: i64`, so no schema change is needed.

The upcoming guard preserves `NR-16`'s intent that advance singles never
count as ownership: a not-yet-released album whose lead singles are already
in the library would otherwise vanish before it exists.

An unknown official track count (`track_count IS NULL`) yields no share and
therefore no hiding — the row stays as it does today. Guessing ownership
from an unknown denominator would hide albums the listener does not have.

**Singles need their own ownership test, twice over.** The `new_releases`
schema forbids `track_count = 1` (`CHECK (track_count IS NULL OR
track_count >= 2)`), so a one-track single can never reach `Complete` and
would sit in the catalog forever. And `local_album_track_counts` keys the
library by *(album artist, album)*: a single you own as a track on its later
album matches nothing, because the album field holds the album's name, not
the song's. Single ownership therefore consults a second map,
`local_track_titles(conn) -> HashSet<(String, String)>`, keyed by
*(normalized release artist, normalized track title)* over the same
non-removed, non-missing tracks. One song by that artist under that title is
ownership. Without this, switching the Single chip on would list hundreds of
songs the listener demonstrably has.

### Core: singles become durable catalog rows

Two changes outside the view module:

`release_kind` in `artist_news_parsing.rs` currently returns `None` for a
single past the news window, so it is never stored. It gains a catalog
branch:

```rust
"single" if date_text.len() == 10 && delta > 0 => Some(NewsKind::Upcoming),
"single" => Some(NewsKind::Catalog),
```

The `include_singles` parameter is removed from the parsing and fetch chain.
The `module.new_releases.include_singles` preference, setting helpers, GTK
row, and strings are removed as well. The Single chip is the only control:
it determines both catalog visibility and whether singles enter the delta.

`enforce_retention` in `artist_news_history.rs` treats album and EP rows as
durable and everything else as transient (six months by `first_seen`, or
beyond the 200 newest). Catalog singles must join the durable set, otherwise
the Single chip shows a set that silently churns. After this only primary
types the fetch no longer requests can still be reaped — legacy rows from
older versions — which is the correct residual scope for that code.

Storage grows: the table gains every single of every library artist rather
than the handful inside the news window. For a 500-artist library that is
thousands of rows, not millions, in a database that already holds every
track. Acceptable, and stated here so it is a decision rather than a
surprise.

Migration v62 resets `artist_news_fetch.last_attempt_at` to zero and removes
the dead `module.new_releases.include_singles` setting while preserving each
known `artist_mbid`. This checkout already supported schema v60 when the
design was implemented, so the originally planned migration number 40 was
advanced monotonically to 61. The reset is intentional: the definition of a
complete cached discography now includes singles, so every prior per-artist
answer is incomplete. Existing 30-artist batches keep the refill bounded.

### Core: collapsing duplicates

Group key is `(normalize(artist_name), normalize(title), first_release_date)`
— the same `normalize` the presence lookup uses. The date must match: two
release groups sharing a title years apart are a re-recording or a
re-issue, not a duplicate.

Within a group the winner is the highest type rank, `Album` before `EP`
before `Single`. Ties resolve deterministically: a row with a known
`track_count` wins over one without, then the lexicographically smallest
`release_group_mbid`. Determinism matters because the sidebar badge counts
the same set.

Collapsing runs after the type and hidden filters, so it can only ever
remove a row the listener asked to see: with Album switched off and EP on,
the EP row of a collapsed pair is what remains, which is what "EPs only"
means.

### Core: persistence

`releases.filter.type` keeps its key, extended to a comma-separated list:

| stored | selection |
|---|---|
| absent | Album + EP (default, unchanged) |
| `album` | Album only (unchanged) |
| `ep` | EP only (unchanged) |
| `album,ep` | Album + EP, written when both are re-enabled |
| `single`, `album,single`, `album,ep,single`, … | any combination |

Old databases therefore need no migration; an unparsable value falls back to
the default. `releases.filter.window` is new, holding `1y` / `5y` / `10y` /
`all`; an absent or unparsable value means `5y`. Existing installs will see
a shorter list after the update — that is the point of the change, and `All`
restores the old reach in one click.

### GTK: the filter row

`releases_filter_bar.rs` renders the window chip beside the three type
toggles and the hidden chip. `releases_view.rs` is already 721 lines against
the repo's 800-line ceiling, so new wiring goes into the filter-bar module
or a new small sibling rather than into the view.

Zero results keep `NR-17`'s single "Show all" step: it now clears type,
window, and hidden in one action. An empty type selection means all three
types, not an empty table — a filter row must not be able to produce a dead
end it cannot explain.

The count line (`release_count_line`, "8 of 19 gaps") needs its reference
point moved. `render_cache` computes the total today as
`count_releases_view(ReleasesFilter::default())`. Once the default carries a
five-year window and hides singles, widening the chips would render "200 of
19 gaps". The total therefore becomes the **widest** scope — all three
types, window `All`, current hidden state — so `shown ≤ total` holds by
construction. The line always names both values, including at the widest
scope ("200 of 200 gaps"); on defaults it reads "19 of 200 gaps", which is
the honest statement that a filter is doing work.

`releases_empty_state_for` takes its "is filtered" flag from the same
comparison: `filter != widest`, not `filter != default`. Otherwise a library
whose only gaps predate the window would claim "No missing albums or EPs"
while 200 rows sit behind the chips, instead of the `NoResults` state with
its "Show all" step.

New user-visible strings go to `strings_releases.rs`; regenerating the
`.pot` belongs to the same change.

### GTK: popover and badge

The delta popover (`updates/feed_snapshot.rs`) switches from strict complete
presence to `counts_as_owned`, applies the persisted type selection and
window, excludes hidden rows, and uses the same duplicate collapsing as the
table. `unseen_release_count` counts the unseen portion of that exact
candidate set, so the popover and its badge cannot disagree. A 1975 album
discovered today enters the durable catalog but remains quiet under the
default five-year window. A single announces itself exactly when the Single
chip is active.

The sidebar badge needs no code change — `count_releases_view` calls
`query_releases_view_in` with the persisted filter and follows
automatically. Its rule text does need updating, because it names the
filters explicitly.

## Edge cases

| Case | Behaviour |
|---|---|
| Release date unparsable (`""`, `"unknown"`) | Stays visible in every window. Losing a real gap to missing third-party metadata is the worse error. |
| Upcoming release, majority of tracks local | Stays visible (advance singles are not ownership). |
| `track_count IS NULL` on an album | No majority computable, row unchanged. |
| `track_count = 2`, one local track | 1 × 2 = 2 is not > 2 — stays visible. Half is not a majority. |
| Single whose song sits on a library album | Owned, via the track-title map. |
| Album hidden, EP visible, same work | Each view collapses its own set; the hidden view shows the EP. |
| Window `All` | No date comparison runs at all. |
| Single chip off/on | Singles are absent/present in both the catalog and delta; there is no second preference. |

## Rule changes (`docs/ux-rules.md`, section R)

Append-only, per `AGENTS.md`. All four land `[planned]` and flip to
`[active]` in the commit that implements them.

- **NR-24** `[active]` `[core]` `[gtk]` — replaces `NR-16`. The full
  releases view is a discography-gap catalog for artists currently in the
  library, containing regular albums, EPs, and singles; secondary types
  never enter it. A release counts as owned, and therefore does not appear,
  when its distinct local track identities cover at least the smallest
  official MusicBrainz edition, **or** more than half of the official track
  count of a release that has already come out, **or** — for a single — when
  the library holds any track by that artist under that title. An unknown
  official count and any not-yet-released title never count as owned. Two
  catalog entries sharing artist, normalized title, and release date
  collapse to a single row, album ahead of EP ahead of single. Catalog rows
  of all three types are durable and exempt from time-based cache retention.
- **NR-25** `[active]` `[gtk]` — replaces `NR-17`. The gap view remains the
  table `Date · Title · Artist · Type · Status`, sorted by date descending.
  Its permanent filter row carries independent Album, EP, and Single
  toggles — album and EP on by default, single off — a persistent age window
  of `1 year · 5 years · 10 years · All` defaulting to five years, and the
  Hidden chip. An empty type selection shows all types; a release without a
  parsable date survives every window. Activation opens the external release
  URL, Hidden activates `Show again`, and zero results offer exactly one
  "Show all" step that clears type, window, and hidden together.
- **NR-26** `[active]` `[core]` `[gtk]` — replaces `NR-18`. "Releases"
  remains a sidebar location in SMART, before Concerts, visible only with
  the `new_releases` module active. Its badge equals exactly the number of
  discography gaps visible under the persistent type, window, and hidden
  filters; 0 renders no badge.
- **NR-9c** `[active]` `[core]` `[gtk]` — replaces `NR-9b`. Unchanged in
  its batch and stamping semantics; releases owned under NR-24, excluded by
  the persisted type or window scope, or hidden do not enter the popover.
  Duplicates collapse there the same way. Singles announce themselves
  exactly when their chip is on; there is no separate preference.

## Testing

Rule-named tests, per the repo's gate convention:

- `nr_24_majority_coverage_hides_a_released_album` — 7 of 12 local tracks,
  row gone; 6 of 12, row stays.
- `nr_24_unknown_official_count_never_counts_as_owned`
- `nr_24_upcoming_release_is_never_owned_by_advance_singles`
- `nr_24_single_is_owned_through_a_matching_track_title`
- `nr_24_single_survives_cache_retention` — a two-year-old single is still
  in the cache after `enforce_retention`.
- `nr_24_duplicate_album_and_ep_collapse_to_the_album` — the observed
  *Visions of Inner Depth* case, as a fixture.
- `nr_24_same_title_on_a_different_date_is_not_a_duplicate`
- `nr_25_default_window_hides_releases_older_than_five_years`
- `nr_25_singles_are_absent_until_their_chip_is_on`
- `nr_25_undated_release_survives_every_window`
- `nr_25_window_all_shows_the_full_catalog`
- `nr_25_empty_type_selection_shows_every_type`
- `nr_25_all_selected_types_with_all_window_is_the_widest_scope` — all three
  active toggles plus `All` must not offer a no-op "Show all" step.
- `nr_25_count_line_never_exceeds_its_total` — window `All` plus singles
  still renders `shown ≤ total`.
- `nr_25_gaps_beyond_the_window_offer_show_all` — empty default view with
  older gaps reaches `NoResults`, not `Empty`.
- `nr_26_badge_follows_the_window_filter`
- `nr_9c_owned_release_does_not_enter_the_popover`
- `nr_9c_single_badges_only_when_its_chip_is_on`
- `nr_9c_ancient_discovery_does_not_badge`
- migration test: v62 resets the fetch ledger, preserves `artist_mbid`,
  removes the dead setting, and is idempotent.

Core tests go to `artist_news_view_tests.rs` and
`artist_news_parsing_tests.rs`, presentation and filter-bar tests to their
existing display-test neighbours. Display tests run single-process
(`--test-threads=1`), and the base branch is checked first — `dev` carries
known-red display tests unrelated to this work.

Beyond green tests: a visual pass on the real Releases view with a library
whose artists predate the window, confirming that the old rows are gone,
that `All` brings them back, that the Single chip fills and empties the
table without listing songs the library already holds, and that the
duplicate pair from the screenshot renders once.

## What review and the visual pass changed

Four things were decided after the implementation landed, three of them
found by looking rather than by testing.

**"Clear all" returns to the default, not to the widest scope.** The
screenshot showed it permanently on screen and accented, because the default
is now itself a filter and the button's visibility was measured against the
widest scope. Worse than the noise was the meaning: "clear" landed on the
*most* open state and there was no one-click way back. It now appears only
when the filter differs from the default and returns to it. The zero-result
step and the shell's cross-section "clear filters" keep the old behaviour
through a separate `show_widest()`, because at zero results under the default
filter, returning to that same default would change nothing on screen.

**`NR-1a` was false and still `[active]`.** It described a pipeline that kept
ninety days of albums and exclusively future singles. NR-16 had already voided
the first half and NR-24 voids the second, yet the rule stood unmarked.
`NR-27` replaces it and states what the pipeline really does — durable album,
EP and single catalog rows regardless of age, with the twenty-per-artist cap
belonging to the news candidates alone.

**`NR-13` promised an action that does not exist.** No "Show in library" lives
in the gap catalog, and since NR-16 no owned release can even reach the table.
`NR-28` replaces it and a test pins that the filtered view never yields the
`In library` status, so a future filter change cannot revive the dead branch
in silence.

**One library scan instead of two.** `local_track_titles` was a second full
pass over `tracks` beside `local_album_track_counts`, on every catalog render,
badge count and popover open. Both indexes now come from a single
`local_library_index`.

The migration number moved from v61 to v62 during the merge with `dev`, which
had claimed v61 for its own. Two migrations sharing a number do not conflict —
the second silently skips, which on any database already at 61 would have
meant the ledger reset never ran.
