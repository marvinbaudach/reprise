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
- [ ] **Manual Up Next ordering:** start track A in a longer Library, album, or
  playlist context, add two tracks X and Y through Add to Queue, and reorder them in
  Queue. Expected: Queue starts empty, shows only the two pending manual entries and
  its sidebar count is two; the accent insertion line marks the reorder target while
  Library and sorted/filtered playlist views show none. Next consumes the reordered
  X/Y entries with count two to one to zero, then resumes context B. Duplicate manual
  entries remain allowed; removing a Queue row changes neither files nor the hidden
  context. The mapped-X11/MPRIS harness proves this complete ordering and resume flow;
  native GNOME pointer/touch confirmation remains pending.
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

## Pending: compact layouts and preferences

### Preferences window

- [ ] Open Preferences, drag the window, and switch all five tabs. Expected: one
  independent movable window is reused, Playback/Appearance/Layout/Library/Plugins
  stay in the top header, no bottom switcher appears, and Layout does not duplicate
  Compact View. The isolated GTK regression proves the non-modal window, transient
  parent, top switcher and exact page order; native window-manager dragging remains.

### Header and navigation design

- [ ] At a wide native GNOME/Wayland size, open and close the Information panel.
  Expected: one flat header spans the sidebar, library, and Information panel; Music
  remains geometrically centered in the complete window rather than the track pane.
- [ ] Inspect Search plus the menu, Compact, Information, Import, and Scan actions in
  light/dark mode and at 100% plus an available HiDPI scale. Expected: every action is
  a compact symbolic icon with a correct tooltip, accessible name, and comfortable
  native target; no header item clips or forces the centered title aside.
- [ ] Inspect Music, Queue, manual playlists, the three built-in smart playlists,
  Import errors, Missing files, and New playlist. Expected: text remains present,
  each row has a stable aligned symbolic icon, counts align at the end, and long names
  ellipsize without moving the icon or count columns.
- [ ] Resize through wide, intermediate, and collapsed navigation widths with the
  Information panel open and closed. Expected: the sidebar stays approximately
  220–280 px while split, collapses through the existing native navigation path, and
  selecting or reselecting a source returns to content without losing state.
- [ ] Exercise sidebar selection, playlist creation/context menus, track-to-playlist
  drag and drop, Queue count/reorder, and Library/Compact round trips. Expected: only
  presentation changed; callbacks, selection, counters, DnD, and restored Library root
  remain intact. The isolated display tests and mapped-X11 pointer/screenshot harness
  pass these structures and flows; native visual, touch, icon-theme, and Wayland input
  judgment remains pending.

- [ ] Inspect the Library player bar at wide and narrow native GNOME widths, with
  the Information panel both open and closed and the saved position set to Top and
  Bottom. Expected: the bar spans the complete window below/above sidebar, library,
  and Information panel; Cover/Title/Artist remain left, Previous/Play/Next sit in a
  centered row above Time/Seek/Duration, and Shuffle/Repeat/Volume remain right.
  The measured native-widget regressions and mapped-X11 screenshots prove the full
  width and zone ancestry, both isolated position starts pass, and the real-input
  Compact round-trip preserves Playing state; native Wayland visual judgment and
  narrow-width target comfort remain pending.
- [ ] Open Compact View through the Library-header button, the main menu,
  Preferences, and Ctrl+M. Return through **Return to Library in the Compact context
  menu only**, then repeat the round trip with Ctrl+M. Expected: Bar, Cover, Pill, and
  Card have no visible restore or volume control; every route switches the same window
  immediately, retains the selected layout, and never creates a second playback state.
  The mapped real-input harness proves the menu-only return, layout persistence and
  Ctrl+M routes headlessly; native Wayland confirmation remains pending.
- [ ] Inspect Bar, Cover, Pill, and Card with long English and German metadata, plus
  missing album/year values. Expected: proportions feel intentional, title/artist
  ellipsize cleanly, optional rows collapse, icons remain legible, and controls meet
  native Adwaita target sizes. The four isolated display tests prove accessible
  controls and natural-size bounds, but visual judgment remains manual.
- [ ] With playback paused, scroll one wheel/touchpad step vertically on each
  layout's free cover or metadata surface. Expected: volume changes by exactly five
  percent per step and remains clamped from zero to 100 percent. Repeat over seek,
  transport, menu, and window controls: neither volume nor seek position changes.
  Compact exposes no visible volume slider; the Library bar, MPRIS, and media keys
  remain alternative volume routes. The mapped X11 run proves Bar metadata and seek
  separation against private-bus MPRIS values; native Wayland touchpad behavior and
  audible loudness remain pending.
- [ ] Drag Pill only from its free metadata region under Wayland and try dragging
  from its seek, transport, menu, and window controls. Expected: the metadata region
  moves the window, controls remain interactive, and integrated window actions work;
  no transparency, always-on-top, or dock behavior appears.
- [ ] Toggle Library → each compact layout → Library repeatedly, resize/maximize the
  Library, then close once in Library and once in Card. Expected: the last full
  Library size/maximized state survives and compact geometry never overwrites it;
  the compositor chooses placement. Isolated transition and two-start smokes pass,
  while native window-manager placement and resizing remain pending.
- [ ] Exercise all four layouts with pointer, keyboard, and touch at 100% and available
  HiDPI scales in light and dark appearance. Check real square and non-square covers.
  Expected: focus, tooltips, hit targets, cover cropping/placeholder behavior and
  active Shuffle/Repeat state remain clear without relying on color alone.
