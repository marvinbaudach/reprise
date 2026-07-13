# Reprise manual GNOME QA ledger

This ledger separates checks already confirmed by the maintainer from checks that
still require a real GNOME desktop, pointer, keyboard, speakers, portal, file
manager, or Flatpak runtime. Automated agents must not mark a pending item complete
from a headless run. Use only copied/disposable music and an isolated XDG data
directory for destructive checks.

## Test environment record

Record these once when the final pass starts:

- [ ] Distribution and version:
- [ ] GNOME version:
- [ ] Architecture:
- [ ] Reprise commit/build:
- [ ] Host codec packages:
- [ ] Flatpak GNOME runtime/SDK branches:
- [ ] Light/dark style and display scale:

## Confirmed during the 2026-07-12 maintainer pass

- [x] A held seek gesture no longer jitters audio; one seek occurs on release.
- [x] Tracks added while Reprise is running appear without a manual rescan.
- [x] Tracks added while Reprise is closed appear after the next start.
- [x] The table selection follows the playing track on automatic next.
- [x] A stopped restored session selects its current track and Play can resume it
  without surprise autoplay.
- [x] Repeat modes behave as presented, including Repeat All versus Repeat One.
- [x] Genre/Artist/Album selection, album filtering, column sorting, browse-search
  filtering, rating resorting, and stable empty dropdown geometry work visually.
- [x] Ctrl and Shift multi-selection work in the track table.
- [x] Dragging tracks onto a playlist refreshes an already open playlist.
- [x] Removing tracks from a playlist keeps the library tracks and files.
- [x] Reordering inside a playlist persists.
- [x] Creating an empty playlist from the sidebar keeps the Music view open.
- [x] M3U import matches the expected tracks and order; unmatched entries are omitted.
- [x] M3U export followed by re-import preserves the expected titles and order.
- [x] Imported playlist rows display existing library ratings, as intended.
- [x] Batch tag changes become visible immediately in the library.

## Recent regression confirmations

Run these first after restarting the current `target/release/reprise`.

- [x] **Playlist duplicate prevention:** drag a track onto a playlist that already
  contains it. Expected: the count and row set do not change. Existing historical
  duplicates are not silently removed.
- [x] **Full-cell drag surface:** start a playlist reorder from blank space inside a
  Title/Artist/Album cell, not directly on glyphs. Expected: the drag starts across
  the cell allocation, including the cover cell.
- [x] **Playlist reorder insertion feedback:** hover a single-row drag over another
  playlist row. Expected: an accent insertion line shows the insertion target.
- [ ] **Queue reorder insertion feedback:** hover a single-row drag over another Queue
  row. Expected: an accent insertion line appears; Library and sorted/filtered
  playlist views show no false reorder target. The mapped-X11 release harness
  captures the accent target and completes the Queue reorder; native GNOME
  confirmation remains pending.
- [x] **Imported playlist selection:** import a populated M3U. Expected: the new
  playlist is visibly selected in the sidebar and agrees with the title and table.
- [x] **Tag editor Enter key:** edit any valid field and press Enter. Expected: Apply
  runs, saves the change, and closes the dialog.
- [ ] **Tag editor invalid number:** enter an invalid Year or Track value and press
  Enter. Expected: the dialog stays open and shows the validation error. The
  mapped-X11 release harness passes this exact flow; native GNOME confirmation
  remains pending.

## Pending: first run, layout, and language

Use a fresh disposable `XDG_DATA_HOME` for each onboarding variant.

- [ ] Fresh start shows the setup dialog once, with clear local-library/privacy copy.
- [ ] Cover download is off by default. When Rhythmbox is detected, a clear
  `Rhythmbox found` import offer appears and remains off by default; without the
  schema/key, no false Rhythmbox offer appears. Both decision paths and explicit
  fixture import pass in the isolated smoke; native copy/layout remains pending.
- [ ] Skip completes onboarding without opening a picker or scanning music.
- [ ] Set Up Library opens the portal folder chooser; cancel is harmless; choosing a
  disposable folder scans it and does not expose broader filesystem access.
