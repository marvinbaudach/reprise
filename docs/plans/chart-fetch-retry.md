---
slug: chart-fetch-retry
worktree: /home/marvin/Projects/reprise-chart-fetch-retry
branch: feature/chart-fetch-retry
phase: coded
codex_session:
created: 2026-10-03
---
# The "Popular in <country>" chart survives one flaky answer

## Why (measured, self-contained — the handoff that carried this is untracked)

- 2026-10-03, ~12:20 CEST, GNOME Add Podcast dialog, chip `Popular in CH`: first
  "podcast source timed out", then "podcast source returned an HTTP error". The journal has no
  line for either — neither `itunes_charts.rs` nor `podcasts/http.rs` logs anything.
- 8 sequential curl probes the same day:
  - chart `https://rss.marketingtools.apple.com/api/v2/ch/podcasts/top/12/podcasts.json`:
    5× 200 (one of them at 9.5 s), 2× timeout at curl's 12 s limit, 1× 502.
  - lookup `https://itunes.apple.com/lookup?id=<ids>&entity=podcast`: 8× 200 in 0.3–0.6 s.
  The chart endpoint is the flaky step (3 of 8 failed); the lookup was stable.

Goal: one automatic retry on a timeout or an HTTP 5xx, so a single bad answer no longer ends the
chip, and one journal line per failed request, so the next report can be read from the journal.

Suggested PR title: `The podcast chart chip survives one flaky answer`.

## Decisions from the grill (user, 2026-10-03)

1. **Placement:** the retry lives in `itunes_charts::top_podcasts`, in core. Apple text search is
   a non-goal — no retry, no log line there.
2. **Per step:** the chart request and the lookup each get one more attempt. A failed lookup is
   asked again on its own and never refetches the chart.
3. **Qualifying set:** `Timeout` and HTTP 5xx only — the two shapes measured.
4. **Spacing:** no wait of our own; the shared podcast limiter is the only gap.
5. **Budget:** the worst case of ≈ 40 s of `Searching…` is accepted; the per-request budget stays
   at the shared 10 s.
6. **Journal line:** one `warn` per failed attempt — retried or final — and for a 200 whose body
   cannot be parsed. A success logs nothing.
7. **Storefront:** the lowercase country code is part of the line.
8. **Rule:** one sub-rule, SRC-19a, covering retry and log line, `[active] [core]` in the commit
   that implements both.
9. **Wording:** SRC-19a's text below is approved verbatim — insert it unchanged.
10. **Cut:** none — one strand.
11. **Test seam:** an injected fetcher with a scripted fake, plus the existing `CapturedLogs`.

Rejected, with the reason:

- Retry in `http::get_json` — would also retry Apple search, whose failure rate nobody has
  measured, and `get_json` cannot name the step without the URL it must not log.
- Retry in `get_with_budget` — would reach feed refreshes, which already back off across runs
  (`pipeline_retry.rs`); stacking both multiplies requests to a failing host.
- Retrying `Transport` (offline, DNS, TLS — a retry only delays the error) and 429 (Apple asking
  us to stop; `Retry-After` can be minutes).
- An explicit back-off before the retry; a chart-specific shorter budget; a deadline for the whole
  flow.
- A log line for Apple search; `info` for the retried attempt; dropping the storefront code;
  splitting the rule into SRC-19a/SRC-19b; extending the `http.rs` fixture layer.

## Facts the design rests on (verified on origin/dev 3441c38c59)

- **One caller.** `crates/reprise-gnome/src/ui/podcasts/add_dialog.rs:469` (`load_charts`), on its
  own thread `reprise-podcast-charts` via `one_shot_task::spawn`. The dialog shows `Searching…`
  (`PODCAST_SEARCHING`, set at `:328`) until the result arrives; on `Err` it shows
  `preview_error(&error)` = `PodcastError::classify()` (`:577`, displayed at `:566`). A generation
  token drops a result whose request the user has since replaced. No caller in
  `reprise-android-ffi`, `reprise-mcp`, `reprise-cli`, `reprise-view` or `android/`.
