---
slug: wave-0-agent-handover
worktree: one per agent, see §2
branch: one per agent, see §2
phase: handover
codex_session:
created: 2026-07-31
base: 577765b
---
# Handover — seven agents, running in parallel

Each section in §3 is **self-contained and meant to be pasted as one agent's
whole prompt**. An agent needs its own section plus §1; nothing else. The two
big documents (`architecture-consolidation.md`, `consolidation-plan.md`) are
background for you, not required reading for them.

**Lifetime.** Delete this file once the seven branches have merged. The two
plan documents stay.

---

## 1. The shared preamble — paste this above every agent brief

> You are working in the Reprise repository (`~/Projects/reprise`), a
> nine-crate Rust workspace for a GTK4/libadwaita music player. Read
> `AGENTS.md` before writing code.
>
> **Four safety rules, non-negotiable.**
> 1. The real database `~/.local/share/reprise/reprise.db` (1,686 tracks) and
>    the library root `/home/marvin/Music` are off limits. Never scan, mutate
>    or point tooling at them.
> 2. Every app launch is fully isolated. A run or smoke command must contain
>    **all** of this, and you must grep your own command for `XDG_DATA_HOME`
>    and `XDG_STATE_HOME` before running it:
>    ```bash
>    dbus-run-session -- xvfb-run -a env \
>      XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) \
>      XDG_STATE_HOME=$(mktemp -d) \
>      GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
>      cargo run
>    ```
> 3. Never build under `/tmp` — it is a 16 G tmpfs and a `target/` there lives
>    in RAM. Work in your own worktree. Do not share `CARGO_TARGET_DIR`.
> 4. Touch **only** the files your brief lists as yours. Other agents are
>    working in this repository at the same time.
>
> **Method.** Test-first, without exception: write the failing test, run it and
> **see it red**, implement the minimum, see it green, run the gate, commit.
> One commit per task. A test that was never red proves nothing.
>
> **Gate before every commit:**
> ```bash
> cargo fmt --check
> cargo clippy --all-targets --workspace -- -D warnings
> cargo test --workspace          # bare `cargo test` only runs the gnome member
> cargo audit                     # only RUSTSEC-2024-0436 is accepted; a NEW one means STOP
> scripts/check-architecture.sh
> scripts/check-ux-traceability.sh
> ```
>
> **Two reds you will inherit — do not fix them, they are not yours.**
> `scripts/check-frontend-thinness.sh` fails on untouched `dev`
> (`filesystem` 17→19, `threads` 14→15, from `#189`); that is agent 1's task
> 0.0. And GitHub Actions fails on every branch in this repository including
> `main` — the job never reaches a runner. The enforcing gate is the local run
> above.
>
> **Language.** Code, comments, log/error/UI strings, commit messages and every
> document in this repository are English. `README.de.md` and `po/` are the
> only German things and stay that way.
>
> **The ledger will conflict.** Everyone appends one line to
> `.superpowers/sdd/progress.md`. Append at the very end, keep it to one line,
> and if you hit a conflict on it take **both** sides — it is append-only
> history, never a choice between versions.
>
> **Finish by** pushing your branch and opening a squashed PR against `dev`.
> Never push to `dev` or `main`. Do not merge; the owner decides.

---

## 2. Who runs when, and who owns what

```
     ┌─ Agent 1  diagnostics chain (0.0 → 0.5)      ── the critical path, sequential
     │
start┼─ Agent 2  the missing index (1.1)            ── independent
     ├─ Agent 3  build and shipping (0.6, 0.8)      ── independent
     ├─ Agent 4  docs and decisions (0.7, 0.9, 0.10)── independent
     ├─ Agent 5  the #189 defects (2.4g, 2.4h)      ── independent
     ├─ Agent 6  RefCell hoisting (2.4i)            ── independent
     └─ Agent 7  security debts (2.4a/c/d)          ── independent
```

All seven can start at once. Agent 1 is the long pole and the only one whose
six tasks must run in order.

**Ownership — no two agents share a file.** Verified against the tree at
`577765b`.

