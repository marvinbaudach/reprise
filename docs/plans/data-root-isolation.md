---
slug: data-root-isolation
worktree: /home/marvin/Projects/reprise-data-root-isolation
branch: feature/data-root-isolation
phase: refactored
codex_session:
created: 2026-10-09
---
# Tests resolve a private data directory instead of the user's real ~/.local/share/reprise (#1241)

## Goal

No test build can read or write the user's real `~/.local/share/reprise` (real DB, staging,
podcast downloads, stem models, row-loss diagnostics), whether it runs as `cargo test
--workspace` or as a single `-p <crate>` suite, and with or without `XDG_DATA_HOME` set. This
uses the same mechanism as #1240 (`cache_root`): the isolation is compiled in, so nobody has to
remember to set an environment variable.

## Findings (origin/dev 01b04dee49)

- Five lookups of the platform data directory:
  - `db::default_path` (`crates/reprise-core/src/db.rs:80`)
  - `ai_staging::default_staging_dir` (`ai_staging.rs:26`)
  - `podcasts::downloads::default_download_root` (`downloads.rs:152`)
  - `reprise_stems::provision::default_model_dir` (`provision.rs:91`; stems depends on core)
  - `row_loss_watchdog.rs:129` in gnome, which writes `reprise/diagnostics` through
    `gtk4::glib::user_data_dir()`
- `preference_rhythmbox.rs:70` reads Rhythmbox's own `rhythmdb.xml` through
  `glib::user_data_dir()`. It is a different app's data and stays as it is.
- Only `reprise-android-ffi` enables `test-cache-root`. With `resolver = "2"`,
  `cargo test --workspace` unifies that dev-dependency feature into every crate's build, including
  the binaries built for integration tests. A `-p` run gets it only if that crate enables it.
- Test-built binaries end up at `target/debug/<bin>` too:
  - The CLI and MCP integration tests spawn them (`CARGO_BIN_EXE_*`). All of them pass `--db`, and
    `worker_basic` uses `XDG_DATA_HOME` only to keep the model directory empty. An isolated binary
    satisfies that as well.
  - Two scripts reuse a stale `target/debug/reprise` without rebuilding:
    `shoot-updates-popover.sh` and `verify-radio-favicons.sh`. After a test build they would see an
    empty library instead of the one they seeded. That is a safe failure, but a silent one. The same
    already holds for the cache since #1240.
- The ignored measurement `diagnostic_trail_tests.rs::measure_generated_library_reload_latency`
  validates `XDG_DATA_HOME` and then calls `db::default_path()`. Under the feature that would
  point at the private root.

## Design

1. **One feature, renamed:** `test-cache-root` becomes `test-private-dirs` and governs both roots.
   No compatibility alias is kept (AGENTS.md: nothing has shipped).
2. **New `crates/reprise-core/src/data_root.rs`**, a sibling of `cache_root.rs`:
   - `user_data_root() -> Option<PathBuf>`. Under `cfg(any(test, feature = "test-private-dirs"))`
     it returns `Some(temp_dir/reprise-test-data-<pid>)`. Otherwise it is the single sanctioned
     `dirs::data_dir()` call.
   - `is_isolated()`.
   - It returns `Option` so that each caller keeps its own fallback: `"."` for db, staging and
     podcasts; `ProvisionError::NoDataDir` for stems. Behaviour stays identical apart from the root.
   - `cache_root` switches its cfg to the renamed feature.
3. All five lookups go through `user_data_root()`. Gnome's diagnostics directory becomes
   `user_data_root().unwrap_or_else(temp_dir).join("reprise/diagnostics")`.
4. **`clippy.toml`** disallows `dirs::data_dir`, `dirs::data_local_dir` and `glib::user_data_dir`.
   - The Rhythmbox site gets `#[allow(clippy::disallowed_methods, reason = "reads Rhythmbox's own data, not Reprise's")]`.
   - Verify that clippy really resolves the re-exported glib path: put in a deliberate violation,
     see clippy fire, then remove it.
5. **Dev-dependency feature** in every crate whose tests can reach a default path: ffi (already),
   gnome, cli, mcp and stems. `-p` runs are then isolated too.
6. **The measurement test** derives the DB path, and the diagnostics path if it reads that back,
   from its own validated `XDG_DATA_HOME` instead of from `default_path()`.
7. **A visible stale binary:** when `data_root::is_isolated()`, the gnome `main` logs one warning at
   startup: the binary is a test build and its data lives under `<root>`; use `cargo build` /
   `cargo run`. In addition, `shoot-updates-popover.sh` and `verify-radio-favicons.sh` run
   `cargo build -p reprise-gnome` before they start the binary, unless the caller supplies an
   explicit binary (`REPRISE_BIN`). That way they never pick up a test build.

## Tasks (test-first)

1. Red: in `data_root.rs`, `a_test_build_never_resolves_the_users_real_data_dir` and
   `every_default_data_directory_hangs_below_the_isolated_root` (db, staging, podcasts). Run them
   and see the second fail because the defaults still use `dirs::data_dir`.
2. Green: the resolver, the feature rename in core, `cache_root`'s cfg, and the routing of db,
   staging and podcasts.
3. stems: `default_model_dir` goes through `user_data_root()`. Add a test that it hangs below the
   isolated root. Add the dev-dependency feature.
4. gnome: the watchdog goes through core, the measurement test derives its paths from the env, the
   startup warning is added, and so is the dev-dependency feature. Add a small test in gnome that
   `reprise_core::data_root::is_isolated()` holds.
5. cli and mcp: the dev-dependency feature, plus one isolation assertion each, the way ffi's
   `cache_isolation_tests.rs` does it. Extend that ffi test to the data root and rename it
   `dir_isolation_tests.rs`.
6. The two scripts build before they launch (design 7).
7. The `clippy.toml` entries, the Rhythmbox allow, and the deliberate-violation check.
8. Docs: the doc comment on `db::default_path`, which says it honours `XDG_DATA_HOME`, now applies
   to production builds only. Update the `cache_root` module docs for the shared feature.
9. Gates: fmt, clippy `-D warnings`, `cargo test --workspace`, and the core purity check.

## Grill decisions (2026-10-09)

1. One feature, renamed to `test-private-dirs`.
2. It is enabled by ffi, gnome, cli, mcp and stems.
3. Stale binaries are handled by a startup warning plus a build step in the two scripts.
4. The proof is one-off and recorded in the PR. It runs after the refactor, through a worker
   under `heavy-run`, outside the Codex sandbox, and is not Codex's job.

## Proof (one-off, recorded in the PR, not a permanent gate)

Build a fake HOME:

- `chmod 555` on `.local/share` and `.cache`;
- a canary `.local/share/reprise/reprise.db` of garbage bytes;
- `XDG_DATA_HOME` and `XDG_CACHE_HOME` unset;
- `CARGO_HOME` and `RUSTUP_HOME` pointing at the real ones.

Run it twice:

- **Control arm** on pristine origin/dev: it must show a write attempt or a touch of the canary.
- **Fix arm:** it must show none.

Each arm covers `cargo test --workspace` and the per-crate `-p reprise-gnome|reprise-cli|reprise-mcp|reprise-stems` runs.

## Parallelität

No cut. `clippy.toml`, `crates/reprise-core/Cargo.toml` and the new `data_root` module are the
precondition for every other task, and the per-crate tasks are each a few lines. One strand.
