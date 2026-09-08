---
slug: the-updates-popover-stops-updating-forever
worktree: /home/marvin/Projects/reprise-the-updates-popover-stops-updating-forever
branch: feature/the-updates-popover-stops-updating-forever
phase: shipped
codex_session:
created: 2026-09-05
---
# The Updates popover stops updating forever

Grilled 2026-09-05; every decision below is the grill's outcome, not a proposal.

## Problem (measured 2026-09-05, app 0.1.139, PID 3293202)

Opening the ✦ Updates popover shows „Updating …" with a progress bar that does
not move, for minutes, and it does so on practically every open. Two runs were
observed in one session, both started by the popover:

| run | started  | ended  | artists | failed |
|-----|----------|--------|---------|--------|
| 1   | 14:39:40 | ~14:42 | 50      | 13     |
| 2   | 14:47:44 | ~14:52 | 50      | 6      |

Run 2 re-fetched 20 artists that run 1 had finished eight minutes earlier.
`new_releases.last_completed_at` has been frozen at 2026-09-03 11:59:32 for two
days. Yesterday the hourly timer produced a run every hour from 11:43 to 16:43.

## Root causes

1. **The popover forces every run.** `crates/reprise-gnome/src/ui/updates/popover.rs:625`
   calls `artist_news::refresh(&conn, today, scope, true)`. `force = true` skips
   `artist_cache_is_fresh` (`crates/reprise-core/src/artist_news_pipeline.rs`,
   `FETCH_TTL_SECONDS` = 7 days), so every run spends requests on all 50
   candidates (20 top artists + 30 rotating rest, `artist_news_candidates.rs`).
2. **A run only counts when nothing failed.** `artist_news_pipeline.rs:291-295`
   advances `last_completed_at` only if `report.failures.is_empty()`. Every run
   has 6–13 transient MusicBrainz failures, so the timestamp never moves, and
   `refresh_due` (`artist_news_refresh.rs`, 6 h + jitter) is permanently true.
   Consequence: `maybe_background_refresh` starts a run on every popover show,
   every hourly tick (`REFRESH_TIMER_SECONDS` = 3600) and every app start.
3. **Each run takes minutes by design.** MusicBrainz allows one request per
   second (`musicbrainz.rs`, `MIN_REQUEST_INTERVAL`). One discography page per
   artist plus one detail request per locally owned album (84 in run 1) makes
   ~140 sequential requests. Not changed here.
4. **The footer hides the progress.** The pipeline reports
   `RefreshProgress { checked, total }`, but the popover passes `|_| {}` and
   `updates/footer_state.rs:26` hardcodes `Fetching { checked: 0, total: 0 }`,
   so the bar is indeterminate, and `ui/feed_footer.rs:230` pulses it exactly
   once per render, so it stands still. **NR-37 [active]** already requires
   „determinate checked/total artist progress" in the popover footer.
5. **Failures are invisible.** `report.failures` is collected and never
   logged; the journal contains no warning for either run.

A per-artist failed attempt also stamps `last_attempt_at`, so once `force` is
gone a transiently failed artist would stay „fresh" for seven days. That gap is
closed in the same change (D4), otherwise fix 1 trades a storm for a hole.

The Releases view (`releases/releases_view.rs:600`) forces too, but only from
its manual reload button; it has no automatic trigger and is out of scope.

## Goal

- A background check (popover show, hourly tick, app start) spends requests
  only on artists whose last check is stale; a check with nothing to do ends
  within a second.
- A finished check with per-artist failures counts as the most recent check.
  The next background check waits the regular interval.
- Failed artists are due again at the next check.
- The popover footer shows determinate progress from both feeds while a check
  runs (NR-37, CONC-15), and an indeterminate bar keeps pulsing.
- Every per-artist failure and every run summary appears in the log.

### Non-goals

- Changing the MusicBrainz rate limit, the request count per artist, or the
  candidate rotation (20 + 30).
