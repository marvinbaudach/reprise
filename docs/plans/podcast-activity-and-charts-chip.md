---
slug: podcast-activity-and-charts-chip
worktree: ~/Projects/reprise-podcast-activity-and-charts-chip
branch: feature/podcast-activity-and-charts-chip
phase: planned
codex_session:
created: 2026-08-08
---

## Re-anchored against origin/dev (2026-08-08)

The first draft of this plan was written against a local checkout 47 commits
behind `origin/dev`. This pass re-verified every `file:line` reference in this
worktree (`origin/dev` at `2534219330`) and corrects the following:

- **`add_dialog.rs` is 752 lines today, not 815**, and the carve-out described
  below as T3 has partly already happened: `add_dialog_input.rs` and
  `add_dialog_subscription.rs` now exist and already own `classify_input`,
  `dialog_hint`, `subscribe`, `baseline_for_import_choice` and friends — all of
  which the original T3 planned to move. **T3 survives, but rescoped**: the
  only things still sitting in `add_dialog.rs` that are worth carving out are
  `Preview`, `append_heading`, `append_candidate`, `append_preview`,
  `images_allowed` and `candidate_row`. It is not optional busywork — see the
  arithmetic in T3 below for why skipping it risks tripping the 800-line gate
  once T7's wiring lands, even though the file is comfortably under the limit
  today.
- **The architecture gate's pre-existing violator list has changed
  completely.** Only one file sits at or over 800 lines in this checkout —
  `crates/reprise-core/src/library/tag_edit_write.rs` at 824 — and it is
  unrelated to podcasts. The six other files the original plan named
  (`cover.rs`, `radio/radio_view.rs`, `queue_tests.rs`, `radio/add_dialog.rs`,
  `external_media_state.rs`, `tag_mutation.rs`) have all been refactored back
  under the limit since. The "one thing that must be fixed before anything is
  added" section is rewritten to match.
- **Two `check-frontend-thinness.sh` budgets moved**: `rusqlite` is now 114
  (was 112 when the plan was written), and `view_floor` is now 1782 (was
  1352). The `workers` budget is still exactly 7, unchanged. These are
  ceiling-and-floor budgets, so citing the wrong number is not cosmetic — a
  task written against 112 would misdiagnose a passing run as a regression.
- **A round of `file:line` drift throughout**, all from ordinary commits
  landing on `dev` since the plan was drafted — none of it changes what the
  code does, only where. Every reference below has been re-checked against
  this worktree; the corrected numbers are inline. The rulebook edit in T8 in
  particular moves from a claimed `ux-rules.md:4059` to the real `4685`.
- **Nothing in the design decisions changed.** `.pipeline-spec.md` needed no
  correction; the itunes.rs, http.rs, radio_chips.rs and location.rs boundaries
  it and the plan both lean on are exactly where the plan assumed, almost line
  for line — the only real drift was in the GTK-side `podcasts/` directory,
  which has grown substantially since (60+ files now share that directory) and
  in the file-size gates, whose numbers are simply data that moves as the
  codebase does.

---

# Add Podcast: say when the last episode landed, and open on a country's charts

The design is settled in
`docs/superpowers/specs/2026-08-08-podcast-activity-and-charts-chip-design.md`.
Read it first; nothing in it is up for renegotiation here. This document is only
about *how* it gets built without duplicating what already exists, without
breaking the gates, and with enough of the work running in parallel to be worth
splitting.

Two behaviours ship together because they share one dialog and one country:

- **A.** Every Apple Podcasts result row states when the show last published.
- **B.** That dialog's one chip stops suggesting a genre and starts loading the
  country's charts.

---

## What already exists — reuse it, do not rebuild it

This is the section that decides how big the task actually is. Almost every
moving part is already in the tree.

**The HTTP boundary.** `podcasts::http::get`
(`crates/reprise-core/src/podcasts/http.rs:71`) is the one blocking GET for
everything podcast-shaped: a shared `ureq` agent, the Reprise user agent, the
10-second `SOURCE_REQUEST_TIMEOUT`, a process-wide 1-second rate limiter
(`MIN_REQUEST_INTERVAL`, line 22) and the `PodcastError` status fold
(`feed_status_error`, line 207). **Both** new chart requests go through it.
Building a second `ureq::Agent` would also trip `check-architecture.sh`'s
engine-HTTP-boundary budget, which is pinned at exactly 16 and is a floor as
well as a ceiling.

**The search parser.** `itunes::parse_results`
(`crates/reprise-core/src/podcasts/itunes.rs:76`) already projects Apple's
search envelope into `SearchResult` and already drops rows without a feed URL or
a title. Apple's `/lookup` answers in the same envelope, so the charts arrive
through this function unchanged. There is no second model and no second row
widget to build.

**`releaseDate` is not missing — it is discarded.** `SearchRow`
(`itunes.rs:26`) simply does not declare the field, so serde drops it.
`artwork_url600`/`artwork_url100` show the exact fallback shape to copy.

**The country fallback.** `itunes::locale_country`
(`crates/reprise-core/src/podcasts/itunes.rs:36`) already turns `de_DE.UTF-8`
into `DE` and falls back to `US`. It is already what `itunes::search` uses
(line 72). Do not write a second locale parser.

**The stored country.** `location::app_location` →
`AppLocation.country_code: Option<String>`
(`crates/reprise-core/src/location.rs:39`), populated only when the location
came from city search; the XDG-portal path honestly leaves it `None`
(`location.rs:20-33`). Radio already reads it exactly this way, without ever
touching a geocoder: `radio/add_dialog.rs:487` does
`app_location(&self.conn).ok().flatten()`. Copy that call shape — see the
thinness trap below for why `.ok().flatten()` and not `unwrap_or_else`.

**The chip precedent.** `radio_chips.rs` is the model to follow: the *decision*
is a pure function over plain values (`near_you_action`, `library_suggestion`,
lines 36 and 63) with its own unit tests and no display, and only the widget
build (`build`, line 98) needs GTK. A chip that runs a search directly rather
than through the entry already exists too —
`radio/add_dialog.rs:452 run_chip_search` — including the comment explaining
why the entry text is decoration rather than a reproducible query.

