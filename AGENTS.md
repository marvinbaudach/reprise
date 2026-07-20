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
   ground truth; only committed work exists. Active work lives on a dedicated branch and is
   published through the GitHub flow below.

Throwaway per-stage implementation plans are not kept in the repo — the
`.superpowers/sdd/progress.md` ledger plus `git log` are the authoritative record of what is
done and in flight. Work new features via the brainstorm → spec → plan → TDD method below,
holding any working spec/plan in the session rather than committing it.

The exception is a plan that outlives its own execution because later work has to follow it:
those live in `docs/plans/` and are maintained, not archived (`docs/plans/android-sync.md`,
`docs/plans/ux-rules-acceptance-tests.md`). Binding contracts (`docs/ux-rules.md`) are never
plans — they live at `docs/` top level and outrank the code.

## Shared workflow skills (read these)

- **`building-gtk4-rust-apps` skill** — GTK4/gtk4-rs 0.11 / GStreamer / MPRIS / SQLite pitfalls,
  each a real caught bug. Read before touching frontend/platform code.
- The **superpowers** process skills (brainstorming, TDD, systematic-debugging,
  verification-before-completion) carry the iron rules: TDD, verify-before-done, hard gates,
  isolation, honesty.

## UX rules are binding

`docs/ux-rules.md` is the single UX source of truth (German). Before touching
any user-facing behavior, read the sections you work in. The contract:

- `[aktiv]` rules are enforceable: deviation is a bug; every `[aktiv]` rule
  has a rule-named test (`fn play_1a_…` / cua-e2e `play-1a-…`) that gates
  merges via `scripts/check-ux-traceability.sh`.
- A rule flips `[geplant]` → `[aktiv]` in the same commit that implements
  the behavior and adds its test — never retroactively.
- Rule IDs are append-only; replaced rules stay as `[ersetzt durch <ID>]`
  and their tests are re-pointed in the same commit.
- If you hit a case no rule covers: do NOT decide locally. Add a
  `[geplant]` draft with the next free ID in the affected section, marked
  `<!-- REVIEW: Regelvorschlag -->`, and surface it for human review.

## How to resume (the method — no special tooling required)

The project is built **plan-by-plan, task-by-task, test-first**. To continue:

1. Read the ledger + `git log` to find where work left off. For new work, brainstorm and
   plan the task before writing code (superpowers process skills).
2. Work each task test-first: **write the failing test → run it, see it fail → implement the
   minimal code → run tests, see them pass → run the full gate battery → commit.**
3. One commit per task (fixes get their own follow-up commits). **No attribution footer.**
   Push only the dedicated task branch, then open a pull request targeting `dev`.
4. Append one line to `.superpowers/sdd/progress.md`:
   `Task N: complete (commit <hash>, base <hash>, <one-line note>)`.
5. Recommended: after each task, do (or dispatch) an adversarial review of the diff before
   moving on — this pipeline has caught several real bugs that way.

## GitHub contribution flow — mandatory for every agent

- Never commit or push directly to `dev` or `main`. Create a dedicated branch for every
  change and open a pull request whose base branch is `dev`.
- Agents may prepare, update, and verify pull requests into `dev`, but must not merge
  `dev` into `main` or approve a production release.
- Only the repository owner promotes `dev` to `main`, after reviewing the accumulated
  changes and confirming that all required checks are green.
- Emergency production fixes still start on a `hotfix/*` branch and require an explicit
  pull request and owner approval before reaching `main`.

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

**File-size rule:** every *code* file created or substantially edited ends **< 800 lines**. If
an edit would breach it, extract a cohesive sibling module — do NOT trim doc comments to fit.
Markdown is exempt: docs are split by subject, never by line count.

## NON-NEGOTIABLE safety rules

- **English everywhere** — code, comments, log/error/UI strings, commit messages. (User-facing
  translations come later via gettext; German first.) Internal design docs/specs are in German
  — deliberately, it is the project's working language. Tests and shell scripts are code, so
  they stay English even when they enforce a German doc; rule IDs and status tokens
  (`[aktiv]`, `[geplant]`) are quoted verbatim and stay German.
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
- **Never clone the repo or build under `/tmp`.** `/tmp` is a 16G tmpfs, so a cargo `target/`
  there lives in RAM — two stray clones filled it to 13G and pushed the machine 13G into swap.
  Long-lived branch work goes in `.worktrees/<name>` (`git worktree add`, on disk). Throwaway
  clones for merge checks or visual runs go under `~/.cache/reprise-scratch/`:
  ```
  mkdir -p ~/.cache/reprise-scratch
  scratch=$(mktemp -d ~/.cache/reprise-scratch/<task-name>.XXXXXX)
  ```
  Do NOT work around this with a shared `CARGO_TARGET_DIR` — cargo takes an exclusive lock on the
  build directory, so one shared target dir serialises parallel agents. One worktree per agent
  means one `target/` per agent, which is what keeps waves parallel. The small `$(mktemp -d)`
  XDG dirs in the headless recipe above are fine and stay in `/tmp`.

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

Each next stage starts with a design spec → an implementation plan (held in-session, not
committed) → task-by-task execution as above.

## Agent skills

### Issue tracker

GitHub Issues via `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Default labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `CONTEXT.md` + `docs/adr/` at the repo root. See `docs/agents/domain.md`.

## Key conventions to match

- Immutable data, small focused files, early returns, named constants. See existing modules.
- **RefCell discipline** (the #1 recurring panic class): never hold a `Ref`/`RefMut` across a
  call that can re-enter GTK/callbacks — clone/copy the value out in its own statement first.
- GTK cell widgets with per-row async work use a **generation token** so a recycled row never
  shows a stale result (see `cover_loader.rs`).
- Runtime-optional features are **modules** in `reprise-core::modules` (a descriptor + a
  persisted `module.<id>.enabled` flag); gate the behavior on `modules::is_enabled`.
