# STATUS — shared coordination board (Claude ⇄ Codex)

**Both agents read this file FIRST and keep it current.** It is the single, git-tracked source of
truth for "what's done / who's working / what's next", so two agents never collide on `main`.
(The full task history with commit hashes is in `.superpowers/sdd/progress.md` — that ledger is
local/gitignored; THIS file is the shared, versioned summary.)

## Protocol (read before touching anything)

1. **Read this file + `git log --oneline -10`.** Trust them over memory.
2. **Only ONE agent works `main` at a time.** Before you start coding, set the **Lock** below to
   yourself with a timestamp + the task you're taking, and commit *only this file* first
   (`docs: claim work lock`). If the Lock is already held by the other agent and the timestamp is
   recent, do NOT start — coordinate via the user or wait. (True parallel work requires a separate
   branch/worktree — ask the user to set that up.)
3. **Do the task** per `development-method.md` (test-first → gates → commit → self-review → ledger line).
4. **When done**, update "Current position" + "Done so far" here, set the Lock back to
   `FREE`, and commit this file (`docs: release work lock; <what you finished>`).
5. **Never push.** Never touch the user's real DB/music (see `AGENTS.md`).

## 🔒 Lock

```
OWNER:    FREE            # FREE | claude | codex
TASK:     —
SINCE:    —
```

_As of 2026-07-14: the installed app advertises and handles local audio and M3U files._

## Parallel feature work

- None.

## Current position

- **Completed plan:** `docs/superpowers/plans/2026-07-14-file-associations.md` (1 task).
- **Last completed:** **Desktop file associations** (`ffa049c`) — GNOME can forward supported
  local audio and M3U/M3U8 files into the one existing Reprise window.
- **Current plan:** none.
- **➡️ NEXT:** joint stage review; do not start another roadmap stage without explicit user direction.
- **Main implementation:** `ffa049c` on `main`.

## Done so far (compact)

- ✅ **MVP** (Stages 1–3): playback (GStreamer), full MPRIS, library scan/organize (move-detection,
  playlists, M3U, trash-with-confirm).
- ✅ **Refactor** (Stage 4): 3-crate workspace; `reprise-core` made **dependency-pure** (proven by
  `cargo tree`); platform seam; settings façade; module registry.
- ✅ **GUI-A**: album covers in list + player bar, Now-Playing full view, cover in the track-change
  notification. Whole-branch review: READY TO MERGE.
- ✅ **GUI-A2**: automatic missing-cover retrieval via MusicBrainz/Cover Art Archive; the privacy
  copy discloses network lookup and no stale module, menu, onboarding, or Preferences toggle remains.
- ✅ **GUI-B**: tag editor with multi-select batch edit + confirmed DB-only delete/safe trash.
- ✅ **Edit-tags ratings**: the same mixed-safe batch editor supports unrated and 1–5 stars;
  untouched mixed values remain unchanged, rating-only edits never write audio-file tags, and its
  header relies on the native close control instead of duplicating it with a Cancel button.
- ✅ **GUI-C**: browse bar + read-only Rhythmbox column import.
- ✅ **GUI-C follow-up**: native editable/persistent column layout with accessible button and
  whole-row drag ordering, before/after insertion feedback, reset, and conditional Rhythmbox-found
  first-run offer that remains default off.
- ✅ **Sidebar issue cleanup**: transient Import errors and Missing files rows provide one
  GNOME context action each; diagnostics clear directly, while missing entries require a
  database-only destructive confirmation, compact playlists, purge exact queue IDs, and
  remove the empty Issues group with a Music fallback.
- ✅ **Preferences column-layout navigation**: Layout opens that editor as a native detail page in
  the existing Preferences window with Back navigation instead of an obscured child dialog.
- ✅ **Whole-row column-layout drag restoration**: movable editor rows use capture-phase drag
  recognition across labels, empty space and embedded controls, while short Switch and Up/Down
  clicks keep their existing behavior.
- ✅ **GUI-D**: first-run wizard + validated no-autoplay session restore.
- ✅ **Release readiness**: Meson install, desktop/AppStream/icons, complete German gettext,
  portal-safe Trash, GNOME-50 Flatpak manifest/offline sources, and release checker/docs.
- ✅ **Native About and licensing**: the main menu opens the libadwaita About dialog with the
  installed version, Marvin Baudach as developer and copyright holder, GPL-3.0-or-later for the
  Linux app, and an additional MIT legal section for the Engine and Linux Platform components.
- ✅ **Native offline Help**: the main menu places Help before About, and F1 opens the same
  translated libadwaita shortcuts dialog from Library or Compact View without network access.
- ✅ **Desktop file associations**: installed metadata advertises supported audio and M3U/M3U8;
  forwarded opens reuse one window, play known library tracks in order, import playlists through
  the existing path, and never silently add unknown audio files.
- ✅ **Minimal view + preferences**: one shared-player compact window; immediate persistent layout,
  library and plugin controls; real ten-band equalizer/presets and ReplayGain with synchronized
  live controls; isolated runtime persistence and full release QA green.
