# Changelog

Reprise release notes are curated from the changes that reached the stable
branch. They describe user-visible changes rather than reproducing commit
messages.

## [0.1.42] - 2026-08-21

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
