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

## How GP-7 to GP-11 were measured

Task 3 needs the running application, which the Codex sandbox cannot provide.
The runs below were made outside the sandbox, on 2026-08-12, against the
**release** build of `8d062859de` (no `test-fixtures` feature), each on its own
Xvfb screen with its own D-Bus session, its own XDG profile and
`REPRISE_AUDIO_SINK=fakesink` — never on the user's desktop and never against
the user's database. The library shown is a `sqlite3 .backup` copy of the real
one (2,166 tracks, real cover art), so the measurements are made on a realistic
library rather than on fixtures.

Frames were taken with `import -window root` on a screen sized exactly to the
window under test, and `magick identify -format '%[mean]'` (0 – 65,535) is used
below as the light/dark discriminator.

### GP-7 — The window is usable at 1024x600

**No violation.** At 1024x600 the header bar, the filter bar, all five folded
table columns (Title, Artist, Album, Year, Length) and the entire player bar
are present and legible. Nothing is clipped, no control disappears without a
replacement, and no widget forces a width larger than the window.

The floor GTK enforces is **600x400**. Sizes were requested with
`xdotool windowsize` and read back with `xdotool getwindowgeometry`:

| requested | resulting geometry |
|---|---|
| 1024x600 | 1024x600 |
| 900x600 | 900x600 |
| 800x600 | 800x600 |
| 700x500 | 700x500 |
| 600x400 | 600x400 |
| 400x300 | **600x400** |
| 320x240 | **600x400** |
| 200x200 | **600x400** |

At 600x400 the table folds to Title and Artist, three rows stay visible and the
window remains operable.

### GP-7 — The declared `display_length` is not the measured floor

- **Severity:** minor
- **Location:** `data/io.github.marvinbaudach.Reprise.metainfo.xml:88`
- **Observation:** `<requires><display_length compare="ge">768</display_length></requires>`,
  while the measured minimum longest side is 600.
- **Why it violates the rule:** the requirement is stricter than the app
  actually is, so AppStream clients hide Reprise from displays between 600 and
  767 logical pixels on which it demonstrably runs. Either lower the value to
  the measured 600 or record why 768 is a deliberate floor rather than a
  measured one.
- **Effort estimate:** klein (< 1 h)

### GP-7 — The fold toast covers the transport at the smallest size

- **Severity:** minor
- **Location:** `crates/reprise-gnome/src/ui/track_list/responsive_columns.rs:14`
  (`FOLD_BREAKPOINT_WIDTH = 760`), toast raised on crossing it
- **Observation:** at the 600x400 floor the toast *Some columns were folded to
  fit the window / Show columns* occupies the bottom band of the window and
  covers the play, previous, next and shuffle buttons for as long as it is up.
- **Why it violates the rule:** GP-7 asks that the window stay usable at small
  sizes; at the smallest size the app itself hides its primary controls behind
  a notification.
- **Effort estimate:** klein (< 1 h)

### GP-8 — Light, dark and follow-system are all supported

**No violation for the three modes themselves.** Same view, same content, one
run per setting:

| `ui.color_scheme` | resulting style | mean |
|---|---|---|
| `light` | light | 61,501 |
| `dark` | dark | 10,024 |
| `system` (the shipped default) | dark | 9,966 |

`system` maps to `adw::ColorScheme::Default`
(`crates/reprise-gnome/src/ui/style/mod.rs:277`), and on the measuring host the
system preference **is** dark, so following it is the correct outcome, not a
defect.

**Not measurable on this host: whether light is the default.** GP-8 requires
light to be the default when the system expresses no preference, and this
machine has no such neutral state:
`/usr/share/glib-2.0/schemas/99_manjaro-settings.gschema.override` compiles
`color-scheme='prefer-dark'` into the *schema default* of
`org.gnome.desktop.interface`. That default survived a private session bus, a
redirected `XDG_CONFIG_HOME` and `GSETTINGS_BACKEND=memory` — all three still
report `'prefer-dark'`. Forcing the light path with
`ADW_DEBUG_COLOR_SCHEME=prefer-light` does produce a light window (mean
61,501), so the light path is intact; only the *default* is unobservable here.
By construction it holds: libadwaita documents
`ADW_COLOR_SCHEME_DEFAULT` on the default style manager as prefer-light. A
runtime proof needs a host without that override.

