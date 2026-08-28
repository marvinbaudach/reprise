---
slug: one-bad-artist-no-longer-holds-the-queue
worktree: /home/marvin/Projects/reprise-one-bad-artist-no-longer-holds-the-queue
branch: feature/one-bad-artist-no-longer-holds-the-queue
phase: coded
codex_session:
created: 2026-08-28
---
# One bad artist no longer holds the queue

## The defect

`crates/reprise-core/src/artist_portrait/backfill.rs:399-405` retries a failed
artist by putting it back at the **front** of the queue, with no attempt bound:

```rust
Err(error) => {
    tracing::debug!(%error, "artist portrait backfill request will retry");
    queue.push_front(artist);
    consecutive_errors = consecutive_errors.saturating_add(1);
    if consecutive_errors >= 3 {
        progress.state = PortraitBackfillState::Paused;
    }
}
```

An artist whose fetch fails permanently — a name Deezer answers with a 404, a
response that never parses — is retried forever at the head of the queue. Every
artist behind it waits. After the third consecutive failure `progress.state`
becomes `Paused`, which the Android UI renders as **"Waiting for a connection"**
(`ArtistPhotoProgressBar.kt:236`, chosen from `state` alone), and `RETRY_DELAYS`
caps the backoff at 120 s. The run therefore never ends, never advances, and
tells the user the network is down while it is fine.

Both halves are defects: the run that cannot finish, and the label that blames
the network for one artist's bad response.

**Not reproduced on the device.** On the measured run the bar read `Artist
photos complete 62 / 65`, the worker thread was gone from the process, and no
Reprise log line appeared for 18 minutes. The three artists without a photo had
reached `Ok(PortraitOutcome::NotFound)` — Deezer answered with a known
placeholder image, which `artist_portrait` correctly rejects. This plan fixes a
defect found by reading the code, not one observed in that run. Say so in the
commit message; do not claim it closes a reproduced hang.

## The root cause

`consecutive_errors` is used as a network detector, and it cannot be one. It
cannot distinguish "the connection is down" from "the artist at the head of the
queue is broken", and at the tail of a run — when only broken artists remain —
the two are numerically identical. Any fix that keeps counting errors inherits
that ambiguity; an earlier draft of this plan did, and deadlocked at the tail in
exactly the same way as the original bug.

The distinction the code needs already exists in the data it throws away: the
error *kind*.

## The fix

### 1. Classify the failure

Add a private classifier in `backfill.rs` over `PortraitError`
(`artist_portrait/mod.rs:36`, wrapping `musicbrainz::FetchError`
at `musicbrainz.rs:32`):

**Network-shaped** — the connection is the problem, every artist would fail the
same way:

- `Fetch(FetchError::Timeout)`
- `Fetch(FetchError::Transport)`
- `Fetch(FetchError::Body)` — the body could not be read, i.e. the connection
  dropped mid-response
- `Fetch(FetchError::HttpStatus(429))` and `HttpStatus(500..=599)`

**Artist-shaped** — the server answered, and the answer is specific to this
request:

- `Fetch(FetchError::HttpStatus(_))` for any other status (the 4xx family)
- `Fetch(FetchError::BodyTooLarge)` — a deterministic property of this response
- `InvalidResponse` — Deezer replied and the reply did not parse

Note that `SourceErrorKind` cannot serve here: `cover_download.rs:29-38`
collapses every `FetchError` to `Unreachable`, so it carries no distinction to
reuse. Do not widen `SourceErrorKind` for this — the classifier is local to the
backfill's retry policy and should stay there.

### 2. Two bounds, artist-shaped failures only

```rust
/// Immediate retries at the head of the queue before an artist yields its slot.
const MAX_HEAD_RETRIES: u32 = 1;
/// Total artist-shaped failures for one artist within a single run.
const MAX_ATTEMPTS_PER_ARTIST: u32 = 3;
```

`const` at module scope beside `RETRY_DELAYS`, not literals in the loop.

The worklist inside `run_worker` becomes `VecDeque<(String, u32)>` — the artist
and the artist-shaped failures it has spent. `run_worker`'s `artists: Vec<String>`
parameter and the public `start` / `start_prepared` signatures do not change, so
no FFI, no caller and no test fixture changes shape.

### 3. The failure branch

**Network-shaped** — behaves exactly as today:

- `push_front`, so the run resumes at the same artist;
- `consecutive_errors += 1`, and `Paused` at `>= 3` with the existing
  `RETRY_DELAYS` wait;
- spends **no** budget, and is **never** dropped.

**Artist-shaped** — new:

- spends one attempt for this artist;
- does **not** touch `consecutive_errors`, so one broken artist can no longer
  drive the run to `Paused` and claim the connection is down;
- `attempts <= MAX_HEAD_RETRIES` → `push_front` (catches a genuine one-off);
- otherwise, `attempts < MAX_ATTEMPTS_PER_ARTIST` → `push_back`, so the rest of
  the library proceeds;
- `attempts >= MAX_ATTEMPTS_PER_ARTIST` → drop from this run's queue and
  `progress.failed += 1`.

`consecutive_errors` keeps its reset on any `Ok`.

### 4. What a dropped artist does and does not get

