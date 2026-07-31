---
slug: consolidation-plan
worktree: .worktrees/consolidation-wave-0
branch: feat/consolidation-wave-0
phase: planned
codex_session:
created: 2026-07-31
base: 577765b
foundation_schema: 51
foundation_ux_section: I, G
---
# Consolidation plan — from the review to landed work

Execution document for `docs/plans/architecture-consolidation.md`. The review
says *what* and *why*; this says *in which order, with which failing test, in
which files, behind which gate*.

Base `origin/dev` @ `577765b`. Schema 50 today, 51 after wave 1.

**Depth is staged on purpose.** Waves 0 and 1 are written out task by task,
because they run next and wave 0 blocks the test release: a session should be
able to work through §3 and §4 top to bottom without re-deriving a decision.
Waves 2 to 5 are fixed at package level with file ownership, ordering and
acceptance; their task breakdown happens when the package starts, against
whatever the tree looks like then. A plan that describes work three months out
line by line describes work that will not exist in that shape.

| Wave | Content | Blocks the test release? |
| --- | --- | --- |
| 0 | Release blockers: startup path, logging, crash report, MSRV, shipped scope | **yes** |
| 1 | The missing index on the default sort | no, but immediately noticeable |
| 2 | Source grammar: HTTP boundary, filter bar, add dialog | no |
| 3 | Core API for a second app: `CoreError`, ports | no |
| 4 | Execute the runtime decision | no |
| 5 | FTS and keyset paging — only after measuring | no |

Out of scope: new features, UI redesign, and the stem feature beyond task 0.8.

---

## 1. Preflight — before a line of code

### 1.1 Safety rules that are live on the target machine

That machine holds real data. These four are not negotiable, and the most
common way to break them is a literally-followed command that omits one of
them.

1. **The real database is off limits.** `~/.local/share/reprise/reprise.db`
   (1,686 tracks), library root `/home/marvin/Music`. Do not scan it, do not
   mutate it, do not point tooling at it.
2. **Every app launch is fully isolated.** Every run/smoke command line must
   contain **all** of this:

   ```bash
   dbus-run-session -- xvfb-run -a env \
     XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) \
     XDG_STATE_HOME=$(mktemp -d) \
     GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
     cargo run
   ```

   `XDG_STATE_HOME` is **new and load-bearing** in wave 0: task 0.1 puts the
   log file there. Without it, a smoke run writes into the user's real
   `~/.local/state/reprise/`. Grep your own command for `XDG_DATA_HOME` **and**
   `XDG_STATE_HOME` before running it.
3. **Never clone or build under `/tmp`.** It is a 16 G tmpfs; a `target/` there
   lives in RAM. Use `.worktrees/consolidation-wave-0`. The small
   `$(mktemp -d)` directories in the isolation recipe above are fine.
4. **Do not share `CARGO_TARGET_DIR`.** Cargo takes an exclusive lock; one
   shared target directory serialises parallel agents.

### 1.2 Establish the base

```bash
cd ~/Projects/reprise
git fetch origin dev
git worktree add .worktrees/consolidation-wave-0 -b feat/consolidation-wave-0 origin/dev
cd .worktrees/consolidation-wave-0
git log --oneline -1        # 577765b or newer
```

If `dev` has moved on, that is fine — work against the newer state and note the
deviations from this plan in the first task.

### 1.3 Base gate: prove that green is green

Before changing anything, run the full gate on the **untouched** base and write
the result down:

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo audit
scripts/check-architecture.sh
scripts/check-frontend-thinness.sh
scripts/check-ux-traceability.sh
```

This is not ceremony. The ledger records several sessions that attributed an
already-red gate to their own work. **Note the test count from this run** —
every task below says how it should change.

Known and accepted:
- `cargo audit`: RUSTSEC-2024-0436 only (`paste`, via `lofty`). A **new**
  advisory means STOP.
- `scripts/check-stem-runtime-packaging.sh` is red on the base (missing ONNX
  markers in `build-aux/meson-cargo-build.sh`). It is not part of the merge
  gate; task 0.8 makes it cleanly switchable.

**Known and NOT accepted — fix this first (task 0.0).**
`scripts/check-frontend-thinness.sh` is **red on `origin/dev` right now**,
measured 2026-07-31 against an untouched checkout:

```
frontend thinness: filesystem grew from 17 to 19
frontend thinness: threads   grew from 14 to 15
```

`65f0b14` (`#189`, lyrics and covers) added
`crates/reprise-gnome/src/ui/lyrics/lyrics_batch.rs` with its own worker
thread and filesystem access, and did not move the budgets in the same commit
as the convention requires. That script is part of
`scripts/check-merge-readiness.sh`, so **every** wave-0 PR will hit it.

### 1.4 The cycle, per task — no exceptions

1. Write the failing test.
2. `cargo test -p <crate> <name>` → **see it red**. Do not skip this; a test
   that was never red proves nothing.
3. Minimal implementation.
4. Same test → green.
5. Full gate (§1.3).
6. **One** commit with the given title.
7. Append one line to `.superpowers/sdd/progress.md`:
   `Task N: complete (commit <hash>, base <hash>, <one sentence>)`.

### 1.5 Standing conventions

- **UX rules.** Any change to visible behaviour needs a rule in
  `docs/ux-rules.md`. New IDs are proposed in §5; they enter as `[planned]` and
  the implementing commit flips them to `[active]` **and** brings the
  rule-named test. The test name must read `fn <prefix>_<nr>_…`
  (`start_3_…`), or `check-ux-traceability.sh` will not find it.
  **Rule texts are English** — `AGENTS.md` claims German, which is wrong and is
  corrected by task 0.7.
- **Budgets are ceiling and floor.** `check-frontend-thinness.sh` stands at
  `rusqlite=112, filesystem=17, threads=14, workers=7`. A task that moves a
  category changes the number **in the same commit** and justifies it in the
  message. Task 0.1 does that for `filesystem`.
- **New string files** must be listed in `po/POTFILES.in`;
  `check-architecture.sh` checks four of them by name.
