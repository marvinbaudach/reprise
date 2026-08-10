# The queue gets its own tab, and rows get a menu

> **For agentic workers:** implement task-by-task, test-first. Each task ends
> with its own green run and its own commit.

**Spec:** `docs/superpowers/specs/2026-08-10-android-queue-search-context-menu-design.md`
(read it first — it carries the decisions and the reasoning behind them).

**Base:** `75a24b35a9` (`origin/dev`, 2026-08-10).

## What is there today

**The queue is hidden inside Now Playing.** `NowPlayingQueuePage`
(`NowPlayingQueue.kt`) already lists the future, promotes a row, moves one, and
removes one. It is reached only by a toggle that replaces the artwork
(`NowPlayingSheet.kt:145`, `NowPlayingScene.kt:342`), so it is invisible until
you already know it exists.

**Nothing can be enqueued from the library.** M12 said so in as many words:
*"No play next, no add to queue on a library row. `append_tracks` stays
unused."* That was right for a package whose queue had no visible home. It is
wrong now, and **this plan reverses it deliberately** — say so in the commit
message rather than letting a reader think the earlier decision was forgotten.

**There is no context menu anywhere.** Not one `combinedClickable`, not one
`onLongClick` in the whole app.

**The search field opens without focus.** `TitleSearchField`
(`BrowseTabs.kt:62`) is a plain `OutlinedTextField` — no `FocusRequester`, no
trailing icon. A close *does* exist in code (`LibraryFrame.kt:85–90` swaps the
magnifier for `close`), but the owner reports on a current build that no ✕ is
visible. That contradiction is unresolved and stays unresolved here: this plan
makes closing reachable by three independent routes instead of guessing which
one is broken. **The emulator diagnosis is the owner's pass, not yours.**

**Deleting is not possible at all**, from anywhere.

## What ships

Seven packages. Rust first, because Kotlin has nothing to call otherwise.

---

### Task 1 — Core learns to enqueue

**Files:** `crates/reprise-core/src/queue.rs`, tests in
`crates/reprise-core/src/queue_tests.rs`.

```rust
/// Where an explicitly enqueued track belongs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuePlacement { Next, Last }

/// Enqueues explicit user picks and returns how many were taken. Never
/// starts playback.
///
/// Unlike `append_tracks`, this revives an exhausted queue: an explicit
/// user action must not evaporate.
pub fn enqueue(&mut self, new_ids: &[i64], placement: QueuePlacement) -> usize
```

`Next` inserts the new order indices at `pos + 1`; `Last` appends them. Both
push the ids onto `ids` and keep the `order.len() == ids.len()` invariant, and
both call `note_sequence_changed()`.

Do **not** change `append_tracks`. Its documented contract — an exhausted queue
stays exhausted — is what the desktop relies on, and the desktop shares this
type.

**Prove, each mutation-proven:** an empty queue takes the tracks and sets `pos`
to `Some(0)` without playing; an **exhausted** queue (ids present, `pos == None`
after `Repeat::Off` ran off the end) revives onto the first newly enqueued
track — without this, `remaining_window` returns empty and the whole feature is
invisible; with shuffle on, `Next` lands the track next in *play* order, not in
`ids` order; two `Next` calls in a row put the second batch **before** the
first; duplicates are kept; an empty slice is a no-op returning `0`;
`sequence_identity` changes. One test pins `append_tracks`' unchanged
exhausted-queue behaviour, so a later edit cannot quietly move desktop ground.

Run `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` and
show it empty.

---

### Task 2 — The FFI enqueues, and stops computing the window start twice

**Files:** `crates/reprise-android-ffi/src/playback_session/queue_boundary.rs`,
tests in `crates/reprise-android-ffi/src/queue_boundary_tests.rs`.

```rust
pub fn queue_tracks_next(&self, track_ids: Vec<i64>) -> Result<u32, AndroidPlaybackError>
pub fn queue_tracks_last(&self, track_ids: Vec<i64>) -> Result<u32, AndroidPlaybackError>
```