**Chip styling.** `chip.add_css_class("pill")` plus
`set_halign(gtk4::Align::Start)` (`podcasts/add_dialog.rs:104-107`). The shape
is decided; do not restyle it.

**Result rendering.** `candidate_row` (`add_dialog.rs:712`), `append_candidate`
(line 594), `append_heading` (line 586) and `attach_candidates` (line 390)
already render a `Vec<Candidate>` under a heading, run
`discovery::filter_unsubscribed` against the subscribed set, and guard against a
stale response with the generation counter. Chart rows are `Candidate`s like any
other and go through all of it untouched.

**The subtitle assembly point.** `add_dialog_results.rs:15 youtube_subtitle`
already shows the pattern the freshness sentence must copy: a pure function, the
`·` separator, and an optional trailing segment that is *appended*, never
substituted. `rss_candidate` (line 26) is where `subtitle` is set to the bare
author today.

**String plumbing.** `strings_podcasts.rs` already has `plural(...)`
(`podcast_episode_count`, line 204) and `formatted(...)`
(`podcast_chip_genre`, line 406) and is listed in `po/POTFILES.in:10`. No new
`strings_*.rs` file is needed, which also means no new entry in
`check-architecture.sh`'s POTFILES allowlist. Note that once T7 lands,
`podcast_chip_genre` itself becomes unreachable — the RSS dialog stops calling
it, and only `youtube_chip_genre` stays wired to a caller. That is expected
(§B of the spec deliberately drops the RSS genre chip) and harmless: the file
carries its own `#![allow(dead_code)]`, so nothing about the gates forces its
removal. It is called out here so nobody "fixes" the apparently-dead function
mid-task.

**The genre derivation stays.** `library::taste::top_genre`
(`crates/reprise-core/src/library/taste.rs:41`) keeps feeding the YouTube dialog
and radio. Nothing is orphaned and nothing gets deleted.

---

## The one thing that must be checked before anything is added

The original draft of this plan claimed `add_dialog.rs` was 815 lines and
already failing `check-architecture.sh`'s `>= 800` gate, alongside six other
files. Neither is true in this checkout: `add_dialog.rs` is **752 lines**, and
running the gate today turns up exactly **one** violator —
`crates/reprise-core/src/library/tag_edit_write.rs` at 824 lines, which has
nothing to do with this feature and is out of scope. The six other files the
original plan named have all been refactored back under 800 since it was
written.

So `add_dialog.rs` is not a fire to put out. But it is not free room either.
Wave 3's wiring (T7) adds new plumbing to exactly this file: a country
parameter threaded through `build_surface`, `present` and `search`, a new
`load_charts` function mirroring `search`'s RSS branch, and a chip
`connect_clicked` handler that now branches on two chip kinds instead of one.
That is roughly 45-65 lines of net addition, by the same kind of estimate the
original plan made for its own (larger) wiring pass. Landing all of that on
top of 752 lines puts the file at 800-820 — right on top of the gate, and
close enough that a slightly more verbose implementation tips it over.

The six functions that are still sitting in `add_dialog.rs` and are not pulled
into their own use by anything else in the file — `Preview` (a plain data
struct), `append_heading`, `append_candidate`, `append_preview`,
`images_allowed` and `candidate_row` — total roughly 170 lines and have zero
behavioural coupling to the wiring T7 is about to add. Moving them out first
is cheap insurance: it drops the file to roughly 585 lines before T7 touches
it, so the wiring lands around 630-650 instead of flirting with 800. **T3
below does exactly this, rescoped from the original plan's larger move (which
assumed `subscribe`/`baseline_for_import_choice` were still in this file —
they are not; that part of the original T3 already happened).**

Nothing else in the plan may add a line to `add_dialog.rs` until T3 has
landed, for the same ordering reason the original plan gave — it just is not
digging out of an existing hole any more, it is staying out of one that
T7 would otherwise dig.

---

## Waves and file ownership

Every task names the files it owns. **A file has exactly one owner per wave.**
An agent that needs to read another task's file may read it; it may not write
it.

| Wave | Tasks | Runs in parallel |
|---|---|---|
| 1 | T1, T2, T3 | yes — three disjoint files |
| 2 | T4, T5, T6 | yes — three disjoint files |
| 3 | T7 | alone; it is the wiring and it owns the dialog |
| 4 | T8, T9 | yes — docs and catalogs are disjoint |

| Task | Owns (writes) |
|---|---|
| T1 | `crates/reprise-core/src/podcasts/itunes.rs` |
| T2 | `crates/reprise-gnome/src/ui/strings_podcasts.rs` |
| T3 | `crates/reprise-gnome/src/ui/podcasts/add_dialog.rs`, `crates/reprise-gnome/src/ui/podcasts/add_dialog_rows.rs` *(new)*, `crates/reprise-gnome/src/ui/podcasts/mod.rs` |
| T4 | `crates/reprise-core/src/podcasts/itunes_charts.rs` *(new)*, `crates/reprise-core/src/podcasts.rs` |
| T5 | `crates/reprise-gnome/src/ui/podcasts/add_dialog_results.rs` |
| T6 | `crates/reprise-gnome/src/ui/podcasts/add_dialog_chips.rs` *(new)* |
| T7 | `crates/reprise-gnome/src/ui/podcasts/add_dialog.rs`, `crates/reprise-gnome/src/ui/podcasts/add_dialog_tests.rs`, `crates/reprise-gnome/src/ui/podcasts/add_dialog_rows.rs`, `crates/reprise-gnome/src/ui/podcasts/mod.rs` |
| T8 | `docs/ux-rules.md` |
| T9 | `po/reprise.pot`, `po/{ar,bn,de,es,fr,hi,zh_CN}.po` |

T3 and T7 both own `add_dialog.rs`, but they are two waves apart, so they never
run at the same time. T6 creates `add_dialog_chips.rs` but does **not** declare
it in `mod.rs` — T7 does, together with its own wiring, so `mod.rs` has one
owner per wave.

---

## Wave 1

### T1 — core reads `releaseDate`, and can be asked for a country directly

*Owns `crates/reprise-core/src/podcasts/itunes.rs`. Parallel with T2, T3.*