### GP-8 — The Now Playing panel is unreadable in the light style

- **Severity:** blocker
- **Locations:**
  - `crates/reprise-gnome/src/ui/now_playing/surface_css.rs:12` — the root cause
  - `crates/reprise-gnome/src/ui/now_playing/now_playing.rs:261` (class attached)
  - `crates/reprise-gnome/src/ui/now_playing/cover_bloom.rs:31` (`BLOOM_HEIGHT = 330.0`)
  - `crates/reprise-gnome/src/ui/now_playing/up_next_panel.rs:703, 704, 707, 709, 710`
  - `crates/reprise-gnome/src/ui/lyrics/lyrics_view.rs:709, 717, 724`
- **Observation:** `.reprise-now-playing-stage` is declared as
  `background-color: @sidebar_bg_color; color: #ffffff`. The background follows
  the style, the foreground does not. The dark backdrop the white text was
  designed for is the cover bloom, and that bloom is clipped to the top 330 px
  — deliberately, "stopping short of the tabs". Everything below it (the Up
  Next list and the lyrics view) therefore sits directly on
  `@sidebar_bg_color`, which is light in the light style. Captured at 1000x700
  in the light style: Up Next titles render white on near-white, the artist
  sub-lines (`alpha(#ffffff, MUTED_TEXT_ALPHA)`) are invisible, and the lyrics
  lines are equally unreadable. The identical widgets in the dark style are
  perfectly legible, which isolates the cause to the missing light variant.
- **Why it violates the rule:** GP-8 requires text to stay readable in both
  styles. A whole panel of the application is illegible in one of the two
  supported styles; the effective contrast ratio is close to 1:1.
- **Effort estimate:** mittel (1–4 h) — the eight declarations need
  libadwaita colours (or a light variant), and the bloom band's relationship to
  the content below it has to be decided rather than patched per rule.

### GP-11 — Bespoke CSS is the exception, with one violating cluster

**Mostly satisfied, measured.** Across `crates/reprise-gnome/src` there are
**335** references to libadwaita named colours (`@…_color`) against a handful
of hard-coded colour literals, and only 6 files define CSS at all. The
hard-coded declarations concentrate in five files:

| file | hard-coded white declarations |
|---|---|
| `crates/reprise-gnome/src/ui/now_playing/surface_css.rs` | 9 |
| `crates/reprise-gnome/src/ui/now_playing/up_next_panel.rs` | 6 |
| `crates/reprise-gnome/src/ui/player_bar/player_bar_layout.rs` | 4 |
| `crates/reprise-gnome/src/ui/lyrics/lyrics_view.rs` | 3 |
| `crates/reprise-gnome/src/ui/compact/compact_player_layouts.rs` | 1 |

- **Severity:** major (as a rule violation; the readability consequence is
  filed as the blocker above, not counted twice)
- **Observation:** these declarations are not "bespoke CSS with a stated
  reason" — they are colour values that libadwaita already provides as
  variables, written as literals with no light/dark variant. No `@media`,
  `.dark` or light-style alternative exists in either file.
- **Why it violates the rule:** GP-11 allows bespoke CSS as an exception with a
  reason. Brand colours from `palette.toml`, the animations and the glow
  gradients qualify and are **not** findings; a plain `color: #ffffff` on body
  text does not.
- **Effort estimate:** medium (1–4 h), shared with the GP-8 blocker above

### GP-9 — The rule has no working evidence, and the suite says otherwise

The repository already carries a keyboard-only surface inventory
(`scripts/cua-e2e/keyboard.sh` with `scripts/cua-e2e/keyboard-surfaces.tsv`,
**10** surfaces). It was run against the release build:
`CUA_E2E_PROFILE=release CUA_E2E_ONLY=populated-library scripts/cua-e2e/run.sh`.
Evidence: `/tmp/reprise-cua-e2e/run-20260812T050653Z-1266041` (413 files).

