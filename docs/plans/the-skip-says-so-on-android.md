---
slug: the-skip-says-so-on-android
worktree: /home/marvin/Projects/reprise-the-skip-says-so-on-android
branch: feature/the-skip-says-so-on-android
phase: refactored
codex_session:
created: 2026-09-02
---
# The skip says so on Android

Base: `origin/dev` @ `625ca3464a`.

FB-6 promises: *"the currently playing queue item faults → skip. A track shows
one toast «Track unavailable — skipped»."* On Android the skip happens and the
notice does not. This plan makes the notice visible, and fixes a second defect
found while grilling: Android shows the wrong one of FB-6's two texts.

## The bug, as measured

Measured on a Pixel 10 Pro XL against a debug build of `625ca3464a`, three
induced faults, ~20 UI samples over adb and then an on-device `uiautomator`
loop: the banner was never caught once. Full record in
`docs/plans/HANDOVER-2026-09-02-one-missing-file-postmerge-checks.md`, section
"The finding: the FB-6 notice is never visible".

Two correct halves meeting badly, both in
`crates/reprise-android-ffi/src/playback_session/stream_events.rs`:

```rust
// :108 — the fault raises the notice
state.snapshot.error = Some(fault_notice_text(policy.notices[0]).to_owned());

// :50-52 — the replacement track, milliseconds later, wipes it
if playback == PlaybackState::Playing {
    state.consecutive_faults = 0;
    state.fault_skip_limit = None;
    state.snapshot.error = None;
}
```

**On a successful skip — the normal case, the one FB-6 exists for — the notice
is set and cleared before a frame can show it.** It survives only when the
replacement *also* fails, i.e. exactly when it is least useful.

### Why no test caught it, and what that means for this plan

`crates/reprise-android-ffi/src/playback_terminal_event_tests.rs` has six
`fb_6_*` tests and they all pass. They emit `AndroidPlayerEvent::Error` and read
`session.snapshot()` synchronously; the fake bridge never emits the follow-up
`Playing` for the replacement track, so the clearing arm never runs in a test.
The suite asserts the notice is *set*. Nothing asserts it is still there one
event later.

The Kotlin side cannot close that gap either: there is **no `androidTest`
directory and no `createComposeRule` anywhere in the repo** — all 96 Kotlin test
files are plain JVM unit tests. No automated test in this repository can prove a
message reaches the screen. That is why the hardware check below is a landing
gate and not a nicety.

## Direction: the code is wrong, not the rule

1. **FB-6 already names the shape.** "one toast" is not vague — a toast is
   self-dismissing by definition.
2. **The desktop already implements it correctly** —
   `crates/reprise-gnome/src/ui/playback/playback_faults.rs` +
   `strings_issues.rs:251`, pinned by `missing_fault_notice_matches_fb_6_copy_exactly`.
   Weakening FB-6 to "silent skip" would delete working behaviour and its tests
   to match an Android defect.
