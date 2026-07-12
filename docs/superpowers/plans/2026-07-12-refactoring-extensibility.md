# Refactoring & Extensibility Implementation Plan (Queue C)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Behavior-preserving restructure of Reprise after MVP stages 1–3: pay the waived <800-line DoD debt, split the codebase into a GTK-free `reprise-core` library crate plus a GTK4/libadwaita frontend crate (Transmission model — the core must be able to serve a future Qt/Kirigami or plain-GTK frontend), and lay the minimal extensibility substrate (typed settings façade, module registry with persisted on/off flags, MPRIS as the first gated module) that stage 5 builds on.

**Architecture:** Six mechanical, compiler-verified moves plus three small additive layers. First the three oversized files are split *in place* (queries.rs → per-`ViewSource` query modules; track_list.rs → columns/sort/activation/smoke siblings; window.rs → shell + scan flow), so that the subsequent Cargo-workspace split is a near-pure directory move of already-final files. The workspace boundary itself — `reprise-core` has **no gtk4/libadwaita dependency at all** — is the load-bearing seam: GTK/adw API churn is compiler-confined to `crates/reprise`, and a second DE frontend depends on `reprise-core` alone. On top land: a typed settings façade, a module-descriptor registry (`module.<id>.enabled` in the settings table) proving itself by gating MPRIS, and a thin toast/dialog consolidation inside the GTK crate.

**Tech Stack:** Rust 2021, rusqlite (bundled SQLite), gstreamer-rs 0.25 (playbin3), zbus 5 (blocking), notify 8, lofty, gtk4-rs 0.11.4 (v4_22), libadwaita-rs 0.9.2 (v1_9). No new dependencies anywhere in this plan.

## Current state (verified 2026-07-12, HEAD 7880b4b + field-fix/close-out commits)

- Single binary crate, no `lib.rs`. 366 tests passing + 1 ignored (`cargo test`, 0.25 s).
- GTK-free already (verified by grep, only `gst::glib` — gstreamer's own re-export, **not** gtk4): `db.rs`, `models.rs`, `queries.rs`, `queue.rs`, `format.rs`, `view_source.rs`, `player.rs`, `mpris/{mod,state}.rs`, `library/{scanner,scanner_tests,playlists,settings,watcher,m3u,stats,mod}.rs`. The only gtk4/adw importers are `main.rs` and `src/ui/*` — **nothing blocks a clean core boundary**.
- Two loose ends the split must resolve: `mpris/mod.rs` imports `crate::APP_ID` (a const in `main.rs`), and `main.rs::db_path()` is the on-disk DB location every frontend must share.
- Oversized files (waived DoD gate): `src/ui/track_list.rs` 1744 (tests start ~1533), `src/ui/window.rs` 1060, `src/queries.rs` 2673 (tests start at 1158 → 1157 non-test). `library/playlists.rs` (1242, tests from 550) and `queue.rs` (1220, tests from 459) are ~55–60 % tests and inside the non-test gate — **not** split here.
- Ledger debt folded in: `mark_vanished_under_root` full-table scan per watcher reconcile; settings access layer; `DEFAULT_VOLUME` dedup is already done (lives in `mpris/state.rs`).

## Global Constraints

- **Behavior-preserving throughout.** No schema migration, no SQL-semantics change, no UI-visible change, no string changes. Each task's safety net: **all 366 existing tests pass unchanged** (test count may only grow; never edit an existing test's expectations — if a split moves a test file, the moved tests must be byte-identical apart from `use` paths).
- **Gates per commit** (stage-3 convention, all must pass before every commit): `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo audit` (known accepted advisory: RUSTSEC-2024-0436 `paste` via lofty).
- **Headless smoke run per task** (never open a window on the desktop): `dbus-run-session -- xvfb-run -a env REPRISE_SMOKE_QUIT=1 cargo run` → exit code 0, no `ERROR` lines in output.
- **No new dependencies.** Dependency edits are limited to *partitioning* the existing set between the two crates in Task 5.
- **File-size gate:** every file created or substantially edited by this plan ends <800 lines.
- English for all code, comments, log messages, and commit messages. No commit attribution footer (disabled globally). **Do not push;** the controller reviews each task.
- The curated `[lints.clippy]` set (6 pedantic lints) must keep applying to *all* code after the workspace split (moves to `[workspace.lints]`).