| Agent | Owns | Must not touch |
| --- | --- | --- |
| 1 | `main.rs`, `ui/diagnostics/**`, `ui/startup_failure*.rs`, `ui/primary_menu.rs`, `ui/strings_app_shell.rs`, `ui/window/window.rs`, `ui/mod.rs`, `docs/ux-rules.md`, `scripts/check-frontend-thinness.sh` | `scripts/check-architecture.sh` beyond its own added block |
| 2 | `core/src/db_sort_indexes.rs`, `core/src/db.rs`, `core/src/lib.rs` | anything in `reprise-gnome` |
| 3 | `Cargo.toml`, `Cargo.lock`, `scripts/tests/msrv.sh`, `meson.build`, `data/meson.build`, `meson_options.txt`, `scripts/check-runtime-service-install.sh`, `scripts/check-release.sh`, `flatpak/cargo-sources.json` | any `.rs` file |
| 4 | `AGENTS.md`, `docs/adr/003-runtime-ownership.md`, `ui/preferences/**`, `core/src/modules.rs` | `docs/ux-rules.md` (agent 1 has it) |
| 5 | `core/src/library/watcher.rs`, `core/src/writeback_publish.rs` | `core/src/lib.rs` (agent 2 has it) |
| 6 | the 19 files in its brief | `ui/primary_menu.rs`, `ui/window/window.rs` (agent 1) |
| 7 | `core/src/podcasts/ytdlp.rs`, `ytdlp_download.rs`, `core/src/cover.rs`, `core/src/cover_download.rs`, `ui/browse/filter_restriction.rs`, `ui/sidebar/sidebar.rs` | `core/src/lib.rs` |

**Three shared files, three rules.** `.superpowers/sdd/progress.md` — everyone
appends, take both sides on conflict. `docs/ux-rules.md` — agent 1 only.
`scripts/check-architecture.sh` — agents 1 and 7 each add one independent
block; if both land, the conflict is two additions and both stay.

---

## 3. The briefs

### Agent 1 — diagnostics chain

