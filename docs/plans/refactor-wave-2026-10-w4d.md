---
slug: refactor-wave-2026-10-w4d
worktree: /home/marvin/Projects/reprise-refactor-wave-2026-10-w4d
branch: feature/refactor-wave-2026-10-w4d
phase: planned
codex_session:
created: 2026-10-05
---
# Refactor wave 2026-10, wave 4 — strand D: lyrics — the red smoke and the fixture seams

Mother plan: `docs/plans/refactor-wave-2026-10-w4.md`; its "Shared context" binds this strand.
Origin: `scripts/check-lyrics-smoke.sh` is red on `dev` (no lyrics request is ever made), and wave 3
left "gating the lrclib/netease fixture seams behind `test-fixtures` (today compiled in release
builds)" for this wave. Both live in the same files, so they are one strand.

This plan is your only channel. When the code disagrees with a table below, the code wins: keep
what the code does and say so in your final message.

## Purpose

1. `scripts/check-lyrics-smoke.sh` passes again, because the smoke harness seeds the one setting it
   forgot — the global online-sources gate — before it enables the lyrics module.
2. The lrclib and NetEase fixture seams (`REPRISE_LYRICS_FIXTURE_DIR`, `REPRISE_LRCLIB_FIXTURE_DIR`,
   `…_FIXTURE_LOG`) compile only under `cfg(any(test, feature = "test-fixtures"))`, like the
   MusicBrainz, radio, podcasts and concerts seams; `scripts/check-architecture.sh` pins that.
3. The lrclib seam honours `<fixture>.delay-ms` again, so the smoke's "Slow stale" rejection
   exercises the stale-response race it was written for.

**Behaviour-preserving means:** no change for a user. A release build stops reading two
environment variables it should never have read; the smoke harness (`REPRISE_SMOKE_LYRICS=1`,
test-fixtures feature) seeds one more setting; test-only code gains a sleep.

## Evidence (origin/dev @ 9465e997e8, 2026-10-05)

### The smoke — root cause (by code reading, pinned by diff; confirm by running it in D1)

- `scripts/check-lyrics-smoke.sh` (55 lines) copies `sine.flac` three times, tags them
  `SmokeFirst/SmokeSlow/SmokeFast` with `metaflac`, starts the app under the full isolation
  recipe with `REPRISE_SCAN_DIR`, `REPRISE_LRCLIB_FIXTURE_DIR=crates/reprise-core/tests/fixtures/lyrics-smoke`,
  `REPRISE_LRCLIB_FIXTURE_LOG=$tmp/requests.jsonl`, `REPRISE_SMOKE_LYRICS=1`, `REPRISE_SMOKE_QUIT=1`,
  `cargo run -p reprise-gnome --features test-fixtures`, under `timeout 15s`; then greps the log
  for three `lyrics smoke state` lines with `line_count=2 … latest=true` and expects exactly three
  lines in the request log. It is listed in `TESTING.md:242`; no CI workflow runs it.
- The harness, `crates/reprise-gnome/src/ui/lyrics/lyrics_smoke.rs:26`:
  `player.set_online_lyrics_enabled(true)` — nothing else is seeded.
- `crates/reprise-gnome/src/ui/lyrics/player_lyrics.rs:393-416` (foreign-owned, read only):
  `set_online_lyrics_enabled` writes `modules::set_enabled(.., ONLINE_LYRICS_MODULE, ..)` then
  `recompute_lyrics_enabled()`, which sets the view's `enabled` to
  `online_sources::network_allowed(&conn, &ONLINE_LYRICS_MODULE).unwrap_or(false)`.
- `crates/reprise-core/src/online_sources.rs:176-181`:
  `network_allowed_in = settings::get_bool_in(conn, ENABLED_KEY, false)? && modules::is_enabled_in(conn, module)?`
  — **both** flags. `crates/reprise-core/src/db_grandfather.rs:35-43`: `online_gate_default` is
  `existing_database && (…)`, so a fresh `XDG_DATA_HOME` writes the gate as off. Every GTK test
  database opens it explicitly: `crates/reprise-gnome/src/test_db.rs:88`
  `reprise_core::online_sources::set_enabled(&db, true)?;`. The smoke has no equivalent.