## Explicitly NOT doing (over-abstraction traps — judged, not forgotten)

1. **No widget-adapter layer over libadwaita.** After Task 5 the compiler guarantees adw types cannot leak into the core; after Tasks 2–3 the structural adw widgets (`NavigationSplitView`, `ToolbarView`, `HeaderBar`, `StatusPage`, `WindowTitle`) each live in 1–2 frontend files. An adw 1.x→2.x break will rewrite frontend layout code no matter what; a wrapper that mirrors the adw API 1:1 would just add a second place to rewrite. The crate boundary **is** the version seam. Only the two genuinely repeated adw patterns (toasts ×14 construction sites, name-prompt/confirm `AlertDialog`s duplicated across ≥3 files) get thin helpers (Task 8).
2. **No abstract "frontend toolkit trait".** The Transmission model is: core = full engine with a plain Rust API; each frontend is a complete client. Transmission's GTK and Qt clients share zero widget abstraction. Same here.
3. **No `Module` trait / dyn-dispatch plugin lifecycle yet.** The spec plans it for stage 5 with EQ + ReplayGain (GStreamer pipeline elements that genuinely support live insert/remove). Today there would be exactly one implementor (MPRIS), whose lifecycle is thread-spawn-at-startup — a trait designed around one awkward case is a bad trait. Task 7 lands the *substrate* stage 5 needs (descriptors, persisted enable flags, the Plugins-list data source, one really-gated module) and defers the trait until it has ≥2 real implementors.
4. **No extension-point registry** (sidebar entries, context-menu actions, settings pages, pipeline slots). Zero consumers until stage 5's Plugins UI and modules exist. Defining hook signatures now with no caller would be speculation the first real module would immediately invalidate.
5. **No split of `library/playlists.rs` / `queue.rs`** (test-heavy, non-test portions ~550/~460 lines, cohesive) and **no async-runtime / message-bus rearchitecture** (callbacks + `async-channel` + `glib::MainContext` work and are load-tested).

## Ordering & parallelization

**Order: file splits (1–3) and the SQL prefilter (4) first — in parallel if desired — then the workspace split (5), then settings façade (6) → module registry (7), with adw consolidation (8) last.**

Rationale: Tasks 1–4 are intra-crate and mechanically verifiable, and doing them first means Task 5's big commit is a near-pure `git mv` of already-final files — the riskiest change in the plan then contains *no content restructuring to review at the same time*, and `git log --follow` stays readable. The alternative (workspace first) would move 2673-line `queries.rs` across crates and then re-split it inside the new crate — churning the same lines twice and doubling review surface on the highest-risk task.

- **Independent / parallelizable:** Tasks 1, 2, 3, 4 (disjoint files: `queries.rs` / `ui/track_list*.rs` / `ui/window.rs` / `library/scanner.rs`).
- **Strictly ordered:** 5 after 1–4 (pure-move discipline). 6 after 5 (lands in `reprise-core`). 7 after 6 (uses `get_bool`). 8 after 2, 3 and 5 (touches the split UI files at their new paths); 8 is independent of 6–7 and may run in parallel with them.

---

### Task 1: Split `queries.rs` into a `queries/` module directory

**Files:**
- Delete: `src/queries.rs` (2673 lines)
- Create: `src/queries/mod.rs`, `src/queries/clauses.rs`, `src/queries/library.rs`, `src/queries/playlist.rs`, `src/queries/smart.rs`, `src/queries/queue.rs`, `src/queries/maintenance.rs`, `src/queries/tests.rs`
- Modify: nothing else — **every external `crate::queries::X` path must keep compiling unchanged** (re-exports in `mod.rs`).

**Interfaces:**
- Consumes: current `src/queries.rs` content (function inventory below).
- Produces: identical public API under `crate::queries::…`. Additionally `pub(crate) use library::playlists::…`-style visibility bumps only where the compiler demands them. Task 5 moves this directory verbatim into `reprise-core`.

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

### Task 5: Workspace split — `reprise-core` (lib) + `reprise` (GTK4 frontend)