> **Branch** `feat/wave-0-diagnostics` from `dev`. Six tasks, in this order,
> one commit each. This is the critical path for opening a test round.
>
> **Why this exists.** The app cannot report on itself. A panic in a GTK
> callback is a process abort with no hook, opening the database is an
> `expect()`, and 793 `tracing` calls go to stderr where no tester sees them.
> A tester's crash currently leaves nothing behind at all.
>
> **0.0 — make the base gate green.** `scripts/check-frontend-thinness.sh`
> fails on untouched `dev`: `filesystem` grew 17→19, `threads` 14→15. `#189`
> added `crates/reprise-gnome/src/ui/lyrics/lyrics_batch.rs` with its own
> worker thread and filesystem access without moving the budgets in the same
> commit. Raise both budgets to the measured values with the reason in the
> commit message. Do **not** guess the numbers — run the script, it prints
> them. (Moving that worker into `reprise-core` would be the better answer on
> the merits; record it as a follow-up, do not do it here.)
> Commit: `fix(gates): restore the frontend thinness budgets to the measured values`
>
> **0.1 — the diagnostics module.** New `ui/diagnostics/{mod,paths,paths_tests}.rs`,
> declared in `ui/mod.rs` alphabetically between `device_sync` and `dialogs`.
> Public surface: `log_path()`, `previous_log_path()`, `crash_marker_path()`
> (all `-> Option<PathBuf>`, under `$XDG_STATE_HOME/reprise/`, falling back to
> the data dir), `rotate_on_start() -> io::Result<()>`, and
> `const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024`.
> At the cap, further writes are **dropped, not rotated again** — a second
> rotation mid-run hands out half a log later, and the log you want whole is
> the one from the crash. Rotation returning an error must never stop startup.
> Tests: `log_path_follows_xdg_state_home`,
> `log_path_falls_back_to_the_data_dir_when_state_home_is_unset`,
> `rotation_keeps_exactly_one_previous_run`,
> `rotation_reports_but_survives_a_read_only_directory`.
> `std::env::set_var` is `unsafe` since Rust 2024 and tests run in parallel —
> make the pure function `log_path_in(root: &Path)` and have `log_path()` read
> the environment once, so no test needs a variable.
> This moves the `filesystem` budget again; run the script for the number.
> Commit: `feat(diagnostics): give the app one log file with a bounded size`
>
> **0.2 — mirror the log to that file.** `init_logging` in `main.rs` currently
> uses `tracing_subscriber::fmt()` with a stderr writer. Change to
> `registry()` with two layers: stderr as before, plus a file layer with
> `.with_ansi(false)`. Filter and default (`REPRISE_LOG`, `info,lofty=error`)
> stay word for word. With no file, everything runs on stderr alone — logging
> must never stop startup. Rotation from 0.1 happens here, before the first
> write. Test: `file_writer_is_absent_when_the_state_directory_cannot_be_created`.
> Do not add a redaction filter; check in review that no music paths or tokens
> are logged. A filter scanning free text invites false confidence.
> Commit: `feat(diagnostics): mirror the log to a file testers can send`
>
> **0.3 — panic hook and crash marker.** New `ui/diagnostics/{crash,crash_tests}.rs`.
> `diagnostics::crash::install_hook()` becomes the **first statement in
> `main`**, before `init_logging()`. It writes message, location and
> `std::backtrace::Backtrace::force_capture()` to the log, then creates the
> marker with version, schema version and timestamp. Force the capture: a
> tester never sets `RUST_BACKTRACE`, and a crash report without a backtrace
> has no content. On the next start, `window::build` shows **exactly one**
> toast offering "Copy Diagnostics", then deletes the marker; a clean shutdown
> deletes it in `close-request`.
> Add `docs/ux-rules.md` rule `START-4` in section I as `[active]`, in the same
> commit as its test — the exact text is in
> `docs/plans/consolidation-plan.md` §8.
> Tests: `start_4_a_crash_marker_written_by_the_previous_run_is_offered_once`,
> `start_4_a_clean_shutdown_removes_the_marker`,
> `crash_report_contains_version_schema_and_the_panic_location`,
> `crash_report_never_contains_a_library_path`. The first two must carry the
> rule ID or `check-ux-traceability.sh` will not see them.
> Test the **state machine** (marker present → offered once → gone), not a real
> abort. Put in `crash.rs` as a comment that an `abort()` from C — a GTK
> assertion, a Wayland protocol error — never reaches a Rust hook, so nobody
> later believes the coverage is total.
> Commit: `feat(diagnostics): a crash leaves a report and offers it once`
>
> **0.4 — "Copy Diagnostics" in the primary menu.** New
> `ui/diagnostics/{report,report_tests}.rs`. In `primary_menu.rs`, add
> `ACTION_COPY_DIAGNOSTICS = "copy-diagnostics"` beside the other action
> constants and the entry in `settings_section_entries()` **after "Help" and
> before "About Reprise"**; register the `gio::SimpleAction` in `install`
> following the `ACTION_ABOUT` pattern including `window.downgrade()`.
> Strings go in `strings_app_shell.rs` (already in `po/POTFILES.in`):
> `COPY_DIAGNOSTICS`, `DIAGNOSTICS_COPIED`, `CRASH_LAST_RUN`.
> `report::build()` takes its inputs as parameters — version, schema, module
> list, log path — so all three tests are free of a display and the
> environment. Content: app version, schema version, toolkit versions, enabled
> modules, language, last 64 KB of log. Nothing from the library.
> Copy to the clipboard via `gdk::Display`, then a toast — under Flatpak a file
> manager is not guaranteed reachable, the clipboard is.
> Rule `FB-9` in section G, text in the plan's §8.
> Tests: `fb_9_the_report_carries_version_schema_modules_and_the_log_tail`,
> `fb_9_the_report_is_capped_so_the_clipboard_stays_usable`,
> `fb_9_the_report_omits_the_library_root_and_track_paths`.
> Commit: `feat(diagnostics): let a tester copy a diagnostics report`
>
> **0.5 — the startup path reports instead of panicking.** Replace
> `db::Db::open_migrated(Some(&path)).expect(...)` in `main.rs` with a `match`
> that returns `startup_failure::present(&app, &path, &error)`.
> **Do not use a parentless `AdwAlertDialog`** — it needs a parent widget and
> there is no window yet. Open a minimal `adw::ApplicationWindow` with an
> `adw::StatusPage`: `dialog-error-symbolic`, a headline per `DbError` case,
> the database path as a secondary line, and two buttons — "Copy Diagnostics"
> (0.4) and "Close". That is the same form `START-2` already prescribes for an
> unreachable library folder. If GTK initialisation itself fails, `eprintln!`
> plus exit code 1.
> Four cases: `SchemaTooNew` (a tester tried a newer build and went back —
> this is the downgrade case `db.rs` deliberately detects), `SchemaNotReady`,
> `Io` (full disk, NFS home, permissions), `Sqlite` (hard power-off). The
> technical cause goes in the report only, never on the page — the separation
> `SourceError` already draws and tests.
> **Never** repair, rename or replace the file. A library Reprise cannot read
> is still the user's library.
> Rule `START-3` in section I, text in the plan's §8. Tests:
> `start_3_a_newer_schema_names_the_downgrade_and_never_migrates`,
> `start_3_an_io_failure_names_the_path_and_offers_diagnostics`,
> `start_3_a_corrupt_database_offers_diagnostics_not_a_repair`,
> `start_3_the_failure_copy_never_contains_the_technical_cause`.
> Add to `scripts/check-architecture.sh`, same commit:
> ```bash
> if rg --quiet '\.expect\(|\.unwrap\(\)' crates/reprise-gnome/src/main.rs; then
>   echo "the startup path must report failures, not panic on them" >&2
>   exit 1
> fi
> ```
> Verify by hand:
> ```bash
> scratch=$(mktemp -d ~/.cache/reprise-scratch/startup.XXXXXX)
> mkdir -p "$scratch/data/reprise"
> sqlite3 "$scratch/data/reprise/reprise.db" "PRAGMA user_version = 99;"
> dbus-run-session -- xvfb-run -a env \
>   XDG_DATA_HOME="$scratch/data" XDG_CACHE_HOME=$(mktemp -d) \
>   XDG_STATE_HOME=$(mktemp -d) \
>   GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink cargo run
> ```
> Expect a StatusPage, exit code 1, and an `error!` line naming the case.
> Commit: `fix(startup): report a database failure instead of panicking`
>
> **Done when** a deliberately triggered panic leaves a log line with a
> backtrace, a marker, and exactly one offer on the next start; `user_version
> = 99` produces a StatusPage; the report carries no library path;
> `check-ux-traceability.sh` counts three more active rules.