- **File size** below 800 lines; `window.rs`, `track_list.rs` and `sidebar.rs`
  below 600.
- **Language.** Code, comments, log/error/UI strings and commit messages in
  English. Planning documents in this repository are English as well.

### 1.6 Order within wave 0

```
0.0 green base ──► 0.1 diagnostics ──► 0.2 log file ──► 0.3 panic hook ──► 0.4 copy ──► 0.5 startup
                                                                                          │
       0.6 MSRV · 0.7 AGENTS.md · 0.8 shipped scope · 0.9 MCP · 0.10 ADR  ◄────────────────┘ (free)
       1.1 index (own branch, independent of wave 0)
```

0.0 comes first: the base gate is red today and a wave that starts from a red
base cannot tell its own failures from inherited ones. 0.1 to 0.5 are then a
chain, each using the previous one. 0.6 to 0.10 are independent and may run in
any order or in parallel. Task 1.1 depends on nothing in wave 0 and can go
first if a quick visible win is wanted.

---

## 2. Wave 0 — release blockers

Branch `feat/consolidation-wave-0` from `dev`. One squashed PR, ten commits
inside it. One to two days.

### Task 0.0 — make the base gate green again

**Goal.** Start wave 0 from a green base, so a later red is information.

`origin/dev` fails `scripts/check-frontend-thinness.sh` (§1.3). Two ways out,
and the choice is a judgement about `lyrics_batch.rs`:

- **Raise the budgets to the measured values** (`filesystem=19`, `threads=15`)
  with the reason in the commit message. Correct if the lyrics batch worker
  genuinely belongs in the frontend.
- **Move the work into `reprise-core`** and leave the budgets. Correct if it
  does not — the batch runs a provider chain and writes `.lrc` sidecars, which
  is core-shaped work, and `reprise-core::lyrics` already owns the providers,
  the cache and the circuit breaker.

The second is the better answer on the merits and the first is the honest one
for wave 0's timebox. **Pick the first, and record the second as a follow-up**
— a budget raised with a written reason is the mechanism working, whereas a
budget quietly raised is the mechanism dying.

Whichever is chosen, this lands **before** task 0.1, because task 0.1 moves the
`filesystem` budget again and two moves in one number are impossible to read
afterwards.

**Commit.** `fix(gates): restore the frontend thinness budgets to the measured values`

### Task 0.1 — `diagnostics`: where the log file lives, and its rotation

**Goal.** One place for log lines and crash reports that callers need not know
about. No GTK, so it is testable without a display.

| File | Action |
| --- | --- |
| `crates/reprise-gnome/src/ui/diagnostics/mod.rs` | new |
| `crates/reprise-gnome/src/ui/diagnostics/paths.rs` | new |
| `crates/reprise-gnome/src/ui/diagnostics/paths_tests.rs` | new |
| `crates/reprise-gnome/src/ui/mod.rs` | `mod diagnostics;`, alphabetically between `device_sync` and `dialogs` |
| `scripts/check-frontend-thinness.sh` | `filesystem` budget to the measured value |

**The API this task fixes:**

```rust
/// `$XDG_STATE_HOME/reprise/reprise.log`, or the data dir when the state
/// home is unset. Never fails: a caller that cannot write simply logs to
/// stderr alone.
pub(in crate::ui) fn log_path() -> Option<PathBuf>;

/// The previous run's log, kept as exactly one generation.
pub(in crate::ui) fn previous_log_path() -> Option<PathBuf>;

/// `$XDG_STATE_HOME/reprise/last-crash`, written by the panic hook.
pub(in crate::ui) fn crash_marker_path() -> Option<PathBuf>;

/// Moves an existing log aside so a run starts on a clean file. Returns the
/// io error for logging; callers deliberately ignore it — a read-only state
/// directory must never keep the app from starting.
pub(in crate::ui) fn rotate_on_start() -> io::Result<()>;

/// Bytes the running log may reach before further writes are dropped.
pub(in crate::ui) const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;
```

**Failing tests** (`paths_tests.rs`, all display-free):

```rust
#[test] fn log_path_follows_xdg_state_home()
#[test] fn log_path_falls_back_to_the_data_dir_when_state_home_is_unset()
#[test] fn rotation_keeps_exactly_one_previous_run()
#[test] fn rotation_reports_but_survives_a_read_only_directory()
```

**Environment variables in tests.** `std::env::set_var` is `unsafe` since Rust
2024 and tests run in parallel in one process. Prefer injection over a guard:
make `fn log_path_in(root: &Path) -> PathBuf` the pure function, have
`log_path()` read the environment exactly once and call it. Then no test needs
an environment variable at all.

**Why a cap rather than rolling rotation.** At 8 MB further writes are dropped;
it does **not** rotate a second time. A rotation mid-run would hand out half a
log later, and the log you want in full is the one from the crash.

**Budget.** Run `scripts/check-frontend-thinness.sh` and enter the reported
actual for `filesystem`. **Do not guess** — the script names the number. Note
that task 0.0 has already moved this budget once; this is the second move, and
the commit message should say which of the two added what.

**Commit.** `feat(diagnostics): give the app one log file with a bounded size`

### Task 0.2 — the log is mirrored to that file

**Goal.** The GTK crate's 793 `tracing` calls reach a place a tester can send.

| File | Action |
| --- | --- |
| `crates/reprise-gnome/src/main.rs` | extend `init_logging` |
| `crates/reprise-gnome/src/ui/diagnostics/mod.rs` | `file_writer()` |
| `crates/reprise-gnome/src/ui/diagnostics/paths_tests.rs` | one more test |

Today:

```rust
fn init_logging() {
    let filter = EnvFilter::try_from_env("REPRISE_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,lofty=error"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}
```

Target: `tracing_subscriber::registry()` with two layers instead of `fmt()` —
stderr as before, plus a file layer with `.with_ansi(false)`. Filter and
default stay word for word. With no file, everything runs unchanged on stderr
alone; logging must **never** keep the app from starting. `init_logging` stays
the first statement in `main`, before `i18n::init`, and the rotation from 0.1
happens inside it before the first write.