The load-bearing extensibility move. After this task, `cargo tree -p reprise-core` contains **no gtk4 and no libadwaita** — a Qt/Kirigami or plain-GTK-XFCE frontend can depend on `reprise-core` for the entire engine: DB + migrations (`db`), scanner/watcher/playlists/M3U/settings (`library`), the `ViewSource` query layer (`queries`, `view_source`), the queue engine (`queue`), the GStreamer player with its callback API (`player`), the MPRIS service (`mpris`), and formatting (`format`). Note: `gstreamer` transitively pulls the `glib` *crate* (GLib the C library) — that is expected and fine on any Linux DE; GLib ≠ GTK.

**Files:**
- Create: `Cargo.toml` (workspace root — replaces current package manifest), `crates/reprise-core/Cargo.toml`, `crates/reprise-core/src/lib.rs`, `crates/reprise/Cargo.toml`
- Move (git mv, content-identical except the edits listed in Step 4): `src/{db,models,format,player,queue,view_source}.rs`, `src/queries/`, `src/library/`, `src/mpris/` → `crates/reprise-core/src/…`; `src/main.rs`, `src/ui/` → `crates/reprise/src/…`
- Modify: `crates/reprise/src/main.rs` + every `src/ui/*.rs` (`use crate::queries` → `use reprise_core::queries`, etc. — mechanical sweep)

**Interfaces:**
- Consumes: all prior tasks' final file layout.
- Produces: crate `reprise-core` (lib name `reprise_core`) exposing `pub mod db, format, library, models, modules (Task 7), mpris, player, queries, queue, view_source;` plus `db::default_path() -> PathBuf` (moved `db_path`). Crate `reprise` (binary, name unchanged) depending on it via `reprise-core = { path = "../reprise-core" }`. `mpris::start` gains a `desktop_entry: &'static str` parameter (see Step 4).

- [ ] **Step 1: Baseline.** `cargo test 2>&1 | tail -1` → `368 passed; 1 ignored`.
- [ ] **Step 2: Write the three manifests.**

Root `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = ["crates/reprise-core", "crates/reprise"]
default-members = ["crates/reprise"]

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

`crates/reprise-core/Cargo.toml`:

```toml
[package]
name = "reprise-core"
description = "GUI-free engine for the Reprise music player: library, queue, player, MPRIS"
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
gstreamer = "0.25"
async-channel = "2.5.0"
fastrand = "2"
notify = "8"
zbus = "5"

[dev-dependencies]
tempfile = "3"

[lints]
workspace = true
```

(Carry over the existing `notify`/`zbus` rationale comments verbatim — they document decisions.)

`crates/reprise/Cargo.toml`:

```toml
[package]
name = "reprise"
description = "A native GTK4 music player, successor to Rhythmbox"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
reprise-core = { path = "../reprise-core" }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
gtk4 = { version = "0.11.4", features = ["v4_22"] }
libadwaita = { version = "0.9.2", features = ["v1_9", "gtk_v4_22"] }
async-channel = "2.5.0"
gstreamer = "0.25"

[dev-dependencies]
tempfile = "3"

[lints]
workspace = true
```

(`gstreamer` stays in the frontend **only if** `ui/` references `gst::` types directly — check with `grep -rn 'gst::\|gstreamer' src/ui/`; if the only user is `player.rs`, drop it from the frontend manifest. Same check for `tempfile` dev-dep: keep it wherever `#[cfg(test)]` code uses it after the move. `dirs`/`serde` land core-side only.)
- [ ] **Step 3: git-mv the trees** exactly as listed in Files, create `crates/reprise-core/src/lib.rs`:

```rust
//! reprise-core: the GUI-free engine behind Reprise (Transmission model —
//! one core, multiple native frontends). Everything here compiles without
//! gtk4 or libadwaita; the only GLib linkage comes via gstreamer, which any
//! frontend needs for playback anyway. A frontend consumes: `db` (open/
//! migrate/default_path), `library` (scanner, watcher, playlists, m3u,
//! settings, stats), `queries` + `view_source` (the windowed ViewSource
//! query layer), `queue` (playback order engine), `player` (GStreamer
//! playbin3 with a callback/event API — no GTK main loop assumed), `mpris`
//! (D-Bus service on its own blocking thread), and `format`.

pub mod db;
pub mod format;
pub mod library;
pub mod models;
pub mod mpris;
pub mod player;
pub mod queries;
pub mod queue;
pub mod view_source;
```