This file is unchanged from what the original plan assumed — it is 143 lines,
and every line reference below was re-verified against this worktree.

**Tests first**, all in the existing `mod tests` at the bottom of `itunes.rs`,
all pure string parsing — no fixture directory, no network, matching how
`search_parser_drops_rows_without_feed_url` (line 115) already works:

1. `src_18_a_search_row_carries_the_date_of_its_newest_episode` — feed
   `parse_results` a two-row envelope, one with
   `"releaseDate":"2026-08-04T04:00:00Z"` and one without, and assert the first
   yields `last_episode == Some(1_785_816_000)` while the second yields `None`.
2. `src_18_a_malformed_release_date_costs_only_its_own_row` — a row with
   `"releaseDate":"not a date"` parses to `last_episode: None` **and the
   surrounding rows survive with their own dates intact**. This is the
   assertion that stops a future refactor turning the parse into a `?`.
3. `search_url_and_country_search_agree_on_the_country` — `search_in_country`
   and `search` reach the same URL for a locale whose territory is that country.
4. Extend `search_parser_drops_rows_without_feed_url`'s expected literal with
   the new field (it is the only `SearchResult` struct literal in the
   workspace outside its own test module — `reprise-mcp`
   (`discovery_actions.rs:127`) reads fields, it does not construct the
   struct, so nothing else breaks).

**Then the change:**

- `SearchRow` gains `release_date: Option<String>` (serde's `rename_all =
  "camelCase"` already maps `releaseDate`).
- `SearchResult` gains `pub last_episode: Option<i64>` — Unix seconds, the
  currency the GTK side already deals in.
- A private `fn parse_release_date(value: Option<&str>) -> Option<i64>` using
  `chrono::DateTime::parse_from_rfc3339` (the crate is already a core
  dependency: `chrono = { version = "0.4", features = ["clock", "serde"] }`).
  Absent or unparseable is `None`, never an error.
- `pub fn search_in_country(terms: &str, country: &str)`; `search(terms,
  locale)` becomes `search_in_country(terms, &locale_country(locale))`. The
  existing MCP caller (`crates/reprise-mcp/src/discovery_actions.rs:127`) keeps
  using `search` and is not touched.
- `pub fn parse_results_with_ids(json: &str) -> Result<Vec<(Option<i64>,
  SearchResult)>, PodcastError>`, carrying `collectionId` alongside each row;
  `parse_results` becomes the thin projection that drops the id. T4 needs this
  seam — see **Open questions**, item 1, for why the spec as written cannot
  restore chart order without it.

### T2 — the vocabulary

*Owns `crates/reprise-gnome/src/ui/strings_podcasts.rs` (691 lines today, ample
room under the 800-line gate). Parallel with T1, T3.*

**Tests first**, in that file's existing `mod tests` (starting at line 571;
there is already a `podcast_episode_count` assertion around line 576 to sit
beside):

1. `freshness_wording_pluralises_and_drops_new_past_five_weeks` — asserts the
   English output of each helper at a representative count.

**Then the strings.** Follow the file's own idiom exactly: `plural(...)` takes
the two literals inline (xgettext is configured with `--keyword=plural:1,2`,
`scripts/tests/gettext-catalogs.sh:22`), `formatted(...)` takes an `N_!`
constant.

| Helper | English |
|---|---|
| `PODCAST_LAST_EPISODE_TODAY` | `New today` |
| `PODCAST_LAST_EPISODE_YESTERDAY` | `New yesterday` |
| `podcast_last_episode_days(n)` | `New {count} day ago` / `New {count} days ago` |
| `podcast_last_episode_weeks(n)` | `New {count} week ago` / `New {count} weeks ago` |
| `podcast_last_episode_months(n)` | `{count} month ago` / `{count} months ago` |
| `podcast_last_episode_on(date)` | `Last {date}` |
| `podcast_chip_popular_in_country(cc)` | `Popular in {country}` |
| `podcast_charts_heading(cc)` | `PODCASTS · TOP IN {country}` |

The singular forms of the day and week helpers are unreachable in English (the
day branch starts at 2, the week branch at 7 days = 1 week — so the week
singular *is* reachable; the day singular is not, but ngettext still needs it
and other locales have more than two plural forms).

`podcast_charts_heading` returns a `String`, not a `&'static str` — see T7 for
the `append_heading` signature change that makes that possible.

### T3 — carve the remaining rendering helpers out of `add_dialog.rs`

*Owns `add_dialog.rs`, new `add_dialog_rows.rs`, `podcasts/mod.rs`. Parallel
with T1, T2.*

**This task is scoped down from the original plan.** Its original move list —
`Preview`, `append_heading`, `append_candidate`, `append_preview`,
`images_allowed`, `candidate_row`, `subscribe`, `baseline_for_import_choice` —
assumed a checkout where none of that had been carved out yet. In this
worktree, `subscribe` and `baseline_for_import_choice` already live in
`add_dialog_subscription.rs` (imported into `add_dialog.rs` at the top of the
file today). Only six items are left to move, all still in `add_dialog.rs` at
these lines:

- `struct Preview` (`add_dialog.rs:39-48`)
- `append_heading` (586), `append_candidate` (594), `append_preview` (636)
- `images_allowed` (700, doc comment included), `candidate_row` (712)

**No behaviour change and no new test.** The proof is that the existing suite —
including the display tests in `add_dialog_tests.rs` — still compiles and passes
untouched.

Move all six into a new `crates/reprise-gnome/src/ui/podcasts/add_dialog_rows.rs`,
verbatim, doc comments included, each item `pub(super)`. `add_dialog.rs`
re-imports them with a single `use super::add_dialog_rows::{…};` next to the
existing `use super::add_dialog_results::{…};` and
`use super::add_dialog_subscription::{…};`. That import is what keeps
`add_dialog_tests.rs` compiling: the test module reaches those names through
its `use super::*;`, exactly as it already reaches `Candidate`, `PodcastKind`
and `strings`.

`mod.rs` gains `mod add_dialog_rows;` in alphabetical position (it currently
declares `add_dialog`, `add_dialog_input`, `add_dialog_results`,
`add_dialog_subscription`, then `css`, in that order). It inherits `mod.rs`'s
file-level `#![allow(dead_code)]`, so no new entry is needed in
`check-frontend-thinness.sh`'s dead-code allowlist — `podcasts/mod.rs` is
already listed there with count 1, covering every submodule it declares.

Expected result: `add_dialog.rs` ≈ 585 lines, `add_dialog_rows.rs` ≈ 190.
Verify with `wc -l` and by running `scripts/check-architecture.sh` and
confirming `add_dialog.rs` does not appear among its complaints (it never did
in this checkout — the point of this task is that it still does not after T7).

---

## Wave 2

### T4 — `top_podcasts(country)`

*Owns new `crates/reprise-core/src/podcasts/itunes_charts.rs` and the one `mod`
line in `crates/reprise-core/src/podcasts.rs`. Depends on T1. Parallel with T5,
T6.*

`crates/reprise-core/src/podcasts.rs` currently declares its submodules
alphabetically (`channel_window`, `config`, `discovery`, …, `itunes`,
`media_character`, …); `pub mod itunes_charts;` goes between `itunes` and
`media_character`.

**Tests first**, in that file's own `mod tests`, all pure — **no network, no
fixtures**:

1. `src_19_the_chart_request_uses_the_lowercase_storefront_code` — asserts
   `chart_url("DE")` is
   `https://rss.marketingtools.apple.com/api/v2/de/podcasts/top/12/podcasts.json`.
   Apple's marketing-tools path segment is lowercase; the chip label is
   uppercase, so exactly one of the two has to convert and it is this one.
2. `src_19_the_lookup_batches_every_charted_id_into_one_request` — asserts
   `lookup_url(&ids)` contains `id=` with the ids comma-joined in chart order
   and `entity=podcast`, and that it is **one** URL for all twelve.
3. `src_19_chart_ids_are_read_in_chart_order` — `parse_chart_ids` over a small
   `{"feed":{"results":[…]}}` literal returns the ids in the feed's own order.
4. `src_19_the_lookup_answer_is_restored_to_chart_order` — hand
   `in_chart_order` a deliberately shuffled row list and assert the ids come
   back in the requested order.
5. `src_19_an_id_the_lookup_drops_falls_out_rather_than_leaving_a_hole` —
   twelve ids in, eleven rows back, eleven results out, order preserved, no
   placeholder.
6. A malformed chart body yields `PodcastError::Parse`, not a panic
   (`PodcastError::Parse(String)` already exists, `podcasts.rs:134`).

**Then the change** — the whole module is four small pure functions plus one
that does I/O:

```
pub const CHART_LIMIT: usize = 12;
pub fn chart_url(country: &str) -> String
pub fn lookup_url(ids: &[String]) -> String
pub fn parse_chart_ids(json: &str) -> Result<Vec<String>, PodcastError>
pub fn in_chart_order(ids: &[String], rows: Vec<(Option<i64>, SearchResult)>) -> Vec<SearchResult>
pub fn top_podcasts(country: &str) -> Result<Vec<SearchResult>, PodcastError>
```

`top_podcasts` is the only function that touches the network: two
`super::http::get` calls, then `itunes::parse_results_with_ids` on the second
body, then `in_chart_order`. The chart feed's rows are deserialized with a
private serde shape (`feed.results[].id`) that lives in this file — that *is* a
second envelope, but it is a different endpoint returning a different document,
not a second copy of the search model.

Note that `http::get`'s 1-second global rate limit means `top_podcasts` costs
at least one second more than a single search. That is acceptable — it runs on
`one_shot_task`'s worker thread, and the dialog shows its "Searching…" status
throughout — but it is the reason the chip must never be fired speculatively on
dialog open.

### T5 — the freshness sentence

*Owns `crates/reprise-gnome/src/ui/podcasts/add_dialog_results.rs` (95 lines
today). Depends on T1 and T2. Parallel with T4, T6.*

**Tests first**, in that file's existing `mod tests` (starting at line 62,
which already holds the `src_9_…` subtitle tests — same shape, no display
needed):