Both take **ids only** and resolve the uris from the database themselves. The
session holds `track_ids` and `uris` in parallel (`playback_session.rs:160`); a
uri handed down from the UI is exactly the stale value Media3 would later choke
on. Ids with no resolvable path are skipped and do not count toward the return.
Then, as the existing operations do: `persist_queue`, refresh `set_next`,
`notify`. **Never call `start_current`.**

**The window start.** `upcoming_tracks` reads from `pos + 1`. Enqueue three
tracks into an empty queue and the first becomes current — invisible in the tab
and invisible in Now Playing, because nothing is loaded and the FFI has no
load-without-start path (`start_current` calls `play_uri` directly). So:

> While nothing is loaded (`current_loaded == false`), the queue view starts
> **at** the current track instead of after it.

Put that decision in **one** helper that yields the window's start offset, and
have `upcoming_tracks` *and* `upcoming_order_position` — meaning every edit and
promote operation — read it from there. Two separate computations would point at
different rows while playback is paused. This project has shipped that exact
class of bug twice; do not ship it a third time.

**Prove:** enqueued tracks appear in `upcoming_tracks` immediately, survive
`persist_queue` plus reload, and leave `set_next` correct; enqueueing starts
nothing; ids with no path are skipped; with playback stopped, `upcoming_tracks`
contains the current track **and** `play_upcoming_track_now(0)` starts that same
track — that pair is what proves view and edits share one computation.

---

### Task 3 — A whole album's ids

**Files:** `crates/reprise-android-ffi/src/lib.rs`, tests alongside the existing
browse tests.

```rust
pub fn album_track_ids(&self, album: String, album_artist: String)
    -> Result<Vec<i64>, LibraryError>
```

Unwindowed, in disc/track order, keyed by the same `(album, album_artist)` pair
`list_album_tracks` uses. "Enqueue album" must not depend on how much of the
list happens to be loaded.

**Prove:** an album with more tracks than one window returns all of them, in
order; an unknown album returns an empty vec rather than an error.

---

### Task 4 — Deleting, with Core keeping the books

**Files:** `crates/reprise-android-ffi/src/lib.rs` (or a focused sibling module
if that file is near 800 lines — check before adding).

```rust
#[uniffi::export(callback_interface)]
pub trait TrashAction: Send + Sync {
    /// Deletes the file at `uri`. Returns the error message on failure,
    /// `None` on success.
    fn trash(&self, uri: String) -> Option<String>;
}

pub fn trash_tracks(&self, track_ids: Vec<i64>, action: Box<dyn TrashAction>)
    -> Result<AndroidTrashReport, LibraryError>
```

Resolve the paths from the database, hand them to
`reprise_core::library::trash_tracks_with`, and return its `TrashReport` as a
uniffi record. Core already refuses a stale request whose registered path
changed — keep that behaviour visible in the report rather than swallowing it.

Then clear the successfully deleted ids out of the queue with
`Queue::remove_ids`. That call already advances the playhead to the next
surviving track in play order when the current one is removed, so: if the
current track was among them, follow with `start_current()`; if nothing
survives (`pos == None`), stop instead.

**Prove:** a deliberately failing action yields a partial report and leaves
those database rows intact; deleting the playing track advances to the next one
and it plays; deleting the last surviving track stops playback; deleted ids are
gone from `upcoming_tracks`.

---

### Task 5 — Kotlin plumbing

**Files:** `PlaybackControls.kt`, `ActivityPlaybackControls.kt`,
`ReprisePlaybackService.kt`, `MainActivity.kt`, `LibrarySession.kt`,
`AndroidLibrarySessionPort.kt`.

**Corrected 2026-08-10** after the first run stopped here, correctly: the
original four-file list was impossible. `AndroidPlaybackSession` lives as a
`private var coreSession` inside `ReprisePlaybackService` (`:25`), and
`AndroidLibrarySessionPort` holds only `resolver`, `preferences` and `library` —
neither of the four files could reach playback at all.