- [ ] **Step 4: The only permitted content edits** (each is required to sever a bin↔core tie; list them in the commit body):
  1. Path sweep: in core files `crate::…` paths keep working (all intra-core); in `main.rs`/`ui/*` replace `crate::{db,models,queries,queue,player,mpris,format,library,view_source}` with `reprise_core::…` (mechanical; `sed` + compiler).
  2. `db_path()` moves from `main.rs` to `crates/reprise-core/src/db.rs` as `pub fn default_path() -> PathBuf` (same body, same doc comment adjusted); `main.rs` calls `db::default_path()`. Rationale: every future frontend must find the *same* library database.
  3. `mpris::start()` → `mpris::start(desktop_entry: &'static str)`: `mpris/mod.rs` drops `use crate::APP_ID` and uses the parameter for the MPRIS `DesktopEntry` property; `ui/player_controller.rs:283` passes `crate::APP_ID`. Rationale: the desktop-entry name belongs to the frontend (a Qt frontend ships its own `.desktop`); `APP_ID` stays a frontend const.
  4. Visibility: items that were `pub(crate)` in the old single crate but are consumed across the new boundary become `pub` (compiler-driven; e.g. `mpris::DEFAULT_VOLUME`, `SharedMprisState`). Do **not** blanket-`pub` anything the frontend doesn't import.
- [ ] **Step 5: Build + tests.** `cargo build` then `cargo test` (workspace-wide) → total `368 passed; 1 ignored` across both crates (`cargo test 2>&1 | grep 'test result'` shows two summary lines; their sums must equal the baseline).
- [ ] **Step 6: THE portability proof.** Run and record in the report:

```bash
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita'   # expected: NO output
cargo build -p reprise-core                               # core builds standalone
```