- Changing the Releases view's manual reload (force stays: the user asked).
- New user-visible strings. The footer label stays „Updating …"; only the bar
  becomes determinate. No `po/` changes.
- Concerts cadence; it was handled by `the-concerts-list-stays-current`.

## Decisions (grill outcome)

**D1 — Background never forces, manual reload forces.** `start_fetch` gets an
explicit trigger instead of the bare `include_concerts: bool`:
`FetchTrigger::Background` → `FetchPlan { include_concerts: false, force: false }`,
`FetchTrigger::Manual` → `FetchPlan { include_concerts: true, force: true }`.
The mapping is a pure function so a rule-named test pins it. Rejected: a
non-forcing reload — the reload icon is the one place where „check everything
now" is the user's intent, and it now shows real progress.

**D2 — The due gate reads the last check start, whatever its outcome.** New
`artist_news_refresh::last_check_started_at(db)` =
`max(ledger.latest_attempt, settings.last_completed_at)`. Only
`maybe_background_refresh` switches to it. After an all-failed run the next
background check therefore waits the regular 6 h + jitter, not the next hourly
tick (rejected: a second time criterion in the gate). `latest_fetched_at` keeps
its meaning (last counted completion) for the footer age.

**D3 — Completion advances on a finished check unless nothing succeeded.**
`artist_news_pipeline.rs:291`: `report.failures.is_empty() || report.artists_fetched > 0`.
An aborted run (`refresh_result?` returns early) and an all-failed run keep the
previous age (NET-3: cache stays, previous age stays). A zero-candidate run
still advances (`nr_37_successful_empty_refresh_still_records_its_completion_time`).
The existing `nr_37_failed_refresh_preserves_the_previous_successful_age`
(`artist_news_progress_tests.rs:114`) is an all-failed fixture — one artist,
`fetch` always `Err(Transport)` — and stays exactly as it is. Rejected: „every
finished run counts", which would hide an outage in the footer.

**D4 — A failed attempt is never a fresh cache.** `artist_cache_is_fresh`
reads the ledger's `last_outcome`; only `Ok` and `Unmatched` within
`FETCH_TTL_SECONDS` count as fresh, `Failed` never does. Bounded cost: a
permanently failing artist costs one request per run, and the candidate
rotation already caps a run at 50 artists. Rejected: a 6 h backoff constant.

**D5 — Progress is summed across feeds.** The popover keeps
`{ news: (checked, total), concerts: (checked, total) }`;
`footer_state::aggregate` receives the sum and yields
`Fetching { checked, total }`. `feed_footer.rs` already renders a `Fraction`
when `total > 0`. Rejected: news-only (bar sits at 100 % while concerts run)
and „slower feed" (bar can jump back).

**D6 — Failures are logged in core, at the failure site.** Both
`record_failure` call sites emit
`tracing::warn!(artist = %candidate.name, %error, "New Releases: artist check failed")`;
the run end emits one `tracing::info!` with queued/fetched/unmatched/failed.
`SourceError`'s Display is the safe copy — no URLs, no bodies.

**D7 — A rule pins the cadence.** New **NR-41 [active] [core] [gtk]** (T7).
The rulebook has no rule about check cadence today; NR-37 covers the footer.
NR-39 is a duplicated ID on dev and is not touched; NR-40 is the last one, so
NR-41 is free.

**D8 — The indeterminate bar pulses on a timer.** `FeedFooter` is the shared
footer of the Releases, Concerts and Updates surfaces (CONC-15). While the
presentation is `Indeterminate` a glib timer pulses the bar; any other
presentation cancels it. Pattern: `ui/scan/scan_edge_line.rs:61-83`
(`PulseGeneration`, `PULSE_INTERVAL` from `ui/scan/scan_progress.rs`,
`crate::ui::motion::animations_enabled()`).

## Tasks

