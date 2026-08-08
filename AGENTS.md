# AGENTS.md — Resuming work on Reprise

This file tells any coding agent (Codex, Claude, a human) how to pick up development
exactly where the last session left off. **Read this fully before writing code.**

## What Reprise is

A native **GTK4 / libadwaita** music player for GNOME (Rust; MIT engine + GPL-3.0 Linux GUI — see LICENSING.md) — a Rhythmbox
successor. **Nine-crate** Cargo workspace:

- `crates/reprise-core` — pure, cross-platform engine (DB, queue, queries, scanner,
  playlists, M3U, settings, module registry, podcasts/YouTube/radio, concerts, releases,
  device sync, the cover pipeline, and the platform *contracts*
  `playback`/`media_integration`). **Dependency-pure:** it must never depend
  on gtk4/libadwaita/gstreamer/zbus. Enforced — see Gates.
- `crates/reprise-platform-linux` — Linux platform backends: GStreamer playback (`player`),
  MPRIS/D-Bus media integration (`mpris`), MTP device sync, Trash, and the D-Bus host for
  the runtime service.
- `crates/reprise-gnome` — the GTK4/libadwaita frontend. Binary name stays `reprise`.
- `crates/reprise-runtime` — the toolkit-neutral single-owner runtime for playback, queue,
  jobs and device runs. **Built and tested, but no shipped surface uses it yet** — see
  `docs/plans/architecture-consolidation.md` §2.2. Whether it is cut over to or shelved is
  still open; `docs/plans/consolidation-plan.md` task 0.10 is where that decision gets
  written down.
- `crates/reprise-runtime-protocol` — the versioned command/snapshot contract between the
  runtime and its clients.
- `crates/reprise-runtime-client` — the client every surface would use to reach the runtime.
- `crates/reprise-cli` — headless CLI over core facades; `mpris` and `worker` are the two
  sanctioned feature-gated exceptions to its core-only dependency rule.
- `crates/reprise-mcp` — local stdio MCP server exposing read-only library resources and
  capability-gated create tools to agents.
- `crates/reprise-stems` — the removable ML stem-separation backend behind the experimental
  instrumental jobs.

`scripts/check-architecture.sh` enforces the dependency direction between all nine.

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
- **`docs/agents/branching.md`** defines the GitHub flow: every branch opens a squashed pull
  request into `dev`, and only a green `dev` reaches `main`, by fast-forward promotion.

## UX rules are binding

`docs/ux-rules.md` is the single UX source of truth. Before touching
any user-facing behavior, read the sections you work in. The contract:

- `[active]` rules are enforceable: deviation is a bug; every `[active]` rule
  has a rule-named test (`fn play_1a_…` / cua-e2e `play-1a-…`) that gates
  merges via `scripts/check-ux-traceability.sh`.
- A rule flips `[planned]` → `[active]` in the same commit that implements
  the behavior and adds its test — never retroactively.
- Rule IDs are append-only; replaced rules stay as `[replaced durch <ID>]`
  and their tests are re-pointed in the same commit.
- If you hit a case no rule covers: do NOT decide locally. Add a
  `[planned]` draft with the next free ID in the affected section, marked
  `<!-- REVIEW: Regelvorschlag -->`, and surface it for human review.

## How to resume (the method — no special tooling required)

The project is built **plan-by-plan, task-by-task, test-first**. To continue:

1. Read the ledger + `git log` to find where work left off. For new work, brainstorm and
   plan the task before writing code (superpowers process skills).
2. Work each task test-first: **write the failing test → run it, see it fail → implement the
   minimal code → run tests, see them pass → run the full gate battery → commit.**
3. One commit per task (fixes get their own follow-up commits). **No attribution footer. Do
   not push unless explicitly requested.** Branch from `dev`, open squashed pull requests to
   `dev`, and leave the fast-forward promotion of `dev` to `main` to the owner; emergency
   `hotfix/*` branches also start from `dev` and pass the same full gate. See
   `docs/agents/branching.md`.
4. Append one line to `.superpowers/sdd/progress.md`:
   `Task N: complete (commit <hash>, base <hash>, <one-line note>)`.