- ✅ **Compact-to-Library restoration**: Library content is mounted before the window grows, so the
  compositor cannot expose Compact content stretched to the full Library geometry.
- ✅ **Selectable compact-player layouts**: persistent Bar, Cover, Pill and Card roots share one
  controller, cover pipeline, queue and accessible menu; visible buttons, right-click, Shift+F10,
  Ctrl+M, playing-state continuity and no-autoplay restart are covered by isolated real-input QA;
  Library enters through the main menu or shortcut without a duplicate header button.
- ✅ **ListenBrainz scrobbling**: default-off live integration validates a securely keyring-stored
  token, reports playing-now, submits threshold-completed listens, persists a bounded FIFO offline,
  retries with cancellation/generation guards, and exposes translated connect/status/disconnect UI.
- ✅ **Last.fm scrobbling**: default-off live integration signs desktop-auth, playing-now and
  scrobble requests with bring-your-own API credentials, stores credentials/session only in the
  system keyring, persists a separate bounded FIFO, and runs independently beside ListenBrainz.
- ✅ **Artist & Album News**: a persistent right-side Information panel follows the current
  selection, exposes a default-off privacy boundary, resolves conservative MusicBrainz matches,
  filters cached Upcoming/New album and EP cards, rejects stale selection responses, pins at
  every window width without covering the Library, and exposes visible indeterminate request progress.
- ✅ **Synchronized played-track lyrics**: the Information panel's top Lyrics tab retrieves only
  title, artist, album and duration after successful playback, caches LRCLIB results locally, and
  highlights plus centers timed lines from the existing position stream with generation-safe
  track changes, selectable plain text and instrumental/offline states.
- ✅ **Android USB/MTP synchronization**: Settings shows mounted devices, recognized music and
  Reprise playlists; dropping library tracks enqueues managed copies with file and overall progress,
  cancellation, disconnect-safe resume for stable devices, strict same-device FIFO execution, and
  a queue-aware free-space preflight with an explicit warning before oversized work is accepted.
- ✅ **Full-width Library player bar**: the global status/player block now spans sidebar, library,
  and Information panel at Top or Bottom; Track, centered Transport+Seek, and secondary control
  zones retain one playback controller across immediate Library/Compact switching.
- ✅ **Design-aligned header and sidebar**: one flat full-window header keeps the current source
  title strictly centered with compact accessible actions and Search on the right; the narrower
  navigation adds stable symbolic icons, aligned counts and mockup-derived section spacing while
  preserving adaptive navigation, DnD, menus and Library/Compact restoration. Its persistent
  leftmost toggle removes and restores the complete Sidebar column at wide widths while retaining
  the native narrow navigation path. Scan maintenance stays in Preferences instead of duplicating
  a header action. Problem sources use a translated muted Issues heading rather than a separator
  that can resemble selection.
- ✅ **Compact Library statistics hierarchy**: count and duration sit as a click-through
  bottom-right track-content overlay, while the window header remains above the full-width player
  bar even when its saved position is Top.
- ✅ **Window decoration mode**: Appearance defaults to the flat Chromium-like CSD header and can
  live-switch to a persisted separate native GTK title bar across Library plus Cover, Pill and
  Card; one persistent outer content host keeps it visible through view changes, integrated controls
  and duplicate Compact titles hide while active, and fixed Compact geometry includes its height.
- ✅ **Post-release hardening**: Equalizer slider changes no longer rebuild/seek the pipeline;
  effect failures preserve playback and restore truthful controls; notification cover work is
  off-main; Library preferences update live and expose safe rescan; mapped pointer QA covers all.
- ✅ **Plugin boundary correction**: Equalizer and ReplayGain exist only under Playback; Plugins
  contains optional integrations, while future MTP/iPod device support belongs to Synchronization.
- ✅ **Visible cover-download progress**: the default-off toggle starts/cancels one serial background
  library check, skips local/cached covers, deduplicates albums, refreshes downloaded art, and
  broadcasts checked/downloaded/unavailable counts to persistent Preferences plus a transient
  main-window row; successful scans restart it when enabled so first-run opt-in covers new tracks.
- ✅ **MPRIS cover metadata**: local, embedded and downloaded art resolves off-main to the shared
  cache and is exposed as a generation-guarded `mpris:artUrl` with live metadata updates.
- ✅ **Visible library scan progress**: Setup, Preferences scans, Library rescans, and the
  post-launch smoke share a native row that pulses during discovery and then shows bounded, coalesced
  processed/total plus current-file updates without blocking the scan worker or retaining stale UI.
- ✅ **Stable Equalizer toggle**: enabling or disabling Equalizer updates the persistent neutral
  filter in place, preserving the current pipeline, Playing state and playback position; native
  GStreamer regression coverage verifies the non-rewinding transition.
- ✅ **Manual-QA fixes**: stable one-shot seek-on-release, live additions, and startup reconciliation
  for files added while the app was closed, current-track table selection, and playable stopped
  session restoration without autoplay, functional browse-option search, rating resorting, stable
  empty browse-popup geometry, and repaired playlist row/menu/create/reorder flows including
  duplicate prevention and insertion feedback; all exact user-reported paths have regression coverage.