**Failing test:**

```rust
#[test] fn file_writer_is_absent_when_the_state_directory_cannot_be_created()
```

**A review point, not a code filter.** The file layer gets no redaction layer.
Music-collection paths and tokens are not logged today; that is something to
check in the PR, not to filter at runtime. A filter that scans free text
invites false confidence.

**Commit.** `feat(diagnostics): mirror the log to a file testers can send`

### Task 0.3 — panic hook, crash marker, a single offer

**Goal.** A crash leaves something behind. Today: nothing.

**Rule.** `START-4`, new — exact text in §5.

| File | Action |
| --- | --- |
| `crates/reprise-gnome/src/ui/diagnostics/crash.rs` | new |
| `crates/reprise-gnome/src/ui/diagnostics/crash_tests.rs` | new |
| `crates/reprise-gnome/src/main.rs` | hook as the very first statement |
| `crates/reprise-gnome/src/ui/window/window.rs` | check the marker at start, clear it on close |
| `crates/reprise-gnome/src/ui/strings_app_shell.rs` | two constants |
| `docs/ux-rules.md` | add START-4 as `[active]` |

**1. The hook, before everything else in `main`:**

```rust
fn main() -> glib::ExitCode {
    diagnostics::crash::install_hook();   // FIRST statement
    init_logging();
    …
```

`install_hook` sets `std::panic::set_hook`. It writes message, location and
`std::backtrace::Backtrace::force_capture()` to the log file, then creates
`crash_marker_path()` carrying version, schema version and timestamp.
`force_capture` is deliberate: a tester never sets `RUST_BACKTRACE`, and a
crash report without a backtrace is a crash report without content.

**2. On the next start**, in `window::build` after construction: if the marker
exists, show exactly **one** toast with a "Copy Diagnostics" action (task 0.4),
then delete the marker. No banner, no repetition.

**3. A clean shutdown** deletes the marker in the `close-request` handler.

**Failing tests** (`crash_tests.rs`):

```rust
#[test] fn start_4_a_crash_marker_written_by_the_previous_run_is_offered_once()
#[test] fn start_4_a_clean_shutdown_removes_the_marker()
#[test] fn crash_report_contains_version_schema_and_the_panic_location()
#[test] fn crash_report_never_contains_a_library_path()
```

The first two must carry the rule ID or the traceability gate will not see them
(`fn start_4_…`, snake case, with `#[test]` within the five lines above).

What is tested is the **state machine** (marker present → offer → marker gone),
not a real process abort. The hook itself is tested through
`crash::write_report(&PanicInfoLike, &path)` with synthetic input.

**A limit that belongs in the code.** An `abort()` from C (a GTK assertion, a
Wayland protocol error) never reaches the Rust hook. That is known and
accepted — the Rust-side `RefCell` panic is the common class. Put it in
`crash.rs` as a comment so nobody later believes the coverage is total.

**Commit.** `feat(diagnostics): a crash leaves a report and offers it once`

### Task 0.4 — "Copy Diagnostics" in the primary menu

**Goal.** The tester reaches the log without a terminal.

**Rule.** `FB-9`, new — text in §5.

| File | Action |
| --- | --- |
| `crates/reprise-gnome/src/ui/diagnostics/report.rs` | new |
| `crates/reprise-gnome/src/ui/diagnostics/report_tests.rs` | new |
| `crates/reprise-gnome/src/ui/primary_menu.rs` | action plus menu entry |
| `crates/reprise-gnome/src/ui/strings_app_shell.rs` | constants |
| `docs/ux-rules.md` | FB-9 |

**Menu entry — exact position.** `primary_menu.rs` has three sections (view,
library, settings). The entry belongs in settings, **after** "Help" and
**before** "About Reprise":

```rust
fn settings_section_entries() -> Vec<(String, &'static str)> {
    vec![
        (strings::text(strings::PREFERENCES), "win.preferences"),
        (strings::text(strings::KEYBOARD_SHORTCUTS), "win.keyboard-shortcuts"),
        (strings::text(strings::HELP), "win.help"),
        (strings::text(strings::COPY_DIAGNOSTICS), "win.copy-diagnostics"),   // new
        (strings::text(strings::ABOUT_REPRISE), "win.about"),
    ]
}
```

Plus `pub(super) const ACTION_COPY_DIAGNOSTICS: &str = "copy-diagnostics";`
beside the other action constants, and the `gio::SimpleAction` registered in
`install` following the `ACTION_ABOUT` pattern including `window.downgrade()`.

**Strings** (`strings_app_shell.rs`, near "Primary menu items."):

```rust
pub const COPY_DIAGNOSTICS: &str = N_!("Copy Diagnostics");
pub const DIAGNOSTICS_COPIED: &str = N_!("Diagnostics copied to the clipboard");
pub const CRASH_LAST_RUN: &str = N_!("Reprise closed unexpectedly last time");
```

That file is already in `po/POTFILES.in` — nothing to do. A **new** string file
would need adding there.

**Report contents.** App version, schema version, GTK/libadwaita versions,
enabled modules, interface language, and the last 64 KB of the log. Nothing
from the library.

**Failing tests** (`report_tests.rs`):

```rust
#[test] fn fb_9_the_report_carries_version_schema_modules_and_the_log_tail()
#[test] fn fb_9_the_report_is_capped_so_the_clipboard_stays_usable()
#[test] fn fb_9_the_report_omits_the_library_root_and_track_paths()
```

`report::build()` takes its inputs as parameters (version, schema, module list,
log path) rather than fetching them, which keeps all three tests free of a
display and of the environment.

**Copy rather than "open folder":** under Flatpak a file manager is not
guaranteed to be reachable; the clipboard is. Via `gdk::Display` →
`clipboard()`, then a toast through `toasts::show`.

**Commit.** `feat(diagnostics): let a tester copy a diagnostics report`

### Task 0.5 — the startup path reports instead of panicking

**Goal.** No `expect` on the only way into the app.

**Rule.** `START-3`, new — text in §5.