5. Recommended: after each task, do (or dispatch) an adversarial review of the diff before
   moving on — this pipeline has caught several real bugs that way.
6. After a pull request is squash-merged into `dev`, close its local worktree with
   `scripts/close-worktree.sh --repo /home/marvin/Projects/reprise --worktree <path> --pr <number>`.
   The command verifies the merged PR and exact head before removing anything. If the current
   session or another process still owns the directory, it records a pending cleanup for the
   weekly collector.

## GitHub contribution flow — mandatory for every agent

- Never commit or push directly to `dev` or `main`. Create a dedicated branch for every
  change and open a pull request whose base branch is `dev`.
- Agents may prepare, update, verify, and merge pull requests into `dev` after the gate is
  green, but must not promote `dev` to `main` or approve a production release. Which gate
  that is depends on the repository's plan — see `docs/agents/branching.md`; today it is a
  local `scripts/check-merge-readiness.sh` run, because GitHub enforces nothing here.
- **Every pull request is squashed**, and every pull request targets `dev`. The repository
  allows no other merge method, so this is not a choice to make on the merge button. One
  commit per pull request, titled as a conventional commit. A squashed branch is never
  reported as merged by `git branch -d`, so delete it with `-D`, and never stack a topic
  branch on another topic branch. See `docs/agents/branching.md`, "Merge method".
- A merged topic is not fully closed until its local worktree was removed or an exact,
  PR-verified pending cleanup was recorded. Dirty, locked, active, or unmerged worktrees are
  never cleanup candidates.
- Only the repository owner promotes `dev` to `main`, and the promotion is a fast-forward
  push (`git push origin origin/dev:main`), not a pull request — a squashed promotion would
  make the two branches diverge permanently. Agents never run it.
- Emergency production fixes start on a `hotfix/*` branch **from `dev`** and reach `main`
  through the same promotion. A `hotfix/*` merged straight into `main` breaks the
  fast-forward property irrecoverably; `docs/agents/branching.md` explains why.

## Gates — ALL must pass before every commit

Run from the repo root:

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings   # ALL clippy lints are errors, not just the workspace set
cargo test --workspace                                  # NOTE: bare `cargo test` runs only the gnome default-member; always use --workspace
cargo audit                                             # ONLY accepted advisory: RUSTSEC-2024-0436 (`paste`, via lofty). A NEW advisory = STOP.
```

Baseline test count: take it from the **latest entry in
`.superpowers/sdd/progress.md`**, not from this file — a number hardcoded here goes stale
within days and a stale baseline is worse than none. Each task states its expected new total
relative to the run it starts from.

**Core purity proof** (run after any `reprise-core` change):
```bash
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'   # MUST be empty
```

**File-size rule:** every *code* file created or substantially edited ends **< 800 lines**. If
an edit would breach it, extract a cohesive sibling module — do NOT trim doc comments to fit.
Markdown is exempt: docs are split by subject, never by line count.

## NON-NEGOTIABLE safety rules

- **English everywhere** — code, comments, log/error/UI strings, commit messages, **and every
  document in this repository**: `docs/ux-rules.md`, the plans under `docs/plans/`, the ADRs,
  research notes and design specs. This was decided on 2026-07-31 and the existing German
  documents were translated in the same pass; a new document written in German is a defect, not
  a style choice. The one deliberate exception is `README.de.md`, which is a German translation
  *for users* and stays German — as do the gettext catalogs under `po/`, which are how the UI
  reaches non-English users.
- **Never touch the user's music files or real database unasked.** Reprise writes inside the
  music collection only in three cases: tags through an explicit Tag Editor action, a new `.lrc`
  beside an existing track after downloading synchronized lyrics, and a new `cover.<ext>` in an
  album directory after downloading a cover when no known folder image exists. Sidecars and cover
  targets are derived only from track paths and never overwrite an existing file. Reprise writes
  nothing else beside music files; deletes are DB-only or trash-with-confirmation, never silent
  file ops. The single exception is its own abandoned writeback temporary —
  `.reprise-<16 hex digits>.tmp`, a regular file untouched for an hour — which a later write in
  that same directory sweeps up. Nothing else is ever matched.
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

The staged GUI-A…GUI-D roadmap this section used to carry is done and has been superseded by
feature work. Landed since: the tag editor with multi-select batch edit, the browse bar and
editable column layout, first-run and session restore, album covers and the cover pipeline,
podcasts, YouTube, radio, concerts, new releases, device sync, library doctor, my stats,
lyrics, the visualizer, the experimental stem separation, the CLI and MCP surfaces, and the
headless runtime (built, not yet wired — see the crate list above).

**Where the project stands now:** a project-wide review and its execution plan live in
`docs/plans/architecture-consolidation.md` (findings) and `docs/plans/consolidation-plan.md`
(waves, task by task). Wave 0 there is the set of release blockers for opening a test round.
Read those two before starting architectural work.

New work still starts with a design spec → an implementation plan (held in-session, not
committed) → task-by-task execution as above.

## Agent skills

### Issue tracker

GitHub Issues via `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Default labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `CONTEXT.md` + `docs/adr/` at the repo root. See `docs/agents/domain.md`.

