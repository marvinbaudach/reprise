# AGENTS.md — Resuming work on Reprise

This file tells any coding agent (Codex, Claude, a human) how to pick up development
exactly where the last session left off. **Read this fully before writing code.**

## What Reprise is

A native **GTK4 / libadwaita** music player for GNOME (Rust; MIT engine + GPL-3.0 Linux GUI — see LICENSING.md) — a Rhythmbox
successor. Three-crate Cargo workspace:

- `crates/reprise-core` — pure, cross-platform engine (DB, queue, queries, scanner,
  playlists, M3U, settings, module registry, the cover pipeline, and the platform
  *contracts* `playback`/`media_integration`). **Dependency-pure:** it must never depend
  on gtk4/libadwaita/gstreamer/zbus. Enforced — see Gates.
- `crates/reprise-platform-linux` — Linux platform backends: GStreamer playback (`player`),
  MPRIS/D-Bus media integration (`mpris`).
- `crates/reprise-gnome` — the GTK4/libadwaita frontend. Binary name stays `reprise`.

## Where we are RIGHT NOW — read these two, in order

1. **`.superpowers/sdd/progress.md`** — the authoritative ledger. Append-only history of
   every task: which are complete (with commit hashes), what was fixed, deferred minors,
   and incidents. **Tasks marked `complete` are DONE — do not redo them.** The last lines
   tell you which stage/task is in flight.
2. **`git log --oneline -20`** — cross-check the ledger against reality. Commits are the
   ground truth; only committed work exists. Nothing is ever pushed — work lives on `main`
   locally.

The **current work plan** is the newest file in `docs/superpowers/plans/`. Right now that is
`docs/superpowers/plans/2026-07-12-gui-a2-cover-download.md` (GUI-A2: automatic online album-cover
download, 7 tasks). Each plan has a matching design spec in `docs/superpowers/specs/`.

## Coordinating two agents (Claude ⇄ Codex) — READ FIRST

`docs/agent-workflow/STATUS.md` is the shared, git-tracked coordination board: who's working,
what's done, what's next. **Before touching `main`, read it and claim the Lock** by editing the
gitignored `docs/agent-workflow/LOCK` file (set OWNER to yourself) — **never commit LOCK**; it is
shared via the working tree on disk, so no commit is needed (this replaced ~150 `docs: work lock`
noise commits). Release the Lock (set OWNER back to `FREE`) when you finish. Only ONE agent works
`main` at a time — if the Lock is held by the other agent and recently active, do not start. True
parallel work needs a separate branch/worktree (ask the user).

## Shared workflow skills (read these — both agents use them)

`docs/agent-workflow/` holds the tool-agnostic working method and the accumulated GTK4 pitfalls,
so any agent works the same way without a plugin:

- `docs/agent-workflow/development-method.md` — brainstorm → spec → plan → task-by-task TDD →
  per-task review → stage close-out, and the iron rules (TDD, verify-before-done, hard gates,
  isolation, honesty).
- `docs/agent-workflow/building-gtk4-rust-apps.md` — GTK4/gtk4-rs 0.11 / GStreamer / MPRIS /
  SQLite pitfalls, each a real caught bug. Read before touching frontend/platform code.

## How to resume (the method — no special tooling required)

The project is built **plan-by-plan, task-by-task, test-first** (full detail in
`docs/agent-workflow/development-method.md`). To continue:

1. Open the current plan. Find the **first task whose steps are not yet done** (cross-check
   the ledger + `git log`).
2. Follow that task's steps literally — they contain the exact code and test cases. The
   flow per task is: **write the failing test → run it, see it fail → implement the minimal
   code → run tests, see them pass → run the full gate battery → commit.**
3. Commit message is given verbatim in the task's final step. One commit per task (fixes get
   their own follow-up commits). **No attribution footer. Do not push.**
4. Append one line to `.superpowers/sdd/progress.md`:
   `Task N: complete (commit <hash>, base <hash>, <one-line note>)`.
5. Recommended: after each task, do (or dispatch) an adversarial review of the diff against
   the task's spec before moving on — this pipeline has caught several real bugs that way.

## Gates — ALL must pass before every commit

Run from the repo root:

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings   # ALL clippy lints are errors, not just the workspace set
cargo test --workspace                                  # NOTE: bare `cargo test` runs only the gnome default-member; always use --workspace
cargo audit                                             # ONLY accepted advisory: RUSTSEC-2024-0436 (`paste`, via lofty). A NEW advisory = STOP.
```

Baseline test count as of the current plan: **390 passed; 1 ignored** at plan start; each task
states its expected new total.

**Core purity proof** (run after any `reprise-core` change):
```bash
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'   # MUST be empty
```

**File-size rule:** every file created or substantially edited ends **< 800 lines**. If an edit
would breach it, extract a cohesive sibling module — do NOT trim doc comments to fit.

## NON-NEGOTIABLE safety rules

- **English everywhere** — code, comments, log/error/UI strings, commit messages. (User-facing
  translations come later via gettext; German first.) Internal design docs/specs are in German.
- **Never touch the user's music files or real database unasked.** Reprise only ever *reads*
  the user's audio files; deletes are DB-only or trash-with-confirmation, never silent file ops.
  The real DB is `~/.local/share/reprise/reprise.db` (1686 real tracks; library root
  `/home/marvin/Music`). Do not scan, mutate, or point tooling at it.
- **Headless verification MUST be fully isolated** — this bit us twice. Never run the app on the
  live desktop. Every run/smoke command string MUST contain **all** of:
  ```
  dbus-run-session -- xvfb-run -a env \
    XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) \
    GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
    <REPRISE_SMOKE_* hooks> cargo run
  ```
  Omitting `XDG_DATA_HOME`/`XDG_CACHE_HOME` writes to the user's real DB/cache. Omitting
  `dbus-run-session` hijacks their real MPRIS/session bus. Omitting `GDK_BACKEND=x11` +
  unset `WAYLAND_DISPLAY` opens a window on their real Wayland desktop. Grep your own command
  for `XDG_DATA_HOME` before running it.
- Headless CANNOT verify actual rendering, pointer gestures, media keys, or lock-screen —
  leave those for a human manual pass (the plans list them).

## Roadmap (stages)

Done: MVP (playback, MPRIS, library organize) · Refactor (3-crate split, core made
dependency-pure) · **GUI-A** (album covers in list + bar, Now-Playing full view, cover in
track-change notification) — final review READY TO MERGE.

In progress: **GUI-A2** (automatic online album-cover download via Cover Art Archive — the current
plan).

Next: **GUI-B** (tag editor with **multi-select batch edit** — mixed fields show
"(multiple values)", only user-changed fields are written, never clobber per-track values —
plus delete/trash) · **GUI-C** (browse bar + Rhythmbox column-layout import) · **GUI-D**
(first-run wizard + session restore). Then release (Flatpak/Flathub, gettext, AppStream).

Each next stage starts with a design spec (`docs/superpowers/specs/`) → an implementation plan
(`docs/superpowers/plans/`) → task-by-task execution as above.

## Key conventions to match

- Immutable data, small focused files, early returns, named constants. See existing modules.
- **RefCell discipline** (the #1 recurring panic class): never hold a `Ref`/`RefMut` across a
  call that can re-enter GTK/callbacks — clone/copy the value out in its own statement first.
- GTK cell widgets with per-row async work use a **generation token** so a recycled row never
  shows a stale result (see `cover_loader.rs`).
- Runtime-optional features are **modules** in `reprise-core::modules` (a descriptor + a
  persisted `module.<id>.enabled` flag); gate the behavior on `modules::is_enabled`.