### Agent 2 — the missing index

> **Branch** `feat/library-sort-index` from `dev`. One task, one commit.
>
> **Why.** The library's default sort has no index. Measured against a replica
> of the real schema at 100k rows: `SCAN tracks` plus `USE TEMP B-TREE FOR
> ORDER BY`, 14.9 ms at offset 0 and up to 380 ms deep — and
> `TrackListModel::item()` runs that query **synchronously on the GTK thread**
> on a cache miss. With the index: 0.44 / 1.95 / 3.37 ms.
>
> **Files.** New `crates/reprise-core/src/db_sort_indexes.rs`;
> `crates/reprise-core/src/lib.rs` (`mod db_sort_indexes;` beside the other
> `db_*`); `crates/reprise-core/src/db.rs` (`SUPPORTED_SCHEMA_VERSION` 50 → 51,
> and the call at the end of `migrate_with_cache_dirs`).
>
> **Copy the shape** of `db_recently_added.rs::migrate_v35` — version check,
> `unchecked_transaction`, `execute_batch`, `pragma_update`, `commit`. Do not
> invent a new one.
>
> ```sql
> CREATE INDEX IF NOT EXISTS idx_tracks_present_artist_order
> ON tracks(artist COLLATE NOCASE, year, album COLLATE NOCASE, track_no)
> WHERE missing_since IS NULL AND removed_at IS NULL;
> ```
> The column order must match `SORT_WHITELIST["artist"]` in
> `queries/clauses.rs` **exactly**, and the `WHERE` must match
> `clauses::PRESENT`.
>
> **Tests.** `v51_serves_the_default_artist_sort_from_an_index` and
> `v51_is_idempotent_and_bumps_the_schema_version`.
> The first asserts the **plan, not a duration** — durations drift per machine,
> plans do not. Build the query with `crate::queries::build_track_query("artist",
> "ASC", false)` (`clauses` is private; the builder is re-exported at
> `queries::build_track_query`), pass its two bound parameters (`LIMIT ?1
> OFFSET ?2`) or the prepare fails, and assert the `EXPLAIN QUERY PLAN` output
> does not contain `USE TEMP B-TREE FOR ORDER BY` and does name the index.
> On an empty table the planner may spurn the index — fill a few hundred rows
> and run `ANALYZE`, or reuse the fixture from `queries/tests.rs`.
>
> **Prove it.** `performance-query-compare.sh` compares two directories that
> `performance-baseline.sh` produces; it measures nothing itself. The tree must
> be clean for both runs — the script records the commit in its manifest.
> ```bash
> scripts/performance-baseline.sh ~/perf/before     # on untouched dev
> scripts/performance-baseline.sh ~/perf/after      # after the commit
> scripts/performance-query-compare.sh ~/perf/before ~/perf/after
> ```
> Run it **without** `--quick` — the 100k scenario is the interesting one. Put
> the comparison JSON in the commit message. The numbers above come from a
> Python-SQLite replica with synthetic rows; yours are the real ones, and if
> they differ markedly **yours are the truth**.
>
> **One index only.** `added_at` is the next candidate but every index costs
> write load during a scan. Measure first, propose separately.
> Commit: `perf(db): serve the default library sort from an index`