| File | Action |
| --- | --- |
| `crates/reprise-gnome/src/main.rs` | replace the `expect` |
| `crates/reprise-gnome/src/ui/startup_failure.rs` | new |
| `crates/reprise-gnome/src/ui/startup_failure_tests.rs` | new |
| `crates/reprise-gnome/src/ui/strings_app_shell.rs` | one headline per case |
| `docs/ux-rules.md` | START-3 |
| `scripts/check-architecture.sh` | ban `expect`/`unwrap` in `main.rs` |

From:

```rust
let conn = db::Db::open_migrated(Some(&path)).expect("failed to open or migrate database");
```

to:

```rust
let db = match db::Db::open_migrated(Some(&path)) {
    Ok(db) => db,
    Err(error) => return startup_failure::present(&app, &path, &error),
};
```

**Presentation — deliberately not a parentless dialog.** `AdwAlertDialog` is
the house form (`ui/dialogs.rs`, `ui/issues/missing_dialogs.rs`) but needs a
parent widget, and without a main window there is none. `present` therefore
opens a minimal `adw::ApplicationWindow` holding an `adw::StatusPage`: the
`dialog-error-symbolic` icon, a headline per case, the database path as a
secondary line, and two buttons — "Copy Diagnostics" (0.4) and "Close". That is
the same form `START-2` already prescribes for an unreachable library folder,
so there is no second language for the same state. If GTK initialisation itself
fails, `eprintln!` plus exit code 1 remains.

| `DbError` | Headline (in substance) | Realistic because |
| --- | --- | --- |
| `SchemaTooNew` | library comes from a newer version | a tester tried a newer build and went back |
| `SchemaNotReady` | library is not ready | should not happen, but is named rather than swallowed |
| `Io` | library cannot be opened | full disk, home on NFS, permissions |
| `Sqlite` | library file is damaged | a hard power-off |

The technical cause appears **only** in the report, never on the page — the
separation `SourceError` already draws and tests.

**Deliberately not:** automatic repair, renaming the file, falling back to an
empty database. A user's database is never touched unasked.

**Failing tests** (`startup_failure_tests.rs`, pure `DbError` → presentation
mapping, display-free):

```rust
#[test] fn start_3_a_newer_schema_names_the_downgrade_and_never_migrates()
#[test] fn start_3_an_io_failure_names_the_path_and_offers_diagnostics()
#[test] fn start_3_a_corrupt_database_offers_diagnostics_not_a_repair()
#[test] fn start_3_the_failure_copy_never_contains_the_technical_cause()
```

**Manual verification — only possible on the target machine:**

```bash
scratch=$(mktemp -d ~/.cache/reprise-scratch/startup.XXXXXX)
mkdir -p "$scratch/data/reprise"
sqlite3 "$scratch/data/reprise/reprise.db" "PRAGMA user_version = 99;"
dbus-run-session -- xvfb-run -a env \
  XDG_DATA_HOME="$scratch/data" XDG_CACHE_HOME=$(mktemp -d) \
  XDG_STATE_HOME=$(mktemp -d) \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
  cargo run
```

Expected: a StatusPage instead of a panic, exit code 1, and an `error!` line in
the log naming the case.

**Gate line, same commit** — in `scripts/check-architecture.sh`, with the other
frontend bans:

```bash
if rg --quiet '\.expect\(|\.unwrap\(\)' crates/reprise-gnome/src/main.rs; then
  echo "the startup path must report failures, not panic on them" >&2
  exit 1
fi
```

**Commit.** `fix(startup): report a database failure instead of panicking`

### Task 0.6 — make the MSRV honest

**Goal.** The declared toolchain is the one that builds.

Every manifest says `rust-version = "1.92"`. `cargo build -p reprise-core
--locked` fails reproducibly on rustc 1.94.1:

```
Compiling libsqlite3-sys v0.38.1
error[E0658]: use of unstable library feature `cfg_select`
  --> libsqlite3-sys-0.38.1/build.rs:110:9
```

`scripts/tests/msrv.sh` cannot catch it: it reads `cargo metadata` and checks
that the **field** says `1.92`. It never builds.

**Step 1 — measure, do not guess:**

```bash
rustc --version
cargo build --locked --workspace 2>&1 | tail -20

flatpak run --command=sh --devel org.gnome.Sdk//50 -c \
  '/usr/lib/sdk/rust-stable/bin/rustc --version'
```

**The second number decides.** `org.reprise.Reprise.yml` builds with
`CARGO_NET_OFFLINE=true` against `flatpak/cargo-sources.json`, i.e. against
exactly the pinned tree.

**Step 2, case A — the SDK toolchain builds.** Raise `rust-version` in
`[workspace.package]` to the measured version, and give `msrv.sh` a real build:

```bash
rustup toolchain install "$expected_msrv" --profile minimal
cargo "+$expected_msrv" build --locked --workspace
```

Keep the existing metadata check — it catches a manifest that does not follow.

**Step 2, case B — the SDK toolchain does not build.** Take
`rusqlite`/`libsqlite3-sys` back to the last version that does
(`cargo update -p rusqlite --precise <version>`), regenerate
`flatpak/cargo-sources.json` (the tooling is in `RELEASING.md`), and run
`scripts/check-release.sh` — it compares the checksums in `Cargo.lock` and
`cargo-sources.json` and fails if they diverge.

**Abort criterion.** If the rollback touches more than `rusqlite`, it is its own
PR, not a task in wave 0. Finish wave 0 without 0.6 and carry 0.6 separately.

**Optional, recommended.** A `rust-toolchain.toml` with the measured version so
developers and CI see the same one — check first whether it collides with the
Flatpak build, which brings its own.

**Commit.** `fix(build): make the declared MSRV the one that actually builds`

### Task 0.7 — bring `AGENTS.md` up to date

**Goal.** The first document an agent reads describes this project.

Five corrections, all verified:

1. **"Three-crate Cargo workspace"** → there are nine. Take the table from
   `README.md`, where it is correct.
2. **The roadmap** ends at "GUI-A2 (cover download)". Actually landed:
   podcasts, YouTube, radio, concerts, new releases, device sync, library
   doctor, my stats, the tag editor, stems, the runtime. Link the open runtime
   decision from task 0.10.