File lists are starting points, not fences: stop only if the contract itself
turns out wrong. Write each test first and see it fail for the stated reason
before implementing (red → green); report the failing assertion text. Tests
that pin a rule carry the rule ID as their name prefix (`nr_41_…`, `nr_37_…`,
`conc_15_…`) — the traceability gate reads them.

### T1 — A failed attempt is not fresh (core)

Files: `crates/reprise-core/src/artist_news_ledger.rs`,
`crates/reprise-core/src/artist_news_pipeline.rs`, tests in
`crates/reprise-core/src/artist_news_progress_tests.rs` (or a new
`artist_news_freshness_tests.rs` next to it, registered like its siblings).

1. Test `nr_41_a_failed_artist_is_due_again_at_the_next_check`: seed one
   candidate artist with an MBID in `tracks` (fixture shape as in
   `artist_news_progress_tests.rs:125-132`); `record_attempt(…, now - 3600,
   FetchOutcome::Failed, 0)` under the artist's normalized key; run
   `refresh_with_progress_at` with `force = false` and a fetch closure that
   counts calls; assert ≥ 1 call. Control arm in the same test: the same seed
   with `FetchOutcome::Ok` → 0 calls. Fails today because
   `artist_cache_is_fresh` ignores the outcome.
2. Add `pub fn last_attempt(conn, artist_key) -> Result<Option<LedgerAttempt>>`
   with `LedgerAttempt { at: i64, outcome: FetchOutcome }` (parse
   `last_outcome`; unknown text → `Failed`). Keep `last_attempt_at` for its
   existing callers.
3. `artist_cache_is_fresh` → fresh iff outcome ∈ {Ok, Unmatched} and
   `now - at <= FETCH_TTL_SECONDS`.

### T2 — A partially failed check still counts (core)

Files: `crates/reprise-core/src/artist_news_pipeline.rs`, tests in
`artist_news_progress_tests.rs`.

1. Test `nr_41_a_partially_failed_check_still_advances_the_checked_timestamp`:
   two candidates, fetch closure returns `Err(FetchError::HttpStatus(503))` for
   the first artist's URLs and valid bodies for the second (reuse the
   successful fixture bodies the neighbouring tests use); assert
   `latest_fetched_at` == the injected completion time and `report.failed == 1`.
   Fails today (completion withheld).
2. Change the condition at line ~291 to
   `report.failures.is_empty() || report.artists_fetched > 0`.
3. Leave `nr_37_failed_refresh_preserves_the_previous_successful_age` untouched;
   it is the all-failed arm of D3 and must stay green.

### T3 — The due gate reads the last check start (core + gnome)

Files: `crates/reprise-core/src/artist_news_refresh.rs`,
`crates/reprise-core/src/artist_news.rs` (re-export),
`crates/reprise-gnome/src/ui/updates/popover.rs` (`maybe_background_refresh`).

1. Test (core, `artist_news_refresh.rs` `#[cfg(test)]`, next to
   `latest_fetched_at_returns_the_maximum_across_rows`):
   `last_check_started_at_prefers_the_newest_of_attempt_and_completion` — seed
   `last_completed_at = 100` and a ledger attempt at 500 → `Some(500)`; only the
   completion → `Some(100)`; nothing → `None`.
2. Add `pub fn last_check_started_at(db) -> Result<Option<i64>>` and export it
   through `artist_news.rs`.
3. `maybe_background_refresh` (popover.rs:451) calls it instead of
   `latest_fetched_at`; the `render` path (popover.rs:392) keeps
   `latest_fetched_at`.

### T4 — Failures reach the log (core)

Files: `crates/reprise-core/src/artist_news_pipeline.rs`.

1. Both `record_failure` call sites: `tracing::warn!` with `artist` and
   `%error` (D6). At the end of `refresh_with_progress_at`, one
   `tracing::info!(queued = report.artists_queued, fetched = report.artists_fetched,
   unmatched = report.unmatched, failed = report.failed, "New Releases: check finished")`.