### Agent 3 — build and shipping

> **Branch** `feat/wave-0-build` from `dev`. Two tasks, two commits. Touch no
> `.rs` file.
>
> **0.6 — make the MSRV honest.** Every manifest declares
> `rust-version = "1.92"`. The pinned graph does not build on `rustc 1.94.1`:
> ```
> Compiling libsqlite3-sys v0.38.1
> error[E0658]: use of unstable library feature `cfg_select`
> ```
> `scripts/tests/msrv.sh` cannot catch this — it reads `cargo metadata` and
> checks that the *field* says 1.92. It never builds.
>
> Measure first, do not guess:
> ```bash
> rustc --version
> cargo build --locked --workspace 2>&1 | tail -20
> flatpak run --command=sh --devel org.gnome.Sdk//50 -c \
>   '/usr/lib/sdk/rust-stable/bin/rustc --version'
> ```
> **The second number decides**, because `org.reprise.Reprise.yml` builds with
> `CARGO_NET_OFFLINE=true` against `flatpak/cargo-sources.json` — exactly this
> pinned tree.
> *If the SDK toolchain builds:* raise `rust-version` in
> `[workspace.package]` to the measured version and give `msrv.sh` a real
> build (`rustup toolchain install`, then `cargo +$expected build --locked
> --workspace`). Keep the metadata check; it catches a manifest that lags.
> *If it does not:* take `rusqlite`/`libsqlite3-sys` back to the last version
> that does, regenerate `flatpak/cargo-sources.json` (tooling in
> `RELEASING.md`), and run `scripts/check-release.sh` — it compares the
> checksums in `Cargo.lock` and `cargo-sources.json` and fails if they diverge.
> **Abort criterion:** if the rollback reaches beyond `rusqlite`, stop, commit
> nothing, and report — that is its own PR, not a task here.
> Commit: `fix(build): make the declared MSRV the one that actually builds`
>
> **0.8 — ship only what a surface uses.** `reprise-runtime`, its two
> `.service` files and its Meson target are installed today, and **no shipped
> surface uses them** — the GTK session that would is `#![allow(dead_code)]`,
> and MCP/CLI drive playback over MPRIS.
> Add to `meson_options.txt`:
> ```meson
> option('runtime_service', type: 'boolean', value: false,
>        description: 'Install the headless runtime service (no surface uses it yet)')
> ```
> Put the `reprise-runtime` target (`meson.build`) and both `.service` files
> (`data/meson.build`) behind it. `check-runtime-service-install.sh` checks
> only **when** the option is on, and then as strictly as before, in both
> prefixes. Set `stem_backend` off for the test round and make
> `check-release.sh` skip `check-stem-runtime-packaging.sh` consistently when
> it is off — that check is red on the base and this stops it blocking
> **without** anyone silencing it.
> The crates stay in the workspace, still built and tested; only the install
> changes.
> Verify:
> ```bash
> meson setup build-off . --prefix="$HOME/.local"
> meson introspect build-off --installed | grep -c reprise-runtime   # 0
> meson setup build-on . --prefix="$HOME/.local" -Druntime_service=true
> scripts/check-runtime-service-install.sh                            # green
> ```
> Commit: `build: ship only what a surface uses (runtime service, stems opt-in)`

