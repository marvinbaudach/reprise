---
slug: the-concerts-list-stays-current
worktree: /home/marvin/Projects/reprise-the-concerts-list-stays-current
branch: feature/the-concerts-list-stays-current
phase: shipped
codex_session:
created: 2026-09-02
---
# The concerts list stays current

## Why

Concerts has been failing to refresh. The banner in the app shows
`concert provider returned HTTP status 403`, and the saved list is whatever the
last successful run produced. Three separate causes, measured 2026-09-02:

1. **Bandsintown's public REST API denies every request.**
   `https://rest.bandsintown.com/artists/<name>?app_id=<any>` answers `403` with
   `{"Message":"User is not authorized to access this resource with an explicit
   deny in an identity-based policy"}`. Verified against three app ids (our
   bundled `io.github.marvinbaudach.Reprise`, a foreign id, a nonsense id) and
   three user agents (ours, a browser UA, `curl/8`). This is not a configuration
   problem on our side — the endpoint is closed to unregistered apps. We ship
   that app id as an unconditional default (`concerts/config.rs:17`), so every
   run is guaranteed to hit it.

2. **One dead provider blocks the working one.** `resolve_provider`
   (`concerts/pipeline.rs:303`) calls `retry_provider_call(...)?` inside the
   provider loop, so a Bandsintown error aborts resolution for that artist
   before Ticketmaster is ever asked. Evidence from the live database:
   `concert_artists` holds `provider=ticketmaster, last_outcome=ok` for 90 rows
   (resolved while Bandsintown still worked) and `provider=NULL,
   last_outcome=failed` for 78. No artist has resolved since the 403 started.
   The Ticketmaster key is present and valid.

3. **The refresh cadence is a day, not an hour.** `FETCH_TTL_SECONDS = 24h`
   (`concerts/refresh.rs:3`) gates both `refresh_due` (the whole run) and
   `artist_due` (each artist), plus up to `REFRESH_JITTER_MAX_SECONDS = 2h` of
   jitter. The hourly timer (`concerts_view.rs:37`) and the refresh on entering
   the view (`concerts_view.rs:371` via `library_shell.rs:233`) already exist —
   they just find `refresh_due == false` 25 times out of 26.

The user's requirement: the list should simply be current — refresh hourly, and
refresh when navigating into the view.

## Scope

`crates/reprise-core/src/concerts/**` only. The GNOME side already calls
`refresh()` on view entry and runs an hourly timer; no UI change is needed.

## Tasks

### 1. A failing provider must not block the next one

In `crates/reprise-core/src/concerts/pipeline.rs`, `resolve_provider`:

- Replace the `?` on `retry_provider_call(...)` inside the provider loop with
  explicit handling:
  - `AttemptFailure::Cancelled` → return immediately, unchanged.
  - `AttemptFailure::QuietPeriod(_)` → return immediately, unchanged (the caller
    aborts the whole run on it; a rate-limit quiet period must stay global).
  - `AttemptFailure::Failed(error)` → remember the **first** such error and
    continue with the next provider.
- After the loop:
  - a provider resolved → `Ok(ResolvedProvider::Found(...))` as today;
  - no provider resolved and at least one **errored** → return
    `Err(AttemptFailure::Failed(remembered_error))`;
  - no provider resolved and all said `Unmatched` → `Ok(ResolvedProvider::Unmatched)`.

The last distinction is the point of the task: the caller turns `Unmatched` into
`resolution::store_unmatched`, which arms `negative_retry_blocked`. Caching "this
artist has no concerts" after only half the providers answered would suppress
retries for an artist we never actually asked about.

### 2. Stop shipping the denied Bandsintown app id

In `crates/reprise-core/src/concerts/config.rs`, `credentials_with_env`: drop the
`.or_else(|| non_empty(DEFAULT_BANDSINTOWN_APP_ID))` fallback so the app id comes
only from the setting (`concerts.bandsintown_app_id`) or the
`REPRISE_BANDSINTOWN_APP_ID` env var. Keep the setting, the env var, the
`BandsintownProvider` and its parsing intact — a user who registers an app id
with Bandsintown still gets that provider.

