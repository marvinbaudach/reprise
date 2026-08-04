# Reprise Mobile — what option 2a actually became

The record for packages M1 through M3b of
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
- **Rating writes are synchronous on the main thread.** Play counting was moved
  to a dedicated writer thread; a star tap was left, being a discrete action
  rather than a 500 ms tick. It is the same class of thing.
- **The `tracing` events have no sink.** The artwork path warns about an unusable
  cache or a poisoned handle, but the Android app installs no subscriber, so
  those lines surface only in Rust tests. Every other frontend in this workspace
  installs its own.
- **One shared decode thread.** The 1092 px sheet cover queues behind list
  thumbnails on the same single-thread executor.
- **No Compose-level tests.** The project has no `androidTest` source set at all,
  so the slider's wiring and the rating's failure path are pinned only through
  the state machines beneath them.
- **`library/stats.rs` still documents its writes as happening "on the UI
  thread"**, which stopped being true for Android when play recording moved off
  it.