### Agent 4 — docs and decisions

> **Branch** `feat/wave-0-docs` from `dev`. Three tasks. Do **not** touch
> `docs/ux-rules.md` — another agent owns it.
>
> **0.7 — bring `AGENTS.md` up to date.** Four corrections, all verified:
> the crate list says "Three-crate Cargo workspace" and there are nine (take
> the table from `README.md`); the roadmap ends at "GUI-A2" while podcasts,
> YouTube, radio, concerts, new releases, device sync, library doctor, my
> stats, the tag editor, stems and the runtime have landed; the baseline test
> count ("390 passed") is stale by orders of magnitude — replace it with a
> pointer to the latest ledger entry, because a wrong number is worse than
> none.
> **Do not** flip the "Not released yet — no backwards compatibility" section.
> It is correct until someone installs. It already carries a note saying it
> expires with the first test release and where the replacement text lives.
> Setting it early would license deleting data in libraries that do not exist
> yet; setting it late licenses deleting data in libraries that do.
> Commit: `docs: describe the workspace and the roadmap as they are`
>
> **0.9 — name the agent capabilities.** First **establish whether
> `reprise-mcp` is reachable at all without a user action.** It is a separate
> binary an MCP client launches; if it is only ever started externally there is
> nothing to switch off and this task is one line. **Record that finding in the
> commit message** so the question is not asked a third time.
> If it shrinks: one line on the plugins page naming the granted capability
> classes — read, mix planning, playlist creation, the three from `CONTEXT.md`.
> If it is reachable: additionally a module descriptor in
> `core/src/modules.rs` with `default_enabled: false`, following the other
> `ONLINE_MODULES`, and hang `capability.rs`'s gate on it.
> Commit: `feat(preferences): name the agent capabilities that are granted`
>
> **0.10 — record the runtime decision as an ADR.** New
> `docs/adr/003-runtime-ownership.md`, following ADR 002's shape.
> Context: `reprise-runtime` plus its protocol, client, D-Bus host and the GTK
> session are about 15,400 lines across five places; the GTK session is
> `#![allow(dead_code)]`; MCP and CLI reach playback over MPRIS; there are two
> command surfaces for one domain; a D-Bus service ships that nothing uses.
> Decision: **(A)** cut over — `PlayerController` becomes a client of
> `RuntimeSession` — or **(B)** shelve. The plan recommends B for the test
> round.
> The section that matters most is the **resumption trigger**: "as soon as a
> second frontend begins", or "as soon as an agent should control playback with
> no window open", or whatever the owner actually means. Without a named
> trigger, "shelved" quietly becomes "abandoned", and then 15,000 lines sit in
> the repository with no owner.
> Commit: `docs: record the runtime ownership decision as an ADR`

### Agent 5 — the two `#189` defects

