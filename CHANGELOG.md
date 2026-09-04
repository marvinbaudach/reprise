# Changelog

Reprise release notes are curated from the changes that reached the stable
branch. They describe user-visible changes rather than reproducing commit
messages.

## [0.1.138] - 2026-09-04

### Playback

- Starting Reprise greets you with a random track from the library in the player
  bar, stopped — instead of the position you left behind. Nothing autoplays, and
  the restored queue stays exactly as it was until you press Play.

### Discovery

- Concerts refreshes again. A provider that fails no longer aborts the artists a
  working provider could have resolved, and the list is checked hourly rather
  than once a day.
- Bandsintown now needs an app id of your own. The identifier the app used to
  ship is rejected by the service, so an unconfigured Bandsintown is simply
  absent instead of failing every request.

### Device sync

- The copy uses the folder the phone already has. A folder that differs only in
  capitalisation is no longer created beside the resident one, the path the copy
  actually used comes back from the device, and stale entries heal once the
  phone has been scanned.
- A short device walk is no longer read as proof that a file is gone, so tracks
  already on the phone are not copied a second time.
- Sync stops paying a fixed cost three to four times per file and drops a second
  full walk of the device nobody needed.
- A sync that removes files from the phone says so, instead of showing a bare
  device path under a heading reading "Syncing".

### Android

- The Now Playing card keeps its own picture through a track change, instead of
  briefly wearing its neighbour's.
- A faulty track that is skipped says so on Android too — the notice used to be
  wiped by the replacement track milliseconds later.
- The artist-photo card leaves when photos are simply missing, instead of
  standing at "64 / 66" for the rest of the session.
- Every album and every artist page keeps its own scroll position. They used to
  share one saved place per kind of list.
- Searching the Artists tab answers with artists; an open album page no longer
  answers in their place.
- The queue keeps its filter to itself: search is no longer offered there.

## [0.1.126] - 2026-09-02

### Playback

- A queued track whose file has gone missing is skipped instead of stopping the
  queue, on every surface that plays.
- Clicking the stopped waveform sets where the next playback starts.
- The waveform keeps the frame it settles on after a seek, instead of briefly
  falling back to the one before it.
- Under Repeat One, restarting the same track begins at the beginning again
  rather than re-applying the mark the previous pass left.

### Library

- The Doctor scans the whole library again, not a partial scope.
- Moving tracks to the trash no longer holds the library's write lock while the
  files move, so the rest of the library keeps working during a trash run.

### Device sync

- Sync no longer deletes a file on the phone that it is about to copy straight
  back.
- A track added to a sync playlist while the sync runs keeps the copy already on
  the device.
- Sync keeps the file name the phone already uses instead of renaming the track
  on every run.
- A cleanup pass that meets one unreadable file finishes its walk instead of
  giving up on the rest.

### Android

- Now Playing's swipe carries the whole screen with it, and the play button and
  the top edge answer the gesture while it happens.
- Up Next reaches the tracks just before the current one, not only the ones
  ahead of it.
- A playback error names what actually failed instead of reporting a generic
  fault.
- The app asks before it fetches artist photos.
- Play counts are retried when the library database is busy instead of being
  dropped, and the library no longer blocks the first screen while it loads.

### Language

- Added a Spanish translation.

## [0.1.84] - 2026-08-27

### Android

- The navigation mark and the header count follow the swipe while it happens,
  instead of waiting for the gesture to come to rest.
- Swiping to the tab next door no longer lands on an empty list: it is filled
  while the screen is still, before anyone reaches it.
- Browse queries run off the main thread, and a load that was cancelled by
  navigating away no longer raises an error banner.

## [0.1.83] - 2026-08-27

### Playback and presentation

- Replaced the equalizer's ten sliders with ten named profiles.
- Made track sorting reachable from the keyboard, without a pointer.
- Steadied the library while it scrolls: the columns no longer shift as rows
  change, and the player bar centres the playing track in a single landing.