- [ ] During Set Up Library, Scan folder, and Rescan library, a narrow row directly
  below the header appears immediately, pulses while files are discovered, then shows
  a monotone processed/total count and the current filename before disappearing on
  completion. The isolated two-file app smoke proves Discovering → Scanning → Complete,
  and the native GTK widget regression proves reveal/fraction/hide; native GNOME visual
  layout with a deliberately slow disposable tree remains pending.
- [ ] Existing libraries upgrade silently without showing first-run setup.
- [ ] English and German have no clipping, untranslated visible strings, broken
  plurals, or unnatural button/menu labels.
- [ ] Narrow width switches navigation cleanly; wide width restores the split view.
- [ ] Light and dark appearance, 100% and available HiDPI scale, keyboard navigation,
  pointer, and touch targets remain readable and usable.

## Pending: minimal view and preferences

- [ ] Open Minimal View from the menu and with Ctrl+M. Expected: the existing player
  bar becomes the compact window content, cover/title/artist and every transport,
  seek and volume control remain usable, and no second playback state appears. The
  mapped real-input harness proves Ctrl+M both ways and captures a populated,
  nonblank compact player; native Wayland/window-manager confirmation remains.
- [ ] Toggle Full → Minimal → Full repeatedly, then close once from each mode.
  Expected: the last full window size/maximized state is restored and is never
  overwritten by compact geometry. The isolated state and two-transition smokes pass;
  native window-manager geometry remains pending.
- [ ] On Appearance and Layout, change System/Light/Dark, player-bar top/bottom,
  sidebar/status visibility, Comfortable/Standard/Compact density, and column layout.
  Expected: each applies immediately, survives restart, and remains readable at narrow
  widths. The isolated smoke persisted and reread every non-column value; the mapped
  pointer harness additionally opens every page and proves Sidebar/Status writes.
- [ ] On Library, choose/cancel a disposable folder, rescan it, and invoke Rhythmbox
  column import. Expected: cancel is harmless and actions use the established safe
  picker/import paths.
- [ ] On Plugins, toggle Cover download and MPRIS. Expected: enabling Cover download
  immediately reveals the same checked/total, downloaded and unavailable progress in
  the main window and on the Plugins page. The main terminal result remains visible
  briefly and hides; Preferences retain it while open. Disabling a running pass reports
  that it stopped, and re-enabling starts a fresh pass. MPRIS clearly says restart
  required and changes only after restart. Equalizer and ReplayGain must not appear here
  because they are core Playback features. The isolated local-sidecar app smoke proves
  main-window Running → Complete without network; both native GTK progress-widget tests
  pass. Real-network results, native visual confirmation and MPRIS restart remain pending.
- [ ] While disposable music plays through real speakers, enable Equalizer, select
  Flat/Rock/Pop/Bass Boost, and move all ten sliders. Expected: audible changes are
  immediate, bounded to ±12 dB, persist after restart, and do not interrupt or move
  playback unexpectedly. The real GStreamer fakesink pipeline and live state/position
  preservation tests pass; a real pointer scale change persists without criticals or
  filter replacement; audible confirmation remains pending.
- [ ] With files containing valid ReplayGain tags, compare Off, Per Track, and Per
  Album. Expected: normalization mode changes live, album mode is consistent within an
  album, and untagged files remain playable.

## Pending: real audio and transport

Use representative available files for FLAC, MP3, Ogg Vorbis, Opus, WAV, and
M4A/AAC. Missing host codecs should produce an actionable error, not a crash.

- [ ] Each advertised extension scans and a representative supported file plays.
- [ ] Play/Pause, previous/next, seek, volume, shuffle, Repeat Off/All/One, Queue
  append, Queue reorder, and end-of-queue behavior work through real speakers.
- [ ] A corrupt/unplayable file is skipped or reported without wedging later playback.
- [ ] Starting, restoring, filtering, importing, and opening views never cause
  unexpected autoplay.

## Pending: MPRIS, notifications, and covers

- [ ] GNOME Quick Settings and media keys show the player and control playback.
- [ ] Lock-screen controls, metadata, duration, seek position, shuffle, and repeat stay
  synchronized in both directions.
