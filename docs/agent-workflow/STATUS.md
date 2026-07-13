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

_As of 2026-07-13: ListenBrainz scrobbling is complete, merged, and the lock is released._

## Current position

- **Completed plan:** `docs/superpowers/plans/2026-07-13-listenbrainz-scrobbling.md` (5 tasks).
- **Last completed:** **Opt-in ListenBrainz scrobbling** (`1839e2a` through `340d46e`, merged as
  `5b52fe6`) — secure keyring credentials, playing-now, threshold listens, and a durable offline
  queue run through one cancellable retry worker without sending before explicit opt-in.
- **Current plan:** none — the ListenBrainz stage is complete.
- **➡️ NEXT:** joint native-GNOME visual/Wayland stage review using the exact remaining checks in
  `MANUAL-QA.md`; do not begin another roadmap stage without explicit user direction.
- **Main merge:** `5b52fe6`; this QA/coordination update follows it.

## Done so far (compact)

- ✅ **MVP** (Stages 1–3): playback (GStreamer), full MPRIS, library scan/organize (move-detection,
  playlists, M3U, trash-with-confirm).
- ✅ **Refactor** (Stage 4): 3-crate workspace; `reprise-core` made **dependency-pure** (proven by
  `cargo tree`); platform seam; settings façade; module registry.
- ✅ **GUI-A**: album covers in list + player bar, Now-Playing full view, cover in the track-change
  notification. Whole-branch review: READY TO MERGE.
- ✅ **GUI-A2**: opt-in online album-cover download via MusicBrainz/Cover Art Archive; default OFF.
- ✅ **GUI-B**: tag editor with multi-select batch edit + confirmed DB-only delete/safe trash.
- ✅ **GUI-C**: browse bar + read-only Rhythmbox column import.
- ✅ **GUI-C follow-up**: native editable/persistent column layout with accessible button and
  whole-row drag ordering, before/after insertion feedback, reset, and conditional Rhythmbox-found
  first-run offer that remains default off.
- ✅ **GUI-D**: first-run wizard + validated no-autoplay session restore.
- ✅ **Release readiness**: Meson install, desktop/AppStream/icons, complete German gettext,
  portal-safe Trash, GNOME-50 Flatpak manifest/offline sources, and release checker/docs.
- ✅ **Minimal view + preferences**: one shared-player compact window; immediate persistent theme,
  layout, library and plugin controls; real ten-band equalizer/presets and ReplayGain with synchronized
  live controls; isolated runtime persistence and full release QA green.
- ✅ **Selectable compact-player layouts**: persistent Bar, Cover, Pill and Card roots share one
  controller, cover pipeline, queue and accessible menu; visible buttons, right-click, Shift+F10,
  Ctrl+M, playing-state continuity and no-autoplay restart are covered by isolated real-input QA.
- ✅ **ListenBrainz scrobbling**: default-off live integration validates a securely keyring-stored
  token, reports playing-now, submits threshold-completed listens, persists a bounded FIFO offline,
  retries with cancellation/generation guards, and exposes translated connect/status/disconnect UI.
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
- ✅ **Visible library scan progress**: Setup, header scans, Library rescans, and the post-launch
  smoke share a native row that pulses during discovery and then shows bounded, coalesced
  processed/total plus current-file updates without blocking the scan worker or retaining stale UI.
- ✅ **Stable Equalizer toggle**: enabling or disabling Equalizer updates the persistent neutral
  filter in place, preserving the current pipeline, Playing state and playback position; native
  GStreamer regression coverage verifies the non-rewinding transition.
- ✅ **Manual-QA fixes**: stable one-shot seek-on-release, live additions, and startup reconciliation
  for files added while the app was closed, current-track table selection, and playable stopped
  session restoration without autoplay, functional browse-option search, rating resorting, stable
  empty browse-popup geometry, and repaired playlist row/menu/create/reorder flows including
  duplicate prevention and insertion feedback; all exact user-reported paths have regression coverage.
- ✅ **QA handoff**: confirmed and pending real-desktop checks are consolidated in
  `docs/agent-workflow/MANUAL-QA.md`; display-only test execution is documented in `RELEASING.md`;
  the release checker rejects Rustdoc warnings and broken intra-doc links; a clean release pointer
  harness covers rating, keyboard context/tag validation, Queue badge/reorder feedback, playback,
  screenshots, and GTK/GLib/panic/`RefCell` log rejection.

## Deferred minors / follow-ups (triage at stage reviews)

- `listenbrainz_runtime.rs` (787 lines) is edge-tight — its next edit must extract a sibling module,
  not add inline feature logic.
- Full `flatpak-builder`/sandbox start was unavailable locally because neither the builder nor
  GNOME-50 runtime/SDK is installed; manifest/YAML/checksums and optimized Meson DESTDIR passed.
- Public release remains externally blocked by the absence of a public immutable source remote
  and verified ownership appropriate for `org.reprise.Reprise`.
- Online-cover publication additionally requires a real reachable maintainer-controlled project or
  contact URL for the MusicBrainz `User-Agent`; the current URL must not ship as a placeholder.
- Human manual QA remains exactly as listed in `RELEASING.md`: real rendering/pointer/touch,
  portal picker and Trash visibility, audible codecs, media keys/lock screen, geometry, and
  screenshots from a populated disposable library.