- [ ] On Appearance and Layout, change System/Light/Dark, player-bar top/bottom,
  sidebar/status visibility, Comfortable/Standard/Compact density, and column layout.
  Expected: each applies immediately, survives restart, and remains readable at narrow
  widths; disabling Sidebar removes the complete left column and enabling it restores
  that slot. The isolated smoke persisted and reread every non-column value; the mapped
  pointer harness additionally opens every page and proves Sidebar/Status writes, and
  the GTK regression proves complete sidebar-slot removal/restoration.
- [ ] On Appearance, confirm **Chromium (CSD)** is the default, then
  switch repeatedly to **System title bar** and back in Library plus Bar, Cover, Pill,
  and Card Compact View; restart once in each mode. Expected: Reprise's flat header and
  native window buttons appear only in client-side mode, the desktop supplies the title
  bar and borders in system mode when the compositor supports SSD, drag/resize/close and
  all header actions remain usable, no duplicate controls appear, and the saved choice
  returns on restart. The isolated mapped GDK regression proves both decoration requests,
  GTK's retained CSD resize frame, all four Compact control projections, and persisted
  startup; actual GNOME/Wayland compositor acceptance, frame appearance, HiDPI, touch,
  and live window-manager behavior remain native manual checks. A compositor that rejects
  SSD may fall back to CSD; this is expected desktop policy, not a Reprise failure.
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
- [ ] Play/Pause, previous/next, seek, volume, shuffle, and Repeat Off/All/One work
  through real speakers. Manual Up Next entries interrupt in their visible order,
  disappear when consumed, and resume the unchanged Library/playlist context.
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
- [ ] Artist & Album News is off by default and the Information panel explains that
  only selected artist names are sent to MusicBrainz. With disposable tagged tracks,
  confirm the 340px desktop proportions and narrow overlay, Upcoming/New copy, cached
  and offline copy, and that Open in MusicBrainz launches the system browser only after
  a click. The isolated mapped-window fixture passes opt-in persistence, Upcoming/New
  rendering, stale-selection rejection, close/reopen reuse, one shared request per
  second, request-field privacy, and zero calls before opt-in or after disable. Native
  GNOME proportions, browser launch and offline/cache wording remain pending.
- [ ] MPRIS bus loss/name collision and clean application shutdown do not crash or
  leave a ghost player in GNOME Shell.

## Pending: ListenBrainz account and scrobbling

Use a disposable ListenBrainz account and copied test tracks. Never paste its token
into a terminal, issue, screenshot, log capture, or repository file.

- [ ] ListenBrainz is off on a fresh profile and causes no request before opt-in.
- [ ] Connect from Plugins with the masked token field. Expected: the row becomes
  “Connected as …”, restart reconnects without asking again, and the token exists in
  the native Secret Service on the host and the encrypted Secret-Portal-backed store
  in Flatpak—not in `reprise.db`, settings, environment output, or logs.
- [ ] Start a short track and a track longer than eight minutes. Expected: playing-now
  appears after successful playback start; a permanent listen appears once at half of
  the short track and once at four minutes of the long track. Seeking backward after
  crossing the threshold must not duplicate either listen.
- [ ] Disconnect the network, complete a copied track, close Reprise, reopen it, and
  restore the network. Expected: the UI reports the pending count, playback/local play
  counts remain normal, and the listen is delivered once when connectivity returns.
- [ ] Disable the module while connected. Expected: no new playing-now or permanent
  listens are sent; pending metadata stays local. Re-enable and verify delivery resumes.
- [ ] Choose Disconnect. Expected: the keyring item and local ListenBrainz queue are
  removed, the plugin becomes disabled, and no music file or general library row changes.
- [ ] Enter an invalid token and simulate an unavailable/locked keyring. Expected: the
  UI shows rejected/error status and never falls back to plaintext storage.

## Pending: Last.fm account and scrobbling

Use a disposable Last.fm account, a disposable desktop API application, and copied
test tracks. Never put the API key, shared secret, session key, or real account
metadata in a terminal command, repository file, issue, screenshot, or log capture.

- [ ] Last.fm is off on a fresh profile and causes no request before opt-in.
- [ ] Open Plugins → Last.fm and enter the API key and shared secret in masked rows.
  Expected: the browser opens only after clicking Open Browser; approve the disposable
  app, return, and Continue. The displayed account survives restart. All credentials
  exist only in Secret Service (or the Flatpak Secret Portal store), never the database,
  settings, environment output, repository, or logs.
- [ ] Start copied short and long tracks. Expected: playing-now omits a timestamp and
  each completed listen is sent once at half the track or four minutes, with artist,
  title, optional release, duration, and start time. Seeking backward cannot duplicate it.
- [ ] Take the network offline, complete a copied track, restart, and restore the
  network. Expected: Last.fm's pending count survives and drains once without affecting
  local play counts or any ListenBrainz queue.
- [ ] Enable ListenBrainz and Last.fm together, then make one service unavailable.
  Expected: the other continues independently; both receive one completion when healthy.
- [ ] Disable Last.fm. Expected: no new Last.fm request begins and pending metadata stays
  local. Re-enable to resume. Disconnect must delete only the Last.fm keyring item and
  Last.fm queue, leaving ListenBrainz state and all music/library data untouched.
- [ ] Revoke the disposable app/session on Last.fm, use invalid credentials, and lock
  the keyring in separate runs. Expected: rejected/error status with no plaintext fallback.

## Pending: browse, columns, playlists, and Rhythmbox

- [ ] The unified filter bar works by mouse and keyboard at narrow and wide widths:
  chips wrap without covering the table, `Add filter` shows one value search, and
  Genre → Artist → Album changes or chip removal reset only dependent facets.
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
- [x] Use the reachable maintainer profile as the MusicBrainz `User-Agent` contact URL.
  A public project homepage remains a separate publication prerequisite below.
- [ ] Add the real homepage, capture authentic screenshots from this manual pass, and
  perform any forge/Flathub upload or submission explicitly.