Remove `DEFAULT_BANDSINTOWN_APP_ID` if nothing else references it (only
`config.rs:17` and `config.rs:75` do today); otherwise leave the constant and
just stop using it as a fallback. Update any test that asserts the default.

Without this, task 1 alone still leaves the error banner firing on every run:
Bandsintown would 403 once per artist, and any artist Ticketmaster cannot match
would be recorded as a failure with a 403 attached.

### 3. Hourly cadence, and jitter that does not swallow it

In `crates/reprise-core/src/concerts/refresh.rs`:

- `FETCH_TTL_SECONDS`: `24 * 60 * 60` → `60 * 60`. One constant for both
  `refresh_due` and `artist_due` — do not split them.
- `REFRESH_JITTER_MAX_SECONDS`: `2 * 60 * 60` → `10 * 60`. This is not cosmetic:
  `refresh_due` requires `elapsed >= FETCH_TTL_SECONDS + jitter`, and
  `jitter_seconds` is a stable hash of the seed, so an unchanged 2h jitter would
  turn "hourly" into 1–3h — and permanently ~3h for an install whose hash lands
  high.

In `crates/reprise-gnome/src/ui/concerts/concerts_view.rs`:

- `REFRESH_TIMER_SECONDS`: `60 * 60` → `10 * 60`. The timer decides how often
  `refresh_due` is *asked*, not how often we fetch. At an hourly tick the tick
  right after a refresh has `elapsed` of exactly one hour, loses to the jitter,
  and the next chance is an hour later — the refresh then re-anchors
  `last_attempt_at` on the tick boundary, so it stays a two-hour cadence forever
  for every install whose jitter is non-zero. A tick with nothing due costs one
  `SELECT MAX(last_attempt_at)`, so the shorter period adds no provider traffic.

Quota check (no change needed, recorded so the next reader does not redo it):
`MAX_ARTISTS_PER_RUN = 30` bounds a run, so hourly runs mean ~30 artists ×
~2 Ticketmaster calls ≈ 1.400 calls/day against the 5.000/day free-tier cap. The
168 artists in the live library cycle through in about six hours.

### 4. Tests

- **Regression guard (the core one):** a pipeline test with two providers where
  Bandsintown returns `ProviderError::HttpStatus(403)` on `resolve` and
  Ticketmaster resolves — assert the artist ends up resolved via Ticketmaster,
  `summary.failed == 0` and `summary.failures` is empty.
- A test that all providers erroring still yields a failure for that artist, and
  that all providers answering `Unmatched` still yields `store_unmatched`.
- A test that a provider error plus another provider's `Unmatched` is recorded as
  a **failure**, not as unmatched.
- `QuietPeriod` from the first provider still aborts the run without asking the
  second.
- Update every test pinned to the 24h TTL or the 2h jitter in
  `refresh.rs`/`pipeline_tests.rs`/`domain_tests.rs`, and any test asserting the
  bundled Bandsintown default in `config.rs`.
- `pipeline_tests.rs:648` asserts the banner text does not leak `HTTP`, `599` or
  `concert provider` — keep that intact.

## Verification

- `cargo test -p reprise-core concerts` green in the worktree.
- `cargo clippy -p reprise-core --all-targets` clean for the touched files.
- No network access in tests — the existing fixture mechanism
  (`REPRISE_CONCERTS_FIXTURE_DIR`) and the in-test fake providers stay the only
  sources.

## Not in scope

- Restoring Bandsintown coverage. Their API is closed to us; registering an app
  id is a product decision, not a code change.
- A per-run circuit breaker for a provider that fails on every artist. With the
  default app id gone, no provider is configured that fails unconditionally, so
  the machinery would have nothing to protect against.
- Forcing a full refresh on every navigation into the view. With a 1h TTL,
  entering the view refreshes when the data is older than an hour; the footer's
  refresh button stays the "right now" path.

## Parallel work

No cut. Tasks 1–4 are three small edits in one crate, and the tests in task 4
cover the code of tasks 1–3 — any split would have one strand's tests reading
another strand's files. Single strand.