3. **"`docs/ux-rules.md` is the single UX source of truth (German)"** — that
   document is **English**. Sampled across FIL, PLAY, NAV, SET, STATS, RUN and
   FX: English rule texts throughout. This is the most dangerous of the stale
   claims, because it would lead a session to write a new rule in the wrong
   language and break the document's voice. Replace with: rule texts in
   English, and — per the repository's language decision — planning documents
   in English too.
4. **"Not released yet — no backwards compatibility"** → the cut-off rule:

   > **Released to testers — compatibility starts here.** From schema 50 /
   > version 0.1.1 onward, installations exist. Migrations are forward-only and
   > lossless. A field may disappear once a migration has carried its content
   > over; settings keys are migrated, not discarded. A clean data model does
   > not justify losing data in someone else's library.

   **Set this only with the actual release**, not before — while nobody has
   installed, the old rule is still right and useful.
5. The baseline test count ("390 passed; 1 ignored") is stale by orders of
   magnitude. Either the measured number or "see the latest ledger entry" — a
   wrong number is worse than none.

No test; documentation only. The full gate runs regardless.

**Commit.** `docs: describe the workspace and the compatibility rule as they are`

### Task 0.8 — shipped scope: runtime and stems

**Goal.** What ships is what a surface uses.

Files: `meson.build`, `data/meson.build`, `meson_options.txt`,
`scripts/check-runtime-service-install.sh`, `scripts/check-release.sh`.

1. A new option in `meson_options.txt`:

   ```meson
   option('runtime_service', type: 'boolean', value: false,
          description: 'Install the headless runtime service (no surface uses it yet)')
   ```

2. Put the `reprise-runtime` target and both `.service` files behind
   `get_option('runtime_service')`.
3. `scripts/check-runtime-service-install.sh` only checks **when** the option
   is on — and then as strictly as before, in both prefixes.
4. `stem_backend` off for the test round; `check-release.sh` consistently skips
   `check-stem-runtime-packaging.sh` when the option is off. The red release
   check stops being a blocker **without** anyone silencing it.

The crates stay in the workspace and keep being built and tested; they are just
not installed.

**Verification:**

```bash
meson setup build-off . --prefix="$HOME/.local"
meson introspect build-off --installed | grep -c reprise-runtime   # 0

meson setup build-on . --prefix="$HOME/.local" -Druntime_service=true
scripts/check-runtime-service-install.sh                            # green
```

**Commit.** `build: ship only what a surface uses (runtime service, stems opt-in)`

### Task 0.9 — MCP off by default, capabilities visible

**Goal.** No agent access nobody switched on.

**Check before building.** Establish whether `reprise-mcp` is reachable at all
without a user action. The server is a separate binary an MCP client launches;
if it is only ever started externally, there is nothing to switch off and the
task shrinks to the visibility line. **Record that finding in the commit** so
the question is not asked a third time.

If it shrinks: one line on the plugins page naming the granted capability
classes (read / mix planning / playlist creation — the three from
`CONTEXT.md`). No new mechanism, only visibility.

If it is reachable: additionally a module descriptor in
`crates/reprise-core/src/modules.rs` with `default_enabled: false`, following
the other `ONLINE_MODULES`, and hang the gate in `capability.rs` on it.

**Commit.** `feat(preferences): name the agent capabilities that are granted`

### Task 0.10 — record the runtime decision as an ADR

**Goal.** The limbo ends in a decision rather than in forgetting.

Files: `docs/adr/003-runtime-ownership.md` (new),
`docs/plans/architecture-consolidation.md` (§2.2, note the decision taken).

Follow ADR 002's shape:

- **Status** — accepted on, with a date.
- **Context** — the numbers from the review: ~15,400 lines across five places,
  GTK is a client of nothing, MCP/CLI go through MPRIS, two command surfaces
  for one domain, a D-Bus service ships that nothing uses.
- **Decision** — A (cut over) or B (shelve). The plan recommends **B for the
  test round**.
- **Consequences** — under B: the crates stay built and tested but are not
  installed (task 0.8); the parity tests remain as evidence that the runtime
  matches the behaviour.
- **Resumption trigger** — the most important section. For example: "as soon as
  a second frontend begins" or "as soon as an agent should control playback
  with no window open". Without a named trigger, "shelved" quietly becomes
  "abandoned", and then 15,000 lines sit in the repository with no owner.

**Commit.** `docs: record the runtime ownership decision as an ADR`

### Acceptance — wave 0

- [ ] A deliberately triggered panic leaves a log line with a backtrace, a
      marker, and is offered **exactly once** on the next start.
- [ ] `PRAGMA user_version = 99` produces a StatusPage, not a panic (command in
      task 0.5).
- [ ] "Copy Diagnostics" produces a report with no library paths.
- [ ] `scripts/tests/msrv.sh` fails when `rust-version` is lowered artificially.
- [ ] `meson setup` with no options installs neither `reprise-runtime` nor the
      `.service` files.
- [ ] `scripts/check-ux-traceability.sh` counts three more active rules.
- [ ] Full gate green, `scripts/check-merge-readiness.sh` green.
- [ ] Display tests: the herd via `scripts/check-display-tests.sh --rule-named`
      (the script takes `--rule-named | --motion | --css`, **not** a test
      name). If one breaks, rerun it **individually** — only the single run is
      evidence:

      ```bash
      env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) \
          XDG_CONFIG_HOME=$(mktemp -d) XDG_STATE_HOME=$(mktemp -d) \
          GIO_USE_VFS=local GTK_USE_PORTAL=0 \
          GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
        dbus-run-session -- xvfb-run -a \
          cargo test -p reprise-gnome <testname> -- --ignored --exact
      ```

---

## 3. Wave 1 — the missing index

Own branch `feat/library-sort-index` from `dev`, independent of wave 0.

### Task 1.1 — migration 51: an index for the default sort