It increments `failed`, so it appears in the bar's "N without a photo" tail and
`done + failed` still reaches `total` — the bar must be able to close, or the
run looks unfinished, which is the impression this whole investigation started
from.

It must **not** call `cache::write_negative`. A negative marker suppresses the
artist for `NEGATIVE_MARKER_MAX_AGE` (7 days), and a malformed response is not
evidence that no photo exists. The next run retries it from a full budget. This
is the only place where "we gave up" and "there is no photo" are deliberately
shown as the same number but stored differently — the distinction that matters
lives in the cache, not in the progress bar.

### 5. Why this terminates

An artist-shaped failure always spends budget and can always be dropped, because
it never sets `Paused`. So every entry either succeeds, reaches `NotFound`, or
is dropped after at most `MAX_ATTEMPTS_PER_ARTIST` artist-shaped failures. The
only way to stay in the loop indefinitely is an unbroken run of network-shaped
failures — i.e. an actual outage, where waiting is correct and where the run
discards nothing. A subway ride costs time, not photos.

## Tests

`crates/reprise-core/src/artist_portrait/backfill_tests.rs`, using the existing
`start_prepared` + injected `fetch` + `no_wait()` harness.

**One existing test changes.**
`transport_errors_are_retried_without_counts_or_negative_marker` injects
`PortraitError::InvalidResponse`, which this plan classifies as artist-shaped.
Its contract — a transport error is retried forever without counts or a negative
marker — remains correct and must keep being pinned, so change its injected
error to `PortraitError::Fetch(FetchError::Transport)`, matching what the test's
own name says. This is a correction of the fixture, not a relaxation of the
fence: after the change it pins strictly more than it did before.

The test at line ~284 (`Paused` is followed by `Running`) must keep passing
untouched; it injects three failures then successes, so classify its injected
error as network-shaped or it stops testing what it names.

New tests:

1. **`one_broken_artist_does_not_block_the_others`** — `fetch` returns
   `InvalidResponse` for `"Bad"` forever and `Ok(Found)` for everyone else;
   worklist `["Bad", "A", "B"]` must reach `Complete` with `done == 2` and
   `failed == 1`. Without the fix this hangs; use the repo's completion-wait
   helper, never an unbounded loop.
2. **`a_run_of_only_broken_artists_still_finishes`** — worklist
   `["Bad1", "Bad2"]`, every fetch returning `InvalidResponse`, so there is
   never a success to reset anything. Assert `Complete` with `failed == 2` and
   no `Paused` update. This is the case a count-based detector gets wrong — it
   would read an unbroken run of failures as an outage, refuse to drop, and loop
   forever. Note that `["A", "Bad"]` would *not* test this: with the classifier
   in place `Paused` is unreachable from artist-shaped errors either way, so
   such a test passes on both the fixed and the broken implementation and pins
   nothing.
3. **`a_broken_artist_never_pauses_the_run`** — same setup as 1; assert no
   published update ever carries `PortraitBackfillState::Paused`. This pins the
   "keine Verbindung" half of the defect.
4. **`a_dropped_artist_gets_no_negative_marker`** — assert
   `cache::negative_marker_path(dir, "Bad")` does not exist, separating a drop
   from `NotFound`.
5. **`a_total_outage_spends_no_attempt_budget`** — `fetch` returns
   `Fetch(Transport)` for **at least `MAX_ATTEMPTS_PER_ARTIST + 1` calls against
   the same artist**, then `Ok(Found)` for all; assert the run completes with
   `failed == 0` and every artist in `done`.

   The count is the whole test and must be derived from the constant, not
   hard-coded: with fewer failures than the budget it passes even when
   network-shaped errors are wrongly classified as artist-shaped, because nobody
   exhausts anything. Exceeding the budget is what makes a misclassification
   show up as a dropped artist and a red test.

## Files

- `crates/reprise-core/src/artist_portrait/backfill.rs` — the classifier, the
  two constants, the tuple worklist, the rewritten `Err` branch in `run_worker`.
- `crates/reprise-core/src/artist_portrait/backfill_tests.rs` — five new tests,
  one fixture correction.

Read-only context, not owned by this task and not to be edited:
`crates/reprise-core/src/musicbrainz.rs:32` holds the complete `FetchError`
enum the classifier matches on, and `crates/reprise-core/src/artist_portrait/mod.rs:36`
holds `PortraitError`. The variant list in this plan is exhaustive against both —
if it turns out not to be, that is a fault in this plan, so stop and say so
rather than widening the classifier by guesswork.

No Kotlin, no FFI, no UI strings: `PortraitBackfillProgress` keeps its fields
and `PortraitBackfillState` keeps its variants, so `ArtistPhotoProgressBar.kt`
renders the new outcome with no edit.

## Verification

- `cargo test -p reprise-core artist_portrait`
- `cargo clippy -p reprise-core --all-targets` — keep it lint-clean rather than
  allowing; the new match arms are where a `clippy::match_same_arms` would bite.
- No Android build is needed for this change and none should be run.

## Parallelität

**Not cut into strands.** Both changed files are one module's implementation and
its own test file, the tests exercise the exact branch the implementation adds,
and the whole diff fits a single Codex run. A cut would buy no wall-clock and
would put the tests in a different worktree from the code they pin. Single
strand, one worktree, one branch.