Follow the pattern the service already uses rather than inventing a second
owner. `ReprisePlaybackService` exposes `internal fun playTracks`,
`togglePause`, `playUpcomingTrackNow` and friends, all delegating to
`coreSession()`; add `queueTracksNext`, `queueTracksLast` and `trashTracks` the
same way, and let `ActivityPlaybackControls` reach them over the same bound-
service route it already uses for the queue operations. `MainActivity` only
wires them through.

`albumTrackIds` needs none of that — it is a plain library query and belongs on
`LibrarySession` / `AndroidLibrarySessionPort`, next to `listAlbumTracks`.

`deleteTracks` passes `DocumentsContract.deleteDocument(resolver, Uri.parse(uri))`
in as the `TrashAction`. Results come back over `ApplicationLooperDispatch`,
exactly as the existing queue operations do.

**Prove:** the port forwards ids unchanged and reports a partial delete as a
partial delete.

---

### Task 6 — One menu, four callers

**Files:** create `TrackContextMenu.kt`; wire it into `LibraryTrackRows.kt`, the
album grid in `BrowseTabs.kt`, the queue page, and both Now Playing action rows
(`NowPlayingSheet.kt` around the sleep-timer/heart row, `NowPlayingScene.kt`
around `now-playing-actions`).

**These file lists are a starting point, not a fence.** Two runs have now
stopped because a list was too narrow. If honouring the contract needs an
adjacent file — the playback service, `MainActivity`, an FFI boundary — change
it, keep the change as small as the contract allows, and name it in the commit
message. Only stop if the *contract itself* is wrong, not merely its file list.

**Playing a whole album needs an id-only route** (this is what stopped run 2).
`ReprisePlaybackService.playTracks` takes `List<LibraryTrack>` and splits it into
ids and uris (`:202`), while `album_track_ids` returns ids alone. Add
`play_track_ids(track_ids, start_index)` at the FFI playback boundary, resolving
the uris there exactly as `queue_tracks_next` does, and an
`internal fun playTrackIds` on the service beside `playTracks`. Do **not** hand
uris down from the UI — that is the stale-path trap Task 2 exists to avoid, and
an opened album's rows are windowed anyway, so the UI does not have them all.

Long press opens a `DropdownMenu` at the touch point with
`HapticFeedbackType.LongPress`; short tap keeps today's meaning
(`combinedClickable(onClick = …, onLongClick = …)`).

| Caller | Items |
|---|---|
| Titles, Favourites | Play · Play next · Add to queue · ─ · Delete from device… |
| Album | the same four, over every track of the album in disc/track order |
| Queue row | Play now · Move up · Move down · Remove from queue |
| Now Playing overflow (`more_vert`) | Delete from device… |

"Play" means what a tap means today — the queue is replaced by the selection and
starts there. "Move up"/"Move down" shift by exactly one position. Enqueueing
never starts playback, not even into an empty queue, and reports through the
existing `TransientMessage` ("3 tracks queued").

**Deleting always goes through a confirmation dialog** naming the tracks and the
count and stating plainly that it cannot be undone — SAF has no wastebasket. The
cancel path is part of the feature and gets its own test.

One file holds the items, the dialog and the callbacks. Three separate menu
implementations would drift.

**Prove:** long press opens the menu on a title row, an album tile, a queue row
and in Now Playing; enqueueing from a library row leaves playback stopped;
confirming deletes and cancelling deletes nothing.

---

### Task 7 — The fifth tab, and the search field

**Files:** `BrowseScreen.kt` (the `BrowseTab` enum), `BrowseTabs.kt`,
`LibraryFrame.kt`, `NowPlayingSheet.kt`, `NowPlayingScene.kt`,
`MobileSurfaceViewModel.kt`.