- With the gate off: `set_track` → `start_request(intent, false)` (local lookup only,
  `player_lyrics.rs:117-118`); `lyrics/mod.rs:213-214` returns `LyricsError::Temporary` when
  `!allow_network` and nothing is local; `apply_response` retries online only `if … && self.enabled.get()`
  (`player_lyrics.rs:242-246`), otherwise `view.show_disabled()` → `line_count=0`, "stale or
  missing", and the fixture seam in `lrclib.rs:449` is never reached — no `requests.jsonl`.
- The break: `ce66eb24a5` (2026-07-30, #180) flipped `online_sources::is_enabled`'s default from
  `true` to `false`; the harness and the script were last touched 2026-07-26/30 and never updated.
  The reproduction on `e2ea7bfdb3` (before #1094) agrees: the `net` move is not involved.
- **Ordering trap:** `online_sources::set_enabled(db, true)` on a fresh database runs the
  first-enable seeding (`first_enable_turns_every_online_source_off_except_radio`,
  `online_sources.rs:385`), which turns the module flags off. The gate must be opened **before**
  the module is enabled, or the module write is undone.
- Secondary: `bf5fca85df` (2026-07-31, #189) rewrote the lyrics seam without the `.delay-ms`
  sleep. `crates/reprise-core/tests/fixtures/lyrics-smoke/lyrics-SmokeSlow--SmokeArtist--SmokeAlbum--1.delay-ms`
  still exists; only `musicbrainz.rs:121-124` reads such files today:
  ```rust
  if let Ok(delay) = std::fs::read_to_string(path.with_extension("delay-ms")) {
      let millis = delay.trim().parse::<u64>().unwrap_or_default();
      std::thread::sleep(Duration::from_millis(millis));
  }
  ```
  Without it, SmokeSlow answers at once and the "Slow stale" rejection the snapshots check is
  never raced.
- `metaflac`, `xvfb-run` and `dbus-run-session` are installed here; the script's `timeout 15s`
  does not cover a cold build — build first.

### The fixture seams

- Gated providers (`#[cfg(any(test, feature = "test-fixtures"))]` on the constant **and** on the
  `if let Some(directory) = fixture_directory()` statement): `musicbrainz.rs:22-25,49-50`,
  `radio/http.rs:18-19,26-27,47-48,93`, `podcasts/http.rs:23-24,76-77,117-118,218`,
  `concerts/http.rs:19-22,26-27,53`. They resolve the directory through
  `crate::net::fixtures::fixture_directory(FIXTURE_DIR_ENV)` (`net/fixtures.rs:20-29`, itself
  gated), which also honours the thread-local test override.
- **Ungated:** `lyrics/lrclib.rs` — constants at lines 22-25 (`FIXTURE_DIR_ENV`,
  `LEGACY_FIXTURE_DIR_ENV`, `FIXTURE_LOG_ENV`, `LEGACY_FIXTURE_LOG_ENV`), `FixtureRequest`
  (serde `Serialize`, `filename`/`legacy_filename`), `fixture_request` (:301),
  `fixture_get_at` (:330, logs before it looks the file up), `append_fixture_log` (:498),
  `fixture_directory` (:513, env with legacy fallback), `fixture_log` (:520), and the seam in
  `fetch` (:449-451). Imports used only by the seam: `std::fs::OpenOptions`, `std::io::Write`,
  `serde::Serialize`. `lyrics/netease.rs` — constant at line 18, `search_fixture_filename`,
  `lyric_fixture_filename`, `FixtureFetcher` (+ its `NeteaseFetcher` impl), `read_fixture`,
  `fixture_directory` (:307), and the seam inside `ProductionFetcher::{search,lyric}` (:270-287,
  written as `fixture_directory().map_or_else(production, fixture)`).
- Tests: `lrclib_tests.rs` (568 lines, `#[cfg(test)] #[path] mod tests;`) calls `fixture_request`
  and `fixture_get_at` directly (:88-108); `netease_tests.rs` (206) uses `FixtureFetcher` through
  `provider_with_fixtures`. Both compile under `cfg(test)`, so the gate must be
  `any(test, feature = "test-fixtures")`, not the feature alone.
- Feature plumbing: `reprise-core` `test-fixtures = []`; `reprise-gnome`
  `test-fixtures = ["reprise-core/test-fixtures"]`; `reprise-mcp` dev-dependency enables it.
  Workspace feature unification turns it on for `cargo clippy --workspace`; the ungated build is
  only proven by `cargo clippy -p reprise-core --all-targets -- -D warnings`.
- `scripts/check-architecture.sh:240-270` is the `== Engine HTTP boundaries ==` block; the
  too-many-arguments block starts at line 298. Strand B edits line 301; this strand inserts after
  line 270.

## Decisions (fixed — do not re-open)

1. **Fix the harness, not the default.** The core default stays off; the harness seeds what a
   consenting user would have set, exactly as `test_db::open()` does.
2. **Gate first, module second, then recompute** (the ordering trap). The harness calls a new
   GTK-free helper `open_isolated_lyrics_gate(conn: &Db) -> Result<(), CoreError>` (gate, then
   module) and then `player.recompute_lyrics_enabled()`; `set_online_lyrics_enabled` is no longer
   called by the harness (it is still used elsewhere; do not touch `player_lyrics.rs`).
3. **The seams are gated with `#[cfg(any(test, feature = "test-fixtures"))]`**, the same literal
   the other providers use; the variable names and the fixture-file naming stay (the smoke script
   and `scripts/ptr-e2e`/`cua-e2e` set them).
4. **The `.delay-ms` sleep returns to `fixture_get_at`**, between the log append and the file
   read, with the MusicBrainz three lines (it is test-only code under the gate; the lyrics request
   runs off the main thread). The delay file in the fixture directory is unchanged.
5. **The architecture gate checks the declarations**: every `const …FIXTURE…: &str` under
   `crates/reprise-core/src` (outside `*_tests.rs`) must be preceded by the exact
   `#[cfg(any(test, feature = "test-fixtures"))]` line. With the constants gated, the compiler
   forces every read behind the same gate — no second check is needed.

## Owns

- `crates/reprise-gnome/src/ui/lyrics/lyrics_smoke.rs` and a new sibling
  `crates/reprise-gnome/src/ui/lyrics/lyrics_smoke_tests.rs` (declared from `lyrics_smoke.rs` the
  way the neighbouring `_tests.rs` siblings are declared; `#[cfg(test)] #[path = "…"] mod tests;`)
- `crates/reprise-core/src/lyrics/lrclib.rs`, `crates/reprise-core/src/lyrics/lrclib_tests.rs`,
  `crates/reprise-core/src/lyrics/netease.rs`, `crates/reprise-core/src/lyrics/netease_tests.rs`
- `crates/reprise-core/src/online_sources.rs` — the `#[cfg(test)] mod tests` only, and only if
  D2's pin is missing
- `scripts/check-architecture.sh` — one new block after line 270
- `scripts/tests/qa-linters.sh` — only if it pins block headings of `check-architecture.sh`
  (read lines ~210-225 first)

Not owned: `ui/lyrics/player_lyrics.rs` and `player_lyrics_tests.rs` (foreign; call only);
`crates/reprise-core/src/net/**`; the smoke script `scripts/check-lyrics-smoke.sh` (unchanged;
if it needs a change, stop and report); `tests/fixtures/lyrics-smoke/**`; any `Cargo.toml`.

## Tasks (in order, one commit each)

**D1 — confirm the diagnosis (no commit).** `cargo build -p reprise-gnome --features
test-fixtures`, then run `scripts/check-lyrics-smoke.sh`; keep its `app.log` (`tmp_root` is
deleted by its trap — copy the script's command line and run it with your own `tmp_root`, or add
`set -x`; do not commit changes to the script). Expected: the three `lyrics smoke state is stale
or missing` lines and no `requests.jsonl`. The gate cannot be pre-seeded from outside (the
database is created at start), so the confirmation is the re-run after D2. If the smoke is still
red after D2, the fallbacks in order are: the Lyrics tab not open (`start_request` returns at
`player_lyrics.rs:168`; check `panel.show_lyrics()`), then a breaker or cache skip
(`lrclib.rs:196`, `lyrics/mod.rs:200-212`). Report what you saw either way.

**D2 — the helper and its test first (red).** `lyrics_smoke_tests.rs`:

- `the_isolated_gate_allows_the_lyrics_module`: `let db = crate::test_db::open_fresh().unwrap();`
  (opens **without** the gate — `open()` sets it); assert
  `online_sources::network_allowed(&db, &ONLINE_LYRICS_MODULE) == Ok(false)`; call
  `open_isolated_lyrics_gate(&db).unwrap()`; assert it is now `true`, and that
  `online_sources::is_enabled(&db) == Ok(true)` and
  `modules::is_enabled(&db, &ONLINE_LYRICS_MODULE) == Ok(true)`.
- `the_module_alone_does_not_allow_the_network` (the ordering pin): on a fresh db call
  `modules::set_enabled(.., ONLINE_LYRICS_MODULE, true)` **then** `online_sources::set_enabled(.., true)`
  and assert `network_allowed(..) == Ok(false)` — this is what the first-enable seeding does to a
  module enabled too early. If `online_sources.rs`' own tests already pin exactly this sequence,
  say so and skip the duplicate; otherwise keep it here (GTK-free, no display).

Then in `lyrics_smoke.rs` add

```rust
/// Opens the global online-sources gate and then the Online Lyrics module — in that order,
/// because the gate's first-enable seeding resets the module flags.
fn open_isolated_lyrics_gate(conn: &Db) -> Result<(), reprise_core::CoreError> {
    reprise_core::online_sources::set_enabled(conn, true)?;
    reprise_core::modules::set_enabled(conn, &reprise_core::modules::ONLINE_LYRICS_MODULE, true)
}
```

and in `arm` replace the `set_online_lyrics_enabled(true)` call with
`open_isolated_lyrics_gate(conn)` (same error logging, message reworded: "could not open the
isolated lyrics gate") followed by `player.recompute_lyrics_enabled();`. `cargo test -p
reprise-gnome lyrics_smoke` green. Re-run the smoke: it must pass now (D1's confirmation).

**D3 — gate the lrclib seam.** In `lrclib.rs` put `#[cfg(any(test, feature = "test-fixtures"))]`
on: the four constants, the `OpenOptions`/`Write`/`Serialize` imports (split the `serde` import),
`FixtureRequest` and its impl, `fixture_request`, `fixture_get_at`, `append_fixture_log`,
`fixture_directory`, `fixture_log`, and the `if let Some(directory) = fixture_directory() { … }`
statement in `fetch` (an attribute on the `if` statement, as `podcasts/http.rs:76-77` does).
Restore the delay (decision 4) inside `fixture_get_at` after the log append succeeds and before
the file loop — read `path.with_extension("delay-ms")` of the *first* candidate filename that
exists (the smoke's delay file uses the legacy name; loop over both names for the delay file the
same way the JSON is looked up). `cargo clippy -p reprise-core --all-targets -- -D warnings`
(feature off) and `cargo clippy -p reprise-core --all-targets --features test-fixtures -- -D warnings`
both green; `cargo test -p reprise-core lrclib` green. Add one test in `lrclib_tests.rs`:
`a_delay_file_holds_the_fixture_answer` — write a fixture JSON plus a `.delay-ms` of `150`, time
`fixture_get_at`, assert ≥ 150 ms and the body is returned (a floor, never a ceiling — no flake).

**D4 — gate the NetEase seam.** In `netease.rs`: the constant, `search_fixture_filename`,
`lyric_fixture_filename`, `FixtureFetcher` and its impl, `read_fixture`, `fixture_directory`
under the gate; rewrite `ProductionFetcher::search`/`lyric` as

```rust
fn search(&self, query: &LyricsQuery, timeout: Duration) -> FetchOutcome {
    #[cfg(any(test, feature = "test-fixtures"))]
    if let Some(directory) = fixture_directory() {
        return FixtureFetcher::new(&directory).search(query, timeout);
    }
    search_url(query).map_or(FetchOutcome::Failed(false), |url| fetch_url(&url, timeout))
}
```

(same for `lyric`). Both clippy runs green; `cargo test -p reprise-core netease` green.

**D5 — the architecture gate.** After line 270 of `scripts/check-architecture.sh`:

```bash
echo "== Fixture seams stay out of release builds =="
# A provider's fixture-directory variable is a test seam. Its constant is declared behind
# cfg(any(test, feature = "test-fixtures")), so the compiler keeps every read behind the same gate.
ungated_fixture_consts=$(rg -n -U --pcre2 \
  '(?<!#\[cfg\(any\(test, feature = "test-fixtures"\)\)\]\n)^(?:pub(?:\([a-z]+\))? )?const [A-Z_]*FIXTURE[A-Z_]*: &str' \
  crates/reprise-core/src --glob '*.rs' --glob '!*_tests.rs' || true)
if [[ -n $ungated_fixture_consts ]]; then
  echo "fixture-directory constants must be declared behind cfg(any(test, feature = \"test-fixtures\")):" >&2
  printf '%s\n' "$ungated_fixture_consts" >&2
  exit 1
fi
echo "  fixture seams: every fixture constant is test-gated"
```

Measure first: `rg -n '^(pub(\([a-z]+\))? )?const [A-Z_]*FIXTURE[A-Z_]*' crates/reprise-core/src`
and make sure the regex matches exactly those declarations (the lookbehind needs `-U` and
`--pcre2`; if the lookbehind proves awkward, an `awk` that remembers the previous line is fine).
Prove it in both directions once: temporarily remove one `#[cfg]` line in `lrclib.rs`, run the
script, see the message, revert. `scripts/check-shell.sh` and `scripts/tests/qa-linters.sh` green.
Never cite a `docs/plans/…` path.

## Known traps

- **Dead-code under `-D warnings`.** Every item the seam uses and nothing else must carry the gate,
  including imports and serde derives; the feature-off clippy run is the proof. Do not reach for
  `#[allow(dead_code)]`.
- **Attribute on an `if` statement** is legal (`#[cfg(..)] if let … { return … }`); the other
  providers use it. Do not wrap it in a block or a helper that changes `fetch`'s shape otherwise.
- **The thread-local override in `net::fixtures`** is `cfg(test)`-only and lyrics tests do not use
  it; do not route lyrics through `crate::net::fixtures::fixture_directory` in this strand (it
  would drop the legacy variable fallback the smoke script depends on). Say in the final message
  that this remains the w3 "one fixture variable" item.
- **`lyrics_smoke.rs` imports `super::player_controller::PlayerController`** through an alias;
  strand A does not retire the `playback` family, leave the import alone.
- **`RefCell` discipline**: the helper takes `&Db`; `arm` already holds only `Rc`s.
- **The smoke needs a warm build**; its `timeout 15s` is for the run, not the compile.
- **Sleep in tests**: the delay test asserts a lower bound only.
- English everywhere, focused commits, no agent attribution lines.

## Verification

```
cargo fmt --check
cargo clippy -p reprise-core --all-targets -- -D warnings                       # test-fixtures OFF
cargo clippy -p reprise-core --all-targets --features test-fixtures -- -D warnings
cargo clippy --all-targets --workspace -- -D warnings
cargo clippy --all-targets --workspace --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo test -p reprise-core lyrics
cargo test -p reprise-gnome lyrics_smoke
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'          # must print nothing
cargo build -p reprise-gnome --features test-fixtures && scripts/check-lyrics-smoke.sh
scripts/check-architecture.sh
scripts/check-shell.sh
scripts/tests/qa-linters.sh
scripts/check-frontend-thinness.sh                                              # unchanged numbers
```

Report: D1's observation before and after D2 (quote the three phase lines and the request-log
line count), the list of items gated in each lyrics file, whether the ordering pin already existed
in `online_sources.rs`, and the gate's red-run output.