- **Two requests.** `itunes_charts::top_podcasts(country)` (`itunes_charts.rs:73`) asks
  `podcasts::http::get_json` for the chart (`:74`), then for one batched lookup (`:79`). An empty
  chart returns `Ok(vec![])` without a lookup.
- **`get_json` is Apple-only.** It has exactly three callers: Apple text search (`itunes.rs:87`),
  the chart and the lookup. Feeds go through `get_feed_conditional`; both share
  `get_with_budget`.
- **The limiter.** `get_with_budget` calls `respect_rate_limit()` first: one process-wide
  `Mutex<Option<Instant>>` stamped with the *start* of the previous podcast request. It sleeps
  only while less than `MIN_REQUEST_INTERVAL` (1 s) has passed since that start. Each request
  builds a ureq agent with `SOURCE_REQUEST_TIMEOUT` = 10 s (`source_error.rs:14`, pinned by
  `feed_requests_use_the_shared_budget_but_downloads_keep_their_own`).
- **Error mapping** (`http.rs:197-217`, `:307-315`): a ureq timeout → `PodcastError::Timeout`;
  any other transport failure → `Transport(<ureq text>)`, and that text can echo the URL;
  404/410 → `SourceGone(code)`; 429 → `RateLimited { retry_after }`; any other non-2xx →
  `HttpStatus(code)`. A read failure inside the body — including a timeout that fires there —
  becomes `Body("response read failed")`.
- **The classifier.** `PodcastError::classify()` (`podcasts.rs:158ff.`) is POD-13's single
  classifier: fixed text, documented as safe for UI display and normal-level logs. The `Display`
  impl is not, because `Transport`/`Body`/`Parse` carry provider text.
- **Log hygiene.** POD-3 and POD-13 allow operation, failure category, exit code/status, timeout
  and the classified reason in a log line — never URLs, tokens, raw provider text or local paths.
- **Logging path.** Core logs through `tracing`. The GNOME app's subscriber writes INFO and above
  to stderr (`crates/reprise-gnome/src/main.rs:93-102`, default filter `info,lofty=error`), which
  the user session forwards to the journal. Field names arrive ANSI-coloured, so a journal check
  greps the message text, not `field=value`.
- **Test seams.** The `podcasts::http` fixtures are static per URL: they cannot produce a timeout,
  cannot answer "fail, then succeed", have no route for the lookup URL (`fixture_route` needs a
  `term` parameter on `itunes.apple.com`), and each fixture request still waits on the 1 s
  limiter. `crate::log_capture::CapturedLogs` captures `tracing` events per thread and is already
  used by `podcasts/fill_downloads_tests.rs`. Each captured line starts with the event's
  metadata name, which embeds the source file and line number.
- **Existing retry machinery does not fit.** `pipeline_retry.rs` persists a per-subscription
  back-off across refresh runs (2 s … 60 s); `library_doctor/remote/network.rs::request_with_retry`
  belongs to a different client. Neither wraps `get_json`.
- **Ownership.** AGENTS.md still lists a "Flathub readiness" strand A owning `docs/ux-rules.md`.
  No such branch exists, and #1056 edited the file on 2026-10-03 — treated as stale. No open PR
  touches `podcasts/` or SRC-19.

## Design

### Where the retry lives — `itunes_charts.rs`, per step

`top_podcasts(country)` keeps its signature and delegates to a new
`top_podcasts_with(country, fetch: &mut dyn FnMut(&str) -> Result<Response, PodcastError>)`
(module-private or `pub(crate)`); the production path passes `super::http::get_json`. Each of the
two steps — chart, lookup — runs through one helper that fetches, parses, and on failure logs and
decides whether to ask once more.

### What qualifies, how often, how fast

