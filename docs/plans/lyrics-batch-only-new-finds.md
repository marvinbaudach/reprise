---
slug: lyrics-batch-only-new-finds
worktree: /home/marvin/Projects/reprise-lyrics-batch-only-new-finds
branch: feature/lyrics-batch-only-new-finds
phase: planned
codex_session:
created: 2026-08-29
---
# Lyrics batch: automatic runs cover only new finds

## Goal

An automatic lyrics pass covers only tracks that entered or changed in the
library since the last completed pass — not the whole library on every launch.
A full sweep still happens when nothing is known to be covered, at most once
every 30 days, and whenever the user switches the module on.

## Why (measured, not assumed)

`LyricsBatch::start()` (`crates/reprise-gnome/src/ui/lyrics/lyrics_batch.rs:137`)
unconditionally calls `query_live_track_summaries` → every present track. The
existing due-check `should_run_time_window(TimeWindowTask::Lyrics, …)` gates only
**whether** a pass runs (skip if the last clean exit is younger than
`STARTUP_SCAN_WINDOW_SECONDS` = 15 min), never **what** it covers. So every normal
launch walks the full library; `NeedsFetch::Skip` items still call
`BatchProgress::advance`, which is why the toast crawls 0→100 % over 1 999 tracks
while almost nothing is fetched.

Two costs follow:

- **Per launch:** two filesystem probes per track (`local_hit_with_source` +
  `cache::read_cache`) for the entire library, serially, with a visible toast
  that looks like a hung app in every screenshot.
- **Per week:** `NEGATIVE_TTL_SECONDS` (7 d, `lyrics/cache.rs`) expires, so
  known-unavailable tracks are re-asked at LRCLIB/NetEase on the first launch
  after the window. Observed in the live app: 16 `unavailable` within the first
  134 checked tracks.

The sweep is pure prefetch — `lyrics_worker.rs` calls
`load_or_fetch_with_options` per displayed track (LYR-2), so nothing the user
looks at depends on the batch having run. What the batch does add is the `.lrc`
sidecar write (LYR-7) for tracks the user never opens, which device sync then
carries along; that is why the change keeps a full sweep rather than dropping
background coverage altogether.

## Things you must not relearn (hard-won)

- **`LYR-6` is an `[active]` rule and this change edits its text.** It currently
  promises a run "for the present library" on three triggers. Narrowing the work
  set without amending `docs/ux-rules.md` orphans the rule.
  `scripts/check-ux-traceability.sh` enforces that every `[active]` rule has ≥ 1
  test whose *function name* carries the ID (`fn lyr_6_…`), so a new clause needs
  a new `lyr_6_*` test, not just prose. **Do not** introduce a new rule ID for
  this — decided in the grill, and the predecessor plan `lyrics-batch-to-core.md`
  already refused to split LYR-6 once.
- **The "after completed library scans" trigger is not a call site.** Only two
  triggers are direct: window construction
  (`ui/window/window_runtime_wiring.rs:472`, `lyrics_batch.start_after_cover(…)`)
  and `recompute_enabled()` → `PermissionEffect::Start` in
  `ui/preferences/preference_online_module_effects.rs:71`. The third is
  **transitive**: `start_after_cover` subscribes to the cover batch's progress
  with an always-true liveness probe, and that subscription lives for the whole
  process, so it re-fires on every later cover-batch completion — including the
  one a rescan causes. `fn lyr_6_cover_completion_subscription_rearms_for_later_library_scans`
  (`ui/progress_subscribers.rs:174`) is the only thing guarding it. Do not turn it
  into a one-shot.
- **`Db` is deliberately neither `Send` nor `Sync`** (`db_handle.rs:19`). The batch
  queries on the calling thread *before* spawning the worker. Keep it that way:
  read both settings keys and run the query on the GTK thread, hand the worker
  only `Vec<BatchTrack>`.
- **A cancelled pass must not advance the watermark.** `run_batch` returns
  `Cancelled` for a generation bump, an explicit cancel, and `network_allowed()`
  going false mid-run; `BatchState::Failed` is a separate terminal state when all
  breakers are open. Recording completion in any of those cases permanently skips
  the tracks that pass never reached — the one bug in this change that would be
  invisible for weeks.
