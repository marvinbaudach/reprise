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
TASK:     —               # e.g. "GUI-A2 Task 2"
SINCE:    —               # timestamp when claimed
```

_As of 2026-07-12 14:13: FREE. Claude paused after handing off; Codex is expected to claim
GUI-A2 Task 2 next._

## Current position

- **Active plan:** `docs/superpowers/plans/2026-07-12-gui-a2-cover-download.md` (GUI-A2, 7 tasks).
- **Last completed:** GUI-A2 **Task 1** — `cover_download` foundation (commit `7c3675c`),
  adversarial review CLEAN. Core stays dependency-pure; `ureq` added no new audit advisory.
- **➡️ NEXT:** GUI-A2 **Task 2** — MusicBrainz URL builders + conservative release matching.
  Then Tasks 3–5 (core), 6–7 (frontend), then the GUI-A2 stage close-out.
- **HEAD at handoff:** `38bca16` (docs). Working tree clean.

## Done so far (compact)

- ✅ **MVP** (Stages 1–3): playback (GStreamer), full MPRIS, library scan/organize (move-detection,
  playlists, M3U, trash-with-confirm).
- ✅ **Refactor** (Stage 4): 3-crate workspace; `reprise-core` made **dependency-pure** (proven by
  `cargo tree`); platform seam; settings façade; module registry.
- ✅ **GUI-A**: album covers in list + player bar, Now-Playing full view, cover in the track-change
  notification. Whole-branch review: READY TO MERGE.
- 🟡 **GUI-A2** (in progress): opt-in online album-cover download. Task 1 done+reviewed; Tasks 2–7 open.
- ⬜ **GUI-B**: tag editor with multi-select batch edit + delete/trash.
- ⬜ **GUI-C**: browse bar + Rhythmbox column import. ⬜ **GUI-D**: first-run wizard + session restore.
- ⬜ **Release**: Flatpak/Flathub, gettext (German first), .desktop/AppStream.

## Deferred minors / follow-ups (triage at stage reviews)

- `notify_now_playing` re-runs cover resolve+thumbnail synchronously on the main thread (cheap on
  warm cache; future off-thread hop).
- Cover cell has no DnD/context-menu gesture (right-click/drag exactly on the 32px thumbnail is a no-op).
- `player_bar.rs` (799) and `window.rs` (~791) are edge-tight — next edit to either must extract a
  sibling module, not inline-add.
- `notify_now_playing` doc comment says "Stage 3 Task 9" (cosmetic; should read GUI-A).