- **Asked once more:** `PodcastError::Timeout` and `PodcastError::HttpStatus(500..=599)`.
- **Not retried** — the step ends at once with its classified reason: `RateLimited`,
  `SourceGone`, every other `HttpStatus` below 500, `Transport`, `Body`, `Parse`, and
  `NotModified` (which cannot occur — the chart flow sends no conditional headers).
- **Attempts:** two per step, a named constant (`ATTEMPTS_PER_REQUEST = 2`). A failed lookup
  never refetches the chart.
- **Spacing:** none of our own. Both attempts go through `get_json`, so the shared limiter spaces
  them: after a fast 5xx the retry waits until 1 s after the first attempt *started*; after a
  10 s timeout it fires immediately. No test asserts timing — the fake bypasses the limiter, and
  the rulebook treats timing as design intent.
- **Cost:** worst case chart 2 × 10 s + lookup 2 × 10 s ≈ 40 s of `Searching…` (today: 20 s).
  The typical recovered case is one 10 s timeout plus a normal retry. The measured 9.5 s success
  sits just under the 10 s budget, so a retry at the same budget can time out the same way; the
  budget stays — it is shared and pinned.

### The journal line

One `tracing::warn!` per failed attempt — including an attempt that will be retried, and a 200
whose body cannot be parsed — from a single call site, with a fixed literal message and these
fields:

```
podcast chart request failed
  step       "chart" | "lookup"
  storefront the country code lowercased, exactly as the chart URL uses it ("ch")
  attempt    1 | 2
  retrying   true | false
  status     HttpStatus(code) / SourceGone(code) → code, RateLimited → 429; absent otherwise
  reason     PodcastError::classify()
```

- Never the URL, the response body, `%error` / `?error`, or any payload string.
- A flow that succeeds at the first attempt logs nothing; a recovered flow logs exactly the one
  line of its failed attempt.

### The rule — SRC-19a, approved verbatim

Inserted unchanged, directly after SRC-19 in `docs/ux-rules.md` § AF, in the same commit as its
tests and its implementation:

```markdown
- **SRC-19a** [active] [core] — Extends `SRC-19`: **one flaky answer does not end the
  chart.** Each of the chip's two requests — the chart feed and the batched lookup — is asked
  **once more** when it times out or Apple answers with a server error (HTTP 5xx); a failed
  lookup is asked again on its own, never by fetching the chart a second time. Nothing else is
  retried: a rate limit is Apple asking us to stop, a 4xx or a storefront that does not exist
  answers the same way twice, an unreachable host usually means the network is down, and an
  unreadable answer is not a network accident — each ends the request at once with its
  classified reason (`POD-13`). The retry adds no wait of its own; the shared
  one-request-per-second podcast spacing is the only gap between the two attempts. Every
  failed request — retried or not, an unreadable answer included — leaves **one** log line,
  `podcast chart request failed`, and a request that succeeds leaves none. The line carries
  only the step (`chart` or `lookup`), the storefront code, the attempt number, whether a
  retry follows, the HTTP status when there is one, and the classified reason — never the
  request URL, the response body or the provider's error text (`POD-3`).
```

## Tasks (one strand)

Starting file list — a starting point, not a fence; stop only if the contract itself is wrong:

- `crates/reprise-core/src/podcasts/itunes_charts.rs`
- optionally a new sibling `crates/reprise-core/src/podcasts/itunes_charts_tests.rs`, if the
  test module moves out — declared inside `itunes_charts.rs` itself as
  `#[cfg(test)] #[path = "itunes_charts_tests.rs"] mod tests;`, the form `classify.rs:122-124`
  uses, so `podcasts.rs` needs no edit
- `docs/ux-rules.md` § AF — the SRC-19a paragraph only

Not touched: `podcasts/http.rs`, `source_error.rs`, `podcasts.rs`, anything under
`crates/reprise-gnome`, `po/`, MCP, Android.

### Task 1 — the fetch seam (behavior-preserving)