Result line: `surfaces passed: 1, failed: 4`. Of ten surfaces, **one** actually
executed its GP-9 assertions.

| Surface | What happened |
|---|---|
| `app-shell` | **passed** — search open, type, `No results`, Escape, focus not left on the `GtkText` |
| `sidebar`, `tracks-playlist-queue`, `issues-import`, `player-now-playing` | counted as failures, but the scenario function never ran |
| `device-sync`, `preferences`, `modals`, `stats`, `compact-minimal` | never started |

- **Severity:** major
- **Locations:**
  - `scripts/cua-e2e/keyboard.sh:29` (`reset_surface_baseline`)
  - `scripts/cua-e2e/keyboard.sh:328` (the `continue` that records `"<surface> (reset)"` as a failure)
  - `scripts/cua-e2e/run.sh` (the scenario-group loop, under `set -euo pipefail`)
- **Observation:** the four "failures" are not app defects. For each of them the
  evidence directory contains **only** the `*-reset-*` files and no
  `acc-<surface>-*` files at all, so `reset_surface_baseline` returned non-zero
  before the surface was ever exercised. The cause is documented in the script
  itself: since SEARCH-4a, Escape closes the search popover but *keeps* its
  query (`keyboard.sh:32-34`), and `keyboard_app_shell` leaves the view
  filtered to `"nomatch"`. The reset then tries Escape twice and Alt+Left,
  none of which clears a filter — the visible, accessible-named `Clear all ×`
  control is never used. Each of the four `*-reset-state.json` files still
  carries the chip `⌕ "nomatch" in track, artist and album ×`.
  Separately, because the scenario-group loop in `run.sh` is not guarded, the
  non-zero return of `populated-library` aborted the script under `set -e`
  before `populated-library-secondary` ran, silently dropping the remaining
  five surfaces from the run.
- **Why it violates the rule:** GP-9 cannot be evidenced today. Worse than a
  gap, the suite reports "failed" for surfaces it never tested, so the number
  reads as a verdict about the app when it is a verdict about the harness. No
  focus loss, no focus trap and no keyboard-unreachable element was observed —
  because for nine of ten surfaces nothing was observed at all.
- **Effort estimate:** small (< 1 h) for the reset (use the `Clear all` action
  instead of Escape + Alt+Left, and fail loudly rather than counting an
  un-run surface as a failed one); small for guarding the group loop so one
  failing group no longer drops the rest.

**Not measured, therefore not claimed:** GP-9 for `sidebar`,
`tracks-playlist-queue`, `issues-import`, `player-now-playing`, `device-sync`,
`preferences`, `modals`, `stats` and `compact-minimal` — zero assertions ran.
The known focus-trap suspicion about the sidebar rows is **still open**; this
run did not touch it.

#### Measured after the harness repair, 2026-08-12

`docs/plans/keyboard-sweep-reset-and-reporting.md` repaired the reset and the
group loop. Re-run against the release build, twice, same result:

```
[cua-keyboard] surfaces passed: 5, failed: 0, not exercised: 0
```

Evidence: `/tmp/reprise-cua-e2e/run-20260812T074043Z-2749161`. The count is not
the claim — the evidence files are. `sidebar`, `tracks-playlist-queue`,
`issues-import` and `player-now-playing` now carry 94, 80, 202 and 185
`acc-<surface>-*` files respectively, where the 2026-08-12 05:06 run had **zero**
for all four. **GP-9 holds for the five `primary` surfaces:** no focus loss, no
focus trap, no keyboard-unreachable element, and Space keeps routing to the
global control (`acc-4a`, six alternations).

The sidebar focus-trap suspicion is thereby **closed for the tab order**:
`keyboard_sidebar` reached `Music` by Tab and moved to `Queue` by Down, with
focus evidence for both.