- [ ] Track-change notifications show correct title/artist and available cover art.
- [ ] Embedded covers, folder covers, and placeholders render correctly in list, bar,
  and Now Playing without stale covers after rapid scrolling or track changes.
- [ ] Opt-in cover download is off by default. When enabled on a disposable library
  with network access, the visible progress reaches the library total, a strong
  MusicBrainz match increments the downloaded count and renders, existing local covers
  are not downloaded again, and ambiguous or wrong albums increment unavailable
  without acquiring an unrelated cover. Disable during a longer run and confirm that
  the status becomes stopped and no new requests begin; cached covers must remain.
- [ ] MPRIS bus loss/name collision and clean application shutdown do not crash or
  leave a ghost player in GNOME Shell.

## Pending: browse, columns, playlists, and Rhythmbox

- [ ] Browse dropdowns work by mouse and keyboard at narrow and wide widths; each
  Genre → Artist → Album change resets only the dependent facets.
- [ ] Search and browse facets combine correctly, including zero-result recovery.
- [ ] Open `Edit column layout…`. Switch optional columns, reorder through both
  Up/Down buttons and whole-row dragging, inspect the before/after accent line,
  reset, and restart. Expected: changes apply immediately and persist; Cover and
  Title stay fixed/visible; sorting never targets a hidden or invalid column. The
  isolated editor smoke passes toggle/reorder/persist and the display regression
  proves each movable row owns both controllers; native pointer visuals remain pending.
- [ ] A real Rhythmbox visible-columns import is read-only, maps supported columns in
  order, ignores unknown tokens, and leaves Rhythmbox settings unchanged.
- [ ] Playlist context-menu add/new/remove, sidebar drag add, multi-select, and M3U8
  import work with Unicode names and paths containing spaces.
- [ ] Exported M3U opens in another compatible player when one is available.

## Pending: tag editing and safe removal

Work only on disposable copies. Before each destructive check, verify the selected
paths are under the disposable QA directory.

- [ ] Multi-select tracks with different Title/Album/Year values. Expected: mixed
  fields say “multiple values”; changing only Genre preserves every untouched value.
- [ ] Clearing a dirty text or numeric field intentionally clears it; leaving a field
  untouched never clears it. Embedded pictures and custom tag items remain intact.
- [ ] A partial batch failure reports exact success/failure counts and successful rows
  refresh without corrupting failed files.
- [ ] Remove from Library removes database rows and playlist references but leaves the
  copied audio files byte-for-byte present.
- [ ] Move to Trash requires confirmation, removes only successful rows, and places the
  copied files in the host file manager Trash. Cancel changes nothing.
- [ ] Repeat the Trash check in Flatpak: the portal is used, visibility is correct in
  the file manager, and denial/failure has no permanent-delete fallback.

## Pending: session and installed application

- [ ] Configure geometry/maximize, source, search, browse facets, sort, queue order,
  current track, shuffle, and repeat; close and reopen twice. Expected: all restore,
  playback remains stopped, the sidebar highlight matches the table, and Play resumes
  the restored current item.
- [ ] Delete a queued/restored track between starts. Expected: the missing id is safely
  dropped and the remaining queue/session is valid.
- [ ] Launch the installed app from GNOME, not the build tree. Verify launcher name,
  comment, keywords, full-color and symbolic icons, AppStream data, application ID,
  German catalog, and clean uninstall paths.
- [ ] On a machine with GNOME Platform/SDK 50 and `flatpak-builder`, build from the
  pinned manifest, run inside the sandbox, repeat portal picker/audio/MPRIS/Trash
  checks, and run `flatpak-builder-lint`.

## External publication blockers

These are maintainer actions, not QA failures:

- [ ] Provide a maintainer-controlled public immutable source archive/tag and SHA-256.
- [ ] Establish verifiable ownership/identity appropriate for `org.reprise.Reprise`.
- [ ] Publish a real maintainer-controlled contact/project URL and replace or verify
  the MusicBrainz `User-Agent` URL before distributing online cover downloads.
- [ ] Add the real homepage, capture authentic screenshots from this manual pass, and
  perform any forge/Flathub upload or submission explicitly.