- ✅ **Manual Up Next + native Compact redesign**: Queue now contains only explicit pending manual
  tracks and consumes them in stable user order before resuming the unchanged playback context;
  Bar/Cover/Pill/Card use opaque native GTK/libadwaita composition, expose Return to Library only
  through the shared context menu, and change volume in five-percent steps only on declared free
  cover/metadata scroll regions without visible Compact volume controls.
- ✅ **Stable track-table geometry**: fixed per-column sizing prevents virtualized row contents from
  changing widths while scrolling; Title alone expands into spare space and columns remain resizable.
- ✅ **Initial Library viewport**: the track-content stack requests the available height during its
  first allocation, preventing a populated Music view from rendering only one row until navigation.
- ✅ **Compact responsive ratings**: the Rating column defaults to a narrow `★ N` popover chooser,
  promotes to five inline stars when widened, and retains real-pointer write-back regression coverage.
- ✅ **Column-header visibility menu**: every track header exposes the same native right-click
  checklist in current column order; fixed columns are visibly disabled, optional visibility is
  immediately persisted, and editor/import changes keep menu state and ordering synchronized.
- ✅ **Confirmed playlist deletion**: manual playlist rows offer a destructive, translated
  right-click action; deletion is DB-only, keeps all tracks, compacts remaining positions, and
  safely returns an open deleted playlist to Music with real-pointer regression coverage.
- ✅ **Android USB/MTP synchronization**: Preferences detects GVfs-mounted devices,
  browses recognized music and phone playlists, accepts the established track DnD
  payload, and copies into the managed `Music/Reprise` area with per-device FIFO,
  visible file/overall progress, cancel cleanup, disconnect handling, and relative
  merged `.m3u8` playlists.
- ✅ **Unified chip filter bar**: the three persistent facet searches are replaced by wrapping,
  removable Genre/Artist/Album chips, a two-step add popover with one temporary value search,
  exact live result counts, and one pill-styled add action while removable chips preserve cascade
  and session behavior without a redundant Reset button.
- ✅ **One-time Rhythmbox import**: detection and explicit column-layout selection live only in
  initial setup; the persistent main menu and Preferences no longer expose a later import path.
- ✅ **Compact playback equalizer**: Enable and Preset remain native settings rows while ten
  accessible vertical scales share one horizontally scrollable card with live dB labels and
  synchronized preset, persistence, backend-failure, and disabled-state behavior.
- ✅ **Preferences visual controls**: Appearance follows the system color scheme and retains only
  window-decoration configuration; Layout uses Top/Bottom Player Bar previews and persistent Sidebar,
  filter bar, Information panel, status line and density controls that apply only after successful
  storage and rollback on error.
- ✅ **Live track-table density**: the persisted Comfortable, Standard and Compact preference now
  reaches concrete virtualized text, cover and rating cells, forces immediate relayout, and is
  inherited by cells created later while scrolling.
- ✅ **Grouped playlist import**: the M3U import action now lives directly beside playlist creation
  in the sidebar instead of occupying a separate Library-header button; picker, result navigation,
  toasts and repeated-activation protection remain shared with the existing import flow.
- ✅ **Finished Information-panel unavailable states**: Refresh disappears whenever Artist News
  cannot run or is already loading; no-selection, multi-selection and missing-artist contexts use
  centered native placeholders, with multi-selection no longer retaining an empty track card.
- ✅ **Inline scrobble-service settings**: ListenBrainz and Last.fm no longer add separate Account
  rows; enabled providers expose a translated Configure action plus live status in the provider row,
  while disabled providers show only their description and always-available toggle.
- ✅ **QA handoff**: confirmed and pending real-desktop checks are consolidated in
  `docs/agent-workflow/MANUAL-QA.md`; display-only test execution is documented in `RELEASING.md`;
  the release checker rejects Rustdoc warnings and broken intra-doc links; a clean release pointer
  harness covers rating, keyboard context/tag validation, Queue badge/reorder feedback, playback,
  screenshots, and GTK/GLib/panic/`RefCell` log rejection.

## Deferred minors / follow-ups (triage at stage reviews)

- `scrobbling.rs` (795 lines) is edge-tight — its next edit must extract a cohesive sibling module.
  Compact, rating, column-header, and playlist-delete pointer flows are already extracted so
  `scripts/ptr-e2e/run.sh` remains below the file-size limit.
- Full `flatpak-builder`/sandbox start was unavailable locally because neither the builder nor
  GNOME-50 runtime/SDK is installed; manifest/YAML/checksums and optimized Meson DESTDIR passed.
- Public release remains externally blocked by the absence of a public immutable source remote
  and verified ownership appropriate for `org.reprise.Reprise`.
- The MusicBrainz `User-Agent` uses the reachable maintainer profile; a public project homepage
  remains part of the general publication handoff above.
- Human manual QA remains exactly as listed in `RELEASING.md`: real rendering/pointer/touch,
  portal picker and Trash visibility, audible codecs, media keys/lock screen, geometry, and
  screenshots from a populated disposable library.
