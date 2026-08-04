# Reprise Mobile — what option 2a actually became

The record for packages M1 through M6 of
`docs/superpowers/plans/2026-08-04-mobile-m3.md` (design source: *Reprise
Mobile*, option **2a — M3 Baseline**, design system **Nocturne**). Written
2026-08-04, after the branch was run on a device rather than only compiled.

This document exists because the mobile surface ships **less** than the frames
show, on purpose. Anyone reading the design next to the app needs to know which
gaps are decisions and which are unfinished work.

## What was built

### M1 — the ground

Nocturne's roles as a Material 3 `ColorScheme` (dark only), the M3 shape scale
`4 / 8 / 12 / 16 / 28`, Roboto Flex typography and Material Symbols Rounded as a
font. `scripts/check-android-theme.sh` fails the build if a raw Compose colour
appears outside the theme file, so later packages could not quietly invent one.

### M2 — the library screen

`play_count` and `rating` carried across UniFFI on `TrackRow`; the 64 dp top app
bar, 32 dp filter chips, 72 dp rows, the playing row tinted at 8 % teal with its
four animated bars, the 72 dp mini player with a 3 dp progress strip, and an
80 dp navigation bar. Paging holds at 200-row windows, one continuation request
per offset.

### M3a — artwork crosses the boundary

The cover cache root became **something the platform supplies** rather than
something `reprise-core` assumes: `MusicLibrary::open` takes the app's private
cache directory, and the XDG wrappers the desktop uses are untouched.
`track_artwork` resolves one track lazily and never inside a paged query.
`ThumbnailSize::MobileList = 168 px` is 56 dp at the target's measured 3×
density — deliberately not the desktop ladder's nearest rung. Kotlin resolves
off the main thread behind a recycle token, so a row that scrolls away cannot
overwrite the cover of the row that replaced it.

The same package fixed the defect the first device run exposed: the app bar drew
*under* the status bar, because the source contained no edge-to-edge handling at
all. Review then found a second one — Material 3 subtracts the system inset
*inside* the navigation bar, so a bar pinned to 80 dp spent that budget on the
inset instead of on itself.

### M3b — Now Playing

An expanding bottom sheet out of the mini player, collapsed by predictive back —
no navigation stack was introduced for a single second surface. A 364 dp cover at
a measured 1092 px rung. A seek slider whose wave is drawn only left of the head,
with the elapsed time in teal and the remainder as `−m:ss`. Shuffle and repeat
both **set and read back**, so the buttons show the mode rather than only
changing it. A rating that writes through to the core, and — the thing the plan
did not know it needed — playback that actually counts.

### M5 — the theme, shared without being stolen

The plan said the core knows nothing about themes. It was wrong: the desktop has
always persisted `ui.theme` through the core's generic settings table, so there
was nothing to invent — only something to read. Android reads that id and renders
it if it recognises it.

The load-bearing half is what happens when it does **not**. An id the phone
cannot draw is kept, not corrected: the surface falls back to its own default and
writes nothing. A desktop-only theme therefore survives a trip through the phone
instead of being quietly replaced by whatever the smaller surface happened to
support. Reading is not authorship — the rule the equalizer inherited a package
later.

### M6 — playback settings

The stored equalizer stopped being ten GStreamer band levels and became a
**backend-independent curve** of ordered `(frequency, gain)` points. The ten
existing values migrate to points at GStreamer's own centre frequencies, so the
desktop's sound does not change — and `crates/reprise-gnome` does not change
either, by a single line. It keeps asking for ten band levels; the core answers
by projecting the curve back onto those ten centres, exact by construction after
the migration, and a test asserts that equality rather than trusting it.

Android reads the band count and the centre frequencies from the **live audio
session**, never from a constant, and projects the same curve onto whatever that
device reports. A projection is never written back — M5's rule, applied to a
payload that has no "unrecognised" state but does have a lossy one. Editing on
the phone genuinely replaces the curve with that device's bands, and the surface
says so in one plain sentence *before* it happens; viewing, opening and applying
write nothing.

The projection itself lives in Rust and is exported over the FFI. It had been
implemented a second time in Kotlin — so the version that was tested was not the
version a phone rendered. This codebase has paid twice for one decision living in
two places; the Kotlin copy is gone, and `DeviceEqualizer` contributes only what
is genuinely the device's own: its real centres, its level range, its millibel
representation.

Gapless maps straight through to both backends. Crossfade and ReplayGain do not
appear at all — see below.

### After the branch review

Four Rust findings and three Kotlin ones, none of which changed what the surface
does:

- The `tracing` events now have a sink. `initLogging()` is exported from the FFI
  and called once from `RepriseApplication.onCreate`, which runs before either
  door into the library opens — the activity's `MusicLibrary` and the playback
  service Media3 may start on its own. On a device the lines land in `logcat`
  under the tag `Reprise`; on a host build they land in a buffer the crate's own
  tests read, which is how the installation is proven rather than asserted.
- A play count that loses to the scanner's write lock is offered again, bounded,
  before it is given up — and giving up names the track.
