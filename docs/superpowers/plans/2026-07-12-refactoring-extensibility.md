# Refactoring & Extensibility Implementation Plan (Queue C)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **REVISION 2026-07-12 (user decision):** cross-platform portability is now a stated product goal — the same Rust core must be able to serve GNOME/GTK4 today and KDE/Qt, macOS, and Windows frontends later. Consequence baked into this revision: `reprise-core` becomes **dependency-pure** (no gtk4/libadwaita — already true — and now also no gstreamer, no zbus); the two genuinely platform-specific subsystems (audio playback, OS media integration) move behind narrow core-defined seams into a new `crates/reprise-platform-linux`. New Task 5 (platform-seam extraction, in place) inserted; old Tasks 5–8 renumbered 6–9. Task 1 (queries split) is already running and is unchanged apart from one renumbered forward reference.

**Goal:** Behavior-preserving restructure of Reprise after MVP stages 1–3: pay the waived <800-line DoD debt, split the codebase into a **dependency-pure, cross-platform `reprise-core`** library crate (Transmission model — one engine, many native frontends; explicitly including future KDE/Qt, macOS, and Windows clients, not just other Linux DEs), a `reprise-platform-linux` crate holding the Linux implementations of the two platform seams (GStreamer audio, MPRIS media integration), and the GTK4/libadwaita frontend crate — and lay the minimal extensibility substrate (typed settings façade, module registry with persisted on/off flags, MPRIS as the first gated module) that stage 5 builds on.

**Architecture:** Six mechanical, compiler-verified moves plus three small additive layers. First the three oversized files are split *in place* (queries.rs → per-`ViewSource` query modules; track_list.rs → columns/sort/activation/smoke siblings; window.rs → shell + scan flow), and the platform seam is extracted *in place* (a `PlaybackBackend` trait and a media-integration handle contract, both defined in core-destined modules and derived strictly from existing call sites; `player.rs`/`mpris/` become their Linux implementations) — so that the subsequent Cargo-workspace split is a near-pure directory move of already-final files into **three** crates. The workspace boundary is the load-bearing seam, twice over: `reprise-core` has **no gtk4/libadwaita/gstreamer/zbus dependency at all** (the portability proof is `cargo tree -p reprise-core`), so GTK/adw API churn is compiler-confined to `crates/reprise-gnome`, GStreamer/zbus churn to `crates/reprise-platform-linux`, and a macOS or Windows port implements exactly two known contracts (playback backend, now-playing publisher) against an engine that already compiles clean of Linux assumptions. On top land: a typed settings façade, a module-descriptor registry (`module.<id>.enabled` in the settings table) proving itself by gating MPRIS, and a thin toast/dialog consolidation inside the GTK crate.

**Tech Stack:** Rust 2021 — same dependency set as today, no additions, now *partitioned*: `reprise-core`: rusqlite (bundled SQLite), lofty, walkdir, notify 8 (cross-platform: inotify/FSEvents/ReadDirectoryChangesW), serde/serde_json, fastrand, tracing, dirs, thiserror, async-channel. `reprise-platform-linux`: gstreamer-rs 0.25 (playbin3), zbus 5 (blocking). `reprise-gnome`: gtk4-rs 0.11.4 (v4_22), libadwaita-rs 0.9.2 (v1_9), tracing-subscriber.

## Current state (verified 2026-07-12, HEAD 7880b4b + field-fix/close-out commits)

- Single binary crate, no `lib.rs`. 366 tests passing + 1 ignored (`cargo test`, 0.25 s).
- GTK-free already (verified by grep, only `gst::glib` — gstreamer's own re-export, **not** gtk4): `db.rs`, `models.rs`, `queries.rs`, `queue.rs`, `format.rs`, `view_source.rs`, `player.rs`, `mpris/{mod,state}.rs`, `library/{scanner,scanner_tests,playlists,settings,watcher,m3u,stats,mod}.rs`. The only gtk4/adw importers are `main.rs` and `src/ui/*` — **nothing blocks a clean core boundary**.
- Two loose ends the split must resolve: `mpris/mod.rs` imports `crate::APP_ID` (a const in `main.rs`), and `main.rs::db_path()` is the on-disk DB location every frontend must share.
- **Platform-seam inventory (verified 2026-07-12, this revision):** the frontend consumes `Player` through exactly **five methods** — `play(&str)`, `toggle_pause()`, `seek_to(i64)`, `set_volume(f64)`, `stop()` — plus the event callback passed to `Player::new` (`PlayerEvent`: `StateChanged`/`Position`/`TrackFinished`/`Error`). `PlayerError` is consumed at exactly **two sites** (`player_controller.rs:153` import, `:265` `new()` return type); nothing outside `player.rs` matches on its variants. The MPRIS surface the frontend consumes is entirely **handle-shaped**: `mpris::start()` returns `(SharedMprisState, Receiver<MprisCommand>, Sender<i64 /*µs*/>)`; the controller writes state snapshots into the mutex and drains the command channel — no method calls on any MPRIS object. `src/ui/` contains **zero** gstreamer code (one doc *comment* in `shortcuts.rs:99` only), so the frontend crate needs no gstreamer dependency at all after the seam. `mpris/state.rs` (610 lines, tests from 330) splits cleanly: platform-neutral state/command types + pure predicates vs D-Bus wire helpers (`build_metadata`/`track_object_path` use `zbus::zvariant` types and must go platform-side).
- Oversized files (waived DoD gate): `src/ui/track_list.rs` 1744 (tests start ~1533), `src/ui/window.rs` 1060, `src/queries.rs` 2673 (tests start at 1158 → 1157 non-test). `library/playlists.rs` (1242, tests from 550) and `queue.rs` (1220, tests from 459) are ~55–60 % tests and inside the non-test gate — **not** split here.
- Ledger debt folded in: `mark_vanished_under_root` full-table scan per watcher reconcile; settings access layer; `DEFAULT_VOLUME` dedup is already done (lives in `mpris/state.rs`).

## Repository & frontend strategy (layout named for the future; only the Linux/GNOME column is built now)

Single monorepo, one Cargo workspace for the core and all Rust frontends:

- `crates/reprise-core` — pure cross-platform engine (this plan).
- `crates/reprise-platform-linux` — Linux implementations of the two platform seams: GStreamer `PlaybackBackend`, MPRIS media integration (this plan).
- `crates/reprise-gnome` — the current GTK4/libadwaita frontend, renamed from `reprise`; ships `[[bin]] name = "reprise"` so the binary, app id, and `.desktop` story are untouched (this plan).
- **FUTURE, not built now:** `crates/reprise-kde` (Qt/Kirigami via cxx-qt; reuses `reprise-platform-linux` unchanged — this reuse is the main argument for the separate platform crate), `crates/reprise-platform-macos` / `crates/reprise-platform-windows` (CoreAudio/AVFoundation + MPNowPlayingInfoCenter; WASAPI + SMTC), `crates/reprise-ffi` (UniFFI/C-ABI bridge for non-Cargo frontends), `frontends/macos/` (Xcode/SwiftUI consuming `reprise-ffi`). CI: path-filtered pipelines per frontend once a second frontend exists.

Naming decision & rationale: the frontend crate is renamed `reprise-gnome` (rather than keeping `reprise` and documenting siblings) because (a) the workspace root directory already carries the product name, (b) an unqualified `reprise` crate next to `reprise-kde` would permanently read as "the real one", and (c) the rename is free *now* (one manifest + one workspace member line; the binary name stays `reprise`) and expensive later (docs, packaging references, muscle memory). The Linux platform code gets its **own crate** rather than a module/feature of the GNOME frontend because a future `reprise-kde` runs on the *same* platform pair — GStreamer + MPRIS — and must be able to depend on it without depending on (or forking) the GNOME frontend; the dependency graph then states the truth: `reprise-gnome → {reprise-core, reprise-platform-linux}`, `reprise-platform-linux → reprise-core`.

## Global Constraints

- **Behavior-preserving throughout.** No schema migration, no SQL-semantics change, no UI-visible change, no string changes (incl. log/error/D-Bus-visible strings — the `PlaybackError` mapping in Task 5 must keep Display output byte-identical). Each task's safety net: **all 366 existing tests pass unchanged** (test count may only grow; never edit an existing test's expectations — if a split moves a test file, the moved tests must be byte-identical apart from `use` paths).
- **Gates per commit** (stage-3 convention, all must pass before every commit): `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo audit` (known accepted advisory: RUSTSEC-2024-0436 `paste` via lofty).
- **Headless smoke run per task** (never open a window on the desktop): `dbus-run-session -- xvfb-run -a env REPRISE_SMOKE_QUIT=1 cargo run` → exit code 0, no `ERROR` lines in output.
- **No new dependencies.** Dependency edits are limited to *partitioning* the existing set among the three crates in Task 6. `uniffi`/FFI tooling is explicitly FUTURE — not added by this plan.
- **File-size gate:** every file created or substantially edited by this plan ends <800 lines.
- English for all code, comments, log messages, and commit messages. No commit attribution footer (disabled globally). **Do not push;** the controller reviews each task.
- The curated `[lints.clippy]` set (6 pedantic lints) must keep applying to *all* code after the workspace split (moves to `[workspace.lints]`).

## Explicitly NOT doing (over-abstraction traps — judged, not forgotten)

