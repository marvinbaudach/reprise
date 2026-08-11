# GNOME Conformance Findings

Collected on 2026-08-11 against section AI of `docs/ux-rules.md`. This document
changes no code. It is the input for Wave 2.

The Strand A gate scripts and GP-1 through GP-20 rule text were not present at
the audited base (`4f6dfc7cb2`), so the baseline uses the direct `ripgrep` and
`find` fallback prescribed by the audit plan. Counts include test modules under
`crates/reprise-gnome/src` because the prescribed scope does not exclude them.

## Baseline

| Rule | Measurement | Value |
|---|---|---|
| GP-2 | blocking-call pattern matches in `reprise-gnome/src` | 33 |
| GP-3 | `clone!` blocks with `#[strong]` | 0 |
| GP-4 | `unwrap()` calls in `reprise-gnome/src` | 2,116 |
| GP-5 | files with `ObjectSubclass` | 11 of 706 Rust files |
| GP-6 | GSettings schemas in the tree | 0 |
| GP-11 | custom-CSS pattern matches | 706 |

The raw command outputs contained 33 lines in `blocking.txt`, 0 in
`strong.txt`, 255 files in `unwrap.txt` totalling 2,116 matches, 11 files in
`subclassed.txt`, 0 in `gsettings.txt`, and 148 files in `css.txt` totalling
706 matches.

## Findings

### GP-1 — Icon-only buttons have accessible names

The prescribed search returned 73 construction sites and 706 context lines.
Manual review found zero product violations: every icon-only product button
has a tooltip, an explicit accessible label, or a visible `Label` child. The
three unnamed icon-button constructions are display-test fixtures at
`crates/reprise-gnome/src/ui/window/window_navigation.rs:454`,
`crates/reprise-gnome/src/ui/window/window_navigation.rs:518`, and
`crates/reprise-gnome/src/ui/primary_menu.rs:469`; they are not shipped widget
construction. GP-1 therefore has a measured product violation count of zero.

### GP-5 — Stateful compound widgets bypass `ObjectSubclass`

- **Severity:** major
- **Locations:**
  - `crates/reprise-gnome/src/ui/browse/browse_bar.rs:47`
  - `crates/reprise-gnome/src/ui/compact/compact_player.rs:71`
  - `crates/reprise-gnome/src/ui/concerts/concerts_view.rs:77`
  - `crates/reprise-gnome/src/ui/issues/missing_progress.rs:72`
  - `crates/reprise-gnome/src/ui/library_doctor/progress_card.rs:62`
  - `crates/reprise-gnome/src/ui/library_doctor/result_pages.rs:41`
  - `crates/reprise-gnome/src/ui/library_doctor/review_page.rs:367`
  - `crates/reprise-gnome/src/ui/library_doctor/running_page.rs:23`
  - `crates/reprise-gnome/src/ui/lyrics/lyrics_view.rs:43`
  - `crates/reprise-gnome/src/ui/now_playing/now_playing.rs:300`
  - `crates/reprise-gnome/src/ui/now_playing/song_visualizer.rs:38`
  - `crates/reprise-gnome/src/ui/now_playing/up_next_panel.rs:110`
  - `crates/reprise-gnome/src/ui/player_bar/player_bar.rs:71`
  - `crates/reprise-gnome/src/ui/player_bar/seek_legend.rs:41`
  - `crates/reprise-gnome/src/ui/player_bar/waveform_seek.rs:130`
  - `crates/reprise-gnome/src/ui/podcasts/podcasts_view.rs:80`
  - `crates/reprise-gnome/src/ui/podcasts/youtube_channel_detail.rs:175`
  - `crates/reprise-gnome/src/ui/radio/radio_view.rs:97`
  - `crates/reprise-gnome/src/ui/releases/releases_view.rs:84`
  - `crates/reprise-gnome/src/ui/scan/scan_chip.rs:67`
  - `crates/reprise-gnome/src/ui/scan/scan_progress.rs:142`
  - `crates/reprise-gnome/src/ui/sidebar/sidebar_device_card.rs:27`
  - `crates/reprise-gnome/src/ui/source_error_banner.rs:73`
  - `crates/reprise-gnome/src/ui/stats/stats_band_card.rs:19`
  - `crates/reprise-gnome/src/ui/stats/stats_band_tile.rs:26`
  - `crates/reprise-gnome/src/ui/stats/stats_genre_card.rs:22`
  - `crates/reprise-gnome/src/ui/stats/stats_songs_card.rs:79`
  - `crates/reprise-gnome/src/ui/stats/stats_view.rs:62`
  - `crates/reprise-gnome/src/ui/tag_edit/autocomplete_entry.rs:138`
  - `crates/reprise-gnome/src/ui/window/search_popover.rs:10`
- **Observation:** 30 manually confirmed compound-widget owners expose a GTK
  root while keeping mutable widget state in `Rc<RefCell<...>>`; none of their
  files appears in the measured 11-file `ObjectSubclass` set. Pure factories,
  controller-only modules, and test fixtures were excluded from this count.
- **Why it violates the rule:** GP-5 requires mutable state belonging to a GTK
  widget to live in an `ObjectSubclass` implementation rather than beside a
  hand-built widget graph.
- **Effort estimate:** large (> 4 h)

### GP-6 — Application settings are not backed by GSettings

- **Severity:** major
- **Locations:**
  - `crates/reprise-core/src/db.rs:196`
  - `crates/reprise-core/src/library/settings.rs:37`
  - `crates/reprise-core/src/modules.rs:186`
  - `crates/reprise-gnome/src/ui/scrobbling/listenbrainz_secret.rs:18`
  - `crates/reprise-gnome/src/ui/scrobbling/lastfm_secret.rs:42`
- **Observation:** ordinary preferences and UI state are stored as text pairs
  in the shared SQLite `settings` table inside the XDG data database
  (`reprise/reprise.db`). Static measurement found 91 literal setting keys;
  one (`playback.equalizer_bands`) is migration-only and deleted by schema v53.
  The remaining 90 live literal keys plus 13 generated
  `module.<id>.enabled` keys give **103 live SQLite setting keys**. Per-device
  synchronization preferences additionally use the normalized
  `device_settings` table at `crates/reprise-core/src/db.rs:258` and are not
  included in 103. ListenBrainz and Last.fm use two system-keyring records;
  there is no separate application configuration file and there are zero
  GSettings schemas.
- **Why it violates the rule:** GP-6 requires ordinary desktop preferences to
  use a declared GSettings schema; SQLite is currently the sole general
  settings backend, while secrets correctly need to remain in the keyring.
- **Effort estimate:** large (> 4 h)