> **Branch** `fix/writeback-registry-and-sweep` from `dev`. Two tasks, two
> commits. Both are in the code that writes `cover.<ext>` and `.lrc` into the
> user's music collection — the riskiest thing the app does, added two commits
> ago, and a tester will run a library-wide lyrics batch on day one.
>
> The publisher itself is good: `hard_link` instead of `rename` so an existing
> file can never be replaced, an `O_EXCL` fallback for the vfat/exfat/MTP
> filesystems that have no `->link`, a sweep pattern narrow enough to match only
> Reprise's own temporaries. Leave that design alone. Two things behind it did
> not keep up.
>
> **2.4g — the ignore registry grows without bound.**
> `library/watcher.rs` holds a process-lifetime
> `static IGNORE_LIST: OnceLock<Mutex<HashMap<PathBuf, Instant>>>`. Its only
> pruning is inside `is_ignored`, for the exact path asked about, once its
> deadline has passed. The comment justifying that says the registry "only ever
> holds a handful of recently-written paths at a time" — true when the tag
> editor was the only caller, false now.
> `writeback_publish::publish` arms a window for **two** paths per publication,
> and the temporary's name is unique by construction (64 random bits). The file
> is unlinked immediately, so no event ever carries that path again, so
> `is_ignored` is never called for it again, so its entry never leaves. **One
> permanently unprunable entry per published file** — a few hundred KB on a
> 1,686-track library, 15–20 MB on a 100k one, growing again every batch.
> Fix either way: a bounded sweep in the registry, or have `publish` drop the
> temporary's entry once the file is gone. **Correct the comment** — the wrong
> rationale sitting in the code matters more than the bytes, because the next
> person will read it and build on it.
> Test that a publication leaves no permanent entry behind. `watcher.rs` has a
> test-only `ignored_paths()` snapshot you can use.
>
> **2.4h — the leftover sweep is quadratic.** `publish()` calls
> `sweep_leftovers(target.parent())` on **every** publication, and that is a
> full `read_dir`. Album folders make it free; a flat music directory does not.
> Measured on warm tmpfs, replicating the sweep's own logic:
> | files in the directory | one sweep | a batch over all of them |
> | ---: | ---: | ---: |
> | 200 | 0.17 ms | negligible |
> | 2,000 | 1.20 ms | 2.4 s |
> | 10,000 | 7.43 ms | **74 s** |
> Seventy-four seconds of pure directory scanning before any lyrics are
> fetched. **Keep the sweep** — an abandoned temporary in someone's album
> folder is exactly the litter nobody else looks for. Move it: once per
> directory per batch, or once at startup. A test should show that N
> publications into one directory perform one sweep, not N.
>
> Commits: `fix(writeback): stop the watcher ignore registry from growing without bound`
> and `perf(writeback): sweep a directory once per batch instead of once per file`

### Agent 6 — hoist the callback borrows

> **Branch** `fix/scrutinee-borrows` from `dev`. Mechanical, one commit per
> file or one for all nineteen — your call, but keep it reviewable.
>
> **The rule** (`AGENTS.md`): never hold a `Ref`/`RefMut` across a call that
> can re-enter GTK or a callback. The dangerous shape is a borrow used as a
> **scrutinee**, because Rust keeps scrutinee temporaries alive until the end
> of the whole statement, body included.
>
> **Two things that look like protection and are not** — measured with `rustc
> 1.94.1` by asking `try_borrow_mut()` inside each body:
> - `.clone()` and `.take()` in the scrutinee do **not** release the borrow.
>   The value is owned, so the code reads as if the cell were free. It is not.
> - Edition 2024 would **not** fix this. It shortened `if let` temporaries for
>   the `else` branch only; then-bodies, match arms and loop bodies are
>   unchanged.
>
> Only hoisting works:
> ```rust
> let callback = cell.borrow().clone();   // Ref drops at this semicolon
> if let Some(callback) = callback { callback(...); }
> ```
>
> **Your nineteen sites** — each reads a callback out of a `RefCell` and
> invokes it while borrowing the cell that holds it, so code the owner does not
> control runs inside the borrow:
> `device_sync/device_sync_target_browser.rs` 239, 251, 279, 318, 455 ·
> `library_doctor/progress_card.rs` 115, 126, 138 ·
> `podcasts/podcasts_failure_ui.rs` 168 · `podcasts/podcasts_view.rs` 252 ·
> `radio/add_dialog.rs` 458 · `radio/radio_filter_bar.rs` 228 ·
> `radio/radio_view.rs` 411, 528, 646 · `releases/releases_filter_bar.rs` 211 ·
> `releases/releases_view.rs` 320 · `track_list/track_list_reload.rs` 306 ·
> `view_session.rs` 138. (Line numbers are from `577765b`; re-run
> `python3 scripts/tests/scan-scrutinee-borrows.py` from the repository root
> for the current list.)
>
> **This is latent, not a live crash** — every callback slot has one writer, a
> `set_on_*` method, and all of those run in window construction or view
> wiring. Do not write a test claiming to reproduce a panic; you cannot. Write
> tests where the existing suite already covers the surrounding behaviour, and
> otherwise treat this as a refactor whose proof is that the suite stays green.
> What makes it worth doing: the guard is convention rather than
> compiler-enforced, the violating idiom looks correct, a panic in a GTK
> callback is a process abort, and one new callback registration inside a
> handler turns a latent site into a silent crash for a tester.
>
> **Do not** enable `clippy::significant_drop_in_scrutinee` in this branch — it
> would fire on the 22 GTK-call sites and the 31 data sites too. Note in the
> commit message that it becomes possible once those land.
> Commit: `refactor(ui): hoist callback borrows out of their scrutinees`

