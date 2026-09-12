# Changelog

Reprise release notes are curated from the changes that reached the stable
branch. They describe user-visible changes rather than reproducing commit
messages.

## [0.1.187] - 2026-09-12

### Playback and presentation

- Six soft drops of the blurred artwork drift behind the cover, where two rigid
  clouds used to move. Each layer baked its shapes into a single picture before,
  so nothing inside a layer could move against anything else; whatever drove
  them, they travelled as one form. Each drop now carries its own picture and
  its own long, slow waves for position and size, and because no two of those
  waves share a period they meet, merge and part without ever repeating. In
  light appearance a layer is composited in one pass instead of one pass per
  drop, so overlapping drops no longer multiply each other down into a dark
  blotch.

### Appearance

- The Now Playing panel settles into its final proportions. The cover grows to
  184 px, the head band follows one specification from the cover's top edge down
  to the footer, and the segment control is rebuilt against libadwaita's own
  nodes instead of selectors it never had. The list rule fades out at both ends.
  The panel cover keeps one static shadow, and the reactive lift that follows
  the music stays where it belongs, on the player bar.
- The album name in the head band gets a tone of its own, and title, artist and
  album are each held to a 4.5:1 contrast floor across all three themes and both
  appearances, including the extreme where the accent colour composites onto the
  sidebar behind them.

### Podcasts and online sources

- Online sources are asked about in one place now: the first-run wizard. A
  discovery banner used to ask the same question a second time on every fresh
  install, tucked behind the welcome dialog even though the wizard had just
  asked it; it also stayed on screen after being answered and drew as two
  overlapping surfaces with its `Not now` button clipped against the window
  edge. The banner is gone outright, so the wizard's answer is the only one
  asked, and Preferences / Plugins remains the one lasting place to change
  sources afterwards.

## [0.1.185] - 2026-09-11

### Playback and presentation

- Two soft clouds of light drift behind the cover. They replace the turning
  shimmer disc, and their colour comes from the blurred artwork itself rather
  than from an extracted palette, so a cover that is mostly one hue lights the
  panel in that hue and a grey cover stays quiet instead of guessing. Dark
  themes add the light to the band, light themes lay it over the band as a
  veil. Both layers share one clock half a period apart, a track change
  crossfades the old cover's light into the new one's, and GNOME's
  reduce-animation setting holds the clouds still without snapping them to a
  rest pose. The path they walk is wide enough to be seen: twice the travel of
  the first cut, a ten-degree turn, and stronger light in both layers.
- Lyrics that already sit on disk appear at once. Measured against a real
  library, a third of all tracks went to the network on every play, and more
  than half of those had a plain-text sidecar the app had already read — the
  spinner covered lyrics that were sitting right there. The upgrade to synced
  lyrics no longer wipes the plain text while it waits, a network round that
  timed out is remembered so the next play does not ask again, and the batch
  scan now upgrades the plain sidecars it used to skip. Lyrics that live only
  in a tag are left alone by the automatic upgrade, because a downloaded `.lrc`
  would shadow them for good.

### Appearance

- The Devices section rests on the sidebar floor. It floated about 95px above
  the bottom edge because a collapsed job card kept reserving its full height
  while it revealed nothing; the card's child now follows its revealed state,
  so an idle dock measures zero.

## [0.1.179] - 2026-09-10

### Playback and presentation

- Leaving the compact player no longer flashes the library at the wrong size.
  The switch back used to resize the one window from the mini card's dimensions
  to the library's, with the library content already mounted in it, and held
  that visibly wrong frame for about a sixth of a second. The compact player is
  now a window of its own: switching modes presents one window and hides the
  other, so no frame is ever shown at a size it should not have. The library
  window keeps its geometry across a round trip through compact mode and across
  quitting from it, the maximized state survives the switch, and the mini player
  keeps its window shortcuts.

### Appearance

- The filter row is as tall as the pills in it. `+ Add filter` and the search
  chip are both authored at 36px, but only the chip rendered there: Adwaita's
  own button padding stacked on top of the authored height, the `pill` class
  added ten more pixels per side, and because the bar stretches its children the
  38px chip was pulled up to the button's 58px. The two looked aligned while the
  row simply took its height from the button. Zeroing the vertical padding makes
  the authored height the one that renders, so **the filter row is 20px
  shorter**.
- The magnifier in the search chip sits on its centre line. At 13px it was
  scaled off the 16px grid symbolic icons are drawn on, and the image was
  centred inside the chip's full height — half a pixel high. It is 16px now, and
  the centring is left to the box, the way the `x` beside it already did it.

## [0.1.175] - 2026-09-10

### Library

- A cover that is only being throttled is no longer remembered as missing. The
  Cover Art Archive sits behind CDNs that answer a rate-limited request with
  `401` or `403`, and both were read as a definitive "this release has no art".
  One such answer was enough to note the album as coverless for a week, so the
  cover stayed away long after the throttling had passed. Those two answers are
  now retried like any other temporary failure, and nothing is written down.

## [0.1.174] - 2026-09-10

### Library