2. No test; verified by reading the journal in acceptance.

### T5 — The popover distinguishes background from manual (gnome)

Files: `crates/reprise-gnome/src/ui/updates/popover_fetch.rs`,
`crates/reprise-gnome/src/ui/updates/popover.rs`,
`crates/reprise-gnome/src/ui/updates/popover_tests.rs`.

1. Test `nr_41_a_background_check_never_forces_and_the_reload_does` on the pure
   mapping `FetchPlan::for_trigger(FetchTrigger)`:
   Background → `{ include_concerts: false, force: false }`,
   Manual → `{ include_concerts: true, force: true }`.
2. `start_fetch(self, trigger: FetchTrigger)`; call sites:
   `footer.connect_reload` (popover.rs:236) → Manual;
   `maybe_background_refresh` (popover.rs:459) and `enabled_changed`
   (popover.rs:518) → Background. `fetch_from_database(path, force, on_progress)`
   passes `force` through to `artist_news::refresh_with_progress`.
3. `popover.rs` is 630 lines; keep the enum, the plan struct and the mapping in
   `popover_fetch.rs` so `popover.rs` stays clear of the 800-line cap.

### T6 — The footer shows determinate progress from both feeds (gnome)

Files: `crates/reprise-gnome/src/ui/updates/popover_fetch.rs`,
`crates/reprise-gnome/src/ui/updates/popover.rs`,
`crates/reprise-gnome/src/ui/updates/footer_state.rs`,
`crates/reprise-gnome/src/ui/updates/popover_tests.rs`.

1. Test `nr_37_the_popover_footer_shows_determinate_progress_from_both_feeds`
   on `footer_state::aggregate`: fetching with news (12, 50) and concerts
   (3, 20) → `Fetching { checked: 15, total: 70 }`; fetching with no progress
   yet → `Fetching { checked: 0, total: 0 }`. Fails today (hardcoded 0/0).
2. `aggregate(…)` takes the run's progress (e.g. `fetching: Option<FeedProgress>`
   with two `(checked, total)` pairs) and sums. Update the existing aggregate
   tests' call sites; their expectations do not change.
3. News: `one_shot_task::spawn_with_progress("reprise-new-releases", …)` with
   `refresh_with_progress`; a `spawn_future_local` loop stores news progress
   and re-renders. Mirror `releases/releases_view.rs:527-560`. Concerts:
   `concerts_runtime.request_with_progress(request, sender)`
   (`concerts/concerts_worker.rs`, used by `concerts_view.rs:537`) and the same
   loop for `ConcertsProgress { checked, total }`. Reset both pairs when
   `finish_feed` completes the run. `check-architecture.sh` keeps a whitelist
   of `one_shot_task` consumers (lines ~406-417); `popover_fetch.rs` already
   consumes it, so no change is expected there — if the gate complains, add the
   file to that list rather than bypassing the helper.

### T7 — Rulebook (docs)

Files: `docs/ux-rules.md` (section R, append after NR-40; IDs are append-only).

> - **NR-41** [active] [core] [gtk] — A background New Releases check
>   (popover show, hourly tick, app start) spends requests only on artists whose
>   last successful or unmatched check is older than the seven-day cache
>   window; a failed artist is due again at the next check. Only the footer's
>   reload button forces every queued artist. A finished check counts as the
>   most recent check even when some artists failed — its timestamp advances
>   and the next background check waits the regular interval — while a check in
>   which no artist succeeded, or that aborted, keeps the previous age (NET-3).
>   The next background check is always measured from the last check's start,
>   whatever its outcome. NR-37's footer progress is unchanged.
>   Tests: `nr_41_a_failed_artist_is_due_again_at_the_next_check`,
>   `nr_41_a_partially_failed_check_still_advances_the_checked_timestamp`
>   (`reprise-core`), `nr_41_a_background_check_never_forces_and_the_reload_does`
>   (`ui/updates/popover_tests.rs`).