1. `src_18_the_freshness_scale_walks_its_boundaries` — one table-driven test
   over ages of 0, 1, 2, 6, 7, 13, 14, 34, 35, 364 and 365 days, plus a
   timestamp **in the future**, asserting exactly the spec's table:

   | age in days | segment |
   |---|---|
   | future, 0 | `New today` |
   | 1 | `New yesterday` |
   | 2 … 6 | `New {n} days ago` |
   | 7 … 34 | `New {n} weeks ago`, n = days / 7 |
   | 35 … 364 | `{n} months ago`, n = days / 30 |
   | ≥ 365 | `Last {Mon YYYY}` |

2. `src_18_a_row_without_a_date_or_an_author_leaves_no_separator_behind` — the
   four combinations: author + date, author only, date only (**no leading
   `·`**), neither (empty string).

**Then the change:**

- `pub(super) fn last_episode_segment(last_episode: Option<i64>, now: i64) ->
  Option<String>` — pure, `now` injected so the test is deterministic. Age is
  `(now - last_episode).max(0) / 86_400` in whole seconds, then integer
  division by 7 and by 30. Deliberately **not** a calendar walk and deliberately
  **not** `podcasts_presentation::relative_date`'s local-date comparison; the
  spec calls this out as a second, intentional time scale. The `≥ 365` branch is
  the only one that needs a real date, and it formats
  `DateTime::<Utc>::from_timestamp(…).with_timezone(&Local).format("%b %Y")` —
  the same `Local` + `chrono` idiom `relative_date`
  (`podcasts_presentation.rs:179`) already uses.
- `pub(super) fn rss_subtitle(author: Option<&str>, last_episode: Option<i64>,
  now: i64) -> String` — joins with `" · "`, dropping either side cleanly.
- `rss_candidate` keeps its signature (`add_dialog.rs:339` maps over it inside
  `search`, unchanged by this task) and calls `rss_subtitle(row.author.as_deref(),
  row.last_episode, chrono::Utc::now().timestamp())`. `Utc::now()` stays out of
  the pure functions so only this one line is untestable. (T7 later touches the
  same `search` function to add the country parameter — the two tasks land on
  disjoint lines within it, but the line number above will drift once T7 lands;
  it is accurate as of the start of Wave 2.)

