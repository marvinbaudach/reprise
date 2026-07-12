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
TASK:     GUI-D + release # first-run/session restore, then packaging/i18n
SINCE:    2026-07-12 15:01 CEST
```

_As of 2026-07-12 15:01 CEST: Codex claimed continuous work through GUI-B, GUI-C, GUI-D, and release._

## Current position

- **Completed plan:** `docs/superpowers/plans/2026-07-12-gui-c-browse-columns.md` (GUI-C, 7 tasks).
- **Last completed:** **GUI-C** — cascading Genre/Artist/Album browse bar and explicit read-only
  Rhythmbox visible-column import, through `e9faba2` plus review fix `1962b4c`; whole-branch
  review READY. Final gates: 440 passed, 1 ignored; core PURE; isolated browse/import/source
  smokes passed. Human layout/keyboard/real-GSettings checks remain.
- **➡️ NEXT:** GUI-D design/spec/plan — first-run wizard plus session restore.
- **Feature HEAD:** `1962b4c`. Codex retains the continuous-work lock. Working tree clean.

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
- ⬜ **GUI-D**: first-run wizard + session restore.
- ⬜ **Release**: Flatpak/Flathub, gettext (German first), .desktop/AppStream.

## Deferred minors / follow-ups (triage at stage reviews)

- `notify_now_playing` re-runs cover resolve+thumbnail synchronously on the main thread (cheap on
  warm cache; future off-thread hop).
- Cover cell has no DnD/context-menu gesture (right-click/drag exactly on the 32px thumbnail is a no-op).
- `player_bar.rs` (799) and `window.rs` (~791) are edge-tight — next edit to either must extract a
  sibling module, not inline-add.
- `notify_now_playing` doc comment says "Stage 3 Task 9" (cosmetic; should read GUI-A).