## Not released yet — no backwards compatibility

> **This section expires with the first test release.** It is correct only while nobody has
> installed Reprise. The moment a tester does, the permission below becomes a licence to
> delete data in someone else's library. `docs/plans/consolidation-plan.md` task 0.7 carries
> the replacement text and says to set it **in the same change that opens the test round** —
> not before, because until then this rule is still right and still useful.

Reprise has **not** shipped and there are **no existing installations**. Migrations,
compatibility fallbacks, dual-write paths and deprecated-key readers are therefore *not*
a design criterion anywhere in this repo.

Where a clean data model and a backwards-compatible one collide, take the clean one and
delete the old shape outright. A leftover second source of truth is worse than either
option on its own. This applies to settings keys, module descriptors, database columns and
on-device layouts alike. (Schema *migrations* still exist as the mechanism for changing the
database — the point is that you never have to preserve an old shape for users who don't
exist.)

## Key conventions to match

- Immutable data, small focused files, early returns, named constants. See existing modules.
- **RefCell discipline** (the #1 recurring panic class): never hold a `Ref`/`RefMut` across a
  call that can re-enter GTK/callbacks — clone/copy the value out in its own statement first.
- GTK cell widgets with per-row async work use a **generation token** so a recycled row never
  shows a stale result (see `cover_loader.rs`).
- Runtime-optional features are **modules** in `reprise-core::modules` (a descriptor + a
  persisted `module.<id>.enabled` flag); gate the behavior on `modules::is_enabled`.
- **Icon names are strings GTK resolves at runtime**, so a wrong one is not a compile error —
  it silently draws the missing-image box. `emblem-ok-symbolic` did that at seven call sites
  (`adwaita-icon-theme 50` dropped the name). Either use a name the theme has, or pair it with
  a fallback in code and list it in `ui::icons`'s `GUARDED`;
  `every_icon_name_the_app_asks_for_can_be_drawn` checks every `"…-symbolic"` literal under
  `src/ui` against the installed theme.

## Completed file ownership — episodes as queue citizens

Packages 1 through 5 are complete and this ownership is released. No repository
lock or coordination board exists in this checkout.

### Package 1 — typed manual queue

| Owner | Files |
| --- | --- |
| episodes-as-queue-citizens | `crates/reprise-core/src/up_next.rs`, `crates/reprise-core/src/library/session.rs`, `crates/reprise-core/src/queries/queue.rs`, `crates/reprise-core/src/queries/mod.rs`, and directly affected Core/GNOME tests and typed-call-site adapters |
| episodes-as-queue-citizens | Minimal append-only rule and plan records in `docs/ux-rules.md`, `docs/plans/podcasts-radio.md`, plus `.superpowers/sdd/progress.md` |
| sibling branches — excluded | `crates/reprise-gnome/src/ui/podcasts/**`, `crates/reprise-core/src/podcasts/store.rs`, `crates/reprise-core/src/podcasts/youtube.rs` |

### Package 2 — queued-episode playback

| Owner | Files |
| --- | --- |
| episodes-as-queue-citizens | `crates/reprise-gnome/src/ui/playback/{preview,external_media_state,external_media,external_media_completion,playback_faults,up_next_transport,session_player,queue_transport,player_controller,player_event_handling}.rs` and their directly affected tests |
| episodes-as-queue-citizens | Narrow typed-state adapters in the GNOME and runtime crates, podcast playback copy in `strings_podcasts.rs`, and the matching gettext catalogs |
| episodes-as-queue-citizens | Minimal append-only rule and plan records in `docs/ux-rules.md`, `docs/plans/podcasts-radio.md`, plus `.superpowers/sdd/progress.md` |
| sibling branches — excluded | `crates/reprise-gnome/src/ui/podcasts/**`, `crates/reprise-core/src/podcasts/store.rs`, `crates/reprise-core/src/podcasts/youtube.rs` |

### Package 3 — mixed queue rendering

| Owner | Files |
| --- | --- |
| episodes-as-queue-citizens | `crates/reprise-gnome/src/ui/track_list/{queue_sections,track_list_model,track_list_columns,track_cover,column_layout,track_list_context_menu}.rs`, their focused tests, and cohesive new queue-row presentation/menu siblings |
| episodes-as-queue-citizens | `crates/reprise-gnome/src/ui/now_playing/up_next_panel.rs`, its focused tests, and the narrow typed projection adapter in `crates/reprise-gnome/src/ui/playback/queue_transport.rs` |
| episodes-as-queue-citizens | Minimal append-only CTX rule draft in `docs/ux-rules.md` and `.superpowers/sdd/progress.md` |
| sibling branches — excluded | `crates/reprise-gnome/src/ui/podcasts/**`, `crates/reprise-core/src/podcasts/store.rs`, `crates/reprise-core/src/podcasts/youtube.rs` |

### Package 4 — queue entry routes (complete; ownership released)

| Owner | Files |
| --- | --- |
| episodes-as-queue-citizens | `crates/reprise-gnome/src/ui/track_list/{track_list_dnd,track_list_dnd_smoke,track_list_keyboard_reorder}.rs`, `crates/reprise-gnome/src/ui/sidebar/{sidebar_dnd,sidebar_session}.rs`, their focused tests, and narrow typed callback adapters |
| episodes-as-queue-citizens | `crates/reprise-gnome/src/ui/now_playing/{up_next_panel,up_next_panel_tests,now_playing}.rs`, `crates/reprise-gnome/src/ui/playback/queue_transport.rs`, and the narrow window wiring for typed queue drops |
| episodes-as-queue-citizens | `crates/reprise-gnome/src/ui/podcasts/{podcasts_dnd,podcasts_groups,podcasts_groups_tests,podcasts_context_menu,podcasts_view,podcasts_view_actions}.rs`, source-view callbacks, queue-entry copy, and matching gettext catalogs |
| episodes-as-queue-citizens | Append-only package-4 rule and reversal records in `docs/ux-rules.md`, `docs/plans/podcasts-radio.md`, plus `.superpowers/sdd/progress.md` |
| sibling branches — excluded | `crates/reprise-core/src/podcasts/store.rs`, `crates/reprise-core/src/podcasts/youtube.rs`, and runtime protocol/MCP/MPRIS outward surfaces |

### Package 5 — outward-facing surfaces (complete; ownership released)

| Owner | Files |
| --- | --- |
| episodes-as-queue-citizens | Runtime-protocol queue DTOs and their runtime/Linux-service projections, including additive typed item lists beside legacy track-only id fields |
| episodes-as-queue-citizens | MCP queue DTO/read surfaces and validation regressions; `PlayTrackIds`, `QueueAddNext`, and `QueueAddLast` remain track-only |
| episodes-as-queue-citizens | MPRIS episode identity/metadata, the GNOME agent-queue mirror, and their focused tests |
| episodes-as-queue-citizens | Append-only package-5 rule, plan, and completion records in `docs/ux-rules.md`, `docs/plans/podcasts-radio.md`, and `.superpowers/sdd/progress.md` |
| sibling branches — excluded | `crates/reprise-core/src/podcasts/store.rs`, `crates/reprise-core/src/podcasts/youtube.rs`, and unrelated source UI or packaging work |

## Active file ownership — multi-surface frontends

Spec: `docs/superpowers/specs/2026-08-01-multi-surface-frontends-design.md`
Branch: `feature/multi-surface-frontends`

This ownership is ACTIVE. A sibling branch that edits an owned path must
rebase onto this branch first, not merge past it.

### P0 — groundwork (this plan)

| Owner | Files |
| --- | --- |
| multi-surface-frontends | `crates/reprise-view/**`, the `members` list in the workspace `Cargo.toml`, `scripts/check-architecture.sh` |
| multi-surface-frontends | `docs/superpowers/specs/2026-08-01-multi-surface-frontends-design.md`, `docs/superpowers/plans/2026-08-01-multi-surface-p0-s1.md`, `docs/research/android-spike-2026-08.md`, this section of `AGENTS.md` |
| sibling branches — excluded | everything under `crates/reprise-gnome/**` until P1a opens |

### P1a — the mobile slice of reprise-view (not yet open)

Package boundaries are drawn when P1a is planned, after the S1 findings
land. Until then no `reprise-gnome` path is owned by this branch.

### Plans checked before P1a — all resolved (2026-08-01)

Four plans carried an unfinished `phase:` when P0 started. None of their
branches still existed, and a check against `origin/dev` found the work
finished in every case. The fields were stale, not the work.

| Plan | was | is | Evidence |
| --- | --- | --- | --- |
| `docs/plans/motion-player.md` | planned | **shipped** | MOT-5 `[active]`; `waveform_seek.rs` carries `crossfade_progress`/`desaturation_progress`, `player_bar.rs` carries `animate_play_pulse()` |
| `docs/plans/ux-rules-motion.md` | reviewed | **shipped** | both phases merged; `ui/motion.rs` exists with the planned tokens; `check-motion-tokens.sh` has an empty phase-two allowlist |
| `docs/plans/list-views-fixes.md` | refactored | **shipped** | every measure verified in code (cover centring, duration format, episode window of 10, title tail dedup, `image_url` migration) |
| `docs/plans/audio-character-mcp.md` | ready-for-review | **reverted** | shipped 2026-07-19/20, then removed wholesale by `eda0edaebb`; migration v27 drops its tables. No production code remains |

`reverted` is a new value in the pipeline's status vocabulary. It was
introduced deliberately: neither `shipped` (nothing is left) nor `planned`
(it went much further) would have been honest.

**Consequence: P1a is not blocked.** No open plan work touches its target
areas (`track_list`, `playback`, `now_playing`, `lyrics`, playlists, search,
scan). The motion work sits *in* those areas but is finished — P1a moves it
like any other code.

## Completed file ownership — Library Doctor guard rails

Branch: `feature/library-doctor-guard-rails`

This ownership is COMPLETE and released. Packages ran in wave order.
Mechanical `DoctorProposal` constructor updates required to keep GUARD-1
buildable travelled with GUARD-1. GUARD-4's cohesive provider fixture lives in
a sibling module because `tests.rs` was already too close to the 800-line cap.

| Package | Owner | Files |
| --- | --- | --- |
| GUARD-0 | library-doctor-guard-rails | `docs/ux-rules.md` § Y and this ownership record |
| GUARD-1 | library-doctor-guard-rails | `crates/reprise-core/src/library/library_doctor/remote/{guard_rails,guard_rails_tests,arbitration,mod}.rs`, `remote_tests.rs`, `types.rs`, and mechanical `DoctorProposal` constructor updates |
| GUARD-2 | library-doctor-guard-rails | `crates/reprise-core/src/library/library_doctor/{review,review_tests}.rs`, `crates/reprise-gnome/src/ui/library_doctor/review_page_tests.rs` |
| GUARD-3 | library-doctor-guard-rails | `crates/reprise-core/src/{db_library_doctor,db}.rs`, `crates/reprise-core/src/library/library_doctor/store.rs` |
| GUARD-4 | library-doctor-guard-rails | `crates/reprise-core/src/library/library_doctor/{tests,guard_rail_scan_tests}.rs` |

## Active file ownership — Library Doctor fix round 3

Branch: `feature/library-doctor-fix-round-3`

This ownership is ACTIVE until the stage is complete. Packages run in the wave
order recorded here. `MATCH-3`, `PERF-1`, and `PERF-3` are the only writers of
`scan.rs` and run strictly in that sequence. The string catalog and UX rules
reach their final stage shape in Wave 0 and are read-only for every later
package.

| Wave | Package | Owned files |
| --- | --- | --- |
| 0 | DIAG-1 | `crates/reprise-core/src/library/library_doctor/remote/{diagnostics,mod}.rs` and the minimum test-only arbitration visibility needed by the diagnostic |
| 0 | DIAG-2 | `crates/reprise-gnome/src/ui/library_doctor/review_page_tests.rs` |
| 0 | DIAG-3 | `crates/reprise-gnome/src/ui/sidebar/sidebar_layout_tests.rs` |
| 0 | STR-1 | `crates/reprise-gnome/src/ui/strings_library_doctor.rs` |
| 0 | RULES-1 | `docs/ux-rules.md` section Y and this ownership record |
| 1 | MATCH-1 | `crates/reprise-core/src/library/library_doctor/remote/{orchestrator,network,acoustid,network_tests,mod,diagnostics}.rs` |
| 1 | MATCH-2 | `crates/reprise-core/src/library/library_doctor/remote/{album_match,album_match_tests}.rs` |
| 1 | NAV-1 | `crates/reprise-gnome/src/ui/library_doctor/mod.rs`, `crates/reprise-gnome/src/ui/window/{window,window_runtime_wiring,library_shell,content_stack,library_chrome}.rs` |
| 1 | CARD-1 | `crates/reprise-gnome/src/ui/library_doctor/progress_card.rs`, `crates/reprise-gnome/src/ui/sidebar/{sidebar_activity_slot,sidebar_issues_section}.rs`, `crates/reprise-gnome/src/ui/issues/missing_progress.rs`, and the existing `scan-card*` stylesheet rules |
| 1 | REV-1 | `crates/reprise-gnome/src/ui/library_doctor/review_page.rs` |
| 1 | START-1 | `crates/reprise-gnome/src/ui/library_doctor/start_page.rs` and the stethoscope SVG in `assets/icons/**` |
| 2 | MATCH-3 | `crates/reprise-core/src/library/library_doctor/{scan,store}.rs`, `crates/reprise-core/src/library/library_doctor/remote/orchestrator.rs`, `crates/reprise-core/src/{db_library_doctor,db}.rs` |
| 2 | MATCH-5 | `crates/reprise-core/src/library/library_doctor/remote/{arbitration,album_match}.rs` |
| 2 | PERF-2 | `crates/reprise-core/src/library/library_doctor/remote/{cache,cache_tests}.rs` |
| 2 | REV-3 | `crates/reprise-gnome/src/ui/library_doctor/{review_page,review_conflicts,review_row}.rs` |
| 2 | REV-4 | `crates/reprise-gnome/src/ui/library_doctor/{review_header,review_model}.rs` |
| 2 | REV-5 | `crates/reprise-gnome/src/ui/library_doctor/review_row.rs` |
| 2 | CARD-2 | `crates/reprise-gnome/src/ui/issues/missing_view.rs`, `crates/reprise-gnome/src/ui/window/window.rs` |
| 3 | PERF-1 | `crates/reprise-core/src/library/library_doctor/{scan,types}.rs`, `crates/reprise-gnome/src/ui/library_doctor/running_page.rs` |
| 3 | PERF-4 | `crates/reprise-core/src/library/library_doctor/remote/orchestrator.rs` |
| 3 | PERF-5 | `crates/reprise-core/src/library/library_doctor/preferences.rs`, `crates/reprise-gnome/src/ui/library_doctor/start_page.rs` |
| 3 | REV-2 | `crates/reprise-core/src/library/library_doctor/{review,review_tests}.rs`, `crates/reprise-gnome/src/ui/library_doctor/{review_page,review_filter_bar}.rs`, `crates/reprise-mcp/src/{doctor_dto,doctor_actions}.rs` |
| 4 | PERF-3 | `crates/reprise-core/src/library/library_doctor/{store,scan}.rs` |