| File | Action |
| --- | --- |
| `crates/reprise-core/src/db_sort_indexes.rs` | new |
| `crates/reprise-core/src/lib.rs` | `mod db_sort_indexes;` beside the other `db_*` |
| `crates/reprise-core/src/db.rs` | `SUPPORTED_SCHEMA_VERSION` 50 → 51, call at the end of `migrate_with_cache_dirs` |

**Template in the repository:** `db_recently_added.rs::migrate_v35` — same
shape: version check, `unchecked_transaction`, `execute_batch`,
`pragma_update`, `commit`. Copy that file's structure rather than inventing
one.

```sql
CREATE INDEX IF NOT EXISTS idx_tracks_present_artist_order
ON tracks(artist COLLATE NOCASE, year, album COLLATE NOCASE, track_no)
WHERE missing_since IS NULL AND removed_at IS NULL;
```

The column order must match `SORT_WHITELIST["artist"]` in
`crates/reprise-core/src/queries/clauses.rs` **exactly** — today
`artist COLLATE NOCASE, year, album COLLATE NOCASE, track_no`. The `WHERE`
clause must match `clauses::PRESENT`.

**Failing tests:**

```rust
#[test] fn v51_serves_the_default_artist_sort_from_an_index()
#[test] fn v51_is_idempotent_and_bumps_the_schema_version()
```

The first is the point. It asserts **not** a duration — those drift per machine
— but that SQLite *picks* the index:

```rust
let db = Db::open_in_memory().unwrap();
// `clauses` is a private module; the builder is re-exported as
// `queries::build_track_query` (queries/mod.rs:119).
let sql = crate::queries::build_track_query("artist", "ASC", false);
// EXPLAIN QUERY PLAN over sql: the output must not contain
// "USE TEMP B-TREE FOR ORDER BY" and must name
// "idx_tracks_present_artist_order".
```

The builder expects two bound parameters (`LIMIT ?1 OFFSET ?2`) — pass them or
the prepare fails.

**Statistics.** On an empty table the planner may spurn the index. Fill the test
with a few hundred rows and run `ANALYZE`, or reuse the fixture from
`queries/tests.rs`. If it passes on an empty table too, so much the better —
then it is stricter than required.

**Prove it, do not claim it.** `performance-query-compare.sh` compares **two
directories** produced by `performance-baseline.sh`; it does not measure
anything itself. The tree must be clean for both runs, because the script
records the commit in its manifest:

```bash
# 1. on the untouched base, BEFORE the migration
scripts/performance-baseline.sh ~/perf/before

# 2. with the index, after the commit
scripts/performance-baseline.sh ~/perf/after

# 3. compare
scripts/performance-query-compare.sh ~/perf/before ~/perf/after
```

Negative time deltas are improvements; positive database-byte deltas are the
price of the index. `--quick` measures 10k only — for this task the 100k run is
the interesting one, so run it **without** `--quick`. **Put the comparison JSON
in the commit message.** The review's numbers (0.44 / 1.95 / 3.37 ms against
14.9 / 312 / 380 ms) come from a replica with synthetic rows in Python's
SQLite; this run's numbers are the real ones. If they differ markedly, **the
measurement** is the truth and the review was the estimate.

**Deliberately one index.** `added_at` is the next candidate, but every index
costs write load during a scan. Measure first — as a separate task 1.2, only if
the comparison supports it.

**Commit.** `perf(db): serve the default library sort from an index`

### Task 1.2 (conditional) — `PRAGMA optimize` after large scans

Only if 1.1 shows the planner going wrong. `PRAGMA optimize` at the end of
`scanner::scan_folder`, after the commit, errors logged and dropped. Failing
test: after a scan, statistics exist in `sqlite_stat1`.

### Acceptance — wave 1

The plan test is green, both performance reports are attached, and the scroll
time in the 100k profile has measurably fallen in a
`performance-baseline.sh` run.

---

## 4. Wave 2 — consolidate the source grammar

A branch per package from `dev`, in this order. **Package 2.1 first** — it
creates the place where the security and policy points live once instead of
sixteen times.

### Package 2.1 — `reprise_core::net`: one HTTP boundary

**Owned files.** New `crates/reprise-core/src/net/{mod,client,rate,breaker,fixtures}.rs`;
converted: `podcasts/http.rs`, `radio/http.rs`, `concerts/http.rs`,
`musicbrainz.rs`, `artist_portrait/deezer.rs`, `lyrics/lrclib.rs`,
`lyrics/netease.rs`, `podcasts/source_artwork.rs`, `cover_download.rs`.
**Excluded:** `scrobbling*` and `library/listenbrainz.rs` (own auth signature,
own rhythm) — after the rest, if at all.

- `SourceClient { agent, user_agent, timeout }` as the only place a
  `ureq::Agent` is constructed.
- **One** rate limiter, keyed **per host** rather than per module. Today there
  are five separate `static LAST_REQUEST` mutexes with no shared budget.
- The circuit breaker is **not rewritten**: `lyrics/breaker.rs` (`#189`) is
  already host-keyed and correctly shaped — lift it to `net/breaker.rs` and
  connect every source to it.
- `SourceTransportError` as the shared return type; the domain enums
  (`PodcastError`, `RadioError`, `ProviderError`, …) lose their HTTP arms and
  keep only the domain ones.
- **One** fixture variable `REPRISE_HTTP_FIXTURE_DIR` with a subdirectory per
  provider, replacing five.
- Check redirect targets: loopback, link-local and private ranges are rejected
  and reported as `Unreachable` (review §7.4).

**Failing tests.** The policy tests against the new boundary first — rate
budget, breaker opening, rejected redirect, size limit — then each converted
source's existing tests, unchanged and green.

**Migration cut.** Source by source, each in its own commit. The old path is
deleted in the same commit the new one takes over — two boundaries side by side
would be exactly the state this package removes.

**Acceptance.** `rg -c 'ureq::Agent::config_builder' crates/reprise-core/src`
reads 1 (plus the excluded scrobbling paths), and a new gate line caps the
number (review §11, gate 4).

### Package 2.2 — one filter bar