- Clearing the filter hands a running queue the whole library again, instead of
  waiting for the queue to run dry.
- The track table follows the music again, and source lists stay where you
  scrolled them.
- A library that stored a wrong row height clears it on the next launch.

### Discovery

- Added multi-selection, a row menu, and reversible hiding to new releases.
- Concert discovery no longer asks for a Bandsintown application id; builds
  carry their own.

### Podcasts and online sources

- The Podcasts and YouTube badges count the shows you follow rather than their
  unplayed episodes.
- Source artwork loads from a cached thumbnail instead of decoding the original
  every time it comes into view.

### Preferences

- The plugins page now reads as one master switch over the content beneath it,
  and every background job it starts is named.
- Each build offers one primary Last.fm setup path rather than several, and
  Flatpak builds carry their own Last.fm credentials.

### Android

- A queue row moves under the thumb instead of one tap per slot, and TalkBack
  gets the same move.
- The visualizer's bars follow playback time instead of updating four times a
  second.
- Reloading artist photos shows its progress.
- The Now Playing haze became a slower, theme-matched oil film that no longer
  flashes on every beat.

### Fixes

- Deleting a playlist clears it from the device page immediately, instead of
  leaving a stale row behind while the remaining playlists pile up under it.
  The playlist summary names its track count again.
- Filled buttons, the checked shuffle toggle, and disabled actions carry
  readable labels again.
- A toast always carries its message.
- The showroom lightbox fits and zooms on a phone.
- Refreshed the translation catalogues for the sort menu's new strings.

## [0.1.45] - 2026-08-21

### Desktop library and navigation

- Added editable track metadata, multi-selection actions, configurable columns,
  richer filtering and search, keyboard navigation, persistent places, and
  session restoration.
- Added album and artist artwork throughout the library, Now Playing, source
  views, concert listings, release discovery, and listening statistics.
- Added Library Doctor review and repair flows, missing-file recovery, import
  diagnostics, Rhythmbox import, device synchronization, and safer file
  writeback boundaries.

### Playback and presentation

- Reworked Now Playing, the typed mixed-item queue, playback history, gapless
  handoff, repeat and shuffle behavior, and failure reporting.
- Added synchronized lyrics, replay-gain and equalizer controls, waveform and
  spectral seeking, an audio-reactive visualizer, and experimental instrumental
  generation for supported native builds.
- Improved accessibility, responsive layouts, focus and selection restoration,
  reduced-motion behavior, and GNOME platform integration.

### Podcasts and online sources

- Added podcast subscriptions, downloads, playback progress, and queue support;
  YouTube channels and audio playback; and internet-radio discovery and
  favorites.
- Added concert discovery with ticket status and artist imagery, new-release
  discovery, artwork downloads, online lyrics, and explicit controls for every
  network-backed feature.

### Android

- Added the native Android library, search and artist surfaces, Media3 playback,
  queue and history, equalizer controls, artwork and artist portraits, and
  mobile library synchronization.
- Added a cover-driven, audio-reactive Now Playing scene with waveform and
  spectral seeking, visualizer choices, appearance settings, and Android 8.0+
  support.

### Tools, packaging, and reliability

- Added a headless CLI and capability-gated MCP server, plus a toolkit-neutral
  runtime and versioned client protocol for future frontend consolidation.
- Added the offline Flatpak manifest, AUR packaging, AppStream screenshots,
  stricter architecture and UX contracts, path-aware CI, and independent
  desktop and Android versioning.
- Hardened private-data handling, provider error redaction, network consent,
  database migrations, background job cancellation, and isolated automated
  verification.

## [0.1.1] - 2026-07-25

- Improved Now Playing panel sizing and playback-marker stability while
  sorting the library.

## [0.1.0] - 2026-07-12

- Initial release with local-library scanning, playback, queueing, playlists,
  search, sorting, ratings, and organization.