### Agent 7 — three security debts

> **Branch** `fix/defensive-gaps` from `dev`. Three independent tasks, one
> commit each. None is exploitable today; all three are one refactor away from
> being so.
>
> **2.4c — `--` before every yt-dlp positional argument.**
> `podcasts/ytdlp.rs` (`list`, `resolve`, and the `run` helper) and
> `ytdlp_download.rs` append the URL as the last positional argument with no
> `--` separator. yt-dlp parses options at any position, so the string's
> content alone decides whether it is read as a URL or an option.
> It is unexploitable today for three **independent and accidental** reasons:
> input goes through `url_detect::detect` which admits only `http`/`https`;
> episode URLs are built from a literal prefix; search terms become
> `ytsearch5:{terms}`. Nothing *holds* that invariant — a future caller passing
> a stored `feed_url` straight through breaks it and the compiler says nothing.
> Insert `--` immediately before the first positional argument, and add a debug
> assertion that the URL starts with `http://` or `https://`.
> Add to `scripts/check-architecture.sh` a check that no `Command::new`
> yt-dlp path passes a positional without the separator.
> Commit: `fix(podcasts): put yt-dlp positional arguments behind a -- separator`
>
> **2.4d — image decode limits.** `cover.rs:173`
> (`image::load_from_memory`), `cover.rs:242` (`image::open`) and
> `cover_download.rs:295` decode without explicit `image::Limits`. The byte cap
> in front (20 MB for covers, 4 MB for source artwork) covers the simple case;
> a decompression bomb — a small PNG with an enormous pixel area — is not
> covered. Set `Limits` with `max_alloc` and a maximum edge length at each
> decode site. Test with a crafted small-file/huge-dimensions image that the
> decode is refused rather than allocated.
> Commit: `fix(cover): bound image decoding against decompression bombs`
>
> **2.4a — one truth for "has a sidebar row".**
> `ui/browse/filter_restriction.rs:21` (`has_place_pill`) and
> `ui/sidebar/sidebar.rs:466` (`has_sidebar_row`) draw the same distinction in
> two separate `matches!` expressions in two modules. They agree today and
> nothing keeps them agreeing; the next place added will be added to exactly
> one of them. Make it one function with two callers. Keep both call sites'
> names if they read better locally — the point is one `matches!`, not one
> name. The existing `fil_1c_*` and `sidebar_tests.rs` tests are your net.
> Commit: `refactor(ui): one truth for whether a place has a sidebar row`

---

## 4. What none of them should do

- **Do not fix another agent's inherited red.** If `check-frontend-thinness.sh`
  fails and you are not agent 1, that is agent 1's task 0.0.
- **Do not chase GitHub Actions.** It fails on every branch in this repository
  including `main`; the job never reaches a runner. The enforcing gate is the
  local run.
- **Do not merge.** Open the PR, report, stop.
- **Do not lower a gate to land faster.** The 800-line file limit, the
  orchestrator caps and the thinness budgets are why a 293k-line repository is
  still navigable.
- **Do not flip a `[planned]` rule to `[active]` without its test in the same
  commit**, and never retroactively.