**Owned files.** `ui/browse/*` (the generic bar),
`ui/podcasts/podcasts_filter_bar.rs`, `ui/radio/radio_filter_bar.rs`,
`ui/releases/releases_filter_bar.rs`, `ui/concerts/concerts_filter_bar.rs`.

`FilterBar<F: FilterModel>` owns geometry, chip construction, popover
navigation (facet and value pages), "Clear all" and the counting line. Each
source keeps a `FilterModel` impl: facets, labels, values, persistence key —
expected 60 to 120 lines instead of 300 to 570.

The duplicated constants (`FILTER_BAR_MIN_HEIGHT` ×5, `FACET_PAGE`/`VALUE_PAGE`
×3) disappear with it; the gate line from review §11 keeps them away.

**Important.** Section K of `docs/ux-rules.md` then applies to every source for
the first time — today it reaches only `browse_bar`. Any K rule that should not
apply to a source needs an explicit exception there, not a silent deviation.
Expect one to three such exceptions.

**Precedent in the repository.** `#193` already made this exact cut with
`ui/source_reveal.rs`: shared decision, source-specific execution.

### Package 2.3 — one add dialog

**Owned files.** New `ui/source_add_dialog.rs`; converted
`ui/podcasts/add_dialog*.rs`, `ui/radio/add_dialog.rs`,
`ui/radio/radio_add_input.rs`.

The phase machine (`Idle → Searching → Results → Previewing → Preview → Error`),
the generation counter and the result list move into the shared dialog. Each
source keeps a trait with `classify_input`, `search`, `preview`, `commit` and
its copy identities.

**The net.** Both dialogs have their own tests (`add_dialog_tests.rs` per
source); they stay and become the proof that merging changed nothing.

### Package 2.4 — small debts of the same family

One commit each, independent of one another:

| # | Content | Review |
| --- | --- | --- |
| 2.4a | `has_place_pill()` / `has_sidebar_row()` into one function with two callers | §5.3 |
| 2.4b | Check `youtube_channel_detail` against FIL-1c and align if needed | §5.3 |
| 2.4c | `--` before every yt-dlp positional argument, debug assertion on `http(s)://` | §7.2 |
| 2.4d | `image::Limits` (edge length, `max_alloc`) at every decode site | §7.6 |
| 2.4e | `recv_or_fault` in `one_shot_task`; `delete_tracks.rs` as the first caller | §8.3 |
| 2.4f | `--cookies-from-browser`: copy in the plugin area, env override debug-only | §7.3 |
| 2.4g | Bound the watcher's ignore registry, or drop the temporary's entry once the file is gone | §8.5 |
| 2.4h | Stop sweeping the directory on every publication — once per directory per batch is enough | §6.3.6 |

2.4g and 2.4h both come from `#189` and are worth doing **before** the test
round rather than in wave 2, because a library-wide lyrics batch is exactly
the kind of thing a tester will try on their whole collection on day one. They
are small: one bounded sweep, one moved call.

2.4a and 2.4c are each under an hour and each close a finding completely — good
entry tasks.

---

## 5. Wave 3 — core API for a second app

After wave 2, because 2.1 already recuts the network layer's error type and
both would otherwise touch the same signatures.

### Package 3.1 — `CoreError`

858 public signatures return `Result<_, rusqlite::Error>` today. Target:
`reprise_core::CoreError` with `NotFound`, `Conflict`, `Busy`, `Invalid`,
`Backend(String)`; `rusqlite::Error` folds in via `#[from]` and is never handed
out.

Module by module, with the `From` impl carrying the intermediate states so
every stage compiles. Order by call frequency: `queries/` first (the widest
surface), then `library/`, then the rest.

**`Busy` is not cosmetic.** `reprise-cli` inspects busy/lock codes directly
today, which is why it is its own variant rather than a `Backend(String)`.

### Package 3.2 — remove `rusqlite` from the headless surfaces

Possible once 3.1 lands. Then a gate line in `check-architecture.sh` banning
`rusqlite` in `reprise-cli`/`reprise-mcp` — a comment becomes a check.

### Package 3.3 — parameter objects

`query_track_window` exists in four overloads with up to eleven parameters;
`queries/mod.rs` alone carries seven `#[allow(clippy::too_many_arguments)]`. A
`TrackWindowQuery { source, sort, filter, browse, window, queue_items, ai }`
replaces them. Purely mechanical, high readability gain, no behavioural risk.

### Package 3.4 — view ports instead of `RuntimeWiring`

`RuntimeWiring` has over 40 fields and knows every view. Target: a narrow
`…Ports` struct per view with exactly its collaborators; `RuntimeWiring` builds
those and hands them over, and the view stops knowing `RuntimeWiring`.

Incremental, one view per commit. Start with a small one (`ConcertsView` or
`ReleasesView`) so the shape proves itself on a cheap case before `TrackList`
follows.

---

## 6. Wave 4 — runtime

Only if ADR 003 (task 0.10) decided on **cut over**. Packages analogous to
"episodes as queue citizens":

1. Wire the ports (GStreamer backend and Linux device effects to
   `runtime::ports`).
2. `PlayerController` reads snapshots instead of owning state.
3. Queue commands go to the runtime; `queue_transport`/`up_next_transport`
   become projections.
4. The MPRIS mirror is fed by the runtime.
5. MCP/CLI move from MPRIS to `org.reprise.Reprise1`.
6. Promote `transport_parity_tests` from a net to a contract and delete the
   GTK-side copy.
7. The Meson option from task 0.8 defaults to `true`.

One PR per package. Step 6 is where the duplication actually disappears; before
it, this is a redirection rather than a consolidation.

---

## 7. Wave 5 — only after measuring

Re-evaluate after wave 1, not before:

- **FTS5** over `(title, artist, album, genre)`, contentless, maintained by
  triggers. Included in `rusqlite`'s `bundled`, so no new dependency. An
  intermediate step that may already suffice: stop counting the total exactly
  above a threshold and answer "more than N" with a `LIMIT`.
- **Keyset paging** instead of `OFFSET`. Needs stable tiebreakers in
  `SORT_WHITELIST` — a larger change, worth it only if libraries beyond 100k
  actually occur.

