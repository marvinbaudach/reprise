---
slug: podcast-download-lane
worktree: /home/marvin/Projects/reprise-podcast-download-lane
branch: feature/podcast-download-lane
phase: coded
codex_session:
created: 2026-10-02
---
# Podcast downloads get their own worker lane

## Why (measured, self-contained — the handoff that carried this is not on `dev`)

The GNOME podcast worker (`crates/reprise-gnome/src/ui/podcasts/podcasts_worker.rs`) runs every
operation — `Refresh`, `LoadMore`, `SyncSubscription`, `Download`, `FillDownloads` — on ONE
thread (`reprise-podcasts`) behind ONE unbounded FIFO channel. Journal measurements
2026-09-22 … 2026-10-02 (instrumentation from #1009/#1010):

- 10 refresh round-trips: min 4.6 s, median 8.0 s, max 55.3 s; per-subscription 0.45–3.85 s;
  no failures.
- The one slow refresh (2026-09-29 07:52:32Z): **55,326 ms total, 42,705 ms of it queue wait**,
  queued behind `operation=FillDownloads`, which held the single worker thread for about
  **66 s**. The refresh pipeline itself took 12.6 s for 13 subscriptions (normal).
- Two smaller waits (2026-09-22 20:36:20, 10.9 s each) queued behind a 15 s Force refresh.
- `FillDownloads` only logs its dequeue; its duration and episode count are invisible.

**Decision (user, 2026-10-02):** downloads get their own worker lane. `FillDownloads` and
`Download` move off the thread that serves `Refresh`, so a refresh never waits behind a
download. An INFO line logs every finished download job with its duration and episode count.

**Rejected:** "measure first" (one measured instance exists); a priority queue (a download that
is already running would still block — it would not have helped on 09-29).

## Decisions from the grill (user, 2026-10-02)

1. The Download button still waits behind a running fill-up (same lane, FIFO) — accepted
   non-goal, documented below. The play-triggered YouTube download is unaffected (own thread).
2. The HTTP rate limiter stays shared between the lanes — no change in `http.rs`.
3. `commit_remove_episode` becomes an IMMEDIATE transaction, with its own test (Task 5).
4. POD-28 enters the rulebook as `[active] [gtk]` with the text in Task 4.
5. Thread names `reprise-podcast-feeds` / `reprise-podcast-downloads`; the dequeue log message
   stays verbatim and only gains a `lane` field. Nothing else references the old name.
6. Control arm = one-off mutation proof, recorded in the commit body, not committed as a test.
7. One strand (see `## Parallelität`).

## Facts the design rests on (verified on origin/dev 46ef71bee9)

1. **Two writers are already normal.** The playback path for a YouTube episode without a local
   file runs `pipeline::download_episode_waiting` on its own thread `reprise-youtube-fetch`
   with its own `Db::open_migrated` connection (`ui/playback/external_media_fetch.rs:45-70`),
   concurrently with the worker's refresh. The MCP server downloads from another process.
2. **DB:** connections open with WAL and `busy_timeout` 5000 ms (`db_connection.rs`).
   `open_migrated` is a no-op on a current schema. The refresh's write transaction
   (`pipeline_sync.rs:259`, IMMEDIATE) starts AFTER the network read and keeps the yt-dlp
   listing outside (comment at `:331`), so it holds the write lock for milliseconds.
   No transaction reachable from refresh / load_more / sync / download / fill reads before its
   first write under DEFERRED. The download lane's only writes are autocommit single
   statements (`persist_completed_with_category_if_active_in`).
3. **Download claims** are a process-wide `OnceLock<Mutex<BTreeMap>>`
   (`podcasts/download_claims.rs`), thread-safe; a second caller of the same episode gets
   `DownloadAlreadyRunning` or waits.
4. **HTTP limiter** (`podcasts/http.rs`, `MIN_REQUEST_INTERVAL` 1 s, static
   `LAST_REQUEST: Mutex<Option<Instant>>`, global): the lock is held only for the ≤ 1 s spacing
   sleep, never for the request or body transfer. Feed GETs and RSS enclosure download *starts*
   take a slot; YouTube downloads (yt-dlp subprocess) bypass it.
5. **View side** (`podcasts_view_requests.rs`, `podcasts_view_downloads.rs`): every request has
   its own bounded(1) response channel and its own `glib::spawn_future_local` loop. Only the
   Refresh and LoadMore loops drop stale generations; Download, FillDownloads and
   SyncSubscription loops have no generation check. `FillDownloads` is requested after each
   `Refreshed` through `begin_fill_request` (running/pending coalescing), and
   `finish_fill_request` replays one fill if a request arrived meanwhile. None of this changes.
6. **Row progress survives a refresh:** `set_download_state` writes an overlay map
   (`download_states`), and `view.refresh()` rebuilds through `refreshed_download_states`
   (`podcasts_download_presentation.rs:76-100`), which keeps `Queued | Downloading | Failed`.
7. **Cleanup** (`downloads::enforce_cleanup_in`, run at the end of every `pipeline::refresh`)
   only touches rows whose `downloaded_path` is set; an in-flight download has none and writes
   to a `.part` file that `reclaim_existing` skips.
8. `RetryKey` (`pipeline_sync.rs:~203`) is keyed by the connection's address — **invariant:**
   refresh keeps using one long-lived connection on one thread.

## What changes in behavior

| Before | After |
|---|---|
| A refresh, load-more or new-subscription sync waits behind any queued/running download or fill-up | It starts at once on the feeds lane |
| Downloads run in FIFO order with everything else | Downloads and fill-ups run in FIFO order among themselves on the downloads lane |
| A fill-up's duration and count are invisible | One INFO line per finished download job |
| Removing an episode can fail with an immediate SQLITE_BUSY when another connection commits mid-transaction | It waits for the lock like every other writer |

Unchanged on purpose: refresh → fill sequencing (a fill is requested only after `Refreshed`)
and its coalescing; the play-triggered YouTube download's own thread; the HTTP limiter.

## Tasks (one strand)

File lists below are a starting point, not a fence: if a contract needs state in a file not
named here (e.g. the struct that must hold it), touch it and say so in the commit body. Stop
only if a contract itself is wrong.

### Task 1 — two lanes in the worker (`podcasts_worker.rs`)

- Add `enum PodcastsLane { Feeds, Downloads }` and a pure, exhaustive
  `const fn lane_for(operation: &PodcastsOperation) -> PodcastsLane`:
  `Refresh | LoadMore | SyncSubscription → Feeds`, `Download | FillDownloads → Downloads`.
  No wildcard arm — a new operation must pick its lane at compile time.
- `PodcastsRuntime` holds one sender per lane instead of `worker`; `request()` routes through
  `lane_for`. Its public surface is otherwise unchanged; callers stay untouched.
- `spawn` becomes `spawn_lane(lane, database_path, executor)`, called once per lane. Thread
  names: `reprise-podcast-feeds` and `reprise-podcast-downloads`. Each lane thread opens its
  own `Db::open_migrated` exactly once and reuses it for its lifetime (fact 8).
- Executor seam for tests: `type LaneExecutor = Arc<dyn Fn(Option<&Result<Db, DbError>>,
  &QueuedRequest) + Send + Sync>`. `PodcastsRuntime::setup(conn)` passes the existing
  `process_request`; a `#[cfg(test)] pub(super)` constructor takes `enabled: bool`, `None` for
  the database path, and an injected executor. `process_request`'s per-operation behavior is
  unchanged.
- The dequeue INFO line keeps its message `"podcast worker dequeued request"` verbatim and
  gains a `lane` field (`feeds` / `downloads`). Rewrite the two comments that say
  "single-threaded worker" (`QueuedRequest` doc, `process_request`) to name the lane.
- If the file passes ~650 lines, move the lane/spawn/runtime-channel code into a cohesive
  sibling `podcasts_worker_lanes.rs` (file cap 800; never trim docs to fit).

### Task 2 — the finished-download-job INFO line

- After a `Download` or `FillDownloads` job returns, log once:
  `tracing::info!(lane, operation = ?…, elapsed_ms, episodes, downloaded, failed, outcome,
  "podcast download job finished")`.
  - `elapsed_ms`: run time from dequeue to job end (queue wait is already on the dequeue line).
  - `downloaded` / `failed`: episodes this job brought to `Downloaded` / `Failed`, counted from
    the `DownloadState` values the job publishes; `episodes = downloaded + failed`. For a
    successful `FillDownloads` they equal `FillSummary { downloaded, failed }`.
  - `outcome`: `ok`, `error` (the job itself returned `Err`), or `already_running`
    (`DownloadAlreadyRunning` — another caller owns the download).
  - **No error text, URL or path in this line** (POD-3 / POD-13 logging hygiene); the error
    still reaches the view as today.
- Put the counting in a small pure helper so it is unit-testable without tracing capture.

### Task 3 — worker tests (new file `podcasts_worker_lane_tests.rs`)

`podcasts_worker_tests.rs` is already 642 lines, so the new tests live in a sibling child
module of `podcasts_worker.rs` (`#[cfg(test)] #[path = "podcasts_worker_lane_tests.rs"] mod
lane_tests;`) with access to private items. No DB, no network, no XDG paths: the injected
executor answers by itself. (A run through the real `process_request` would download into
`default_download_root()`, the user's real data directory — never do that.)

- `pod_28_a_refresh_completes_while_a_fill_up_is_still_downloading`: the fake executor blocks
  `FillDownloads` on a gate after signalling "started" (Mutex+Condvar or channel; precedent
  `DownloadGate` in `crates/reprise-core/src/podcasts/pipeline_download_tests.rs`), and answers
  `Refresh` with `Refreshed` immediately. Request FillDownloads, wait for "started", request
  Refresh, and require the `Refreshed` response within a 10 s deadline while the fill is still
  blocked; then release the gate and require `Filled`. Every wait is bounded (no bare
  `recv_blocking` on the test thread); release the gate in a drop guard. The failure message
  names head-of-line blocking.
- `pod_28_downloads_and_feed_work_take_separate_lanes`: `lane_for` over every variant.
- A pure test for the Task 2 counting helper (not rule-named; it is not a UX rule).
- **Control arm (mandatory, recorded, not committed):** temporarily make `lane_for` return
  `Feeds` for every operation, run the first test, observe it fail at its deadline, restore.
  Put the observed failure line in the commit message body.

### Task 4 — the rule (`docs/ux-rules.md` § AF, directly after POD-27)

`[active]` in the same commit as Tasks 1 and 3 (process rule: status flips with the code).

> - **POD-28** [active] [gtk] — A podcast or YouTube refresh never waits behind an episode
>   download. The Download button's jobs and the background fill-up (`POD-5`) run on their own
>   worker lane; refreshes, load-more and new-subscription syncs run on the other, so they
>   start while a download job is still running. Covered by
>   `pod_28_a_refresh_completes_while_a_fill_up_is_still_downloading` and
>   `pod_28_downloads_and_feed_work_take_separate_lanes`.

**Ownership override, stated by name:** the "Active file ownership — Flathub readiness" table
in AGENTS.md (strand A owns `docs/ux-rules.md`) is stale — it dates from #419, and recent PRs
edit the rulebook freely. That table does not apply to this run; editing § AF to add POD-28 is
authorized.

### Task 5 — `commit_remove_episode` takes the write lock up front (`reprise-core`)

- `crates/reprise-core/src/podcasts/store.rs:533`: `commit_remove_episode` opens a DEFERRED
  transaction (`unchecked_transaction()`), SELECTs at `:535`, then INSERTs/DELETEs at
  `:553-560`. In WAL mode, if another connection commits between that SELECT and the first
  write, SQLite returns SQLITE_BUSY immediately without invoking the busy handler, and the
  POD-6 removal commit fails. Switch it to
  `Transaction::new_unchecked(conn, TransactionBehavior::Immediate)`, as `ai_jobs.rs:390` does.
- Test in `store_tests.rs` (444 lines): `pod_6_removing_an_episode_waits_for_a_concurrent_writer`.
  On a file-backed temp DB with an episode to remove: a second connection on another thread
  runs `BEGIN IMMEDIATE`, writes any row, signals "locked", holds ~500 ms, commits. The test
  thread, after the signal, calls the removal commit and requires `Ok` plus the removal's
  effects. Under DEFERRED this fails with SQLITE_BUSY (the SELECT pins the pre-commit snapshot,
  the write then needs a newer one); under IMMEDIATE the call waits at `BEGIN` and succeeds.
  Run it once against the unfixed code and record the red result in the commit body (control
  arm).

### Frontends

Scheduling change inside the GTK window's worker plus a transaction-behavior fix in core. MCP
and the CLI run downloads in their own process with no shared queue, so there is nothing new to
expose; this deliberately stops at the window. The Task 5 fix benefits every caller of
`commit_remove_episode` automatically.

## Verification

- AGENTS.md gates: `cargo fmt --check`,
  `cargo clippy --all-targets --workspace -- -D warnings`, `cargo test --workspace`,
  `cargo audit`.
- Focused first: `cargo test -p reprise-gnome podcasts_worker` and
  `cargo test -p reprise-core podcasts::store`.
- Core purity proof (Task 5 touches `reprise-core`):
  `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` must be empty.
- `scripts/check-ux-traceability.sh` (POD-28 has its rule-named tests).
- Run the existing podcast display tests (`podcasts_refresh_button_tests`,
  `podcasts_loading_tests`, `podcasts_view_tests`) through the repo's display runner, isolated
  under xvfb. Whether they depend on refresh and fill running back to back is unverified —
  run them, do not argue either way.
- Manual after landing (human): watch the journal
  (`journalctl --user -t io.github.marvinbaudach.Reprise.desktop`) for
  `podcast download job finished`, and for a refresh's `queued_ms` near zero while a fill runs.

## Known residual edges (accepted, documented)

- **The Download button waits behind a running fill-up** (grill decision 1). The new INFO line
  shows how often that matters in practice.
- A fill-up snapshots its episode list once. A refresh that lands mid-fill can push an episode
  out of a subscription's top N; the fill may still download it and the next cleanup evicts it.
  Bounded and self-healing; the coalescing replay downloads the new episodes.
- Refresh's upsert may change the `audio_url` of an episode that is mid-download; the download
  finishes from the old URL into the GUID-stable path. Same today against MCP.
- `download_atomically` renames `.part` before the persist UPDATE; a refresh's
  `reclaim_download` in that window plus a persist that then fails after the 5 s busy wait would
  leave a dangling `downloaded_path` until the next cleanup. Already possible today through the
  playback thread; very low probability.
- The shared HTTP limiter (grill decision 2): a refresh waits at most ~1 s per RSS download
  *start* that interleaves, never for a whole job.

## Parallelität

No cut, by decision (grill 7). Tasks 1–4 all change `podcasts_worker.rs` or test its private
items, and POD-28 must flip to `[active]` in the same commit as those tests. Task 5 is a
disjoint file group in `reprise-core`, but it is one line plus one test: a second worktree
would cost a second cold cargo build and a second landing for no wall-clock gain.

- Strand: one. Owns `crates/reprise-gnome/src/ui/podcasts/podcasts_worker*.rs` (including the
  new `podcasts_worker_lane_tests.rs` and, if needed, `podcasts_worker_lanes.rs`),
  `docs/ux-rules.md` § AF (POD-28 only), `crates/reprise-core/src/podcasts/store.rs`
  (`commit_remove_episode` only) and `crates/reprise-core/src/podcasts/store_tests.rs`.
- Merge order: n/a. Post-merge cross-checks: none — no task reads a file another strand owns.