1. **No widget-adapter layer over libadwaita.** After Task 6 the compiler guarantees adw types cannot leak into the core; after Tasks 2–3 the structural adw widgets (`NavigationSplitView`, `ToolbarView`, `HeaderBar`, `StatusPage`, `WindowTitle`) each live in 1–2 frontend files. An adw 1.x→2.x break will rewrite frontend layout code no matter what; a wrapper that mirrors the adw API 1:1 would just add a second place to rewrite. The crate boundary **is** the version seam. Only the two genuinely repeated adw patterns (toasts ×14 construction sites, name-prompt/confirm `AlertDialog`s duplicated across ≥3 files) get thin helpers (Task 9).
2. **No abstract "frontend toolkit trait".** The Transmission model is: core = full engine with a plain Rust API; each frontend is a complete client. Transmission's GTK and Qt clients share zero widget abstraction. Same here. Note the deliberate asymmetry with Task 5's platform seams: UI toolkits vary **per frontend** and share nothing worth abstracting; audio/media-integration vary **per OS**, have a narrow surface (5 methods + 3 handles, measured), and porting them is now a stated product requirement — one is speculation, the other is design-to-requirement.
3. **No `Module` trait / dyn-dispatch plugin lifecycle yet.** The spec plans it for stage 5 with EQ + ReplayGain (GStreamer pipeline elements that genuinely support live insert/remove). Today there would be exactly one implementor (MPRIS), whose lifecycle is thread-spawn-at-startup — a trait designed around one awkward case is a bad trait. Task 8 lands the *substrate* stage 5 needs (descriptors, persisted enable flags, the Plugins-list data source, one really-gated module) and defers the trait until it has ≥2 real implementors. **Reconciliation with the new `PlaybackBackend` trait (Task 5), since "trait with one implementor" superficially describes both:** `PlaybackBackend` is justified where `Module` is not, on all three axes that matter — (a) *stated requirement:* cross-platform ports are a user-decided product goal with named future implementors at a known variation point (GStreamer / AVFoundation / WASAPI); no one has asked for a second `Module`; (b) *derived vs invented surface:* the backend trait's five methods are copied from live, load-tested call sites, whereas a `Module` lifecycle would be invented around MPRIS's one awkward spawn-and-forget case; (c) *cost of being wrong:* if a future platform needs a sixth method, adding it is additive; a wrong `Module` lifecycle would have every stage-5 plugin built on it. Design-to-requirement, not speculation.
4. **No extension-point registry** (sidebar entries, context-menu actions, settings pages, pipeline slots). Zero consumers until stage 5's Plugins UI and modules exist. Defining hook signatures now with no caller would be speculation the first real module would immediately invalidate.
5. **No split of `library/playlists.rs` / `queue.rs`** (test-heavy, non-test portions ~550/~460 lines, cohesive) and **no async-runtime / message-bus rearchitecture** (callbacks + `async-channel` + `glib::MainContext` work and are load-tested).
6. **No macOS/Windows/KDE backends or stubs now — only the Linux implementation plus the seam.** No `reprise-platform-macos`/`-windows` crates, no mock/null `PlaybackBackend` (the existing fakesink path *is* the headless test double, unchanged), no `reprise-ffi`/UniFFI bridge, no `frontends/` directory, no cross-compile CI. The seam is defined because the variation is a stated requirement; the variants are built when they are. Corollary: the trait surface is the **minimum the current GNOME app already uses** — no `preload()`, no gapless hooks, no `Rate` control, no capability flags. Also **no renaming of the MPRIS-derived state/command types** (`MprisState`, `MprisCommand`, `MprisPlaybackStatus`) to platform-neutral names in this plan: the semantics are already platform-neutral (SMTC and MPNowPlayingInfoCenter have the same shape) and the rename is a mechanical IDE operation, but doing it now would edit test bodies (violating the byte-identical-move rule) and double the diff of the seam task for zero behavior. Accepted, documented naming debt — do it the day a second platform implementation lands and can inform the neutral names.
7. **No moving of controller-side orchestration into the core (decided & documented).** `PlayerController`/`mpris_mirror` logic (event marshalling, skip-on-failure, mirror updates) stays in the GNOME frontend: it is inseparably interwoven with GTK main-loop marshalling (`glib::spawn_future_local`, `Rc`/`RefCell`, `PlayerBar`), and extracting it is a rearchitecture, not a behavior-preserving move. This matches the Transmission model — each client owns its orchestration; what the core owns is the engine (db/queries/queue/scanner) plus the platform *contracts* (Task 5's trait, handle types, and state/command vocabulary), which is exactly what a second frontend reuses first. The frontend is the **composition root**: it alone names `reprise_platform_linux` concrete types, at construction only; every subsequent call goes through the core trait/handles. Revisit only when a second frontend actually exists.

## Ordering & parallelization

**Order: file splits (1–3), the SQL prefilter (4), and the platform-seam extraction (5) first — in parallel if desired (disjoint files) — then the workspace split (6), then settings façade (7) → module registry (8), with adw consolidation (9) last.**

Rationale: Tasks 1–5 are intra-crate and mechanically verifiable, and doing them first means Task 6's big commit is a near-pure `git mv` of already-final files — the riskiest structural change in the plan then contains *no content restructuring to review at the same time*, and `git log --follow` stays readable. This is why the seam extraction (5) sits **before** the workspace split rather than inside it: extracting `PlaybackBackend`/the media-integration contract is the one genuinely content-editing move in the plan, and it must be reviewable on its own inside the single crate (where `cargo test` and the busctl E2E pin behavior with zero path churn), so that Task 6 stays a pure re-homing of files whose contents are already final. The alternative (split first, extract after) would put the gstreamer/zbus dependencies into `reprise-core` temporarily and then need a second dependency-surgery commit — churning manifests twice and leaving an intermediate state that violates the portability goal.

- **Independent / parallelizable:** Tasks 1, 2, 3, 4, 5 (disjoint files: `queries.rs` / `ui/track_list*.rs` / `ui/window.rs` / `library/scanner.rs` / `player.rs`+`mpris/`+`ui/{player_controller,mpris_mirror,player_controller_wiring,playback_faults}.rs`).
- **Strictly ordered:** 6 after 1–5 (pure-move discipline). 7 after 6 (lands in `reprise-core`). 8 after 7 (uses `get_bool`) and after 5 (gates through the seam's handle contract). 9 after 2, 3 and 6 (touches the split UI files at their new paths); 9 is independent of 7–8 and may run in parallel with them.

---

### Task 1: Split `queries.rs` into a `queries/` module directory

**Files:**
- Delete: `src/queries.rs` (2673 lines)
- Create: `src/queries/mod.rs`, `src/queries/clauses.rs`, `src/queries/library.rs`, `src/queries/playlist.rs`, `src/queries/smart.rs`, `src/queries/queue.rs`, `src/queries/maintenance.rs`, `src/queries/tests.rs`
- Modify: nothing else — **every external `crate::queries::X` path must keep compiling unchanged** (re-exports in `mod.rs`).

**Interfaces:**
- Consumes: current `src/queries.rs` content (function inventory below).
- Produces: identical public API under `crate::queries::…`. Additionally `pub(crate) use library::playlists::…`-style visibility bumps only where the compiler demands them. Task 6 moves this directory verbatim into `reprise-core`.

Function → file manifest (move each item *with its doc comment*, byte-identical bodies):

| Destination | Items (from current `queries.rs`) |
|---|---|
| `mod.rs` | module docs; `MAX_WINDOW_LIMIT` and any other top consts; `LibraryStats`, `TrackSummary`, `ImportErrorRow` structs; the public dispatchers that `match` on `ViewSource`: `query_track_window`, `query_track_count`, `query_track_ids`; `query_library_stats`; `mod clauses; mod library; mod playlist; mod smart; mod queue; mod maintenance;` + `pub use` re-exports; `#[cfg(test)] mod tests;` |
| `clauses.rs` | `filter_clause`, `like_pattern`, `order_expr_and_dir`, `build_track_query_base`, `build_track_query`, `build_track_ids_query_base`, `build_track_ids_query`, `row_to_track`, `row_to_playlist_track`, `row_to_id` |
| `library.rs` | `query_track_window_library`, `query_track_window_missing`, `query_track_count_library`, `query_track_count_missing` |
| `playlist.rs` | `build_playlist_track_query`, `query_track_window_playlist`, `query_track_count_playlist`, `query_track_ids_playlist`, `query_playlist_tracks_full` |
| `smart.rs` | `load_smart_playlist`, `build_smart_window_query`, `query_track_window_smart`, `query_track_count_smart`, `query_track_ids_smart` |
| `queue.rs` | `query_track_window_queue`, `query_track_count_queue`, `query_track_ids` queue arm helpers if any, `is_queue_capped`, `QUEUE_LIMIT` |
| `maintenance.rs` | `mark_track_missing`, `remove_missing_track`, `remove_missing_tracks`, `query_track_summary`, `track_id_for_path`, `query_import_error_count`, `query_import_errors`, `delete_import_error` |
| `tests.rs` | the entire current `#[cfg(test)] mod tests` body (lines ≥1158), unchanged except `use` paths (`use super::*;` plus explicit `use super::clauses::{filter_clause, like_pattern, …};` for formerly-private helpers) |

- [ ] **Step 1: Record the baseline.** Run `cargo test 2>&1 | tail -1` — expect `366 passed; 0 failed; 1 ignored`. Copy the exact list of test names in queries' test module for later diffing: `cargo test queries:: 2>&1 | grep '^test ' | sort > /tmp/queries-tests-before.txt`.
- [ ] **Step 2: Create the directory and move code per the manifest.** `mkdir src/queries`. Cut each item listed above into its destination file. In `mod.rs`, re-export so external paths are unchanged, e.g.:

```rust
mod clauses;
mod library;
mod maintenance;
mod playlist;
mod queue;
mod smart;

pub use maintenance::{
    delete_import_error, mark_track_missing, query_import_error_count, query_import_errors,
    query_track_summary, remove_missing_track, remove_missing_tracks, track_id_for_path,
};
pub use queue::is_queue_capped;
// (extend with every other currently-`pub` item; the compiler lists the rest)
```

Formerly-private helpers now crossing a submodule boundary get `pub(super)` (visible inside `queries/` only); nothing gains wider visibility than it has today unless a call site outside `queries` already used it.
- [ ] **Step 3: Compile-fix visibility only.** `cargo build` — resolve errors *exclusively* by adding `pub(super)`/`use` lines, never by editing bodies.
- [ ] **Step 4: Verify tests are unchanged.** `cargo test` → `366 passed; 1 ignored`. Then `cargo test queries:: 2>&1 | grep '^test ' | sort > /tmp/queries-tests-after.txt && diff /tmp/queries-tests-before.txt /tmp/queries-tests-after.txt` — expect: names identical modulo the `queries::tests::` path segment (inspect the diff; only path prefixes may differ).
- [ ] **Step 5: Line-count gate.** `wc -l src/queries/*.rs` — every file <800 (tests.rs may approach it; if `tests.rs` >800, split it mechanically into `tests.rs` + `tests_smart.rs` along the existing inner-mod boundaries).
- [ ] **Step 6: Full gate battery + smoke** (Global Constraints commands). All green.
- [ ] **Step 7: Commit.**

```bash
git add -A src/queries src/queries.rs
git commit -m "refactor: split queries.rs into per-source query modules"
```

---

### Task 2: Split `ui/track_list.rs` into focused siblings

**Files:**
- Modify: `src/ui/track_list.rs` (1744 → ~600: keeps `Shared`, `TrackList` + its `new()`/public setters, `EmptyState`/`empty_state_for` + `empty_state_tests`, `set_filter_and_reload`, `set_source_and_reload`, `notify_import_errors_mutated_and_reload`, `apply_empty_state`)
- Create: `src/ui/track_list_columns.rs` (← `append_column`, `append_rating_column`, `on_rating_changed`, `build_status_page`)
- Create: `src/ui/track_list_sort.rs` (← `SortState` + its `Default` impl, `wire_sort_clicks`, `on_sorter_changed`, `default_sort_for_source`, `resolve_sort_on_switch`, plus test mods `default_sort_for_source_tests` and `resolve_sort_on_switch_tests`)
- Create: `src/ui/track_list_activation.rs` (← `wire_activate`, `queue_ids_for_activation`, `current_queue_ids`)
- Create: `src/ui/track_list_smoke.rs` (← `arm_smoke_activate`, `arm_smoke_filter`, `arm_smoke_source`, `arm_smoke_sort_column`, `parse_smoke_source`, `resolve_smoke_source_playlist_by_name`)
- Modify: `src/ui/mod.rs` (add the four `pub mod` lines)

**Interfaces:**
- Consumes: `Shared` (already `pub(super)` — visible to all `ui/` siblings; bump individual *fields* to `pub(super)` only where the compiler demands).
- Produces: unchanged `TrackList` public API; sibling functions become `pub(super)` so `track_list.rs` (and existing users like `track_list_dnd.rs`) call them as `super::track_list_sort::resolve_sort_on_switch(…)` — or keep current call-site paths via `pub(super) use track_list_sort::SortState;` re-exports inside `track_list.rs` where that is fewer edits.

- [ ] **Step 1: Baseline.** `cargo test 2>&1 | tail -1` → `366 passed; 1 ignored`.
- [ ] **Step 2: Move per the manifest above.** Bodies and doc comments byte-identical; each moved test mod moves with its subject. Add to `src/ui/mod.rs`:

```rust
pub mod track_list_activation;
pub mod track_list_columns;
pub mod track_list_smoke;
pub mod track_list_sort;
```

- [ ] **Step 3: Compile-fix visibility/`use` only** (same discipline as Task 1 Step 3). Existing sibling files (`track_list_context_menu.rs`, `track_list_dnd.rs`, `track_list_model.rs`, `track_list_dnd_smoke.rs`) may need one-line `use` updates — nothing else.
- [ ] **Step 4: Tests unchanged.** `cargo test` → `366 passed; 1 ignored` (names may re-path under the new modules; count identical).
- [ ] **Step 5: Line gate.** `wc -l src/ui/track_list*.rs` — all <800 (`track_list.rs` target ~600).
- [ ] **Step 6: Gate battery + smoke.** Additionally exercise the moved smoke hooks headlessly, since they are dev-only and not unit-tested: `dbus-run-session -- xvfb-run -a env REPRISE_SCAN_DIR=$(mktemp -d) REPRISE_SMOKE_QUIT=1 cargo run` → exit 0, and one run with `REPRISE_SMOKE_FILTER=zzz` → log line for the NoResults empty state appears as before.
- [ ] **Step 7: Commit.**

```bash
git add src/ui
git commit -m "refactor: split track_list.rs into columns, sort, activation, and smoke modules"
```

---

### Task 3: Extract the scan flow from `ui/window.rs`

**Files:**
- Modify: `src/ui/window.rs` (1060 → ~600: keeps `build()` shell, `wire_sidebar_toggle`, `wire_search`)
- Create: `src/ui/scan_flow.rs` (← `wire_scan_button`, `trigger_rescan_of_library_root`, `spawn_scan`, `run_scan`, `arm_smoke_rescan`, `start_or_restart_watcher`, plus the scan-related consts/env-var names they use, e.g. `SMOKE_RESCAN` hook const — move each const with its single consumer)
- Modify: `src/ui/mod.rs` (`pub mod scan_flow;`)

**Interfaces:**
- Consumes: `main.rs::db_path` value passed through `build()` (unchanged), `library::{scanner, settings, watcher}`.
- Produces: `pub(super) fn wire_scan_button(…)`, `pub(super) fn trigger_rescan_of_library_root(…)`, `pub(super) fn start_or_restart_watcher(…)` with **exactly their current signatures** (copy them verbatim from window.rs when moving); `window::build` calls them via `super::scan_flow::…`.

- [ ] **Step 1: Baseline.** `cargo test 2>&1 | tail -1` → `366 passed; 1 ignored`.
- [ ] **Step 2: Move the six functions + their consts** into `src/ui/scan_flow.rs`, byte-identical bodies; add the mod line; update call sites inside `build()`.
- [ ] **Step 3: Compile-fix visibility/`use` only.**
- [ ] **Step 4: Tests + gates + smoke.** `cargo test` → `366 passed; 1 ignored`. Watcher-specific headless check (the take-old-before-start ordering from E3 Task 9 must survive the move): `dbus-run-session -- xvfb-run -a env REPRISE_SCAN_DIR=$(mktemp -d) REPRISE_SMOKE_RESCAN=1 REPRISE_SMOKE_QUIT=1 REPRISE_SMOKE_QUIT_DELAY_SECS=5 cargo run` → exit 0, log shows watcher armed once, no `ERROR`.
- [ ] **Step 5: Line gate.** `wc -l src/ui/window.rs src/ui/scan_flow.rs` — both <800.
- [ ] **Step 6: Commit.**

```bash
git add src/ui
git commit -m "refactor: extract scan flow and watcher wiring from window.rs"
```

---

### Task 4: SQL path prefilter for `mark_vanished_under_root`

Closes the ledger item "full-table scan per reconcile": every watcher debounce currently reads **all** non-missing rows and filters in Rust. A `LIKE`-prefix narrows candidates in SQL; the existing component-wise `Path::starts_with` check stays as the authoritative filter, so semantics are *provably identical* (this is a perf refactor — the new tests are regression nets that are green before and after, not RED/GREEN; state that plainly in the commit body if asked).

**Files:**
- Modify: `src/library/scanner.rs:507` (`mark_vanished_under_root`)
- Modify (visibility only, if needed): `src/library/playlists.rs:300` (`escape_like` is already `pub`)
- Test: `src/library/scanner_tests.rs`

**Interfaces:**
- Consumes: `crate::library::playlists::escape_like(&str) -> String` (existing).
- Produces: unchanged signature `pub fn mark_vanished_under_root(conn: &Connection, root: &Path) -> Result<u32, ScanError>`.

- [ ] **Step 1: Write the regression-net tests** in `scanner_tests.rs` (they must pass on current code too — run them once before changing the implementation):

```rust
#[test]
fn mark_vanished_ignores_sibling_root_with_common_string_prefix() {
    // "/music" reconcile must not touch rows under "/music2" — the SQL
    // prefilter's pattern is "<root>/%", never a bare string prefix.
    let (conn, dir) = scanned_fixture_conn(); // existing helper pattern in this file
    let sibling = dir.path().with_file_name(format!(
        "{}2",
        dir.path().file_name().unwrap().to_string_lossy()
    ));
    std::fs::create_dir_all(&sibling).unwrap();
    insert_raw_track(&conn, sibling.join("gone.flac")); // row whose file never existed
    let marked = mark_vanished_under_root(&conn, dir.path()).unwrap();
    assert_eq!(missing_count(&conn), 0, "sibling-root row must not be marked");
    let _ = marked;
}

#[test]
fn mark_vanished_treats_like_metacharacters_in_root_literally() {
    // A root containing '_' (LIKE single-char wildcard) must not widen the
    // candidate set: "a_b" must not match rows under "axb".
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("a_b");
    let decoy = base.path().join("axb");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&decoy).unwrap();
    let conn = migrated_conn();
    insert_raw_track(&conn, decoy.join("gone.flac"));
    mark_vanished_under_root(&conn, &root).unwrap();
    assert_eq!(missing_count(&conn), 0);
}
```

(Adapt `scanned_fixture_conn` / `insert_raw_track` / `missing_count` to the helpers that already exist in `scanner_tests.rs`; write tiny local helpers if there is no exact match — a raw `INSERT INTO tracks (path, missing, …)` is fine, no audio fixture needed since the file never existed.)
- [ ] **Step 2: Run the new tests on the unchanged implementation.** `cargo test mark_vanished -- --nocapture` → PASS (regression net confirmed green-before).
- [ ] **Step 3: Add the prefilter.** Replace the candidate query in `mark_vanished_under_root`:

```rust
    // Perf (Queue-C ledger item): narrow candidates in SQL instead of
    // streaming the whole table through Rust on every watcher reconcile.
    // The pattern is "<root>/%" with LIKE metacharacters escaped, so it can
    // never match a *sibling* root sharing a string prefix ("/music" vs
    // "/music2"). The component-wise starts_with() below remains the
    // authoritative check — the LIKE only shrinks the candidate set, it
    // never decides membership, so semantics are byte-identical to the
    // pre-filter implementation (including exotic-UTF-8 and trailing-slash
    // edges).
    let root_str = root.to_string_lossy();
    let pattern = format!(
        "{}/%",
        crate::library::playlists::escape_like(root_str.trim_end_matches('/'))
    );
    let candidates: Vec<(i64, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, path FROM tracks WHERE missing = 0 AND path LIKE ?1 ESCAPE '\\'",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![pattern], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        rows
    };
```

(Keep the existing per-row `starts_with` + `exists()` loop and UPDATE untouched. Match the `ESCAPE` character to whatever `escape_like` emits — check its implementation in `playlists.rs:300` and reuse the same escape char the smart-rules SQL uses.)
- [ ] **Step 4: All tests.** `cargo test` → `368 passed; 1 ignored` (366 + the 2 new). Existing watcher E2E ordering tests must be untouched.
- [ ] **Step 5: Gate battery + smoke** (watcher variant from Task 3 Step 4).
- [ ] **Step 6: Commit.**

```bash
git add src/library/scanner.rs src/library/scanner_tests.rs
git commit -m "perf: prefilter mark_vanished_under_root candidates in SQL"
```

---

### Task 5: Extract the platform seam — `PlaybackBackend` + media-integration contract in core-destined modules; GStreamer/MPRIS become the Linux implementation (in place)

**NEW in this revision — the move that makes cross-platform portability real.** Audio playback and OS media integration are the two subsystems every platform implements differently (Linux: GStreamer + MPRIS; macOS: CoreAudio/AVFoundation + MPNowPlayingInfoCenter; Windows: WASAPI + SMTC). This task defines the two contracts in platform-neutral modules and re-points the frontend at them, **entirely inside the current single crate** so `cargo test`, the fakesink E2E, and the busctl introspection pin behavior with zero path churn. Task 6 then re-homes the finished files into their crates as pure moves.

The two contracts have deliberately different shapes, each matching how the frontend *already* consumes the subsystem (measured — see "Platform-seam inventory" in Current state):

- **Playback = a real trait.** The controller calls the player polymorphically at many sites (controller, mirror, wiring, shortcuts, fault handling); a `Box<dyn PlaybackBackend>` field makes every one of those calls compiler-proof that the five-method surface suffices.
- **Media integration = a data contract, not a trait.** The frontend never calls methods on an MPRIS object — the entire runtime interface is already three handles (shared state mutex the app writes, command channel the app drains, seek-notify channel the app feeds). A one-method `trait MediaIntegration { fn start(…) -> Handles }` called exactly once at startup would be dyn ceremony with zero call sites benefiting. The contract is therefore the **handle struct + state/command types in core**; each platform provides a `start(…) -> MediaIntegrationHandles` constructor. (The seek-notify channel keeps its existing µs unit: it is just a fixed unit choice, works for every platform, and changing it to ms would edit existing test expectations — forbidden.)

**Files:**
- Create: `src/playback.rs` (core-destined): `PlaybackState`, `PlayerEvent` (moved verbatim from `player.rs`), new `PlaybackError`, new `trait PlaybackBackend`.
- Create: `src/media_integration.rs` (core-destined): moved from `mpris/state.rs` — `MprisState`, `MprisCommand`, `MprisPlaybackStatus`, `DEFAULT_VOLUME`, `can_play`/`can_pause`/`can_seek`, `metadata_differs`, `ms_to_micros`/`micros_to_ms` (+ their tests, byte-identical apart from `use` paths); moved from `mpris/mod.rs` — `pub type SharedMprisState`, `read_state`; new `pub struct MediaIntegrationHandles`.
- Modify: `src/player.rs` (Linux GStreamer impl: keeps `Player`, `path_to_uri`, `build_playbin`, `attach_bus_watch`, the wedged-pipeline recovery, all existing tests; gains `impl PlaybackBackend for Player`, drops the local `PlaybackState`/`PlayerEvent`/`PlayerError` definitions)
- Modify: `src/mpris/state.rs` (shrinks to D-Bus wire helpers only: `build_metadata`, `track_object_path` — both use `zbus::zvariant` — plus `loop_status_to_repeat`/`repeat_to_loop_status` (MPRIS wire strings) and their tests)
- Modify: `src/mpris/mod.rs` (`start(desktop_entry: &'static str) -> MediaIntegrationHandles`; drops `use crate::APP_ID`)
- Modify: `src/ui/player_controller.rs` (field `player: Box<dyn PlaybackBackend>`; `new()` error type; passes `crate::APP_ID` to `mpris::start`), `src/ui/mpris_mirror.rs`, `src/ui/player_controller_wiring.rs`, `src/ui/playback_faults.rs` (import-path updates only, compiler-driven)
- Modify: `src/main.rs` (`mod playback; mod media_integration;`)

**Interfaces:**
- Consumes: the existing `Player`/`mpris` call sites (the trait is **derived**, not designed — every method signature below is copied from the current `impl Player`).
- Produces (core contract, `src/playback.rs`):

```rust
/// The audio-playback contract every platform implements (Linux: GStreamer
/// playbin3 in `player.rs`; future macOS/Windows: AVFoundation / WASAPI —
/// see "Repository & frontend strategy"). Surface = exactly what the
/// GNOME frontend consumes today, nothing speculative. Event delivery is a
/// construction-time concern, not a trait method: each concrete backend
/// takes a `Box<dyn Fn(PlayerEvent) + Send + Sync>` callback in its own
/// constructor and may invoke it from any thread; frontends marshal.
pub trait PlaybackBackend {
    fn play(&self, path: &str) -> Result<(), PlaybackError>;
    fn toggle_pause(&self) -> Result<PlaybackState, PlaybackError>;
    fn seek_to(&self, position_ms: i64) -> Result<(), PlaybackError>;
    fn set_volume(&self, volume: f64);
    fn stop(&self) -> Result<(), PlaybackError>;
}

/// Platform-neutral playback error. `Backend`'s message is produced by the
/// platform impl and shown to users as-is (toasts/logs) — the Linux impl
/// formats "GStreamer: {source}" into it so user-visible strings stay
/// byte-identical to the pre-seam `PlayerError::Gst` Display output.
#[derive(Debug, thiserror::Error)]
pub enum PlaybackError {
    #[error("{0}")]
    Backend(String),
    #[error("invalid path: {0}")]
    BadPath(String),
}
```

- Produces (core contract, `src/media_integration.rs`):

```rust
/// The OS media-integration contract (Linux: MPRIS/D-Bus in `mpris/`;
/// future macOS/Windows: MPNowPlayingInfoCenter / SMTC). Handle-shaped, not
/// a trait: the app writes now-playing snapshots into `shared_state`,
/// drains OS transport commands from `commands`, and feeds `seek_notify`
/// (µs) after every successful seek; the platform side owns whatever
/// threads/connections it needs. Each platform crate provides a
/// `start(…) -> MediaIntegrationHandles` constructor.
pub struct MediaIntegrationHandles {
    pub shared_state: SharedMprisState,
    pub commands: async_channel::Receiver<MprisCommand>,
    pub seek_notify: async_channel::Sender<i64>,
}
```

- Produces (Linux impl): `impl PlaybackBackend for Player` (bodies = today's methods, moved under the impl block verbatim); `mpris::start(desktop_entry: &'static str) -> MediaIntegrationHandles` (same behavior, `DesktopEntry` served from the parameter; `MprisRoot` gains a `desktop_entry` field). Rationale for the parameter: the desktop-entry name belongs to the frontend (a KDE frontend ships its own `.desktop`); `APP_ID` stays a frontend const — this severs the last `crate::`-upward tie from the platform code, so Task 6 needs no content edits here.

- [ ] **Step 1: Baseline + call-site inventory proof.** `cargo test 2>&1 | tail -1` → expected count from prior tasks (368 if Task 4 landed first; record actual). Re-verify the derived surface is complete and minimal: `grep -rn 'player\.\|Player::' src/ui/ | grep -vE '//|player_bar|player_controller_wiring\b'` and confirm only the five methods + `Player::new` appear; `grep -rn 'PlayerError' src/ --include='*.rs'` → confirm only the two `player_controller.rs` sites outside `player.rs` (no variant matches anywhere). Paste both grep outputs into the task report.
- [ ] **Step 2: Create `src/playback.rs`.** Move `PlaybackState` and `PlayerEvent` out of `player.rs` byte-identically (with doc comments); add `PlaybackError` and `trait PlaybackBackend` exactly as specified above. `main.rs` gains `mod playback;`.
- [ ] **Step 3: Convert `player.rs` into the Linux implementation.** `use crate::playback::{PlaybackBackend, PlaybackError, PlaybackState, PlayerEvent};` — delete the local definitions. Replace every `PlayerError::Gst(e.to_string())` construction with `PlaybackError::Backend(format!("GStreamer: {e}"))` (Display byte-identical to the old `#[error("GStreamer: {0}")]`), `PlayerError::BadPath(…)` → `PlaybackError::BadPath(…)`. Move the five public method bodies verbatim into `impl PlaybackBackend for Player { … }`; `Player::new` keeps its current signature apart from the error type (`Result<Self, PlaybackError>`). Existing tests in `player.rs` stay, unchanged apart from `use` paths and the error-type name in signatures.
- [ ] **Step 4: Create `src/media_integration.rs` and split `mpris/state.rs`.** Move the platform-neutral items per the Files manifest (bodies + tests byte-identical apart from `use` paths); wire helpers stay in `mpris/state.rs`. Split the `#[cfg(test)]` mod along the same line — the sorted-test-name diff discipline from Task 1 Step 4 applies (`cargo test 2>&1 | grep '^test ' | sort` before/after; only path prefixes may differ). `main.rs` gains `mod media_integration;`. Update `mpris/mod.rs`'s imports/re-exports; external `crate::mpris::X` consumers are re-pointed to `crate::media_integration::X` directly (≈3 files, compiler-driven — no re-export shim, since Task 6 rewrites these paths anyway).
- [ ] **Step 5: `mpris::start(desktop_entry)` + handles.** `start` takes `desktop_entry: &'static str`, threads it into `MprisRoot { desktop_entry }` (getter returns it instead of `APP_ID`), drops `use crate::APP_ID`, and returns `MediaIntegrationHandles { shared_state, commands, seek_notify }` instead of the tuple. `player_controller.rs:283` becomes `let handles = mpris::start(crate::APP_ID);` + field init from `handles.…` (field names `mpris_state`/`mpris_seek_notify` unchanged — identifier churn is not the point of this task).
- [ ] **Step 6: Re-point the controller at the trait.** `PlayerController`'s field becomes `player: Box<dyn PlaybackBackend>`; construction stays `Player::new(…)` (boxed) inside `new()` — the frontend is the composition root (Explicitly-NOT-doing #7); `new()` returns `Result<Rc<Self>, PlaybackError>`. All other edits in `ui/` are compiler-driven import updates only — the five call-site methods compile unchanged because the trait signatures are copies.
- [ ] **Step 7: New regression test — the trait surface is sufficient and dyn-dispatchable.** In `player.rs`'s test mod (it needs the fakesink pattern + `AUDIO_SINK_TEST_LOCK`):

```rust
/// Portability seam (refactor Task 5): drives play/stop through
/// `Box<dyn PlaybackBackend>` — the exact shape the controller holds — to
/// pin that the trait surface alone is enough to operate the backend.
#[test]
fn playback_backend_trait_object_drives_play_and_stop() {
    // fakesink + AUDIO_SINK_TEST_LOCK boilerplate as in
    // play_and_stop_emit_state_changed_events, then:
    let backend: Box<dyn PlaybackBackend> = Box::new(player);
    backend.play(path).unwrap();
    // …assert StateChanged(Playing), backend.stop(), assert Stopped —
    // same channel assertions as the existing test.
}
```

- [ ] **Step 8: Tests.** `cargo test` → prior count **+1** (moved tests identical per the sorted-name diff; one new dyn-dispatch test). Never edit an existing test's expectations.
- [ ] **Step 9: Gate battery + full smoke incl. MPRIS through the seam.** Standard smoke, plus:

```bash
dbus-run-session -- sh -c 'xvfb-run -a env REPRISE_SMOKE_QUIT=1 REPRISE_SMOKE_QUIT_DELAY_SECS=6 cargo run & sleep 4; busctl --user introspect org.mpris.MediaPlayer2.reprise /org/mpris/MediaPlayer2; wait'
```

Expected: introspection lists both interfaces exactly as before (including `DesktopEntry`, now sourced from the parameter); app exits 0, no `ERROR` lines. This is the proof that the Linux backend still works **through the trait/handle contract**.
- [ ] **Step 10: Line gate.** `wc -l src/playback.rs src/media_integration.rs src/player.rs src/mpris/*.rs` — all <800.
- [ ] **Step 11: Commit.**

```bash
git add src/playback.rs src/media_integration.rs src/player.rs src/mpris src/ui src/main.rs
git commit -m "refactor: extract PlaybackBackend and media-integration contracts; GStreamer/MPRIS become the Linux implementation"
```

---

### Task 6: Workspace split — `reprise-core` (pure lib) + `reprise-platform-linux` + `reprise-gnome`

The load-bearing portability move, now a near-pure re-homing of files Task 5 already finalized. After this task, **`cargo tree -p reprise-core` contains no gtk4, no libadwaita, no gstreamer, and no zbus** — the whole engine (DB + migrations, scanner/watcher/playlists/M3U/settings, the `ViewSource` query layer, the queue engine, formatting, and both platform *contracts*) compiles from cross-platform crates only (as a side effect, even the `glib` crate vanishes from core's tree — it only ever came in via gstreamer). A macOS or Windows port starts from a core that already builds clean; a KDE frontend depends on `reprise-core` + `reprise-platform-linux` and never sees GTK.

**Files:**
- Create: `Cargo.toml` (workspace root — replaces current package manifest), `crates/reprise-core/Cargo.toml`, `crates/reprise-core/src/lib.rs`, `crates/reprise-platform-linux/Cargo.toml`, `crates/reprise-platform-linux/src/lib.rs`, `crates/reprise-gnome/Cargo.toml`
- Move (git mv, content-identical except the edits listed in Step 4):
  - → `crates/reprise-core/src/…`: `src/{db,models,format,queue,view_source,playback,media_integration}.rs`, `src/queries/`, `src/library/`
  - → `crates/reprise-platform-linux/src/…`: `src/player.rs`, `src/mpris/`
  - → `crates/reprise-gnome/src/…`: `src/main.rs`, `src/ui/`
  - → `crates/reprise-platform-linux/tests/fixtures/`: `tests/fixtures/sine.flac` (the player E2E tests locate it via `env!("CARGO_MANIFEST_DIR")`, which now resolves to the platform crate — move the fixture with its tests; if other crates' tests also use fixtures, check `grep -rn 'tests/fixtures' src/` first and keep/copy per consumer)
- Modify: `crates/reprise-gnome/src/main.rs` + every `src/ui/*.rs` (`use crate::queries` → `use reprise_core::queries`, `use crate::player` → `use reprise_platform_linux::player`, etc. — mechanical sweep)

**Interfaces:**
- Consumes: all prior tasks' final file layout.
- Produces: crate `reprise-core` (lib name `reprise_core`) exposing `pub mod db, format, library, media_integration, models, modules (Task 8), playback, queries, queue, view_source;` plus `db::default_path() -> PathBuf` (moved `db_path`). Crate `reprise-platform-linux` (lib name `reprise_platform_linux`) exposing `pub mod player, mpris;`. Crate `reprise-gnome` (binary name `reprise` via `[[bin]]`) depending on both.

- [ ] **Step 1: Baseline.** `cargo test 2>&1 | tail -1` → count from Task 5 (369 expected if Tasks 4+5 landed; record actual).
- [ ] **Step 2: Write the four manifests.**

Root `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = [
    "crates/reprise-core",
    "crates/reprise-platform-linux",
    "crates/reprise-gnome",
]
default-members = ["crates/reprise-gnome"]

[workspace.package]
version = "0.1.0"
authors = ["Marvin Baudach"]
edition = "2021"
license = "GPL-3.0-or-later"

[workspace.lints.clippy]
needless_pass_by_value = "warn"
redundant_closure_for_method_calls = "warn"
semicolon_if_nothing_returned = "warn"
uninlined_format_args = "warn"
map_unwrap_or = "warn"
unnested_or_patterns = "warn"
```

(Carry over the existing lint-rationale comment block from the current `Cargo.toml` above the table.)

`crates/reprise-core/Cargo.toml` — **the dependency-purity contract; every entry here must be cross-platform**:

```toml
[package]
name = "reprise-core"
description = "Cross-platform, GUI-free engine for the Reprise music player: library, queue, queries, playback and media-integration contracts"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
rusqlite = { version = "0.32", features = ["bundled"] }
thiserror = "2"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
walkdir = "2"
lofty = "0.22"
dirs = "6"
tracing = "0.1"
async-channel = "2.5.0"
fastrand = "2"
notify = "8"

[dev-dependencies]
tempfile = "3"

[lints]
workspace = true
```

(Carry over the existing `notify` rationale comment verbatim — and note next to it that `notify` is deliberately kept in core because it is itself the cross-platform abstraction: inotify/FSEvents/ReadDirectoryChangesW. `async-channel` is core because `MediaIntegrationHandles` and the controller channels are typed with it. **gstreamer and zbus must NOT appear here** — that is the point of this plan revision.)

`crates/reprise-platform-linux/Cargo.toml`:

```toml
[package]
name = "reprise-platform-linux"
description = "Linux platform backends for Reprise: GStreamer playback, MPRIS media integration"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
reprise-core = { path = "../reprise-core" }
gstreamer = "0.25"
zbus = "5"
async-channel = "2.5.0"
tracing = "0.1"

[lints]
workspace = true
```

(Carry over the existing `zbus` rationale comments verbatim — they document decisions. Add `thiserror`/dev-deps only if the compiler demands them after the move — expected: not needed, `PlaybackError` construction doesn't require the derive crate.)

`crates/reprise-gnome/Cargo.toml`:

```toml
[package]
name = "reprise-gnome"
description = "A native GTK4 music player, successor to Rhythmbox — GNOME frontend"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "reprise"
path = "src/main.rs"

[dependencies]
reprise-core = { path = "../reprise-core" }
reprise-platform-linux = { path = "../reprise-platform-linux" }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
gtk4 = { version = "0.11.4", features = ["v4_22"] }
libadwaita = { version = "0.9.2", features = ["v1_9", "gtk_v4_22"] }
async-channel = "2.5.0"

[dev-dependencies]
tempfile = "3"

[lints]
workspace = true
```

(**No gstreamer here** — verified 2026-07-12: `src/ui/` contains one gstreamer *comment*, zero gstreamer code. Keep `tempfile` wherever `#[cfg(test)]` code uses it after the move; `dirs`/`serde` land core-side only.)
- [ ] **Step 3: git-mv the trees** exactly as listed in Files. `crates/reprise-platform-linux/src/lib.rs`:

```rust
//! Linux implementations of reprise-core's two platform seams: `player`
//! (GStreamer playbin3 `PlaybackBackend`) and `mpris` (D-Bus media
//! integration returning `MediaIntegrationHandles`). Any Linux frontend —
//! GNOME today, KDE/Qt later — composes these with reprise-core; macOS and
//! Windows get sibling crates implementing the same contracts (see the
//! plan's "Repository & frontend strategy").

pub mod mpris;
pub mod player;
```

`crates/reprise-core/src/lib.rs`:

```rust
//! reprise-core: the cross-platform, GUI-free engine behind Reprise
//! (Transmission model — one core, multiple native frontends). Everything
//! here compiles from cross-platform crates only: no gtk4/libadwaita, no
//! gstreamer, no zbus, not even glib (`cargo tree -p reprise-core` is the
//! enforced proof). A frontend consumes: `db` (open/migrate/default_path),
//! `library` (scanner, watcher, playlists, m3u, settings, stats), `queries`
//! + `view_source` (the windowed ViewSource query layer), `queue` (playback
//! order engine), `format`, and the two platform contracts — `playback`
//! (`PlaybackBackend` trait + `PlayerEvent`) and `media_integration`
//! (`MediaIntegrationHandles` + state/command types) — whose concrete
//! implementations live in per-OS platform crates (Linux: GStreamer +
//! MPRIS in `reprise-platform-linux`).

pub mod db;
pub mod format;
pub mod library;
pub mod media_integration;
pub mod models;
pub mod playback;
pub mod queries;
pub mod queue;
pub mod view_source;
```

- [ ] **Step 4: The only permitted content edits** (each is required to sever a bin↔core tie; list them in the commit body):
  1. Path sweep: in core files `crate::…` paths keep working (all intra-core); in platform-linux files `crate::playback`/`crate::media_integration` → `reprise_core::…`; in `main.rs`/`ui/*` replace `crate::{db,models,queries,queue,format,library,view_source,playback,media_integration}` with `reprise_core::…` and `crate::{player,mpris}` with `reprise_platform_linux::…` (mechanical; `sed` + compiler). The composition root stays where Task 5 put it: the controller constructs `reprise_platform_linux::player::Player` / calls `…::mpris::start(crate::APP_ID)` at exactly two sites; every other reference is trait/handle-typed through core.
  2. `db_path()` moves from `main.rs` to `crates/reprise-core/src/db.rs` as `pub fn default_path() -> PathBuf` (same body, same doc comment adjusted); `main.rs` calls `db::default_path()`. Rationale: every future frontend must find the *same* library database.
  3. Visibility: items that were `pub(crate)` in the old single crate but are consumed across the new boundaries become `pub` (compiler-driven; e.g. `DEFAULT_VOLUME`, `SharedMprisState`, and whatever `mpris/` needs from `media_integration`). Do **not** blanket-`pub` anything no other crate imports.
  4. The fixture-path move from the Files list (`sine.flac` → platform crate), if `env!("CARGO_MANIFEST_DIR")` paths demand it — verify with `cargo test -p reprise-platform-linux` before touching anything else.
- [ ] **Step 5: Build + tests.** `cargo build` then `cargo test` (workspace-wide) → **three** `test result` summary lines whose sums equal the Task-5 baseline exactly (369 expected; `cargo test 2>&1 | grep 'test result'`).
- [ ] **Step 6: THE portability proof.** Run and record all three in the report:

```bash
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'   # expected: NO output
cargo tree -p reprise-core | grep -E '^\S*glib'                         # expected: NO output (bonus: glib gone too)
cargo build -p reprise-core                                             # core builds standalone
```

- [ ] **Step 7: Gate battery + full smoke incl. MPRIS:**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo audit
dbus-run-session -- sh -c 'xvfb-run -a env REPRISE_SMOKE_QUIT=1 REPRISE_SMOKE_QUIT_DELAY_SECS=6 cargo run & sleep 4; busctl --user introspect org.mpris.MediaPlayer2.reprise /org/mpris/MediaPlayer2; wait'
```

Expected: introspection lists the Player interface exactly as before; app exits 0; the produced binary is still named `reprise`.
- [ ] **Step 8: Commit.**

```bash
git add -A
git commit -m "refactor: split workspace into reprise-core, reprise-platform-linux, and reprise-gnome"
```

---

### Task 7: Typed settings façade

A thin typed layer over the existing `settings` key/value table so call sites stop hand-parsing strings, and so Task 8 (module flags) and the stage-5 preferences dialog have one blessed access path. Deliberately minimal: `bool` accessors (first real consumer: Task 8) and a typed `library_root` pair (migrates the 3 existing scattered call sites). No `i64`/enum accessors until a consumer exists (YAGNI).

**Files:**
- Modify: `crates/reprise-core/src/library/settings.rs` (98 → ~220 with tests)
- Modify: `crates/reprise-gnome/src/main.rs` (~line 172), `crates/reprise-gnome/src/ui/window.rs` (2 sites), `crates/reprise-gnome/src/ui/scan_flow.rs` (1 site — the `set_setting` in `run_scan`'s caller moved there in Task 3)

**Interfaces:**
- Consumes: existing `get_setting`/`set_setting` (which remain `pub` — the generic layer stays for keys that are genuinely free-form).
- Produces:
  - `pub fn get_bool(conn: &Connection, key: &str, default: bool) -> Result<bool, rusqlite::Error>`
  - `pub fn set_bool(conn: &Connection, key: &str, value: bool) -> Result<(), rusqlite::Error>`
  - `pub fn get_library_root(conn: &Connection) -> Result<Option<String>, rusqlite::Error>`
  - `pub fn set_library_root(conn: &Connection, root: &str) -> Result<(), rusqlite::Error>`

- [ ] **Step 1: Write the failing tests** (in `settings.rs`'s existing test mod):

```rust
#[test]
fn get_bool_returns_default_when_never_set() {
    let conn = migrated_conn();
    assert!(get_bool(&conn, "module.mpris.enabled", true).unwrap());
    assert!(!get_bool(&conn, "module.mpris.enabled", false).unwrap());
}

#[test]
fn set_bool_round_trips_both_values() {
    let conn = migrated_conn();
    set_bool(&conn, "flag", true).unwrap();
    assert!(get_bool(&conn, "flag", false).unwrap());
    set_bool(&conn, "flag", false).unwrap();
    assert!(!get_bool(&conn, "flag", true).unwrap());
}

#[test]
fn get_bool_falls_back_to_default_on_unrecognized_value() {
    // A hand-edited or future-version value must never crash or silently
    // flip a feature: unrecognized -> default, with a warning logged.
    let conn = migrated_conn();
    set_setting(&conn, "flag", "banana").unwrap();
    assert!(get_bool(&conn, "flag", true).unwrap());
    assert!(!get_bool(&conn, "flag", false).unwrap());
}

#[test]
fn library_root_typed_accessors_round_trip() {
    let conn = migrated_conn();
    assert_eq!(get_library_root(&conn).unwrap(), None);
    set_library_root(&conn, "/music/library").unwrap();
    assert_eq!(get_library_root(&conn).unwrap(), Some("/music/library".to_string()));
}
```

- [ ] **Step 2: Run to verify failure.** `cargo test -p reprise-core settings` → FAIL: `get_bool` not found.
- [ ] **Step 3: Implement:**

```rust
/// Canonical stored forms for boolean settings. `get_bool` additionally
/// tolerates anything else by falling back to the caller's default (never
/// crash on a hand-edited database; log and move on — the same tolerance
/// posture as the scanner's).
const BOOL_TRUE: &str = "1";
const BOOL_FALSE: &str = "0";

pub fn get_bool(conn: &Connection, key: &str, default: bool) -> Result<bool, rusqlite::Error> {
    match get_setting(conn, key)? {
        None => Ok(default),
        Some(value) => match value.as_str() {
            BOOL_TRUE => Ok(true),
            BOOL_FALSE => Ok(false),
            other => {
                tracing::warn!(key, value = other, "unrecognized boolean setting; using default");
                Ok(default)
            }
        },
    }
}

pub fn set_bool(conn: &Connection, key: &str, value: bool) -> Result<(), rusqlite::Error> {
    set_setting(conn, key, if value { BOOL_TRUE } else { BOOL_FALSE })
}

/// Typed accessors for `LIBRARY_ROOT_KEY` — the one string setting with
/// scattered call sites today (main.rs dev hook, scan flow, watcher
/// startup). Stored as the same string the scanner writes; kept as String
/// (not PathBuf) because the scanner's path storage is string-based and a
/// lossy round-trip here could diverge from what `mark_vanished_under_root`
/// compares against.
pub fn get_library_root(conn: &Connection) -> Result<Option<String>, rusqlite::Error> {
    get_setting(conn, LIBRARY_ROOT_KEY)
}

pub fn set_library_root(conn: &Connection, root: &str) -> Result<(), rusqlite::Error> {
    set_setting(conn, LIBRARY_ROOT_KEY, root)
}
```

- [ ] **Step 4: Tests pass.** `cargo test -p reprise-core settings` → PASS.
- [ ] **Step 5: Migrate the call sites.** Replace the raw `get_setting(&conn, LIBRARY_ROOT_KEY)` / `set_setting(&conn, LIBRARY_ROOT_KEY, …)` calls in `main.rs`, `ui/window.rs`, `ui/scan_flow.rs` with the typed pair. `grep -rn 'LIBRARY_ROOT_KEY' crates/reprise-gnome/src` afterwards → expected: no matches (the key constant is now an implementation detail of the façade; keep it `pub` in core only if a test elsewhere still asserts on it).
- [ ] **Step 6: Full battery.** `cargo test` → Task-6 count **+4** (373 expected). Gates + smoke green.
- [ ] **Step 7: Commit.**

```bash
git add crates/reprise-core/src/library/settings.rs crates/reprise-gnome/src
git commit -m "feat: typed settings facade over the key/value settings table"
```

---

### Task 8: Module registry foundation + MPRIS as the first gated module

Pulls the *substrate* of the spec's stage-5 module system forward: a descriptor list (the exact data the Plugins settings page will render), persisted per-module enable flags in the settings table (`module.<id>.enabled`, spec: "Zustand in der settings-Tabelle"), and one really-gated module — MPRIS, which the codebase already knows how to run "inert" (the no-session-bus degradation path from Stage 2 Task 6). **Deliberately deferred to stage 5:** the `Module` trait with `start/stop` lifecycle (needs ≥2 real implementors — EQ/ReplayGain — to be designed against, see "Explicitly NOT doing" #3) and live toggling (needs the Plugins UI; until then a toggle takes effect on next launch, which the descriptor's doc comment states). Post-seam note: the disabled path needs **no platform code at all** — `MediaIntegrationHandles::inert()` constructs dormant handles in core (spawns nothing, touches no bus), which is exactly the observable behavior of the no-session-bus degradation; only the enabled path calls into `reprise-platform-linux`. Registry-in-core caveat, accepted knowingly: `ALL_MODULES` listing an MPRIS descriptor in cross-platform core is *data*, not a zbus dependency; whether module lists become per-platform/per-frontend compositions is a stage-5 design question with zero cost today.

**Files:**
- Create: `crates/reprise-core/src/modules.rs`
- Modify: `crates/reprise-core/src/lib.rs` (`pub mod modules;`)
- Modify: `crates/reprise-core/src/media_integration.rs` (add `MediaIntegrationHandles::inert()`)
- Modify: `crates/reprise-gnome/src/ui/player_controller.rs` (~line 283, gate the start call) and its constructor call chain (`player_controller_wiring.rs` / `window.rs`) to thread one new `mpris_enabled: bool` argument read once at startup

**Interfaces:**
- Consumes: `library::settings::{get_bool, set_bool}` (Task 7); `MediaIntegrationHandles` (Task 5).
- Produces:
  - `pub struct ModuleDescriptor { pub id: &'static str, pub name: &'static str, pub description: &'static str, pub default_enabled: bool }`
  - `pub const MPRIS_MODULE: ModuleDescriptor`
  - `pub const ALL_MODULES: &[&ModuleDescriptor]` (stage-5 Plugins list iterates this)
  - `pub fn is_enabled(conn: &Connection, module: &ModuleDescriptor) -> Result<bool, rusqlite::Error>`
  - `pub fn set_enabled(conn: &Connection, module: &ModuleDescriptor, value: bool) -> Result<(), rusqlite::Error>`
  - `impl MediaIntegrationHandles { pub fn inert() -> Self }` — fresh default `SharedMprisState`, a command channel whose sender is dropped-into-the-struct-never-fed… (concretely: construct both channels exactly as `mpris::start` does, but spawn no thread — the receiver simply never yields, the seek sender is never drained; identical to today's no-session-bus behavior from the app's perspective)

- [ ] **Step 1: Write the failing tests** (new `#[cfg(test)] mod tests` in `modules.rs`):

```rust
#[test]
fn modules_default_to_their_declared_default() {
    let conn = migrated_conn();
    assert!(is_enabled(&conn, &MPRIS_MODULE).unwrap()); // default_enabled: true
}

#[test]
fn set_enabled_persists_and_round_trips() {
    let conn = migrated_conn();
    set_enabled(&conn, &MPRIS_MODULE, false).unwrap();
    assert!(!is_enabled(&conn, &MPRIS_MODULE).unwrap());
    set_enabled(&conn, &MPRIS_MODULE, true).unwrap();
    assert!(is_enabled(&conn, &MPRIS_MODULE).unwrap());
}

#[test]
fn enabled_key_is_namespaced_per_module() {
    assert_eq!(enabled_key(&MPRIS_MODULE), "module.mpris.enabled");
}

#[test]
fn all_modules_lists_mpris() {
    assert!(ALL_MODULES.iter().any(|m| m.id == "mpris"));
}
```

- [ ] **Step 2: Run to verify failure.** `cargo test -p reprise-core modules` → FAIL: module not found.
- [ ] **Step 3: Implement `modules.rs`:**

```rust
//! Module registry substrate (spec: "Internes Modulsystem", stage 5). This
//! is deliberately the *data* half only: which optional features exist,
//! their UI-facing name/description (the future Plugins page renders
//! exactly this list), and a persisted on/off flag per module in the
//! `settings` table. The behavioral half — a `Module` trait with
//! start/stop lifecycle and extension points (sidebar entries, settings
//! pages, pipeline elements) — is intentionally NOT here yet: it gets
//! designed in stage 5 against its first two real implementors (equalizer,
//! ReplayGain), not speculated against one. Until the Plugins UI exists,
//! toggling a flag takes effect on the next launch.

use rusqlite::Connection;

use crate::library::settings;

pub struct ModuleDescriptor {
    /// Stable machine id; forms the settings key `module.<id>.enabled`.
    pub id: &'static str,
    /// UI display name (the Plugins list, spec stage 5).
    pub name: &'static str,
    pub description: &'static str,
    /// Flag value when the settings table has no row for this module.
    pub default_enabled: bool,
}

pub const MPRIS_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "mpris",
    name: "MPRIS",
    description: "GNOME media controls, media keys, and lock-screen integration (D-Bus)",
    default_enabled: true,
};

/// Every module the app knows about, in the order the Plugins page will
/// show them. Stage 5 appends equalizer and ReplayGain here.
pub const ALL_MODULES: &[&ModuleDescriptor] = &[&MPRIS_MODULE];

pub(crate) fn enabled_key(module: &ModuleDescriptor) -> String {
    format!("module.{}.enabled", module.id)
}

pub fn is_enabled(conn: &Connection, module: &ModuleDescriptor) -> Result<bool, rusqlite::Error> {
    settings::get_bool(conn, &enabled_key(module), module.default_enabled)
}

pub fn set_enabled(
    conn: &Connection,
    module: &ModuleDescriptor,
    value: bool,
) -> Result<(), rusqlite::Error> {
    settings::set_bool(conn, &enabled_key(module), value)
}
```

(Make `enabled_key` `pub(crate)` and adjust the test to live in the same crate, or make it `pub` — pick `pub(crate)` + in-crate test.)
- [ ] **Step 4: Tests pass.** `cargo test -p reprise-core modules` → PASS.
- [ ] **Step 5: Add `MediaIntegrationHandles::inert()`** in core `media_integration.rs` per the Interfaces spec (spawns nothing; doc comment carries over the no-session-bus degradation context from `mpris/mod.rs`'s "Failure is never fatal" section — the disabled module and the busless environment are deliberately indistinguishable to the app).
- [ ] **Step 6: Gate the start.** At startup (where `window::build` has the connection, before the controller is constructed), read the flag once:

```rust
let mpris_enabled = reprise_core::modules::is_enabled(&conn.borrow(), &reprise_core::modules::MPRIS_MODULE)
    .unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read module.mpris.enabled; defaulting to on");
        true
    });
```

Thread `mpris_enabled: bool` through to `player_controller.rs:283` and replace the unconditional call:

```rust
let handles = if mpris_enabled {
    reprise_platform_linux::mpris::start(crate::APP_ID)
} else {
    tracing::info!("MPRIS module disabled (module.mpris.enabled = 0); not claiming the bus name");
    reprise_core::media_integration::MediaIntegrationHandles::inert()
};
```

- [ ] **Step 7: E2E both ways** (scratch `XDG_DATA_HOME` so the real DB is untouched):

```bash
# Enabled (default): bus name present, introspection unchanged
dbus-run-session -- sh -c 'XDG_DATA_HOME=$(mktemp -d) xvfb-run -a env REPRISE_SMOKE_QUIT=1 REPRISE_SMOKE_QUIT_DELAY_SECS=6 cargo run & sleep 4; busctl --user list | grep org.mpris.MediaPlayer2.reprise; wait'
# Disabled: flag written into the scratch DB first; name must be ABSENT; app healthy
SCRATCH=$(mktemp -d)
dbus-run-session -- sh -c "XDG_DATA_HOME=$SCRATCH xvfb-run -a env REPRISE_SMOKE_QUIT=1 cargo run" # first run creates+migrates the scratch DB
sqlite3 $SCRATCH/reprise/reprise.db "INSERT INTO settings (key,value) VALUES ('module.mpris.enabled','0');"
dbus-run-session -- sh -c "XDG_DATA_HOME=$SCRATCH xvfb-run -a env REPRISE_SMOKE_QUIT=1 REPRISE_SMOKE_QUIT_DELAY_SECS=6 cargo run & sleep 4; busctl --user list | grep -c org.mpris.MediaPlayer2.reprise; wait"
```

Expected: first grep prints the name; second grep prints `0`; both runs exit 0 with no `ERROR` lines and the disabled run logs the info line from Step 6.
- [ ] **Step 8: Full battery.** `cargo test` → Task-7 count **+4** (377 expected). Gates + standard smoke green.
- [ ] **Step 9: Commit.**

```bash
git add crates/reprise-core/src crates/reprise-gnome/src
git commit -m "feat: module registry foundation; gate MPRIS behind module.mpris.enabled"
```

---

### Task 9: adw seam consolidation — toasts, shared dialogs, containment policy

The measured adw surface (grep, 2026-07-12): Toast/ToastOverlay ×36 refs across 6 files (14 construction sites), AlertDialog ×10 refs across 4 files — including two *documented-as-duplicated* name-prompt dialogs (`track_list_context_menu.rs::show_new_playlist_dialog` and `ui/sidebar.rs`'s New-playlist dialog, whose own comment admits "same shape … but not shared code") — plus structural widgets (NavigationSplitView, ToolbarView, HeaderBar, StatusPage, WindowTitle) at 1–4 refs each. Consolidate only what repeats; write the containment policy down; explicitly skip wrappers (see "Explicitly NOT doing" #1).

**Files:**
- Create: `crates/reprise-gnome/src/ui/toasts.rs`, `crates/reprise-gnome/src/ui/dialogs.rs`
- Modify: `crates/reprise-gnome/src/ui/mod.rs` (2 mod lines + the policy doc comment), the plain-toast call sites in `import_errors_view.rs`, `playlist_io.rs`, `window.rs`, `player_controller.rs`, `track_list.rs`, `sidebar.rs`, and the two name-prompt dialog sites (`track_list_context_menu.rs`, `sidebar.rs`)

**Interfaces:**
- Consumes: `ui::strings` constants (CANCEL/CREATE etc. — reuse, never re-literal).
- Produces:
  - `pub(super) fn toasts::show(overlay: &adw::ToastOverlay, text: &str)`
  - `pub(super) fn dialogs::prompt_name(parent: &adw::ApplicationWindow, heading: &str, placeholder: &str, confirm_label: &str, on_confirm: impl Fn(String) + 'static)`

- [ ] **Step 1: `toasts.rs`.**

```rust
//! Single construction point for transient notifications. Every plain
//! informational toast goes through `show` so (a) the adw::Toast type
//! appears in exactly one non-test file outside bespoke cases, and (b) a
//! future libadwaita API change or a second notification backend (e.g.
//! XDG portal notifications when the window is closed, spec "Hintergrund-
//! Wiedergabe") edits one function. Sites that need buttons, custom
//! timeouts, or priorities keep building their own Toast locally — this
//! helper is for the 90% case, not a wrapper around the whole API.

use libadwaita as adw;

pub(super) fn show(overlay: &adw::ToastOverlay, text: &str) {
    overlay.add_toast(adw::Toast::new(text));
}
```

Migrate every call site that is exactly `overlay.add_toast(adw::Toast::new(msg))` (or the equivalent two-liner) to `toasts::show(&overlay, msg)`. Leave builder-based toasts (custom timeout/button) untouched. Verify: `grep -rn 'Toast::new' crates/reprise-gnome/src/ui | grep -v toasts.rs` → only the bespoke sites remain (list them in the report).
- [ ] **Step 2: `dialogs.rs` — deduplicate the name-prompt dialog.** Lift the *existing* `show_new_playlist_dialog` body from `track_list_context_menu.rs` (entry + AlertDialog + Suggested-appearance Create + UI-side empty-name validation) verbatim into a generic `prompt_name(parent, heading, placeholder, confirm_label, on_confirm)`; keep the response-id consts with it. Point both the context-menu site and the sidebar's New-playlist dialog at it, passing their existing strings and their differing `on_confirm` behavior (context menu: create-and-add ids; sidebar: create-and-switch). This closes a documented DRY debt without inventing any new dialog shape. Do **not** force the destructive-confirm dialogs into a shared helper in this task unless their bodies are literally identical — inspect `track_list_context_menu.rs:617/639` and `sidebar.rs:711/726` first; if they differ in responses/wiring, leave them and note it (repetition must be real, not approximate).
- [ ] **Step 3: Containment policy** — add to the top of `crates/reprise-gnome/src/ui/mod.rs`:

```rust
//! # GTK/libadwaita containment policy (refactor stage, 2026-07)
//!
//! - `reprise-core` never sees gtk4/libadwaita — and never sees gstreamer
//!   or zbus either: those live in `reprise-platform-linux` behind core's
//!   `playback`/`media_integration` contracts. Both boundaries are
//!   enforced by the crate graph, not by convention
//!   (`cargo tree -p reprise-core` is the proof).
//! - Inside this frontend, adw *structural* widgets (NavigationSplitView,
//!   ToolbarView, HeaderBar, StatusPage, WindowTitle) are used directly and
//!   deliberately unwrapped: an adw major-version port rewrites layout code
//!   wholesale, and each of these types lives in at most two files — a
//!   mirror-wrapper would only add a second rewrite site.
//! - Repeated adw *patterns* are funneled: plain toasts via `toasts::show`,
//!   name-prompt dialogs via `dialogs::prompt_name`. Add the next funnel
//!   only when a third call site repeats a shape.
//! - adw/gtk types must not appear in function signatures of modules whose
//!   job is not widgetry (e.g. `track_actions` takes ids and callbacks, not
//!   widgets) — keeps porting cost proportional to the widget layer only.
//! - Platform concretes (`reprise_platform_linux::…`) are named at the
//!   composition root only (controller construction); everything else goes
//!   through `reprise_core::playback` / `reprise_core::media_integration`.
```

- [ ] **Step 4: Full battery.** `cargo test` → count unchanged from Task 8 (377 expected — this task moves construction sites, it adds no logic). Gates green. Smoke: standard run **plus** one interactive-surface check — `dbus-run-session -- xvfb-run -a env REPRISE_SCAN_DIR=$(mktemp -d) REPRISE_SMOKE_SEED_PLAYLIST=Probe REPRISE_SMOKE_QUIT=1 cargo run` → exit 0 (exercises sidebar refresh + playlist creation path that now flows through `dialogs`-adjacent wiring).
- [ ] **Step 5: Commit.**

```bash
git add crates/reprise-gnome/src/ui
git commit -m "refactor: funnel toasts and name-prompt dialogs; document adw containment policy"
```

---

## Post-plan verification (stage close-out, run once after Task 9)

- [ ] `cargo test` → `377 passed; 1 ignored` (sum over the three per-crate summary lines); `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`; `cargo audit`.
- [ ] **Portability proof (recorded in the stage report):** `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` → no output; `cargo build -p reprise-core` succeeds standalone.
- [ ] Seam-containment greps: `grep -rn 'gstreamer\|zbus' crates/reprise-core/src crates/reprise-gnome/src` → no code hits (doc comments exempt); `grep -rn 'reprise_platform_linux' crates/reprise-gnome/src` → composition-root sites only (controller construction + MPRIS gate).
- [ ] `wc -l` over all three crates: no non-test-dominated file ≥800 among the touched targets (`queries/*`, `track_list*`, `window.rs`, `scan_flow.rs`, `playback.rs`, `media_integration.rs`, `player.rs`, `mpris/*`).
- [ ] Full headless E2E battery: standard smoke, watcher/rescan smoke (Task 3 Step 4 command), MPRIS enabled/disabled pair (Task 8 Step 7), seed-playlist smoke (Task 9 Step 4), busctl introspection (Task 6 Step 7).
- [ ] Ledger updates for `.superpowers/sdd/progress.md`: <800 waiver lifted; `mark_vanished` scan item closed; portability decision recorded (core dependency-pure; platform seams live); Queue-A note — the pointer harness should re-run its star/seek/context-menu flows against the split UI files once it exists.

## Known risks (ranked — the platform extraction changes the top of this list)

1. **Task 5 seam extraction — now the biggest risk, displacing the old path-sweep.** It is the only task in the plan whose edits are not pure moves: an error-type swap on the playback path, a `Box<dyn>` indirection in `player_controller.rs` (the file with the crate's strictest borrow-discipline history), and the `mpris/state.rs` split. Honest accounting: this is real added effort (~one extra task) and real added review surface versus the pre-revision plan. Mitigations, all measured in advance: `PlayerError` has exactly 2 consumer sites and zero variant matches (grep-verified, re-verified in Step 1); the trait's five signatures are byte-copies of live methods, so call sites compile unchanged; dyn-dispatch adds no new RefCell interactions (the field type changes, the borrow patterns don't); Display-string preservation is spelled out per variant; the fakesink E2E, the new dyn-dispatch test, and the busctl introspection pin the behavior on both sides of the seam.
2. **Task 6 path-sweep breadth** (~every `use crate::` in 22 UI files + visibility bumps, now across two dependency edges instead of one — plus the fixture relocation for the player tests). Mitigation: compiler-driven, zero logic edits allowed, the four permitted content edits are enumerated in the task, three-crate test-count-sum equality, and the busctl introspection pins the one API that crosses a process boundary.
3. **Task 8 wiring touch** on `player_controller.rs` — same borrow-discipline caution as above. Mitigation: the change is one `if` around an existing call plus a read *before* controller construction; no new RefCell interactions.
4. **Task 2 `Shared` field-visibility creep** — bumping fields to `pub(super)` widens what siblings *can* touch. Accepted: `ui` is one team-owned module tree; the alternative (accessor methods for 20 fields) is ceremony without safety.
5. **Test-path renames** (Tasks 1, 2, 5) can mask a silently dropped test. Mitigation: the sorted-name diff (Task 1 Step 4, Task 5 Step 4) and the strict total-count equality everywhere else.