`BrowseTab` gains `QUEUE`. It shows the existing `NowPlayingQueuePage`, lifted
out of the sheet: `nowPlayingQueueVisible` and both call sites go away, and Now
Playing shows only the cover again. The page uses the regular layout instead of
the hard-wired `SurfaceLayout.STACKED` it uses inside the sheet; the count line
stays.

M12 kept the playing track out of the list because the sheet showed it directly
above. In the tab that reasoning still holds — the mini player in
`LibraryBottomFrame` sits right below it.

The stored start destination (`AndroidLibraryDestinationChoice`) keeps its four
library values. Queue is **not** among them: starting into an empty queue after
a restart is a poor welcome. Concretely, `selectDestination(BrowseTab.QUEUE)`
leaves the stored destination untouched while the pager switches normally.

`TitleSearchField` gains a `FocusRequester` — focus and keyboard are requested
in a `LaunchedEffect` on open — plus a trailing icon (`clear` while there is
text, `close` when empty) and a `BackHandler` that is active only while the
search is open. The existing summary-row toggle stays. Three routes, on purpose:
the owner sees no ✕ on a current build and the cause is unknown.

**Prove:** five tabs; the queue is reachable as a tab and the sheet toggle is
gone; selecting Queue does not overwrite the stored destination; the search has
focus after opening; back closes it; the trailing icon clears text first and
closes when empty.

**Two things this task cannot settle from code.** Five entries is the ceiling of
a Material 3 `NavigationBar`, so keep the label to one short word and expect the
layout to be at its limit on a narrow screen — the owner checks it on the
device. And pick the tab's Material Symbol name from the names the app already
renders successfully; do **not** conclude one exists by reading the font's
ligature table. That check has already returned a wrong answer here: it found
`close` but missed `play_arrow`, which demonstrably works.

---

## Rule drafts

This adds user-facing behaviour that no `[active]` rule covers, and AGENTS.md
forbids deciding that locally. Append `[planned]` drafts with the next free ids
to `docs/research/android-ux-rule-drafts.md` (the Android drafts live there, not
yet in `docs/ux-rules.md`), each marked `<!-- REVIEW: Regelvorschlag -->`:
enqueueing never starts playback; deleting is confirmed and irreversible;
deleting the playing track advances rather than stops; the queue view includes
the current track while nothing is loaded. Do not flip anything to `[active]`.

## Verification

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p reprise-core --lib
cargo test -p reprise-android-ffi
cargo test --workspace
cargo audit
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'   # must be empty
bash scripts/check-android-theme.sh
bash scripts/android-build.sh
cd android && ./gradlew :app:testDebugUnitTest :app:assembleDebug
```

`JAVA_HOME=/usr/lib/jvm/java-21-openjdk` — the system default is 26 and
Robolectric dies in teardown under it, which looks exactly like a mistake of
your own. `GRADLE_USER_HOME` inside the worktree. `ANDROID_HOME` is
`~/.local/share/android-sdk`; a bare `cargo build` for the Android target fails
on ring/cc-rs, so use `scripts/android-build.sh`.

**Gradle reports `testDebugUnitTest` green without running anything.** Delete
`android/app/build/test-results/testDebugUnitTest` first, compare every
`TEST-*.xml` mtime against the run's start, and report the real numbers —
suite count as well as test count, because an excluded suite has hidden a red
run here before.

Take the starting counts from the newest entry in `.superpowers/sdd/progress.md`
and state each task's expected new total relative to the run it starts from. A
number written into this file would be stale within days.

`crates/reprise-gnome` may change **zero lines**. `reprise-core` changes exactly
once, in Task 1; if you believe it needs more, stop and say so.

Every test mutation-proven: break the mechanism, show the real red output,
restore it, show green. A test that also passes without the change does not
count.

## Nothing on the owner's screen

No emulator, no `adb`, no launching the app. The device pass — including the
unexplained missing ✕ — is the owner's.