- [ ] **Step 7: Gate battery + full smoke incl. MPRIS:**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo audit
dbus-run-session -- sh -c 'xvfb-run -a env REPRISE_SMOKE_QUIT=1 REPRISE_SMOKE_QUIT_DELAY_SECS=6 cargo run & sleep 4; busctl --user introspect org.mpris.MediaPlayer2.reprise /org/mpris/MediaPlayer2; wait'
```

Expected: introspection lists the Player interface exactly as before; app exits 0.
- [ ] **Step 8: Commit.**

```bash
git add -A
git commit -m "refactor: split workspace into reprise-core lib and GTK frontend crate"
```

---

### Task 6: Typed settings façade

A thin typed layer over the existing `settings` key/value table so call sites stop hand-parsing strings, and so Task 7 (module flags) and the stage-5 preferences dialog have one blessed access path. Deliberately minimal: `bool` accessors (first real consumer: Task 7) and a typed `library_root` pair (migrates the 3 existing scattered call sites). No `i64`/enum accessors until a consumer exists (YAGNI).

**Files:**
- Modify: `crates/reprise-core/src/library/settings.rs` (98 → ~220 with tests)
- Modify: `crates/reprise/src/main.rs` (~line 172), `crates/reprise/src/ui/window.rs` (2 sites), `crates/reprise/src/ui/scan_flow.rs` (1 site — the `set_setting` in `run_scan`'s caller moved there in Task 3)

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
- [ ] **Step 5: Migrate the call sites.** Replace the raw `get_setting(&conn, LIBRARY_ROOT_KEY)` / `set_setting(&conn, LIBRARY_ROOT_KEY, …)` calls in `main.rs`, `ui/window.rs`, `ui/scan_flow.rs` with the typed pair. `grep -rn 'LIBRARY_ROOT_KEY' crates/reprise/src` afterwards → expected: no matches (the key constant is now an implementation detail of the façade; keep it `pub` in core only if a test elsewhere still asserts on it).
- [ ] **Step 6: Full battery.** `cargo test` → `372 passed; 1 ignored` (368 + 4). Gates + smoke green.
- [ ] **Step 7: Commit.**

```bash
git add crates/reprise-core/src/library/settings.rs crates/reprise/src
git commit -m "feat: typed settings facade over the key/value settings table"
```

---

### Task 7: Module registry foundation + MPRIS as the first gated module

Pulls the *substrate* of the spec's stage-5 module system forward: a descriptor list (the exact data the Plugins settings page will render), persisted per-module enable flags in the settings table (`module.<id>.enabled`, spec: "Zustand in der settings-Tabelle"), and one really-gated module — MPRIS, which the codebase already knows how to run "inert" (the no-session-bus degradation path from Stage 2 Task 6). **Deliberately deferred to stage 5:** the `Module` trait with `start/stop` lifecycle (needs ≥2 real implementors — EQ/ReplayGain — to be designed against, see "Explicitly NOT doing" #3) and live toggling (needs the Plugins UI; until then a toggle takes effect on next launch, which the descriptor's doc comment states).

**Files:**
- Create: `crates/reprise-core/src/modules.rs`
- Modify: `crates/reprise-core/src/lib.rs` (`pub mod modules;`)
- Modify: `crates/reprise-core/src/mpris/mod.rs` (extract `inert()` from the existing degradation path)
- Modify: `crates/reprise/src/ui/player_controller.rs` (~line 283, gate the start call) and its constructor call chain (`player_controller_wiring.rs` / `window.rs`) to thread one new `mpris_enabled: bool` argument read once at startup

**Interfaces:**
- Consumes: `library::settings::{get_bool, set_bool}` (Task 6).
- Produces:
  - `pub struct ModuleDescriptor { pub id: &'static str, pub name: &'static str, pub description: &'static str, pub default_enabled: bool }`
  - `pub const MPRIS_MODULE: ModuleDescriptor`
  - `pub const ALL_MODULES: &[&ModuleDescriptor]` (stage-5 Plugins list iterates this)
  - `pub fn is_enabled(conn: &Connection, module: &ModuleDescriptor) -> Result<bool, rusqlite::Error>`
  - `pub fn set_enabled(conn: &Connection, module: &ModuleDescriptor, value: bool) -> Result<(), rusqlite::Error>`
  - `pub fn mpris::inert() -> (SharedMprisState, async_channel::Receiver<MprisCommand>, /* seek-notify handle, same type start() returns */)`

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
- [ ] **Step 5: Extract `mpris::inert()`.** In `mpris/mod.rs`, factor the handle construction (`SharedMprisState` + command channel + seek-notify) out of `start(…)` into a private `fn make_handles() -> (…)`; `start` calls it then spawns its threads; new `pub fn inert()` calls it and spawns **nothing** — the receiver simply never yields, exactly the observable behavior of today's no-session-bus degradation (Stage 2 Task 6 negative E2E). Copy the relevant doc-comment context onto `inert()`.
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
let (mpris_state, mpris_receiver, mpris_seek_notify) = if mpris_enabled {
    mpris::start(crate::APP_ID)
} else {
    tracing::info!("MPRIS module disabled (module.mpris.enabled = 0); not claiming the bus name");
    mpris::inert()
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
- [ ] **Step 8: Full battery.** `cargo test` → `376 passed; 1 ignored` (372 + 4). Gates + standard smoke green.
- [ ] **Step 9: Commit.**

```bash
git add crates/reprise-core/src crates/reprise/src
git commit -m "feat: module registry foundation; gate MPRIS behind module.mpris.enabled"
```

---

### Task 8: adw seam consolidation — toasts, shared dialogs, containment policy

The measured adw surface (grep, 2026-07-12): Toast/ToastOverlay ×36 refs across 6 files (14 construction sites), AlertDialog ×10 refs across 4 files — including two *documented-as-duplicated* name-prompt dialogs (`track_list_context_menu.rs::show_new_playlist_dialog` and `ui/sidebar.rs`'s New-playlist dialog, whose own comment admits "same shape … but not shared code") — plus structural widgets (NavigationSplitView, ToolbarView, HeaderBar, StatusPage, WindowTitle) at 1–4 refs each. Consolidate only what repeats; write the containment policy down; explicitly skip wrappers (see "Explicitly NOT doing" #1).

**Files:**
- Create: `crates/reprise/src/ui/toasts.rs`, `crates/reprise/src/ui/dialogs.rs`
- Modify: `crates/reprise/src/ui/mod.rs` (2 mod lines + the policy doc comment), the plain-toast call sites in `import_errors_view.rs`, `playlist_io.rs`, `window.rs`, `player_controller.rs`, `track_list.rs`, `sidebar.rs`, and the two name-prompt dialog sites (`track_list_context_menu.rs`, `sidebar.rs`)

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

Migrate every call site that is exactly `overlay.add_toast(adw::Toast::new(msg))` (or the equivalent two-liner) to `toasts::show(&overlay, msg)`. Leave builder-based toasts (custom timeout/button) untouched. Verify: `grep -rn 'Toast::new' crates/reprise/src/ui | grep -v toasts.rs` → only the bespoke sites remain (list them in the report).
- [ ] **Step 2: `dialogs.rs` — deduplicate the name-prompt dialog.** Lift the *existing* `show_new_playlist_dialog` body from `track_list_context_menu.rs` (entry + AlertDialog + Suggested-appearance Create + UI-side empty-name validation) verbatim into a generic `prompt_name(parent, heading, placeholder, confirm_label, on_confirm)`; keep the response-id consts with it. Point both the context-menu site and the sidebar's New-playlist dialog at it, passing their existing strings and their differing `on_confirm` behavior (context menu: create-and-add ids; sidebar: create-and-switch). This closes a documented DRY debt without inventing any new dialog shape. Do **not** force the destructive-confirm dialogs into a shared helper in this task unless their bodies are literally identical — inspect `track_list_context_menu.rs:617/639` and `sidebar.rs:711/726` first; if they differ in responses/wiring, leave them and note it (repetition must be real, not approximate).
- [ ] **Step 3: Containment policy** — add to the top of `crates/reprise/src/ui/mod.rs`:

```rust
//! # GTK/libadwaita containment policy (refactor stage, 2026-07)
//!
//! - `reprise-core` never sees gtk4/libadwaita — enforced by the crate
//!   graph, not by convention (`cargo tree -p reprise-core` is the proof).
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
```

- [ ] **Step 4: Full battery.** `cargo test` → count unchanged from Task 7 (`376 passed; 1 ignored` — this task moves construction sites, it adds no logic). Gates green. Smoke: standard run **plus** one interactive-surface check — `dbus-run-session -- xvfb-run -a env REPRISE_SCAN_DIR=$(mktemp -d) REPRISE_SMOKE_SEED_PLAYLIST=Probe REPRISE_SMOKE_QUIT=1 cargo run` → exit 0 (exercises sidebar refresh + playlist creation path that now flows through `dialogs`-adjacent wiring).
- [ ] **Step 5: Commit.**

```bash
git add crates/reprise/src/ui
git commit -m "refactor: funnel toasts and name-prompt dialogs; document adw containment policy"
```

---

## Post-plan verification (stage close-out, run once after Task 8)

- [ ] `cargo test` → `376 passed; 1 ignored`; `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`; `cargo audit`.
- [ ] `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita'` → no output (recorded in the stage report).
- [ ] `wc -l` over both crates: no non-test-dominated file ≥800 among the split targets (`queries/*`, `track_list*`, `window.rs`, `scan_flow.rs`).
- [ ] Full headless E2E battery: standard smoke, watcher/rescan smoke (Task 3 Step 4 command), MPRIS enabled/disabled pair (Task 7 Step 7), seed-playlist smoke (Task 8 Step 4).
- [ ] Ledger updates for `.superpowers/sdd/progress.md`: <800 waiver lifted; `mark_vanished` scan item closed; Queue-A note — the pointer harness should re-run its star/seek/context-menu flows against the split UI files once it exists.

## Known risks (ranked)

1. **Task 5 path sweep breadth** (~every `use crate::` in 22 UI files + visibility bumps). Mitigation: compiler-driven, zero logic edits allowed, the four permitted content edits are enumerated in the task, and the busctl introspection pins the one API that crosses a process boundary.
2. **Task 7 wiring touch** on `player_controller.rs` — the file with the crate's strictest borrow discipline history (BorrowMutError class). Mitigation: the change is one `if` around an existing call plus a read *before* controller construction; no new RefCell interactions.
3. **Task 2 `Shared` field-visibility creep** — bumping fields to `pub(super)` widens what siblings *can* touch. Accepted: `ui` is one team-owned module tree; the alternative (accessor methods for 20 fields) is ceremony without safety.
4. **Test-path renames** (Tasks 1–2) can mask a silently dropped test. Mitigation: the sorted-name diff in Task 1 Step 4 and the strict total-count equality everywhere else.