3. **The repo has already written down which channel this belongs in.**
   `android/app/src/main/java/io/github/marvinbaudach/reprise/TransientMessage.kt`
   distinguishes **State** ("read out of whatever the surface currently knows …
   neither needs a lifetime of its own" — that is `playback.error`) from an
   **Acknowledgement** ("raised once … has no state behind it … there is no later
   event that would clear the message again. It therefore has to carry its own
   dismissal").

   The skip notice is an acknowledgement point for point: raised by one event,
   and afterwards there is no condition left to re-read — the track is skipped,
   playback is running. **This is a misclassification against the project's own
   documented rule, not a missing feature.**

This rules out the shape the handover floated ("a notice only a *manual* track
change clears"): that would give `snapshot.error` a lifetime of its own — the one
property `TransientMessage.kt` says it must not have — and break it for the case
where it genuinely *is* state.

## Task 1 — Rust: split the two channels

`crates/reprise-android-ffi/src/playback_session.rs`, `AndroidPlaybackSnapshot`:

```rust
pub error: Option<String>,          // unchanged: persistent, re-readable state
pub fault_notice: Option<String>,   // one-shot text, NOT cleared by Playing
pub fault_notice_count: u64,        // rises once per raised notice
```

The carrier is not invented here. The snapshot already ships a one-shot event
across the FFI boundary the same way — `automatic_advance_count: u64`, *"Rises
only when the backend reports that the current track ended"* — and the Kotlin
side already knows how to read one (`SleepTimer.kt:85`). That is
`TransientMessage.occurrence` one layer down.

`stream_events.rs`, the `PlayerEvent::Error` arm:

- the per-skip notice (`:108`) writes `fault_notice` and increments
  `fault_notice_count`; it no longer touches `snapshot.error`;
- the bound-reached case (`:116`, `TOO_MANY_UNPLAYABLE_TRACKS`) keeps writing
  `snapshot.error`. Playback stopped — that *is* re-readable state.

The `StateChanged`/`Playing` arm (`:50-52`) is unchanged: it still clears
`snapshot.error` and the fault counters, and never touches `fault_notice`.

A **new play intent** clears the one-shot text as well —
`playback_session.rs:205-206` already resets `consecutive_faults` and
`fault_skip_limit`, and `fault_notice = None` joins them there. Without it a
fresh queue would carry the previous queue's notice, which
`fb_6_a_new_queue_clears_the_prior_fault_notice_before_confirmation` correctly
forbids. **`fault_notice_count` is monotonic and is never reset** — resetting it
would let a later notice collide with a value the Kotlin side still remembers.

## Task 2 — the right one of FB-6's two texts

`playback_fault_policy(file_exists: bool)` in
`crates/reprise-core/src/playback/fault_policy.rs` distinguishes two faults:

| `file_exists` | notice | copy | `mark_missing` |
|---|---|---|---|
| `false` | `TrackUnavailableSkipped` | "Track unavailable — skipped" | `true` |
| `true` | `CouldNotPlaySkipped` | "Could not play {title} — skipping" | `false` |

Android calls `playback_fault_policy(true)` **hard-coded** (`stream_events.rs:106`)
and then `fault_notice_text` maps *both* variants to the first row's copy. So the
variant is always the "file exists" one while the text is always the other one's.
The measured hardware case was a missing file, so the text was right there **by
accident**.

**The verdict comes from the Media3 exception**, not from a later probe.
`Media3PlaybackPort.onPlayerError(error: PlaybackException)` (`:127`) already
holds the full cause chain — the same chain strand B made visible, whose logcat
line carries `java.io.FileNotFoundException`. Asking the exception answers the
right question (*what did the playback fail on*) rather than a later, racier one
(*does the file exist now*), and costs no extra provider round trip on the fault
path.

- `Media3PlaybackPort.kt`: classify in `onPlayerError` — walk `cause` for
  `FileNotFoundException`, or `errorCode == ERROR_CODE_IO_FILE_NOT_FOUND`.
- `crates/reprise-android-ffi/src/playback.rs:229`: `AndroidPlayerEvent::Error`
  gains the verdict alongside `message`.
- `stream_events.rs:106`: `playback_fault_policy(!missing)` instead of `true`.
- `fault_notice_text` maps the two variants to their two strings.
  `CouldNotPlaySkipped` needs the track title, which the session has via
  `state.queue.current()` and `library`.

**Kotlin answers only the boolean; policy and copy stay in Rust**, so the copy
test and `scripts/check-ux-traceability.sh` keep their single source of truth.

### Deliberately not in this plan: `mark_missing`

`mark_track_missing_if_current` exists in core, the desktop calls it
(`playback_faults.rs:104`) — and **`reprise-android-ffi` never calls it**, nor
does the Android UI have a missing-row surface to show the result. Honouring
`mark_missing` here would introduce library state with no way to see it, and a
wrong missing verdict changes the user's library. Recorded as a finding for its
own plan; `policy.mark_missing` stays unread on Android.

## Task 3 — Kotlin: render it as the acknowledgement it is

**Raised once, centrally.** `MainActivity.collectPlaybackServiceState()`
(`:347-360`) is the single point where snapshots become `playbackState`. The
counter comparison lives there and only a finished `TransientMessage?` travels
downwards, so `PlaybackUiState` and `LibraryPlayback` carry a message rather than
a raw counter — which keeps `LibraryPlayback` the pure re-readable state this
whole plan is about.

**A freshly attached screen must stay silent.** `playbackSnapshots` is a
`StateFlow` (`ReprisePlaybackService.kt:35`) and replays its current value to
every new collector. So the remembered count is adopted silently on first
observation and only a later *increase* raises a message — exactly
`SleepTimer.kt:83-86`'s shape. Because the collect block sits inside
`repeatOnLifecycle(STARTED)`, keeping the remembered value *inside* that block
re-arms it once per foreground cycle for free; a skip that happened while the app
was backgrounded is not announced on return, which is correct — the queue kept
playing and there is nothing left to act on.

Raising uses `TransientMessage(text).after(previous)`; `.after()` is what
restarts the timer when the same text is raised twice, so three faults in a row
do not ride out the first one's countdown.

**Extract the decision as a pure function** so the JVM suite can test it —
arming, increment, and repeat-restart — following the existing pattern in
`BrowseSurfaceTest.aRepeatedRatingFailureIsANewMessageWithItsOwnLifetime`. Only
the pixels then need hardware.

**Rendered on the surface that is visible.** `NowPlayingSheet` is composed inside
`BrowseScreen` (`:721`) and covers it, so:

- `NowPlayingSheet.kt:443` — render there (it is only composed when open);
- `BrowseScreen.kt:584` — render only when the sheet is not shown, using the
  `shownTrack` condition that already exists at `:717`.

One message, one timer, always where the user is looking. Both sites keep
`playback.error` exactly as they have it — it is still genuine state. The comment
at `BrowseScreen.kt:582` (*"Re-readable state, not timed acknowledgements; see
TransientMessage."*) stops being a half-truth.

**Duration stays 4 s.** Not an arbitrary tap-acknowledgement number: the desktop
sets `TOAST_TIMEOUT_S = 4` for plain informational toasts, *"deliberately shorter
than libadwaita's 5 s default"* (`crates/reprise-gnome/src/ui/toasts.rs:8`), and
`TRANSIENT_MESSAGE_MS` is already `4_000L`. The platforms agree; introducing a
second constant would create a silent divergence.

### Deliberately not in this plan: collapsing consecutive notices

The count-collapsing on the desktop (`flush_episode_skip_toast`,
`skipped_unplayable_episodes(count)`, and the test
`fb_6_consecutive_episode_faults_collapse_to_one_toast_count`) is **episode**
behaviour. A *track* fault there shows one toast per fault with the fixed FB-6
copy. Android matches the track behaviour: one message per fault, timer restarted
by `occurrence`. This also protects the copy, which
`check-ux-traceability.sh` and `missing_fault_notice_matches_fb_6_copy_exactly`
pin exactly.

## Task 4 — the tests that would have caught it

Write the first one first; it fails on `dev` today.

**New:**

- `fb_6_a_successful_skip_keeps_its_notice_after_the_replacement_plays` — emit
  `Error`, then `StateChanged(Playing)` for the replacement; assert
  `fault_notice == Some("Track unavailable — skipped")` and
  `fault_notice_count == 1`. **This is the regression guard for the whole plan.**
- `fb_6_each_fault_raises_the_notice_count` — two faults, count reaches 2.
- `fb_6_a_file_that_exists_but_will_not_play_names_the_track` — the
  `CouldNotPlaySkipped` copy, proving the variant is no longer hard-coded.
- Kotlin (JVM): the extracted raise function — arms silently on first snapshot,
  raises on increment, and a repeat produces a message unequal to its predecessor.

**Changed** (they read the moved field, and every `AndroidPlayerEvent::Error`
construction gains the new argument):

- `playback_terminal_event_tests.rs:243` `fb_6_a_new_queue_resets_the_prior_fault_bound`
- `playback_terminal_event_tests.rs:289`
  `fb_6_a_new_queue_clears_the_prior_fault_notice_before_confirmation` — its
  `snapshot.error == None` becomes `fault_notice == None`; intent unchanged.

**Kept as-is:** `TOO_MANY_UNPLAYABLE_TRACKS` stays asserted on `snapshot.error`,
which proves the split rather than a blanket move.

## Verification

All must be green in the worktree:

- `cargo test -p reprise-android-ffi`
- `cargo test --workspace` — the notice constants are shared with core
- the Android JVM suite
- `scripts/check-ux-traceability.sh` — FB-6 keeps its rule-named tests. The
  pre-existing, unrelated `NET-4b` error will still be there (red on `dev` since
  before this work; it belongs to the artist-photo plan). **Compare against a
  control-arm run on `origin/dev`, never read the exit status alone.**

**Hardware gate — blocking.** No automated test in this repo can prove the
message is visible (see "Why no test caught it"). On a real phone: rename a
queued track's file to `*.HIDDEN`, press `KEYCODE_MEDIA_NEXT`, confirm the
message shows for ~4 s while the replacement plays, then confirm a file that
exists but will not play names the track instead. Sample with an **on-device
`uiautomator` loop**, not adb round-trips — the 4 s window is too short for the
latter, which is exactly how the original bug survived twenty samples.

## Parallelität

**No cut. One strand.**

`AndroidPlayerEvent::Error` gains a required field, so the generated uniffi
binding breaks the Kotlin side immediately. Any seam between Rust and Kotlin
produces a strand that *cannot* go green in its own worktree — the precise
failure this section exists to prevent (measured 2026-08-11, Flathub strand D).
Cutting by feature instead is worse: task 1 and task 2 both edit the
`PlayerEvent::Error` arm of `stream_events.rs`, a guaranteed conflict that the
code phase's disjointness check would only surface after the work was paid for.

Nine files, roughly 250 lines including tests. The coordination cost of two
worktrees exceeds the wall-clock saved.

- **File ownership (single strand):**
  - `crates/reprise-android-ffi/src/playback.rs`
  - `crates/reprise-android-ffi/src/playback_session.rs`
  - `crates/reprise-android-ffi/src/playback_session/stream_events.rs`
  - `crates/reprise-android-ffi/src/playback_terminal_event_tests.rs`
  - `android/app/src/main/java/io/github/marvinbaudach/reprise/Media3PlaybackPort.kt`
  - `android/app/src/main/java/io/github/marvinbaudach/reprise/PlaybackUiState.kt`
  - `android/app/src/main/java/io/github/marvinbaudach/reprise/MainActivity.kt`
  - `android/app/src/main/java/io/github/marvinbaudach/reprise/BrowseScreen.kt`
  - `android/app/src/main/java/io/github/marvinbaudach/reprise/NowPlayingSheet.kt`
  - plus the Android JVM test file for the extracted raise function
- **Merge order:** n/a.
- **Post-merge cross-checks:** none — no verification in this plan reads a file
  the strand does not own. The hardware gate is a post-build activity, not a
  cross-strand comparison.

### Build note

`scripts/android-build.sh` defaults to `~/Android/Sdk`; the real SDK is at
`/home/marvin/.local/share/android-sdk`, and a fresh worktree has no
`android/local.properties`. Export `ANDROID_HOME` and `ANDROID_SDK_ROOT` before
building or all four stages fail. NDK at `/opt/android-ndk` is found correctly.

## Findings recorded for their own plans

1. **`mark_missing` is unreachable on Android.** `reprise-android-ffi` never
   calls `mark_track_missing_if_current`, and there is no missing-row surface to
   show it. The desktop marks and reloads the list; Android does neither.
2. **No Compose rendering tests exist.** No `androidTest` directory, no
   `createComposeRule`. Every Android UI behaviour is currently unprovable except
   by hand on a device — which is how this bug shipped.