---

## 8. The three new UX rules — text to paste

**English**, like the rest of the document. Format exactly
`- **ID** [status] [level] — text`, or `check-ux-traceability.sh` will not find
them. Each rule enters as `[active]` in the **same** commit as its test — never
retroactively, never without a test.

**Section I. Start state — append after START-2:**

```markdown
- **START-3** [active] [gtk] — A database that cannot be opened is
  reported, never a panic. The startup path presents a StatusPage
  naming the case — library from a newer version, library not ready,
  library cannot be opened, library file damaged — with the database
  path as a secondary line and two actions: copy diagnostics (FB-9)
  and close. The technical cause appears only in the diagnostics
  report, never on the page, the same separation SourceError draws.
  Reprise never repairs, renames or replaces the file on its own: a
  library it cannot read is still the user's library.
- **START-4** [active] [gtk] — A run that ended in a panic leaves a
  report in the log and a marker. The next start offers to copy the
  diagnostics exactly once — a toast with one action, not a banner —
  and clears the marker whether or not the offer was taken. A clean
  shutdown clears the marker too, so the offer only ever follows an
  actual crash.
```

**Section G. Feedback vocabulary — append after FB-8:**

```markdown
- **FB-9** [active] [gtk] — "Copy Diagnostics" in the primary menu puts
  one self-contained report on the clipboard: app version, schema
  version, toolkit versions, enabled modules, interface language, and
  the tail of the log. The report is capped so the clipboard stays
  usable, and it never carries the library root or any track path —
  what is wrong with Reprise is diagnosable without shipping what the
  user listens to.
```

| Rule | Test that greens it |
| --- | --- |
| `START-3` | `fn start_3_a_newer_schema_names_the_downgrade_and_never_migrates()` and siblings |
| `START-4` | `fn start_4_a_crash_marker_written_by_the_previous_run_is_offered_once()` |
| `FB-9` | `fn fb_9_the_report_carries_version_schema_modules_and_the_log_tail()` |

The gate looks for `fn <prefix>_<nr>_` with a `#[test]` within the five lines
above. A helper `fn` with a matching name does not count — by design.

`START-1` and `START-2` stay `[planned]`; this plan does not touch them.

---

## 9. Gate additions as their own work

Each line makes a finding unrepeatable. They land **with** the task that closes
their finding, never as a sweep at the end.

| Gate | Checks | With task |
| --- | --- | --- |
| `msrv.sh` really builds | the declared toolchain | 0.6 |
| no `expect`/`unwrap` in `main.rs` | the startup path | 0.5 |
| `ureq` agent budget in `reprise-core` | the HTTP boundary, lowerable only | 2.1 |
| yt-dlp positional arguments behind `--` | argument injection | 2.4c |
| duplicated UI constants defined exactly once | the filter bars | 2.2 |
| unique section letters in `ux-rules.md` | two sections "T", no "AC" | 0.7 |
| `rusqlite` banned in `cli`/`mcp` | the core API | 3.2 |
| runtime installed only when used | shipped scope | 0.8 |
| `cargo deny` in the release gate | licences, duplicates | own task, wave 2 |

---

## 10. Risks and abort criteria

- **Task 0.6 may force a dependency rollback.** Case B touches `Cargo.lock` and
  `flatpak/cargo-sources.json`, both of which `check-release.sh` compares by
  checksum. If the rollback reaches beyond `rusqlite`, it is its own PR, not a
  task in wave 0.
- **Package 2.1 is the single largest risk in this plan.** Nine sources with
  their own failure paths and fixtures. Hence: one source per commit, the old
  boundary deleted in the same commit, and scrobbling excluded.
- **Package 2.2 touches visible behaviour in four views.** The display tests
  are herd-flaky; only single runs count as evidence. Budget time for it.
- **Wave 3 touches 858 signatures.** Mechanical, but the PR will be large.
  Per-module commits are not a matter of taste there; they are the condition
  for anyone being able to read it.
- **Abort criterion for wave 4:** if ADR 003 decided B, wave 4 is not started
  "just in case". Half a cut-over is worse than either end.

---

## 11. Acceptance per wave

**Wave 0.** A deliberately triggered panic leaves a log line, a marker and
exactly one offer on the next start. `user_version = 99` produces a StatusPage.
The diagnostics report contains no library path. `msrv.sh` fails on an
artificially lowered `rust-version`. `meson setup` with no options installs
neither the runtime nor the `.service` files. Full gate green.

**Wave 1.** The planner picks the new index; both performance reports are
attached to the commit message.

**Wave 2.** One `ureq` agent construction in the core (plus scrobbling), one
filter bar implementation, one add dialog, one `has_sidebar_row`. Every
existing source test unchanged and green — merging must not change behaviour.

**Wave 3.** `rusqlite` appears in no `Cargo.toml` of `reprise-cli` or
`reprise-mcp`, and the gate checks it. No
`#[allow(clippy::too_many_arguments)]` left in `queries/mod.rs`.

**Wave 4.** `crates/reprise-gnome/src/ui/playback/queue_transport.rs` and
`up_next_transport.rs` hold no queue semantics any more, only projection.
`transport_parity_tests` has become a contract test.

**Wave 5.** Plan it once wave 1's measurement exists.

---

## 12. When something goes wrong

- **A gate is red and you cannot tell whether it is you.** Repeat the base run
  from §1.3 against `origin/dev`. If it was red there, it is not your work —
  record it as a baseline in the ledger and do not repair it along the way.
- **A display test flickers.** Only single runs are evidence; the command is in
  the wave-0 acceptance list. A herd run breaking four tests is not a finding;
  four single runs breaking is one.
- **A task outgrows its scope.** Stop, commit what is reached, append the rest
  as its own task. One commit per task is the rule; two small commits beat one
  task holding up wave 0.
- **`cargo audit` reports a new advisory.** STOP. Do not filter, do not accept
  — that is its own decision with its own commit.
- **The ledger contradicts the code.** The code wins. `git log` is the truth;
  the ledger is the story about it.
