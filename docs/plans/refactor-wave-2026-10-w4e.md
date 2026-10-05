---
slug: refactor-wave-2026-10-w4e
worktree: /home/marvin/Projects/reprise-refactor-wave-2026-10-w4e
branch: feature/refactor-wave-2026-10-w4e
phase: coded
codex_session:
created: 2026-10-05
---
# Refactor wave 2026-10, wave 4 — strand E: gate hermeticity

Mother plan: `docs/plans/refactor-wave-2026-10-w4.md`; its "Shared context" binds this strand.
Two small, unrelated-looking fixes with one theme: a check that judges the wrong thing. The
motion-token gate scans sibling test files as production code; the stems readiness check reports
a missing native runtime before a missing model, which makes a CLI test depend on the machine.

This plan is your only channel. When the code disagrees with a table below, the code wins: keep
what the code does and say so in your final message.

## Purpose

1. `scripts/check-motion-tokens.sh` treats a file that is declared only under `#[cfg(test)]` the
   way it already treats an inline `#[cfg(test)] mod … { }` block: excluded from the CSS-duration
   scan, still read by the Rust-literal scan. A file declared without the attribute anywhere is
   scanned as before. `scripts/tests/motion-tokens.sh` proves both directions.
2. `reprise_stems::provision::runtime_readiness_in` reports `ModelRequired` before it resolves the
   native library, so `crates/reprise-cli/tests/worker_basic.rs::worker_without_fake_backend_and_no_provisioned_model_is_unavailable`
   passes on a machine with no `libonnxruntime.so` and no `ORT_DYLIB_PATH`.

