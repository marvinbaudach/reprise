# Refactoring stage handoff — 2026-07-16

## Objective

Finish the approved project-wide refactoring stage on
`feat/refactoring-durch-codex-in-reprise` without pushing. The implementation
plan is complete through Task 15; the worktree is currently finishing the
integration of the newest local `main` (`273fa21`).

Follow the root `AGENTS.md` and the user-supplied autonomous implementation
instructions literally. Preserve unrelated changes, keep all Rust files below
800 lines, and never run the app against the live desktop or real database.

## Repository state

- Worktree: `/home/marvin/Projects/reprise/.worktrees/refactoring-durch-codex-in-reprise`
- Branch: `feat/refactoring-durch-codex-in-reprise`
- Last completed plan commit: `c72e389 refactor(ui): centralize one-shot background tasks`
- Current operation: `git merge --no-edit main` is in progress.
- Integrated main target: `273fa21`.
- Do not abort or restart the merge. All textual conflict markers have already
  been resolved in the worktree; the index still reports `UU` until the
  resolutions are staged.
- Repository lock: `.superpowers/sdd/LOCK.md`, owned by this refactoring stage.
- Progress ledger: `.superpowers/sdd/progress.md`. Main now tracks this file;
  it contains the Android-sync ledger followed by the restored refactoring
  ledger. The final refactoring line still says `Main integration: in progress`.

## Completed refactoring commits

- `65428f5` restore mandatory gates and file-size compliance
- `05c067a` architecture/frontend linters, merge-readiness script, hooks
- `ec95d7e` centralized artist styling
- `0aa3ca7` QA and stability-test documentation
- `f04667b` AlbumView split
- `a0b7cb5` tag-editor lifecycle split
- `7df365c` uniform spacing on all six Preferences pages and coherent
  Crossfade/Gapless behavior
- `abdb2f5` scan-flow split
- `3ac5ebe` waveform core contract and Linux backend
- `17fc674` platform backend injection at the composition root
- `1cca6b6` window runtime extraction
- `982b167` TrackList/Sidebar orchestration split
- `5c52b24` core database facades
- `3007cf7` true UI feature-module boundaries
- `c72e389` shared one-shot background-task helper

## Main-integration decisions already applied

The conflict resolution intentionally preserves the refactored side as the
structural base while porting all newer Main behavior:

- Schema v9 and Android-sync migrations/tests were added to the split DB files.
- Android sync remains wired through application actions, Preferences,
  Sidebar device cards, Device View, shared progress/toast feedback, and the
  runtime/platform backends.
- The new `device_view` feature uses a real `mod.rs` boundary.
- Main's persistent column-header drag ordering was ported; tests remain in
  `column_layout_tests.rs`.
- Main's synchronous current-track centering, audible-marker gating, and
  end-of-queue refill were ported.
- Main's waveform fixes and new percentile/gamma/fixed-geometry rendering were
  ported while preserving the core/platform waveform abstraction. The grown
  code was split into `waveform_seek.rs`, `waveform_shape.rs`, and
  `waveform_seek_tests.rs`.
- The obsolete GNOME-local `playback/waveform_peaks.rs` stays deleted.
- Main's toast layer now wraps the complete player-bar overlay and keeps the
  four-second timeout.
- Main's persisted sidebar-collapse and information-panel-tab changes remain.
- `library/settings.rs` was split into `settings.rs` and `settings_tests.rs`
  after Main grew it beyond the 800-line gate.
- New sidebar CSS parsing uses the centralized style test helper instead of
  introducing another direct `CssProvider` construction.
- The one-shot helper migration from Task 15 must remain intact in all seven
  guarded consumers.

## Verification already green on the integrated worktree

- `cargo fmt --check`
- `scripts/check-architecture.sh`
- `cargo clippy --locked --all-targets --workspace -- -D warnings`
- `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace`
  - Core: 573 passed
  - GNOME: 447 passed, 78 ignored
  - Platform: 52 passed
- `env RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps`
- `scripts/tests/qa-linters.sh`
- `git diff --check`
- `cargo audit` — only the explicitly accepted `RUSTSEC-2024-0436` warning
- File-size scan — every Rust file is below 800 lines; largest is 799.
- Focused isolated GTK tests:
  - sidebar device-card CSS parses
  - device row pushes a Preferences navigation subpage
- Fully isolated application startup/shutdown smoke succeeded.

The isolated smoke still prints pre-existing portal/AT-SPI and application CSS
parser warnings. It exits successfully. Do not treat those environment/baseline
warnings as a new merge failure unless the current diff is shown to cause them.

## Safe isolated GTK command requirements

Every GTK run must include all of:

```bash
dbus-run-session -- xvfb-run -a env \
  XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
  <REPRISE_SMOKE_* variables> cargo run
```

Never use the live desktop, the real Reprise database, or the user's music.

## Exact remaining work

1. Recheck `git status --short`, `git diff --check`, conflict markers, and the
   under-800 file-size scan.
2. Stage all intended merge resolutions and new split files with `git add`.
   Confirm there are no `UU`/`DU` entries and inspect `git diff --cached --stat`.
3. Commit the existing merge with its normal merge message. Do not squash the
   fifteen plan commits and do not push.
4. Update `.superpowers/sdd/progress.md`:
   - replace `Main integration: in progress` with the actual merge commit hash
     and the verified behavior-preservation note;
   - add a final stage-review entry with automated results, manual checks,
     assumptions, and residual risks.
5. Commit that ledger/stage-review update separately with a concise English
   message.
6. Run `scripts/check-merge-readiness.sh --no-fetch` from the now-clean branch.
   It must pass against `origin/main`; local `main` and `origin/main` were both
   at `273fa21` when integration began. If the remote moved again, inspect
   before integrating another large delta.
7. Perform the final adversarial review of `main...HEAD`, especially:
   Android-sync wiring, queue refill, waveform backend purity, Preferences
   spacing, one-shot consumers, and toast layering.
8. Record only genuine manual checks/deferred risks. Real Android/GVfs MTP,
   pointer interactions, actual GNOME rendering, media keys, and hardware audio
   remain manual.
9. Release the repository lock by deleting `.superpowers/sdd/LOCK.md` with
   `apply_patch` after every required gate is green and the ledger is current.
10. Confirm a clean worktree, summarize commits/gates/manual checks/risks, and
    do not begin another roadmap stage.

## Important cautions

- Do not replace the refactored files wholesale with Main's monoliths; that
  would undo Tasks 6–14 and violate the file-size/module gates.
- Do not restore direct GStreamer use in `reprise-gnome` or the deleted
  `waveform_peaks.rs`.
- Do not reintroduce direct thread/channel plumbing into the consumers listed
  in `scripts/check-architecture.sh`; use `ui::one_shot_task`.
- Do not discard either ledger section in `.superpowers/sdd/progress.md`.
- Do not push unless the user explicitly asks again after handoff.