### T6 — the country chain and the chip decision

*Owns new `crates/reprise-gnome/src/ui/podcasts/add_dialog_chips.rs`. Depends
on T2. Parallel with T4, T5. Does **not** touch `mod.rs` — T7 declares it.*

Model this file on `radio/radio_chips.rs`: pure decisions with unit tests,
nothing that needs a display. `near_you_action` (`radio_chips.rs:36`) and
`library_suggestion` (line 63) are the pattern for the decision functions;
`build` (line 98) is the pattern for the one that needs GTK, which this file
does not.

**Tests first**, in the file's own `mod tests`:

1. `src_19_the_country_prefers_the_stored_location_over_the_locale` — an
   `AppLocation` with `country_code: Some("CA")` under a `de_DE.UTF-8` locale
   yields `CA`.
2. `src_19_a_location_without_a_country_falls_through_to_the_locale` — the
   XDG-portal case (`country_code: None`) yields `DE`, **not** radio's
   "Set location…" behaviour. This asymmetry with `RAD-5` is deliberate and the
   test is where it is written down.
3. `src_19_no_location_at_all_still_produces_a_country` — `None` yields the
   locale's country, and a broken locale yields `US` via `locale_country`.
4. `src_19_the_apple_dialog_offers_the_charts_chip_and_the_youtube_dialog_the_genre`
   — `chip_for` returns `Charts` for `PodcastKind::Rss` regardless of whether
   the library has a genre, and `LibraryGenre` for `PodcastKind::Youtube` only
   when it does.
5. `src_19_the_charts_chip_is_absent_offline` — `Connectivity::Offline` +
   `PodcastKind::Rss` yields `None`. The YouTube genre chip's offline behaviour
   is unchanged (it is a search term, not a network action, and `SRC-15a` still
   governs it).

**Then the change:**

```
pub(super) enum AddDialogChip {
    Charts { country: String },
    LibraryGenre { genre: String },
}

pub(super) fn dialog_country(location: Option<&AppLocation>, locale: &str) -> String
pub(super) fn chip_for(
    kind: PodcastKind,
    connectivity: Connectivity,
    country: &str,
    library_genre: Option<&str>,
) -> Option<AddDialogChip>
```

`dialog_country` uppercases the stored code and falls back to
`podcasts::itunes::locale_country(locale)`. `AddDialogChip::label()` returns
`strings::podcast_chip_popular_in_country` or the existing
`strings::youtube_chip_genre`.

---

## Wave 3

### T7 — wire it into the dialog

*Owns `add_dialog.rs`, `add_dialog_tests.rs`, `add_dialog_rows.rs`, `mod.rs`.
Depends on T3, T4, T5, T6. Runs alone.*

**Tests first**, in `add_dialog_tests.rs`:

1. Rename `src_15_the_library_chip_appears_only_with_a_genre_to_suggest`
   (currently at `add_dialog_tests.rs:131`) to `src_15a_…` and amend it: the
   **YouTube** half is unchanged (a genre gives a `youtube_chip_genre` pill, no
   genre gives no chip); the RSS half now asserts that an RSS surface built
   with a genre available carries the **charts** label, never
   `podcast_chip_genre`. `scripts/check-ux-traceability.sh:76` turns a test
   still named `src_15_…` into a hard error the moment `SRC-15` is marked
   replaced, so this rename and T8's document edit must land in the same
   commit. This test also currently calls `build_surface(kind, connectivity,
   genre)` with three arguments and reads `surface.library_chip` — both need
   updating for the new `build_surface` signature and the
   `library_chip` → `suggestion_chip` rename below.
2. `src_19_the_apple_dialog_carries_the_charts_chip_and_the_entry_stays_empty`
   *(display test,
   `#[ignore = "requires a display; run via xvfb-run"]`)* — build the RSS
   surface with a country, assert the chip's label is
   `strings::podcast_chip_popular_in_country("DE")`, that it has the `pill`
   class, and that after `emit_clicked()` the search entry is **still empty**.
   That last assertion is the whole difference from `SRC-15a`'s chip and the one
   a future refactor is most likely to undo.
3. `src_18_a_result_row_states_its_freshness_after_the_author` *(display test)* —
   render `candidate_row` with a subtitle produced by
   `add_dialog_results::rss_subtitle` and assert the rendered subtitle text.
   The scale itself is already proven without a display in T5; this one only
   proves the sentence reaches the widget. `candidate_row` lives in
   `add_dialog_rows.rs` after T3, but stays reachable from the test module the
   same way `Candidate` and `strings` already are — through `use super::*;`.

**Then the change** in `add_dialog.rs`:

- `AddDialogSurface.library_chip` becomes `suggestion_chip: Option<gtk4::Button>`
  and its doc comment re-points from `SRC-15` to `SRC-15a`/`SRC-19`.
- `build_surface` takes `(kind, connectivity, country: &str, library_genre:
  Option<&str>)` and asks `add_dialog_chips::chip_for` what to build. Keeping
  the raw inputs rather than a pre-built chip keeps the display tests able to
  drive both modes from one call.
- `present` resolves the country **once**:
  `let location = reprise_core::location::app_location(&conn).ok().flatten();`
  then `dialog_country(location.as_ref(), &locale)`. It keeps reading
  `top_genre` for the YouTube case. The resolved country is captured by the
  `submit` closure and passed down to `search`.
- `search` calls `podcasts::itunes::search_in_country(&terms, &country)` instead
  of reading `LC_ALL`/`LANG` itself, so the chip and the results below it can
  never mean two different storefronts. The locale lookup moves up into
  `present`, beside the location read.
- A new `fn load_charts(country, context: &SearchContext<'_>)` mirroring
  `search`'s RSS branch: `one_shot_task::spawn("reprise-podcast-charts", …)`
  calling `itunes_charts::top_podcasts`, mapping through the same
  `rss_candidate`, and handing the result to the same `attach_candidates` with
  `strings::podcast_charts_heading(&country)` as the heading.
- The chip's `connect_clicked` branches: `Charts` clears the results, bumps the
  generation and calls `load_charts` **without touching the entry**;
  `LibraryGenre` keeps today's behaviour exactly (set the entry text, then
  submit).