- The cover search finds what is there. An album whose tag carries a store
  decoration — `Evolve [Explicit]`, `Self Inflicted (Deluxe Edition)`,
  anything ending in `- Single` — returned no search results at all, because
  the decoration went into the query verbatim and MusicBrainz matched nothing.
  The search now retries once with the decoration stripped, and only when the
  strict attempt found nothing, so an album that already worked cannot change.
  A second defect sat behind it: only the first matching release was ever asked
  for art, and when that one had none the album was given up on even though a
  later release carried the cover. The search now walks the candidates.
- The background-activity panel in Preferences stays hidden while nothing is
  running. It used to show its heading on every page, leaving an empty band
  under it.
- The filter pill shows only the term. It used to spell out every field it
  searches inside the chip itself, which made it long and repeated the same
  list in every view; where the search looks is still explained by the scope
  caption.

### Appearance

- The mini player follows the appearance. Its card, edges and text had never
  been re-themed for light mode — the title and artist were near-black on a
  near-black tint, which is a contrast ratio of about 1.05 and effectively
  invisible. The play button's teal glow was hardcoded in the same way and read
  as glare once the card around it turned light.
- The turning disc becomes visible again. The blurred cover behind the album
  art in Now Playing had faded below the threshold of perception. Dark mode
  keeps exactly the feel it had; light mode turns slower and stays dimmer,
  because the same motion reads as distracting on the paler card.
- The folded panel lights its button. The sidebar and Now Playing toggles now
  both carry the accent while their panel is folded away and stay neutral while
  it is open. The Now Playing toggle used to be lit the wrong way round and the
  sidebar toggle was never lit at all.

### Podcasts

- Episodes show their runtime before the download. The length only appeared
  once the file had been fetched, which is the wrong way round for deciding
  whether to fetch it.

### Android

- The visualizer keeps every sample it is handed. A clamp meant for the desktop
  decoder also capped the phone's visualizer input and discarded fresh
  low-frequency audio whenever the UI thread stalled. The desktop visualizer was
  never affected by this.

### Upgrading

- The first launch after this update runs the album-cover pass over the whole
  library once. This is deliberate: the fixes above cannot reach albums that
  were already recorded as having no cover, so that record is cleared. How long
  it takes scales with the size of your library and with the pacing the
  MusicBrainz and Cover Art Archive requests are held to.
- This update raises the library database format. An older Reprise build will
  refuse to open the library afterwards, so going back to a previous version is
  not possible without restoring a copy of the database made beforehand.

## [0.1.162] - 2026-09-09

### Appearance

- The light appearance gets its own edges. Every hairline, tint and elevation
  rung was authored as a literal white: right on the dark palettes, invisible on
  the light ones, which is why light read flat and left the accent to carry all
  the structure. Surfaces are separated by visible edges again, the cover bloom
  is no longer washed out, a search match stays readable on its own tinted
  background, and the seek waveform keeps its unplayed bars legible. The dark
  appearance is unchanged.

### Playback

- The visualizer sees every beat. Its bars were driven at the decoder's block
  rate — under ten updates a second on a typical FLAC, a fraction of the rate
  the engine is tuned for, with most of the audio never reaching the analysis at
  all. It read as fluid but late, and soft where the beat should land. The audio
  is now split into buffers of the size the visualizer was designed for.

### Packaging

- Source and AUR installs need `gst-plugins-bad`. The visualizer's new audio
  splitting comes from that plugin set, which moves from an optional to a
  required dependency. The Flatpak already carries it, so nothing changes there.

### Android

- The Now Playing swipe animates again. Titles sat left of centre at rest with
  their first letters clipped off the screen, and the settle after a swipe never
  rendered. The card now follows the gesture and eases into place.
- A cover no longer falls back to the generated note. When the full-size read
  came back empty, the panel replaced the real artwork it was already showing
  with a generated one; it now keeps the cover it already has.

## [0.1.157] - 2026-09-08

### Library

- Every album gets its own cover again. One embedded picture reused across many
  album tags made a band's whole discography show the same artwork; Reprise now
  notices a picture already seen under a different album and lets the automatic
  download step in instead of trusting the wrong one.

### Browsing

- Releases, Radio, Podcasts and Concerts share one filter grammar. The same
  chips, the same "+ Add filter" popover and the same sorting behave alike in
  every source list, and "+ Add filter" turns insensitive once no facet is left
  to add.

### Editing and deleting

- The window stays responsive while you edit or delete. The tag editor opens
  before its cover art has arrived, a delete batch uses one trash session
  instead of one connection per file, and the deleted rows leave the list before
  the sidebar and browse bar catch up.
- A tag save only touches what it changed. A metadata-only save re-renders the
  edited cells instead of rebuilding the list, and a save that patches the sort
  field moves the edited block and keeps it selected in the viewport instead of
  reloading every row.
- Up Next pays for the change only. Deleting a track updates the rows around it
  rather than the whole queue projection.

### Discovery

- The Updates popover no longer stops updating for good. A failed artist fetch
  is due again at the next check instead of counting as fresh, a check in which
  at least one artist succeeded counts as completed, failures reach the log, and
  the footer keeps pulsing while a check runs.

### Android

- Library reads leave the main thread, and the results keep up with you. The
  newest search answer wins instead of being overwritten by a slower earlier
  one, an artist you abandoned can no longer reopen itself over what you are
  doing now, and an error stays with the surface that produced it.

## [0.1.139] - 2026-09-04

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