- `track_artwork` reports a poisoned handle or an unconfigured tree as a typed
  `LibraryError` like every sibling method, while "this track has no picture"
  stays the ordinary `null` the UI has always rendered.
- The seven transport commands travel through `LocalPlaybackControls` instead of
  seventeen parameters, and `BrowseScreen.kt` became four files along the seams
  it already had.

### The app was playing into the room

Media3 leaves audio focus and the becoming-noisy handler **off** by default, and
the device confirmed it: while a track was playing, the system's audio focus
stack was empty. An app without focus talks over other players, keeps going
through a phone call, and drowns navigation prompts; without the noisy handler,
unplugging headphones moves the music to the speaker at whatever volume the room
did not ask for. Both are now requested where the player is built, and the same
`dumpsys` that showed the empty stack shows `gain: GAIN` afterwards.

This is the kind of defect no unit test finds, because nothing was wrong with the
code — something was simply never asked for.

### A crash that turned out not to exist

The rating work left behind a suspicion: `TrackArtwork.shutdown()` stops its
thread without waiting, while `onDestroy` closes the `MusicLibrary` immediately
after — so an artwork call still inside the FFI would be calling into a freed
handle.

It was investigated before it was fixed, and there is nothing to fix. uniffi
0.32 counts in-flight calls: `close()` only releases when its decrement reaches
zero, each call holds its own cloned handle, and a call that starts *after* the
close is refused with an `IllegalStateException` before any native code is
touched. `shutdownNow()` would not have helped in any case — an interrupt is a
flag a blocking native call never reads. What was missing was not a guard but an
explanation, so the reasoning now sits at the teardown, next to the deliberately
different one the rating writer needs. Two regression tests pin the halves that
are ours to keep.

### Ratings left the main thread too

The review that moved play counting off Media3's application thread left the
star tap behind, on the grounds that it is a discrete action rather than a
500 ms tick. It was the same blocking SQLite write, on the main thread, behind
the same handle a SAF scan holds for the whole of its folder walk — so a tap
during a scan did not merely stutter, it waited for the walk.

It now goes through `RatingWriter`: one thread the activity owns, one tap at a
time in the order they were made, with the outcome delivered back on the main
thread. **The visible behaviour is unchanged on purpose.** The star still moves
only after the database has agreed, a refusal still arrives as the
self-dismissing message, and nothing is shown optimistically and taken back —
`PlaybackControls.setRating` traded its return value for a callback precisely so
that the star could keep waiting for the same answer it always waited for.
`ComposeBehaviorTest` pins both halves: the failure that must not move a star,
and the success that must not move one *early*.

Two things that follow, and are not defects: an answer that arrives after the
sheet has moved to another track lands in state nobody is showing, so it is
logged rather than shown; and a tap made during a scan is now answered when the
scan lets go instead of freezing the screen until it does.

## Verified on a device, not assumed

Every claim below was observed on the `pixel10xl_api37` emulator against the
1824-track fixture. This matters because M1 and M2 were merged without anyone
ever running them; every ledger entry for those packages ends "no real data,
emulator, device".

| claim | evidence |
| --- | --- |
| Covers come from embedded artwork over SAF | rows and mini player render real art; tracks without a picture keep the no-artwork symbol |
| The cache honours the platform root | `/data/data/de.reprise.spike/cache/reprise/covers/…-168.png` and `…-1092.png` |
| Recycling holds | rows of one album share a cover, two albums by one artist differ, after repeated scrolling |
| Paging survives artwork | the count steps `200 → 400 of 1824` |
| The status bar defect is gone | the title clears the clock |
| The navigation bar keeps its 80 dp | with a system inset present the bar renders 384 px = 80 dp + 48 dp, against 240 px before the fix, where the active pill was clipped |
| Seek is real | dragging moved playback `0:59 → 1:53` with no snap-back |
| Shuffle and repeat are real | both render filled state containers after tapping |
| Rating writes through | tapping the fourth star wrote `rating = 4` to track 830 **in the app's own database** |
| Plays are counted | track 374 went `0 → 1 → 2` across two listens, six further tracks were counted as the queue advanced |
| The 80 dp play button | pixel scan: teal fill from x=552 to x=792 at 3× = 240 px, tapering at top and bottom, i.e. the 28 dp rounded square rather than a circle. Before the fix the same scan read 192 px = 64 dp |
| Logging arrives where it is read | the cover cache was sabotaged on purpose; the resulting `tracing` warning appeared in `logcat` under the tag `Reprise`, so the sink is proven by an event rather than by its installation |
| Nothing announces itself as a picture | all 140 accessibility nodes were read back: the word "artwork" appears in none of them, and a row states its rating and its play count |
| Audio focus is actually held | `dumpsys audio` showed an **empty** focus stack while a track played; after the fix the same command shows `gain: GAIN` |
| A theme the phone cannot draw survives it | `ui.theme = night-terrain` was still `night-terrain` in the database after the phone had opened, rendered its fallback and closed |
| The curve is projected, not stored twice | a real ten-point curve rendered on the emulator's **five** reported bands as +1.1 / +4.0 / +0.1 / −1.0 / +1.9 dB, and the stored curve was unchanged afterwards |
| The merge kept dev's launcher icon | the built APK's own manifest resolves `application: icon='res/mipmap-anydpi-v26/ic_launcher.xml'` — read from the artifact, not from the source file the conflict was resolved in |
| A rating still writes through after leaving the main thread | on the merged build: track 643 went `4 → 2` on a tap of the second star and back `2 → 4` on a tap of the fourth, two writes through the one writer thread in one session, no crash and no ANR |

