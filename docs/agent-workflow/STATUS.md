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
OWNER:    codex           # FREE | claude | codex
TASK:     follow table selection across track changes
SINCE:    2026-07-12 20:58 CEST
```

_As of 2026-07-12 20:58 CEST: Codex claimed the lock for the manually reproduced current-track selection regression._

## Current position

- **Completed plan:** `docs/superpowers/plans/2026-07-12-release-readiness.md` (6 tasks).
- **Last completed:** **Startup library reconcile** — after the user's screenshot proved that only
  the three old tracks were visible, the watcher now arms first and immediately reconciles files
  added while Reprise was closed (`3088c25`). Separate tests cover pre-start and post-start files;
  an isolated two-start full-app E2E changed the UI query from 1 to 2 and persisted both exact
  titles. Final gates: 465 passed, 1 ignored; core PURE; audit has only the accepted `paste` advisory.
- **Current plan:** none — every planned application and local release-readiness stage is complete.
- **➡️ NEXT:** maintainer-controlled public-source/release handoff and manual GNOME QA from
  `RELEASING.md`; no agent should invent a remote, domain identity, screenshots, tag, or upload.
- **Feature HEAD:** `3088c25`; this coordination-board update follows it.

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
- ✅ **GUI-D**: first-run wizard + validated no-autoplay session restore.
- ✅ **Release readiness**: Meson install, desktop/AppStream/icons, complete German gettext,
  portal-safe Trash, GNOME-50 Flatpak manifest/offline sources, and release checker/docs.
- ✅ **Manual-QA fixes**: stable one-shot seek-on-release, live additions, and startup reconciliation
  for files added while the app was closed; all exact user-reported paths have regression coverage.

## Deferred minors / follow-ups (triage at stage reviews)

- `notify_now_playing` re-runs cover resolve+thumbnail synchronously on the main thread (cheap on
  warm cache; future off-thread hop).
- Cover cell has no DnD/context-menu gesture (right-click/drag exactly on the 32px thumbnail is a no-op).
- `window.rs` (~791) is edge-tight — its next edit must extract a sibling module, not inline-add.
- `notify_now_playing` doc comment says "Stage 3 Task 9" (cosmetic; should read GUI-A).
- Full `flatpak-builder`/sandbox start was unavailable locally because neither the builder nor
  GNOME-50 runtime/SDK is installed; manifest/YAML/checksums and optimized Meson DESTDIR passed.
- Public release remains externally blocked by the absence of a public immutable source remote
  and verified ownership appropriate for `org.reprise.Reprise`.
- Human manual QA remains exactly as listed in `RELEASING.md`: real rendering/pointer/touch,
  portal picker and Trash visibility, audible codecs, media keys/lock screen, geometry, and
  screenshots from a populated disposable library.