- Add `top_podcasts_with` as described; `top_podcasts(country)` becomes a one-line delegation
  with `super::http::get_json`. No retry, no logging yet.
- Test fake: records every requested URL in order and returns the next scripted
  `Result<Response, PodcastError>`; it panics on a request beyond its script, so an unexpected
  extra attempt fails loudly. Bodies: a chart JSON with two or three realistic ten-digit ids
  (for example `1535809341`, `1200361736`), and a lookup JSON in the shape
  `itunes::parse_results_with_ids` reads. Short ids like `42` are not allowed — they can collide
  with the line number in a captured event name.
- Two tests on today's behavior, named for SRC-19:
  - `src_19_the_chart_flow_asks_the_chart_then_one_lookup` — requests by endpoint
    `[chart, lookup]`, rows in chart order;
  - `src_19_an_empty_chart_skips_the_lookup` — requests `[chart]`, `Ok(vec![])`.
- Commit.

### Task 2 — retry, journal line and SRC-19a (test-first, one commit)

1. Write the tests below and run them against Task 1's code; keep the `test result:` output for
   the report. Every test that asserts a retry or a log line must fail there.
   `…failure_that_will_not_change…` and `…first_time_success_logs_nothing` already pass on
   Task 1 — they guard against over-retrying and over-logging, and their red evidence comes from
   the mutation probes instead.
2. Implement the per-step helper (fetch → parse → on error: log, then retry if transient and an
   attempt remains) with a private "transient" predicate next to it.
3. Insert SRC-19a verbatim, directly after SRC-19.
4. Run the tests green and commit tests, code and rule together.

All tests go through the fake; endpoints are told apart by `CHART_ENDPOINT` / `LOOKUP_ENDPOINT`
prefix, and log assertions use `CapturedLogs`.

| Test | Script | Asserts |
|---|---|---|
| `src_19a_a_chart_request_that_times_out_is_asked_once_more` | chart `Timeout` → chart ok → lookup ok | `Ok`, chart order; requests `[chart, chart, lookup]` |
| `src_19a_a_server_error_on_the_chart_is_asked_once_more` | chart `HttpStatus(502)` → chart ok → lookup ok | same |
| `src_19a_a_failed_lookup_is_retried_without_refetching_the_chart` | chart ok → lookup `Timeout` → lookup ok | `Ok`; requests `[chart, lookup, lookup]` |
| `src_19a_the_second_failure_ends_the_step` | chart `Timeout` → chart `HttpStatus(503)` | `Err(HttpStatus(503))`; exactly two requests, no lookup |
| `src_19a_a_failure_that_will_not_change_is_not_asked_again` | one case each: `RateLimited`, `SourceGone(404)`, `HttpStatus(403)`, `Transport(..)` | exactly one request; the same error variant |
| `src_19a_every_failed_request_leaves_one_log_line` | chart `Timeout` → chart `HttpStatus(502)` | two lines with the message text; line 1 `attempt=1 retrying=true`, timed-out reason, no `status`; line 2 `attempt=2 retrying=false status=502`, HTTP-error reason; both `step="chart" storefront="ch"` (called with `"CH"`) |
| `src_19a_a_recovered_flow_logs_only_its_failed_attempt` | chart `Timeout` → chart ok → lookup ok | exactly one line |
| `src_19a_a_first_time_success_logs_nothing` | chart ok → lookup ok | no line |
| `src_19a_an_unreadable_answer_is_logged_and_not_asked_again` | chart ok with body `not json` | `Err(Parse)`; one request; one line, reason `podcast source returned invalid data` |
| `src_19a_the_log_line_never_carries_the_url_or_provider_text` | chart ok → lookup `Transport("… https://itunes.apple.com/lookup?id=<both ids>&entity=podcast …")` | `Err(Transport)`; the log carries reason `podcast source could not be reached` and contains none of `://`, `apple.com`, `lookup?`, either id |

### Task 3 — verification inside the worktree

Codex runs exactly these, from the worktree root:

- `cargo fmt --check`
- `cargo clippy -p reprise-core --all-targets -- -D warnings`
- `cargo test -p reprise-core itunes_charts` — a substring filter, no `--exact`, no `--lib`;
  read the `test result:` line: passed > 0, failed 0
- `scripts/check-ux-traceability.sh`

Not the unfiltered workspace suite, and not the merge-readiness wrapper. AGENTS.md's "all gates
before every commit" instruction does not apply to this run — this exception is deliberate: the
orchestrator runs the merge-readiness gate on the finished worktree. If a filtered run shows an
unrelated failure, report it and commit anyway — do not stop with finished work uncommitted.

## Frontends

- **GNOME:** no code change. The dialog keeps `Searching…` through the retry and shows the
  classified reason of the last failure.
- **MCP, CLI, Android:** no chart caller today; any future caller of `top_podcasts` inherits the
  retry and the log line because both live in core. This plan adds no feature, so "every feature
  reaches every frontend" asks for nothing new; exposing charts over MCP would be its own
  decision (SRC-19 shipped GTK-only).
- No new user-facing strings, no `po/` change.

## Verification

1. **Control arm:** Task 2's retry and log tests red on Task 1's commit (the red output in Codex's
   report).
2. **Fix arm:** the same tests green after Task 2.
3. **Mutation proof** (orchestrator, in the worktree, each probe reverted before the next) —
   each must turn the named test red:
   - `ATTEMPTS_PER_REQUEST` 2 → 1: `…times_out_is_asked_once_more`, `…server_error…`,
     `…failed_lookup…`
   - 2 → 3: `…the_second_failure_ends_the_step` (the fake panics on the third request)
   - drop `Timeout` from the predicate: `…times_out_is_asked_once_more`
   - drop 5xx from the predicate: `…server_error_on_the_chart…`
   - add `Transport`, then separately `RateLimited`, to the predicate:
     `…failure_that_will_not_change…`
   - log `%error` instead of `classify()`: `…never_carries_the_url…`
   - retry the whole flow instead of the step: `…failed_lookup_is_retried_without_refetching…`
   - remove the log call: `…every_failed_request_leaves_one_log_line`
4. **Full gate:** `heavy-run heavy -- scripts/check-merge-readiness.sh` on the clean worktree.
5. **Core purity:** `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'`
   stays empty (no new dependency is expected at all).
6. **Manual, optional (human):** in the real app, use `Popular in CH` a few times, then
   `journalctl --user --since -10min | grep 'podcast chart request failed'`.

## Known residual edges (accepted)

- A timeout during the *body* read surfaces as `Body("response read failed")`, classified
  "returned invalid data", and is not retried. Pre-existing classification; the new line at
  least makes it visible.
- A request the user has since replaced still finishes its retry in the background — at most one
  extra request; the generation token drops the result.
- Apple text search still fails on its first timeout and still logs nothing (non-goal by
  decision 1).
- The chart request shares the 1 s limiter with podcast refresh workers and can queue a few
  seconds behind them while a refresh runs. Pre-existing.

## Parallelität

No cut, by decision (grill 10). The code change is one file, `itunes_charts.rs`, and the SRC-19a
paragraph has to land in the same commit as the `src_19a_` tests: `check-ux-traceability.sh`
fails a test that names an unknown rule, and the process rules require a rule to turn `[active]`
in the commit that implements it. A rule strand and a code strand could therefore not pass that
gate before the merge, which is the very failure the cut exists to avoid — and a second worktree
would buy a cold cargo build for one paragraph of markdown.

- **Strand:** one. Owns `crates/reprise-core/src/podcasts/itunes_charts.rs`, a new
  `crates/reprise-core/src/podcasts/itunes_charts_tests.rs` if the tests move out, and
  `docs/ux-rules.md` § AF (SRC-19a only).
- **Merge order:** n/a.
- **Post-merge cross-checks:** none — no task reads a file another strand owns.