That last row took four attempts, and the two failures are worth keeping because
both would have read as green:

1. The first pulled `files/reprise.db` alone. A rating lands in the write-ahead
   log first, so both snapshots were byte-identical and the diff said `NONE` —
   the exact shape a broken write would also have. Android offers no read-only
   URI here, so the honest read pulls `reprise.db`, `-wal` and `-shm` together
   and opens the copy **read-write**, letting SQLite replay the log. The log
   growing `0 → 4152 → 8272` bytes across the two taps is now part of the
   evidence rather than the thing that hid it.
2. The second read correctly and still saw nothing: the playing track already
   stood at 4, so tapping the fourth star wrote 4 over 4. A test whose passing
   state is indistinguishable from its failing state is not a test.

## What was deliberately left out, and why

These are **not** unfinished work. They were measured, found to have no backend,
and omitted rather than mocked — the same rule that elsewhere in this codebase
made `MissingReason::Unknown` unable to delete anything.

- **Desktop sync.** The frames show a paired host, "Wi-Fi gekoppelt · vor 2 Min",
  pending play counts, mirrored playlists, an MTP fallback and a sync history.
  **None of it exists on Android**: no pairing, no transport, no queue of pending
  changes. A screen announcing a connection to a machine nothing is connected to
  is a fabrication, not a placeholder.
- **The sync assist chip** with its breathing dot, for the same reason.
- **Lyrics.** `reprise-core` has a lyrics module; the Android FFI exposes nothing
  from it. A lyrics screen would have had to invent its content.
- **Playlists and Queue as destinations.** The FFI has no playlist call. The
  navigation bar therefore carries the one destination that exists, not the four
  the frames draw. A tab pointing at nothing is a lie, not a stub.
- **A light theme.** Every 2a frame is dark and Nocturne ships no light ramp.
  Generating one would be a second design that nobody reviewed.

## Known gaps — unfinished, not decided

Named here so they are not mistaken for the list above.

- **Rotation loses what is playing.** The sheet's expanded flag survives, but the
  playing selection does not, so the mini player and the sheet's content vanish
  on a configuration change. Fixing it means either writing up to 500 tracks into
  `savedInstanceState` — a `TransactionTooLarge` risk — or introducing a second
  source of truth for "what is playing". That is a decision, not a refactor.
- **A play can still be lost, but no longer silently.** The play-count writer
  now offers a write that lost to the scanner's single folder-walk transaction
  up to four times before giving up, and says which track it gave up on. What it
  still has is no *persistent* queue: a play in hand when Android kills the
  process is gone, and nothing survives to retry it after a restart. That is a
  bigger decision than a retry loop.
- **One shared decode thread.** The 1092 px sheet cover queues behind list
  thumbnails on the same single-thread executor.
- **Compose behavior is host-tested, not device-rendered.** The existing
  `:app:testDebugUnitTest` gate now runs `compose-ui-test-junit4` 1.11.4 on
  Robolectric 4.16.1's simulated API 36. It drives the seek gesture across a
  snapshot tick and its release, a rejected rating without optimistic star
  movement, a rating whose write has not answered yet and whose stars therefore
  have not moved yet, and the mini player / predictive-back sheet lifecycle
  while the Library stays composed.
  It also drives `OnBackPressedDispatcher.dispatchOnBackStarted` /
  `dispatchOnBackProgressed` / `dispatchOnBackCancelled` directly (activity
  1.13.0 routes these through the same `NavigationEventDispatcher`
  `PredictiveBackHandler` registers with, confirmed by driving them under
  Robolectric rather than assumed), and reads the sheet's actual
  `graphicsLayer`-transformed position back with `getUnclippedBoundsInRoot()`.
  So a progressed gesture that moves the sheet without dismissing it, and a
  cancelled gesture that snaps it fully back open, are both covered now, each
  proven to fail when the production behaviour it guards is broken.
  This still does not prove pixel rendering or physical-device touch-to-gesture
  dispatch — Robolectric calls the dispatcher API directly rather than
  synthesizing the platform's edge-swipe recognition — so device checks remain
  the proof for those two.
- **The rating writer has been watched writing, but not while contended.** A
  device now confirms the ordinary path end to end (`4 → 2 → 4` on track 643,
  read with the log included). What no device has shown is the case the change
  was actually made for: a star tapped **while a scan holds the library** for the
  length of a folder walk. `RatingWriterTest` and `ComposeBehaviorTest` prove the
  thread hop, the ordering, the answered teardown and the star that waits; the
  contended case is still argued from the code rather than seen.