**Still not claimed:** the five `secondary` surfaces — `device-sync`,
`preferences`, `modals`, `stats`, `compact-minimal`. The group now starts (that
was the loop defect) but dies on its first snapshot with
`the private AT-SPI bridge is unavailable`, preceded by `A connection to the bus
can't be made`. Run in isolation with `CUA_E2E_ONLY=populated-library-secondary`
it fails identically, so this is a standalone defect of the private session
lifecycle, not cross-group interference and not a product finding. It was
invisible until the group loop was guarded.

**One more thing became visible:** `play-2-doubleclick-row`, the step after the
sweep in `populated-library`, fails with `CUA action did not land cleanly`. It
had never been reached before, because the sweep aborted the scenario at its
second surface. It is a pointer action immediately following ~570 driver calls,
which matches the documented cua-driver 0.8 listener loss that the secondary
group exists to work around.

Both are harness defects and both need their own strand. Neither is evidence
about the application.

### GP-10 — The main track list has no accessible name

Measured from the richest window-state snapshot the run captured
(`populated-missing-4.json`: main track list, sidebar, player bar and the
Missing-files banner): **47 AT-SPI nodes, 9 with an empty name, 0 reporting
0x0 extents while visible.**

Six of the nine empty names are `grid cell` nodes in the Album column for
fixture tracks that carry no album tag — the cell text matches the absent data,
which is not a markup defect and is not counted. One is the `ListView` GTK
builds inside `GtkColumnView`, which Reprise does not construct.

- **Severity:** major
- **Locations:**
  - `crates/reprise-gnome/src/ui/track_list/track_list_builder.rs:44` (the `ColumnView::builder()` call)
  - `crates/reprise-gnome/src/ui/track_list/track_list_keyboard_reorder.rs:167` (the only `update_property` on that widget — `KeyShortcuts`, never `Label`)
- **Observation:** the application's primary content widget, a
  `gtk4::ColumnView` with AT-SPI role `tree grid`, exposes an empty accessible
  name. A screen-reader user who lands on it hears only its role.
- **Why it violates the rule:** GP-10 requires every interface element to
  expose a descriptive accessible name, and this is the element the app is
  mostly *about*.
- **Effort estimate:** small (< 1 h) — a single
  `update_property(&[Property::Label(…)])`, the pattern already used at
  `crates/reprise-gnome/src/ui/stats/stats_songs_card.rs:110`.

**Not measured, therefore not claimed:** the accessible tree of every dialog
and transient — Preferences, Help, Library Doctor, the tag editor and the
device-sync page were never opened in this run. The 0x0-extent check rests on
one snapshot and is not exhaustive. One further unnamed node (role `group`, the
content-area container) could not be attributed to a construction site,
because every node in this run reported the same `x=80,y=0` window origin — a
`cua-driver` artefact, not an app property, so no line is claimed for it.

## Summary

| Severity | Count | Estimated total effort |
|---|---|---|
| blocker | 1 | 1–4 h |
| major | 5 | > 9 h, including 1–4 h shared with the blocker |
| minor | 2 | < 2 h |

**Order for Wave 2:** GP-8 comes first because the light-style Now Playing
panel is the only blocker and leaves core content unreadable. GP-11 follows
because its violating CSS cluster overlaps the GP-8 repair and should be fixed
without duplicating work. GP-9 comes next because repairing the keyboard
harness restores trustworthy coverage for the nine surfaces that were not
measured. GP-10 then closes a small, concrete accessibility gap in the primary
content widget. GP-5 follows as the first large architectural correction,
moving stateful compound widgets onto the GNOME object model. GP-6 comes after
that as the other large migration, replacing the application's general SQLite
settings backend with GSettings while preserving the separate secret store.
GP-7 is last because both remaining findings are minor, each estimated below
one hour, and the window itself was measured as usable at and below the rule's
required size.

**Rules that can be made `[active]` immediately,** because the documented
measurements show no violation: GP-1 — its manual construction-site review measured zero
product violations; GP-3 — the Baseline gate measurement recorded zero
`#[strong]` blocks, with zero lines in `strong.txt`.