Run `scripts/check-ux-traceability.sh` on the combined tree.

### T8 — An indeterminate footer keeps pulsing (gnome, shared widget)

Files: `crates/reprise-gnome/src/ui/feed_footer.rs`,
`crates/reprise-gnome/src/ui/scan/scan_progress.rs` (visibility only).
`PulseGeneration` (line 17) and `PULSE_INTERVAL` (line 12, 100 ms) are
`pub(super)` today; widen both to `pub(in crate::ui)` — do not copy them.

1. Test `conc_15_an_indeterminate_footer_keeps_pulsing_until_the_state_changes`
   in `feed_footer.rs`'s `mod tests`, marked
   `#[ignore = "requires a display; run via xvfb-run"]` like the existing
   display test at line 310: apply an `Indeterminate` presentation → a
   `#[cfg(test)] fn pulse_is_running(&self) -> bool` returns true; apply a
   `Fraction` presentation → false; apply `Indeterminate` then a presentation
   without progress → false. Fails today (no timer exists).
2. `apply_presentation`: `Indeterminate` → start a pulse generation and a
   `glib::timeout_add_local(PULSE_INTERVAL, …)` that pulses while the
   generation is current and `animations_enabled()`; `Fraction` and `None` →
   cancel the generation. Copy the control flow of `scan_edge_line.rs:61-83`
   (weak widget upgrade, `ControlFlow::Break` on a stale generation).
3. The display test is compiled by `cargo test -p reprise-gnome` and run in the
   review phase via `scripts/check-display-tests.sh` — not by Codex.

## Verification

### Codex verification scope

The change touches `crates/reprise-core`, `crates/reprise-gnome` and
`docs/ux-rules.md` only. Run, in this order:

```
cargo fmt --check
cargo clippy -p reprise-core -p reprise-gnome --all-targets -- -D warnings
cargo test -p reprise-core -p reprise-gnome
scripts/check-architecture.sh
scripts/check-ux-traceability.sh
```

Do NOT run: gradlew, uniffi-bindgen, the Android suite, the display suite
(`scripts/check-display-tests.sh`, xvfb), `cargo test --workspace`,
`cargo audit`, or `scripts/check-merge-readiness.sh`. If AGENTS.md or a gate
document tells you to run the full gate before committing, that instruction
does not apply to this run — this exception is deliberate and stated here; the
full gate runs in the review phase.

Report the exact final test counts for both packages and, per new test, the
red-before-green evidence (the failing assertion text before the fix).

### Acceptance (human, on the built binary, control arm first)

1. Baseline: `select count(*) from artist_news_fetch where last_attempt_at >= strftime('%s','now')-600`
   and `select value from settings where key='new_releases.last_completed_at'`.
2. Open the popover twice within a minute. Before the fix: 50 new attempts per
   open. After: the first open may run a check over stale artists only, the
   second open adds 0 attempts and the footer settles within a second.
3. During a manual reload the footer bar advances as a fraction; before the
   first progress event it visibly pulses. `journalctl --user -f` shows one
   `New Releases: check finished` line plus a `warn` per failed artist naming
   the artist and the error kind.
4. After a run with ≥ 1 failure and ≥ 1 success, `last_completed_at` equals the
   run's end; a following popover open within 6 h does not start a run.

## Parallelität

Attempted cut: A = core cadence + popover (T1–T7), B = pulse timer (T8,
`ui/feed_footer.rs` only — disjoint, no compile dependency, no share of
`docs/ux-rules.md`). An earlier cut core/gnome fails outright: the popover
calls the new `last_check_started_at`, `force = false` without D4 opens the
seven-day hole if it lands first, and both halves write the NR-41 block.

The grill chose **one strand**: B is ~40 lines, a second worktree plus a
Workflow review buys perhaps ten minutes of wall-clock for more coordination
than it saves. Merge order and post-merge cross-checks: n/a;
`check-ux-traceability.sh` and the display test run on the single tree.