- **The watermark is the pass's *start* time, not its end time.** A track imported
  while the pass runs must be covered by the *next* pass. Capture `now_unix()`
  before the query, persist that value on completion.
- **`start()`'s full sweep also resets the 30-day clock, and that is safe.**
  Verified 2026-08-29, do not re-derive: the lyrics arm of
  `preference_online_module_effects.rs:71` compares `permission_enabled()` against
  `republish_enabled()`, both of which come from
  `online_sources::network_allowed_or_off` — persisted module state and the global
  online-sources gate only. Live connectivity enters just the *artwork* arm via
  `artwork_effect_for_transition`, exactly as `NET-5` describes. So
  `PermissionEffect::Start` cannot fire on a reconnect, and no connectivity flap
  can turn into a full sweep.
- **The two stored timestamps have deliberately different strictness.** The
  watermark carries correctness and moves only on `Complete`. The full-sweep clock
  carries cadence and moves on *attempt*, so a repeatedly aborted monthly sweep
  cannot turn into a crawling toast on every launch. Do not "fix" this asymmetry.

## Design

### 1. Two settings keys (core)

| key | written | meaning |
|---|---|---|
| `startup_tasks.lyrics_watermark` | on `Complete`, any scope | start time of the last **completed** pass; the `since` value |
| `startup_tasks.lyrics_full_sweep` | at **start** of a full-scope pass | start time of the last full-sweep **attempt**; the cadence clock |

Values are unix seconds as a plain decimal string. Deliberately *not*
`SignatureTask`'s `TaskRecord` JSON — a time-window task has no library signature
to settle, and reusing the struct invites the wrong comparison. An absent or
unparsable value logs a warning and reads as `None`, i.e. "run full" — the same
conservative posture as `exact_signature_decision`'s invalid-record branch.

New constant next to `STARTUP_SCAN_WINDOW_SECONDS` in
`crates/reprise-core/src/library/startup_tasks.rs`:

```rust
pub const LYRICS_FULL_SWEEP_INTERVAL_SECONDS: i64 = 30 * 24 * 60 * 60;
```

### 2. Scope decision (core, pure and testable)

```rust
pub enum LyricsScope { Everything, AddedSince(i64) }

pub fn lyrics_scope(watermark: Option<i64>, last_full_sweep: Option<i64>, now: i64)
    -> LyricsScope
```

In this order — the order is the specification:

1. `watermark` is `None` → `Everything`. Nothing is known to be covered, so there
   is no `since` to narrow by. This is what makes the very first launch after the
   change sweep once, and keep sweeping until one pass actually completes.
2. `last_full_sweep` is `None`, or `now - last_full_sweep >= LYRICS_FULL_SWEEP_INTERVAL_SECONDS`,
   or the difference is negative (clock moved backwards) → `Everything`.
3. otherwise → `AddedSince(watermark)`.

Keep this a free function over plain values so it is exhaustively testable in core
without a `Db`, a GTK loop, or a clock.

### 3. Pass carrier (core)

Mirror `ExactTaskPass`'s shape, including its `tracing::warn!` on a failed write:

```rust
pub struct LyricsPass { started_at: i64, scope: LyricsScope }

pub fn begin_lyrics_pass(db: &Db, scope: LyricsScope) -> LyricsPass;
// captures now_unix(); for LyricsScope::Everything it writes
// `startup_tasks.lyrics_full_sweep = started_at` immediately (attempt semantics)

impl LyricsPass {
    pub fn scope(&self) -> LyricsScope;
    pub fn record_completed_or_warn(self, db: &Db);   // writes the watermark
}

pub fn lyrics_watermark(db: &Db) -> Option<i64>;
pub fn lyrics_last_full_sweep(db: &Db) -> Option<i64>;
```

A narrow pass never touches `lyrics_full_sweep`.

### 4. Narrow query (core)

`crates/reprise-core/src/queries/maintenance.rs`:

```rust
pub fn query_track_summaries_added_since(db: &Db, since: i64)
    -> Result<Vec<TrackSummary>, rusqlite::Error>
```

```sql
SELECT <same columns> FROM tracks
WHERE {PRESENT} AND (added_at > ?1 OR file_mtime > ?1)
ORDER BY path
```

`file_mtime` is in the `OR` on purpose: a re-tagged track keeps its `added_at` but
gets a new `cache_identity()`, so without it a retag would never be re-checked.
Both columns exist on `tracks` (schema v1 / v2).

Share the column list and the row mapper with `query_live_track_summaries` (one
`const` + one `fn row_to_summary`) — the two must not drift. Export from
`queries/mod.rs` next to the existing name.

**Known, accepted gap:** a track that was `missing_since` during a completed pass
and is later relinked keeps its old `added_at`/`file_mtime` and is therefore never
picked up by a narrow pass. There is no column marking a return (`relink` only
nulls `missing_since`). The 30-day full sweep is what eventually covers it.
A track added in the exact same second a pass's start is recorded is likewise
not picked up by the next narrow pass; the 30-day full sweep covers it.

### 5. Scope selection and recording (gnome)

In `lyrics_batch.rs`, one private entry point:

```rust
fn start_with_pass(self: &Rc<Self>, pass: LyricsPass)
```