- `attach_candidates`' `heading: &'static str` parameter (`add_dialog.rs:398`
  today) becomes `heading: String`, and `append_heading` (now in
  `add_dialog_rows.rs` after T3) takes an already-translated `&str` instead of
  calling `strings::text` itself — today it does the opposite (takes a raw
  `&str` msgid and translates it internally at `add_dialog.rs:587`). Its two
  existing call sites (in `search`) pass raw `strings::PODCAST_APPLE_RESULTS` /
  `…PODCAST_YOUTUBE_RESULTS` today and must switch to
  `strings::text(strings::PODCAST_APPLE_RESULTS)` /
  `…PODCAST_YOUTUBE_RESULTS`, and
  `src_7_a_successful_subscribe_acknowledges_the_row_in_place`
  (`add_dialog_tests.rs:254`, which currently calls
  `append_heading(&parent, strings::PODCAST_APPLE_RESULTS)`) is updated the
  same way. This is why T7 also owns `add_dialog_rows.rs`.
- `mod.rs` gains `mod add_dialog_chips;`.

Watch the empty-result path: `attach_candidates` currently reports
`strings::source_nothing_found(&query)` when everything was filtered out. For
the charts path, pass the chip label as `query` so a fully-subscribed chart
reads sensibly rather than quoting an empty string.

Finish by re-checking `wc -l crates/reprise-gnome/src/ui/podcasts/add_dialog.rs`
— the wiring should land it around 630-650, comfortably under the 800-line
gate. That headroom exists *because* T3 ran first; without it the same wiring
would land on top of 752 lines instead of ≈585, and would sit at or over the
gate instead of well clear of it.

---

## Wave 4

### T8 — the rulebook

*Owns `docs/ux-rules.md`. Parallel with T9. Must land in the same commit as T7's
test rename (this is one squashed PR, so that is automatic — but do not merge
T8 alone).*

Three edits in the SRC section. `SRC-15` sits at `docs/ux-rules.md:4685`
today, not the `4059` the original draft cited — that number moved along with
every other line in the document as unrelated rules were added since. The
text of `SRC-15` itself is unchanged from what this plan assumed, so the
`SRC-15a` replacement text below needs no rewording, only correct placement.

**1. `SRC-15` is retired.** Its meaning genuinely narrows — it currently
*requires* a genre chip in the Apple Podcasts dialog and afterwards must forbid
one — and the process rule at line 18 is explicit: "If the meaning changes, a
new (sub-)rule replaces the old one." So the existing bullet
(`docs/ux-rules.md:4685-4696`) keeps its text verbatim and only its status
changes:

```
- **SRC-15** [replaced by SRC-15a] [core] [gtk] — **The add dialogs suggest from the
```

**2. `SRC-15a` succeeds it**, inserted immediately after `SRC-15`:

> - **SRC-15a** [active] [core] [gtk] — **The library chip suggests from the
>   library, never from a hard-coded taste — and it belongs to the surfaces a
>   genre is a real query for.** The YouTube add dialog carries one chip above
>   the result list holding the genre this library has spent the most listening
>   time on ("Metalcore channels"), and radio carries the same fact as its first
>   chip (`RAD-5`). Both read one shared derivation
>   (`library::taste::top_genre`), so they never disagree about what this library
>   listens to. Activating a chip fills the search field with the term it
>   searched for: the run stays visible, editable and repeatable, never a hidden
>   query. A library that has played nothing carrying a genre shows **no chip at
>   all** — an empty or invented suggestion is worse than none, and both dialogs
>   remain fully usable through their search field. The Apple Podcasts dialog
>   does **not** carry this chip: a bare genre word is a weak podcast search
>   term, and that dialog's one chip slot is spent on `SRC-19` instead.

**3. `SRC-18` and `SRC-19` are appended** immediately after `SRC-17`, which
ends at `docs/ux-rules.md:4737` (the next bullet, `POD-1`, follows directly).
`SRC-18` is still the next free number — verified across `docs/`, `crates/`
and `scripts/` in this worktree, unchanged from the original check:

> - **SRC-18** [active] [core] [gtk] — **An Apple Podcasts search result says
>   when the show last published.** Every RSS result row carries the age of its
>   newest episode as the second segment of its subtitle, after the author and
>   behind the same `·` separator the YouTube rows use: `New today`,
>   `New yesterday`, `New 4 days ago`, `New 2 weeks ago`, `3 months ago`, and
>   from a year onwards the absolute `Last Oct 2019`. "New" carries only while
>   the show is fresh, so past five weeks the phrasing drops to "… ago" and past
>   a year to "Last …" — the wording itself signals decay. Counts round **down**
>   and the unit is a plain divisor — 7 days to the week, 30 to the month, never
>   a calendar walk: 20 days is "2 weeks ago", and rounding down never claims a
>   show is staler than it is. A feed dated in the future — a mis-set timezone, a
>   scheduled episode — reads as `New today` rather than producing a negative
>   age. A result whose feed carries no usable date **drops the segment
>   entirely** rather than printing "unknown", and a result with no author drops
>   the leading separator with it. The date is read in core
>   (`itunes::SearchResult::last_episode`) as a Unix second, and one malformed
>   date costs that row its segment and never the other eleven results. This
>   scale is deliberately **not** `podcasts_presentation::relative_date`'s: that
>   one orders episodes the listener already subscribes to, this one judges a
>   stranger. YouTube channel rows carry no freshness at all — yt-dlp's channel
>   search yields only the upload dates of whichever videos the relevance
>   ranking surfaced, so a daily channel could read "last March".
>
> - **SRC-19** [active] [core] [gtk] — **The Apple Podcasts dialog opens on what
>   a country listens to.** Its one chip reads `Popular in DE`, and activating it
>   loads Apple's country chart **directly into the result list** — the search
>   entry stays untouched, because a chart has no search term to fill in with.
>   The section carries its own heading (`PODCASTS · TOP IN DE`) in the same
>   style as `PODCASTS · APPLE PODCASTS`, so a chart is never mistaken for the
>   results of a search nobody typed; submitting a text search afterwards
>   replaces the section, exactly as a second search replaces the first. The
>   country is resolved **once per dialog** from the stored app-level location's
>   country code (`O-4`), falling back to the system locale — unlike `RAD-5`,
>   a location that carries no country falls through to the locale rather than
>   offering "Set location…", because this chip has a working answer either way.
>   That same country drives the text search below it, so the chip and the
>   results it sits above can never mean two different catalogs. The label uses
>   the country **code**, matching `RAD-5`'s "Metal in DE": real country names
>   would need a translated table covering every Apple storefront. Chart rows are
>   ordinary search results — same row widget, same already-subscribed filtering
>   (`SRC-5`), same freshness segment (`SRC-18`) — assembled from the chart
>   feed's ids plus **one** batched lookup, restored to chart order, with ids the
>   lookup drops falling out silently rather than leaving a hole. Offline the
>   chip is **absent**, for the same reason search is (`NET-3` point 4): it is a
>   network action, and a pill that only reports failure is worse than none.

### T9 — the catalogs

*Owns `po/reprise.pot` and all seven `po/*.po`. Parallel with T8.*

`scripts/tests/gettext-catalogs.sh` is a hard gate and it is stricter than it
looks: every one of `ar bn de es fr hi zh_CN` must contain every new msgid
(`msgcmp --use-fuzzy --use-untranslated`), **zero** fuzzy entries anywhere, and
`de` and `es` must have **zero untranslated** messages. English placeholders in
those two fail the gate.

Regenerate the template with the same invocation the gate uses
(`gettext-catalogs.sh:22`), `msgmerge` each catalog, then translate. Suggested
German and Spanish, to be refined rather than invented from scratch:

| msgid | de | es |
|---|---|---|
| `New today` | `Neu heute` | `Nuevo hoy` |
| `New yesterday` | `Neu gestern` | `Nuevo ayer` |
| `New {count} day(s) ago` | `Neu vor {count} Tag(en)` | `Nuevo hace {count} día(s)` |
| `New {count} week(s) ago` | `Neu vor {count} Woche(n)` | `Nuevo hace {count} semana(s)` |
| `{count} month(s) ago` | `vor {count} Monat(en)` | `hace {count} mes(es)` |
| `Last {date}` | `Zuletzt {date}` | `Último {date}` |
| `Popular in {country}` | `Beliebt in {country}` | `Popular en {country}` |
| `PODCASTS · TOP IN {country}` | `PODCASTS · TOP IN {country}` | `PODCASTS · TOP EN {country}` |

The other five locales need the msgids present; leaving them untranslated is
allowed as long as `minimum_seed_messages` (100) still holds, which it does.

---

## Verification

**Per task, before handing off:**

```bash
cargo fmt --check
cargo clippy --locked --all-targets --workspace -- -D warnings
cargo test --locked --workspace          # bare `cargo test` only runs the gnome default member
```

Core-only tasks (T1, T4) can iterate faster with
`cargo test -p reprise-core --locked podcasts::itunes`, but the workspace run is
what counts. After any `reprise-core` change,
`cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` must be
empty.

**Display tests — one process at a time, never as a herd.** The ignored suite in
this repo is reproducibly flaky when run together: different tests go red on
different runs, and the run even reports different test counts. Only single runs
are evidence. Each of T7's two new display tests, individually:

```bash
dbus-run-session -- xvfb-run -a env \
  XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
  cargo test -p reprise-gnome --locked -- --ignored --exact \
  ui::podcasts::add_dialog::tests::src_19_the_apple_dialog_carries_the_charts_chip_and_the_entry_stays_empty
```

`GDK_BACKEND=x11` with `WAYLAND_DISPLAY` unset is not optional — without it
GTK4 ignores Xvfb entirely and the test window opens on the real desktop. And
**read the count before `passed`**: a `--exact` name that matches nothing still
reports `ok`, so `0 passed` means the test never ran.

The same tests must also pass through the gate runner, which is what CI-equivalent
review will use:

```bash
scripts/check-display-tests.sh --rule-named
```

**Gate scripts:**

```bash
scripts/check-architecture.sh        # see the caveat below
scripts/check-ux-traceability.sh
scripts/check-frontend-thinness.sh
scripts/tests/gettext-catalogs.sh
```

`check-architecture.sh` **passes clean on this checkout today**, except for
one unrelated pre-existing violator: `crates/reprise-core/src/library/tag_edit_write.rs`
at 824 lines. Record its complaint list before starting and after finishing
and confirm two things: `add_dialog.rs` has not joined the list (T3 exists to
keep it that way after T7's wiring lands), and no other new file has joined
it either. Do not claim the gate is fully green — that pre-existing violator
is out of scope — only that it is no worse.

`check-ux-traceability.sh` is the one that catches a half-done T7/T8: a test
named `src_15_…` against a `[replaced by SRC-15a]` rule is an error, and so is
an `[active]` `SRC-18`/`SRC-19` with no rule-named test.

**Manual pass** (a human, on a real desktop — headless cannot judge this):
open Add Podcast online and offline, press the chip, confirm the entry stays
empty and the heading names the country; then run a text search and confirm the
chart section is replaced rather than appended to.

---

## Risks and traps found while reading

1. **`add_dialog.rs` is not over the limit today — but T7's wiring would put
   it at risk without T3.** The file is 752 lines now; T3's carve-out (six
   rendering helpers, ≈170 lines) is what keeps T7's ≈50-line addition from
   landing on top of it. This is prevention, not a fix for an existing
   violation — the original plan's framing ("815 lines against a `>= 800`
   failure") described a different, older checkout and is corrected in the
   "one thing that must be checked" section above.

2. **The fixture router cannot see either new endpoint.**
   `http::fixture_route` (`http.rs:252`) maps `itunes.apple.com` to
   `AppleSearch(terms)` **only via a `term` query parameter** — `/lookup?id=…`
   has none and would return "no fixture route for request" — and
   `rss.marketingtools.apple.com` falls into the generic `Feed(url)` branch. This
   is why T4's tests are pure URL/parse tests and never call `top_podcasts`.
   Extending `FixtureRoute` is a real option for a later integration test but is
   deliberately **out of scope**: it would put a second owner on `http.rs`.

3. **`check-frontend-thinness.sh`'s rusqlite budget is exact — a ceiling *and* a
   floor, currently 114** (not 112 — that number has moved since the plan was
   drafted). Reading the location in T7 must use radio's
   `app_location(&conn).ok().flatten()` shape, exactly as `radio/add_dialog.rs:487`
   already does — that call site does not itself match the budget's counted
   patterns (`rusqlite::`, `use rusqlite`, `params!`, `.prepare(`,
   `.query_row(`, `Connection`), so copying it adds nothing to the count.
   None of T3's rescoped move list (`Preview`, `append_heading`,
   `append_candidate`, `append_preview`, `images_allowed`, `candidate_row`)
   touches rusqlite either, so the budget should read 114 unchanged from start
   to finish of this plan.

4. **`view_floor` in the same script is exact too — currently 1782** (not
   1352). Nothing in this plan may add a production line to `reprise-view`.
   All new logic goes to `reprise-core` or `reprise-gnome`.

5. **Do not name any new file `*worker*.rs`.** That script counts worker files by
   filename and the budget of 7 is exact, still accurate.

6. **The heading is not a `&'static str` any more.** `attach_candidates` and
   `append_heading` both need touching, and one of them lives in the file T3
   created. Handled by giving T7 ownership of both; a plan that split them across
   agents would deadlock.

7. **Chart order needs an id that `parse_results` throws away.** See Open
   questions, item 1.

8. **`Local` in the `≥ 365 days` branch.** `format("%b %Y")` under a local
   timezone is stable for a month-and-year at that distance, but a test fixture
   dated on the 1st at 00:30 UTC could flip months in a negative-offset zone.
   Pick mid-month timestamps in T5's test.

9. **Two HTTP calls behind one press.** `http::get`'s process-wide 1-second
   limiter applies to both, so the chip is ≥ 1 s slower than a search. It runs
   off the UI thread on `one_shot_task`, so nothing blocks — but the generation
   guard in `attach_candidates` matters more here than usual, because a user can
   easily type and submit a search while the charts are still in flight.

10. **A fully-subscribed chart.** `filter_unsubscribed` can empty a chart
    completely, and `attach_candidates` then prints
    `source_nothing_found(&query)`. Pass the chip label as the query so the
    message is readable.

11. **`chrono::Utc::now()` inside `rss_candidate`.** It is one untestable line
    by design; keep it there and out of the pure functions, or the whole
    boundary table becomes clock-dependent.

12. **`podcast_chip_genre` becomes unreachable after T7.** The RSS dialog
    stops calling it (the charts chip replaces it); only `youtube_chip_genre`
    stays wired. This is expected per the spec and is harmless under
    `strings_podcasts.rs`'s file-level `#![allow(dead_code)]` — noted so
    nobody spends a task "cleaning up" a function the design deliberately
    orphans.

---

## Resolved in the grill (2026-08-08)

All three questions below were put to the user and settled. Each landed on the
option this plan already assumed, so **no task changes**: build it as written.

1. **Chart ordering** → resolution **(a)**: `parse_results_with_ids` for the
   charts, `parse_results` as its projection for search and MCP. `SearchResult`
   stays as narrow as it is; no existing caller or test is touched.
2. **One rule or two** → **two**: `SRC-18` (freshness) and `SRC-19` (charts chip),
   for the reason T8 gives — one id per behaviour, or one of them can regress
   with the gate still green.
3. **`SRC-15` retirement** → **confirmed**, on the rulebook's authority
   (`ux-rules.md:18`, verified in the grill): retire to
   `[replaced by SRC-15a]`, add `SRC-15a`, re-hang the tests in the same commit.

The spec was corrected to match (question 1 exposed a genuine contradiction in
it); the record below is kept as written, since it is why the decisions went the
way they did.

---

**1. The spec's chart ordering is not buildable as written.**

§B says the lookup "answers in the same envelope as the search, so
`parse_results` consumes it unchanged", *and* that "the results are sorted back
into chart order by their position in the id list", *and* that ids the lookup
drops "fall out silently". Those cannot all hold today: `SearchResult`
(`itunes.rs:10`) carries `title`, `author`, `feed_url`, `episode_count` and
`image_url` — **no id**. After `parse_results` there is nothing left to match a
row against the requested id list, so neither the reordering nor the drop
detection can be performed. Matching on `feed_url` is not a substitute: the
chart feed does not carry one, which is the entire reason the lookup exists.

Two resolutions, both faithful to "no second model":

- **(a)** — assumed by T1/T4 above — add
  `itunes::parse_results_with_ids(json) -> Vec<(Option<i64>, SearchResult)>`
  and let `parse_results` become the projection that drops the id. Nothing about
  the shared model changes; the charts path simply reads one more field that is
  already in the payload.
- **(b)** add `pub collection_id: Option<i64>` to `SearchResult` itself. Simpler,
  one function instead of two, but it widens a struct used by search and by MCP
  for a need only the charts have.

The plan proceeds on (a) so the work is not blocked, but the choice belongs to
the grill, not to me.

**2. One new rule or two?**

§"Rules" says "a new rule beside it governs the charts chip" — singular. The
freshness behaviour in §A has no rule named for it. But `docs/ux-rules.md`'s
traceability process requires "exactly one primary rule ID per test", and
freshness and the charts chip are two independent behaviours with two
independent failure modes; hanging both off one ID would mean one of them can
regress with the gate still green. T8 therefore writes **two** rules, `SRC-18`
for freshness and `SRC-19` for the chip. If the intent was genuinely one rule,
say so and T8 collapses to `SRC-18` with the freshness tests renamed.

**3. `SRC-15` retirement was not in the spec's plan.**

§"Rules" says "`SRC-15` keeps governing radio and the YouTube mode, and loses
the RSS podcast case" — i.e. it expects `SRC-15` to be edited in place. The
rulebook's own process rule (`docs/ux-rules.md:18`) forbids that: a meaning
change gets a successor ID and the old one becomes a signpost. T8 follows the
rulebook and introduces `SRC-15a`, which costs one test rename
(`src_15_…` → `src_15a_…`). Flagging it because it is a deviation from the
spec's literal wording, made on the rulebook's authority rather than mine.
