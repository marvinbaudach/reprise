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
TASK:     Autonomous release QA audit and consolidated manual-test ledger
SINCE:    2026-07-12 23:03 CEST
```

_As of 2026-07-12 23:03 CEST: Codex claimed the lock for autonomous release QA and manual-test consolidation._

## Current position

- **Completed plan:** `docs/superpowers/plans/2026-07-12-release-readiness.md` (6 tasks).
- **Last completed:** **Tag-editor Enter submission** — all seven entry rows activate the dialog's
  Apply default widget on Enter while dirty-state sensitivity and numeric validation remain intact
  (`9111a0c`). The targeted real-display regression passed. Final gates: 479 passed, 4 ignored; core
  PURE; audit has only the accepted `paste` advisory. Real keyboard confirmation remains manual.
- **Current plan:** none — every planned application and local release-readiness stage is complete.
- **➡️ NEXT:** maintainer-controlled public-source/release handoff and manual GNOME QA from
  `RELEASING.md`; no agent should invent a remote, domain identity, screenshots, tag, or upload.
- **Feature HEAD:** `9111a0c`; this coordination-board update follows it.

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
  for files added while the app was closed, current-track table selection, and playable stopped
  session restoration without autoplay, functional browse-option search, rating resorting, stable
  empty browse-popup geometry, and repaired playlist row/menu/create/reorder flows including
  duplicate prevention and insertion feedback; all exact user-reported paths have regression coverage.

## Deferred minors / follow-ups (triage at stage reviews)

- `notify_now_playing` re-runs cover resolve+thumbnail synchronously on the main thread (cheap on
  warm cache; future off-thread hop).
- `window.rs` (~791) is edge-tight — its next edit must extract a sibling module, not inline-add.
- `notify_now_playing` doc comment says "Stage 3 Task 9" (cosmetic; should read GUI-A).
- Full `flatpak-builder`/sandbox start was unavailable locally because neither the builder nor
  GNOME-50 runtime/SDK is installed; manifest/YAML/checksums and optimized Meson DESTDIR passed.
- Public release remains externally blocked by the absence of a public immutable source remote
  and verified ownership appropriate for `org.reprise.Reprise`.
- Human manual QA remains exactly as listed in `RELEASING.md`: real rendering/pointer/touch,
  portal picker and Trash visibility, audible codecs, media keys/lock screen, geometry, and
  screenshots from a populated disposable library.