- `start()` — unchanged signature and unchanged externally visible behaviour:
  always `LyricsScope::Everything`. Every explicit trigger keeps the full sweep
  (module switched on, LYR-6's third trigger), and by §3 that also resets the
  monthly clock.
- `start_automatically()` — reads both keys, calls `lyrics_scope(…, now_unix())`,
  and starts the resulting pass. The 15-minute time-window gate stays in front of
  it, entirely unchanged.

`record_completed_or_warn` is called in exactly two places, both already existing
branches:

- the early return when the work set is empty (`progress.state == Complete` right
  after `BatchProgress::running(len)`) — an empty narrow set **is** a completed
  pass and must advance the watermark, otherwise the window grows without bound;
- in the `glib::spawn_future_local` progress loop, when a `WorkerEvent::Progress`
  arrives with `state == Complete`.

Never on `WorkerEvent::Cancelled`, never on `Failed`, never on the
`failed_progress(...)` paths.

## Tasks

1. **core — keys, constant, carrier.** `startup_tasks.rs`:
   `LYRICS_FULL_SWEEP_INTERVAL_SECONDS`, `LyricsPass`, `begin_lyrics_pass`,
   `record_completed_or_warn`, `lyrics_watermark`, `lyrics_last_full_sweep`.
   Tests: absent key → `None`; round-trip; unparsable value → `None` and a warn;
   `begin_lyrics_pass` writes `lyrics_full_sweep` for a full scope and leaves it
   untouched for a narrow one; the writes do not disturb the `SignatureTask`
   records.
2. **core — scope decision.** `lyrics_scope` as a free function plus exhaustive
   tests over its three branches, including `last_full_sweep` exactly at the
   interval boundary and a `now` earlier than the stored timestamp.
3. **core — narrow query.** `queries/maintenance.rs` + `queries/mod.rs` export,
   sharing the column list with `query_live_track_summaries`. Tests in
   `queries/tests_maintenance.rs`: only rows newer than `since`; old `added_at`
   with new `file_mtime` is included; `missing_since`/`removed_at` rows stay
   excluded; stable path order; `since = 0` returns the same set as
   `query_live_track_summaries`.
4. **gnome — wiring.** `lyrics_batch.rs` per §5.
5. **gnome — tests.** `lyrics_batch_tests.rs`, following the existing
   `crate::test_db::open()` construction:
   - `lyr_6_an_automatic_pass_covers_only_tracks_added_since_the_last_completed_one`
   - `lyr_6_a_cancelled_pass_leaves_the_watermark_untouched`
   - `lyr_6_a_library_that_never_completed_a_pass_is_swept_in_full`
   - `lyr_6_a_full_sweep_attempt_defers_the_next_one_by_the_full_interval`
   - `lyr_6_switching_the_module_on_still_sweeps_the_full_library`
   - `lyr_6_an_empty_narrow_pass_still_advances_the_watermark` — no running
     progress, watermark moved. If this one is wrong the window grows without
     bound and the full sweep silently returns forever.
6. **docs — LYR-6.** Amend `docs/ux-rules.md`, ID and `[active] [core] [gtk]` tags
   unchanged. Insert after the existing trigger sentence:

   > An automatic run covers only tracks added to the library or changed on disk
   > since the last **completed** run; a library that has never completed one, a
   > library whose last full sweep is more than 30 days old, and every run started
   > by switching the module on cover the present library in full. A run that is
   > cancelled or fails does not advance that mark, so its unreached tracks belong
   > to the next run; a full sweep defers the next scheduled one from the moment
   > it starts, whether or not it finishes.

   Keep every following sentence (skip rules, 250 ms host spacing, ScanControls
   counts, breaker behaviour) verbatim.

## What this deliberately gives up

- **The 7-day negative retry no longer reaches old tracks from the batch**, except
  through the 30-day full sweep. Unchanged for on-demand lookups (LYR-2), where
  the TTL still applies. Same for `RetryForSynced`, the once-per-window upgrade of
  a cached plain result to synchronized text.
- **A relinked track is not a new find** (§4). The monthly sweep covers it.
- **A missed monthly sweep is not made up.** The clock moves on attempt, so an
  aborted sweep waits another 30 days. Deliberate: the alternative is a permanent
  crawling toast for anyone who closes the app early.
- **A clock moved backwards** resolves to a full sweep (§2 rule 2), which is noisy
  but never wrong. No monotonic counter is introduced for this.
- **Before the first completed pass there is no protection at all** — rule 1 wins
  over the attempt clock, so every launch sweeps until one finishes. Accepted in
  the grill as the price of an honest watermark.

## Verification

Local, in the worktree, before any review. Write output to `$SCRATCH/<name>.log`
and answer the question with `grep`/`wc`; never read a verdict through a pipe
(`script | tail` reports tail's exit status, so it is always 0).

- `cargo test -p reprise-core lyrics`
- `cargo test -p reprise-core startup_tasks`
- `cargo test -p reprise-core maintenance`
- `cargo test -p reprise-gnome lyrics`
- `cargo test -p reprise-gnome progress_subscribers` — the re-arm test above
- `scripts/check-ux-traceability.sh` — must stay green after the LYR-6 edit
- `scripts/check-architecture.sh` and `scripts/check-frontend-thinness.sh` — the
  query, the scope decision and the keys belong in core; the gnome side stays wiring
- `scripts/check-lyrics-smoke.sh`

Manual control arm, once, on a throwaway DB copy — never
`~/.local/share/reprise/reprise.db`:

1. launch → full sweep, both keys written; let it complete
2. quit cleanly, clear the clean-exit record (or wait past 15 min), launch again
   → no toast at all
3. add one file, rescan, launch → the card reports a total of 1
4. set `startup_tasks.lyrics_full_sweep` back by 31 days, launch → full sweep again

## Parallelität

**This plan is not cut into strands.** Tasks 4 and 5 cannot compile without the API
tasks 1–3 introduce — a compile-time precondition, not merely a merge order, so a
second worktree would sit blocked rather than working. The only genuinely
independent piece is task 6, an ~8-line edit in `docs/ux-rules.md`; giving a doc
edit its own worktree, branch, PR and CI run costs more wall-clock than the
parallel run saves. Confirmed in the grill.

One strand, one worktree. File ownership:

- `crates/reprise-core/src/library/startup_tasks.rs` (+ its test module)
- `crates/reprise-core/src/queries/maintenance.rs`,
  `crates/reprise-core/src/queries/mod.rs`,
  `crates/reprise-core/src/queries/tests_maintenance.rs`
- `crates/reprise-gnome/src/ui/lyrics/lyrics_batch.rs`,
  `crates/reprise-gnome/src/ui/lyrics/lyrics_batch_tests.rs`
- `docs/ux-rules.md`

No post-merge cross-checks: every verification step above reads only files this
strand owns.