**Behaviour-preserving means:** the gate reports every literal it reports today in production
files (the self-tests' negative cases stay); readiness returns the same variant for every state
that has a model file, and `ModelRequired` instead of `Unavailable { NativeRuntime }` only when
both are missing.

## Evidence (origin/dev @ 9465e997e8, 2026-10-05)

### Motion-token gate

- `scripts/check-motion-tokens.sh`: `production_source()` (awk) blanks lines from
  `#[cfg(test)]` + `mod NAME {` to the closing `}` and is used for the CSS scan only; the Rust
  scan reads the whole file ("as it always has"). The file loop is
  `find "$ui_root" -type f -name '*.rs' | sort`; `is_allowlisted` exempts `ui/motion.rs` and
  `ui/style/tokens.rs` (`policy_files`) and an empty `phase_two_allowlist`.
- Sibling test declarations under `crates/reprise-gnome/src/ui`: 38 `#[cfg(test)]` + `mod x;`
  pairs (e.g. `ui/style/mod.rs:50-51 mod composed_css_tests;`, `:261-262 mod tests;`), plus the
  `#[path]` form (`ui/style/theme.rs:667-668 #[cfg(test)] #[path = "theme_role_tests.rs"] mod role_tests;`,
  `ui/strings_podcasts.rs:625-626`, `ui/shortcuts.rs:52-53`, `ui/first_run.rs:589`).
- Today no sibling test file contains a CSS duration literal (measured: the only hits are inside
  inline test blocks of `eq_bars.rs`, `podcasts/css.rs` and `style/mod.rs`), so the gate is green
  by luck; the first `assert!(css.contains("… 600ms"))` in a `*_tests.rs` file turns it red for a
  test that observes the policy rather than setting it.
- Self-tests: `scripts/tests/motion-tokens.sh` (fixture tree under `MOTION_TOKEN_ROOT`, clean
  file, clean CSS with an inline test block, bad Rust literals, bad CSS with a production rule
  below an inline test block, policy files, three ordinary UI files). It is registered in
  `scripts/tests/qa-linters.sh:143` (`require_executable`) and run from `:317`;
  `qa-linters.sh:219` pins `check-motion-tokens.sh` in `check-merge-readiness.sh:123`.
  `.github/scripts/check-gnome-ci.sh:23` runs the gate in CI.
- Rust's resolution of a non-inline module declared in file `F`: with `#[path = "X"]` the file is
  `dirname(F)/X`; without it, `F == dir/mod.rs` → `dir/NAME.rs` or `dir/NAME/mod.rs`, and
  `F == dir/stem.rs` → `dir/stem/NAME.rs` or `dir/stem/NAME/mod.rs`.

### Stems readiness

- `crates/reprise-stems/src/provision.rs:223-270` `runtime_readiness_in(model_dir, spec,
  library_location)`: (1) `resolve_library` → `Unavailable { NativeRuntime, detail }` on
  `LibraryNotFound` ("onnxruntime library not found; looked in: …. Set ORT_DYLIB_PATH to a
  libonnxruntime.so (onnxruntime 1.22.0)."); (2) missing pinned SHA → `Unavailable { NativeRuntime }`;
  (3) `weights_path(model_dir, spec)` not a file → `ModelRequired { path }`; (4) model checksum →
  `Ready` / `Unavailable { Model }`.
- `runtime_readiness()` (:205-221) feeds `default_model_dir()` (XDG data dir) and
  `onnxruntime_location()` (`ORT_DYLIB_PATH`, the bundled path, `<model_dir>/libonnxruntime.so`).
- `crates/reprise-stems/src/ort_backend.rs:65-75` `from_provisioned_default`: `Ready` → backend,
  `ModelRequired` → `Ok(None)`, `Unavailable { detail }` → `Err(StemError::Backend(detail))`.
- `crates/reprise-cli/src/commands/worker.rs:240-258` `select_backend`: `Ok(None)` → `CliError::Unavailable("the stem-separation model is not provisioned yet — download it first, or re-run with --fake-backend …")`;
  `Err(error)` → `CliError::Unavailable("the stem-separation backend is unavailable: {error}. …")`.
  Both exit 8 (`error.rs:69`).
- The test (`worker_basic.rs:53-87`) isolates `XDG_DATA_HOME` to an empty dir, runs `jobs work
  --once` without `--fake-backend`, asserts exit 8, stderr contains `not provisioned`, job still
  `queued`. On this machine (`/usr/lib/libonnxruntime*` absent, `ORT_DYLIB_PATH` unset,
  `~/.local/share/reprise/stems` absent) step (1) fires first, stderr says "backend is
  unavailable: onnxruntime library not found …", the `not provisioned` assertion fails.
- Unit tests: `runtime_readiness_requires_verified_model_and_native_runtime` (:600, library
  present → `ModelRequired`, corrupt model → `Unavailable { Model }`, verified → `Ready`) and
  `runtime_readiness_rejects_missing_unpinned_or_tampered_native_code` (:640, **writes the model
  first**, then missing/unpinned/tampered library → `Unavailable { NativeRuntime }`). Both keep
  passing after the reorder. `provision.rs` is 766 lines.
- CI never builds `reprise-cli --features worker` (no `--all-features` test job;
  `ci.yml` runs `cargo test --locked --workspace --exclude reprise-platform-linux` and clippy
  `--all-targets --workspace`). The CLI test is a local-only proof; nothing here changes CI.

## Decisions (fixed — do not re-open)

1. **CSS scan only.** The sibling-file exemption mirrors the inline-block rule exactly; the Rust
   literal scan keeps reading every file. (A `set_duration(300)` in a test still fails today and
   keeps failing — the policy is not widened.)
2. **Exempt only files whose every declaration is under `#[cfg(test)]`.** The script collects two
   sets over all `*.rs` under `ui/`: files declared with the attribute, files declared without; a
   file is exempt iff it is in the first and not in the second. A file declared nowhere (dead or
   declared from outside `ui/`) is scanned. Only the exact line `#[cfg(test)]` counts;
   `#[cfg(all(test, …))]` does not (conservative).
3. **Model before runtime.** Steps (3) and (4)'s *existence* check moves to the top:
   missing model file → `ModelRequired`; then library resolution, pin check, then the model
   checksum. "Download the model" is the state the app can act on; the runtime check follows once
   a model exists.
4. **No new test-only knob in the CLI.** The test stays as it is; the reorder is what makes it
   hermetic.

## Owns

- `scripts/check-motion-tokens.sh`
- `scripts/tests/motion-tokens.sh`
- `crates/reprise-stems/src/provision.rs` (function body order + one unit test)

Not owned: `crates/reprise-cli/**` (the test is run, not edited; `tests/common/**` and
`src/retry.rs` are foreign); `scripts/tests/qa-linters.sh` unless a `require_pattern` there pins
a line you had to change (read `:214-223` first); any `Cargo.toml`.

## Tasks (in order, one commit each)

**E1 — self-tests first (red).** Append to `scripts/tests/motion-tokens.sh`, before the final
"passed" line, keeping its style (printf'd fixture files, `MOTION_TOKEN_ROOT=$fixture`):

- *Sibling test file declared under `#[cfg(test)]` may quote a duration.* Create
  `$ui_root/chip/mod.rs` containing a clean production function plus
  `#[cfg(test)]` / `mod chip_tests;`, and `$ui_root/chip/chip_tests.rs` with
  `assert!(css().contains("animation-duration: 999ms"))`. The gate must pass.
- *The `#[path]` form.* `$ui_root/glow.rs` with `#[cfg(test)]` / `#[path = "glow_role_tests.rs"]`
  / `mod role_tests;` and `$ui_root/glow_role_tests.rs` quoting `transition: opacity 300ms`. Pass.
- *The same file declared without the attribute is production.* Rewrite `chip/mod.rs` so the
  declaration is a bare `mod chip_tests;`. The gate must fail and name `chip/chip_tests.rs`
  (`rg --quiet 'literal CSS animation duration.*chip/chip_tests.rs' "$fixture/err"`).
- *A test-only sibling file still fails the Rust scan.* Put `stack.set_transition_duration(150);`
  into a `#[cfg(test)]`-declared sibling; the gate must fail with the Rust message naming it.
- Clean up the fixture files after each case as the existing cases do; the final clean run must
  pass.

Run `scripts/tests/motion-tokens.sh`: the first case fails on the unmodified gate. Commit.

**E2 — the gate.** In `scripts/check-motion-tokens.sh`, before the file loop, build the exempt set:

```bash
# A file declared only under `#[cfg(test)]` is test code in its entirety and is treated like an
# inline `#[cfg(test)] mod … { … }` block: skipped by the CSS scan, still read by the Rust scan.
# Resolution follows rustc: `#[path]` is relative to the declaring file's directory; otherwise
# `dir/mod.rs` declares `dir/NAME.rs`, and `dir/stem.rs` declares `dir/stem/NAME.rs`.
declare -A test_only_files=() production_declared_files=()
while IFS=$'\t' read -r declaring gated path_attr name; do
  … resolve candidate(s) as above; for each existing candidate:
  …   [[ $gated == 1 ]] && test_only_files[$candidate]=1 || production_declared_files[$candidate]=1
done < <(awk '…' $(find "$ui_root" -type f -name '*.rs' | sort))
is_test_only_file() { [[ -n ${test_only_files[$1]:-} && -z ${production_declared_files[$1]:-} ]]; }
```

The awk emits one record per `mod NAME;` declaration: declaring file, whether the preceding
non-`#[path]` attribute line was exactly `#[cfg(test)]`, the `#[path = "…"]` value if any, and
the name (`^(pub(\([^)]*\))? )?mod [A-Za-z0-9_]+;$`). Any other line resets the pending state
(the existing `production_source()` awk shows the idiom). In the loop, the CSS scan branch becomes
`if ! is_test_only_file "$file" && production_source "$file" | rg --quiet …`. Paths must be
compared in the same form `find` prints them (relative to `$repo_root`), so resolve candidates
with the declaring file's `dirname` and no normalisation beyond that. Keep `set -euo pipefail`
honest (`${arr[$k]:-}` for lookups). `scripts/tests/motion-tokens.sh` green; `scripts/check-shell.sh`
green; `scripts/check-motion-tokens.sh` on the real tree green and still printing
"Motion token lint passed".

**E3 — readiness order, test first (red).** In `provision.rs` tests add
`a_missing_model_is_reported_before_a_missing_runtime`: temp dir, `fake_spec()`, **no** model
file, `LibraryLocation { candidates: vec![dir.join("libonnxruntime.so")], expected_sha256: Some(…) }`
with no such file; assert `runtime_readiness_in(..) == RuntimeReadiness::ModelRequired { path: weights_path(dir, &spec) }`.
It fails today with `Unavailable { NativeRuntime }`. Then reorder the body per decision 3:

```rust
let model_path = weights_path(model_dir, spec);
if !model_path.is_file() {
    return RuntimeReadiness::ModelRequired { path: model_path };
}
let library_path = match resolve_library(library_location) { … unchanged … };
if library_location.expected_sha256.is_none() { … unchanged … }
match file_sha256(&model_path) { … unchanged … }
```

Update the doc comment of `runtime_readiness_in` (and `from_provisioned_default`'s if it describes
the order) in one sentence. `cargo test -p reprise-stems provision` green (the two existing tests
unchanged). Then `cargo test -p reprise-cli --features worker worker_without_fake_backend` with
`ORT_DYLIB_PATH` unset: green on this machine. `provision.rs` stays below 800 lines (766 + ~20).

## Known traps

- **`reprise-stems` with the `ort` feature** is pulled by `reprise-cli --features worker`; if the
  `ort` crate cannot build in your sandbox, run `cargo test -p reprise-stems` alone and report
  the exact CLI failure instead of skipping it.
- **awk portability**: the repository's scripts run on Arch (gawk) and on CI's Ubuntu (mawk is
  possible); avoid gawk-only functions (`gensub`, `match` with an array). `scripts/check-shell.sh`
  runs shellcheck; `declare -A` is bash 4+, which both have.
- **Do not widen the exemption to `tests/` directories or `_tests.rs` name patterns** — the rule
  is the declaration, not the file name (decision 2).
- **`qa-linters.sh:214-223`** pins patterns in some scripts; if it pins a line of
  `check-motion-tokens.sh` you rewrote, adjust the pin in the same commit and say so.
- **Never cite a `docs/plans/…` path** in either script or in `provision.rs`.
- **Clippy 1.99 vs 1.97**: no suppression is added or removed here.
- English everywhere, focused commits, no agent attribution lines.

## Verification

```
scripts/check-shell.sh
scripts/tests/motion-tokens.sh
scripts/check-motion-tokens.sh
scripts/tests/qa-linters.sh
cargo fmt --check
cargo clippy -p reprise-stems --all-targets -- -D warnings
cargo clippy --all-targets --workspace -- -D warnings
cargo clippy --all-targets --workspace --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -p reprise-stems --no-deps
cargo test -p reprise-stems provision
env -u ORT_DYLIB_PATH cargo test -p reprise-cli --features worker worker_without_fake_backend
scripts/check-architecture.sh
```

Report: the number of test-only files the gate now exempts on the real tree (print the set once
with a temporary `echo`, then remove it), the four self-test cases' outcomes, and the CLI test's
stderr line containing "not provisioned".
