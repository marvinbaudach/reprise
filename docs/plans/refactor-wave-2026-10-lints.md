---
slug: refactor-wave-2026-10-lints
worktree: /home/marvin/Projects/reprise-refactor-wave-2026-10-lints
branch: feature/refactor-wave-2026-10-lints
phase: refactored
codex_session:
created: 2026-10-05
---
# Refactor wave 2026-10, wave 2 — lint suppressions state their reason

Mother plan: `docs/plans/refactor-wave-2026-10.md` (wave 2, package level, decided in its grill).
Wave 1 landed as #1068, #1070 and #1071. This wave is one strand and runs alone. It is
behaviour-preserving apart from deleting code that is provably dead.

## Evidence (origin/dev @ e332d4ff59, 2026-10-05)

There are 181 `#[allow(...)]` and `#![allow(...)]` attributes in 115 files under `crates/`. None
carries a `reason`, and no `#[expect]` is used anywhere. 29 of them are inner attributes
(`#![allow]`), which silence a whole module.

| Lint | Count |
| --- | --- |
| `unused_imports` | 61 |
| `dead_code` | 53 |
| `clippy::too_many_arguments` | 41 |
| `clippy::needless_pass_by_value` | 15 |
| `clippy::cast_possible_truncation` | 4 |
| `deprecated` | 3 |
| `clippy::result_large_err` | 2 |
| `clippy::cast_sign_loss` | 2 |
| `clippy::large_enum_variant` | 1 |
| `clippy::enum_variant_names` | 1 |

Most of the `unused_imports` attributes are the flat re-export groups in
`crates/reprise-gnome/src/ui/mod.rs`. `crates/reprise-gnome/src/ui/browse/filter_bar.rs` opens
with a stale `#![allow(dead_code)] // B1 lands the shared grammar…`.

## The rule every suppression must end up following

Decide every suppression in this order:

1. **Suppresses nothing: delete it.** A suppression whose lint does not fire in any target that
   `cargo clippy --all-targets --workspace` builds is deleted.
2. **The code is provably dead: delete the code.**
   - `dead_code` or `unused_imports` that fires in every target means the code is dead. Delete
     the code and the suppression together. This covers unused re-export aliases in
     `ui/mod.rs`, unused helpers and unused fields.
   - **Exception: user-visible strings in `crates/reprise-gnome/src/ui/strings*.rs`.** Delete such
     a const only if `scripts/tests/gettext-catalogs.sh` stays green afterwards. Otherwise keep it
     with `#[allow(dead_code, reason = "…")]` and list it in the final message.
3. **Used only by tests: gate it, do not suppress it.** If the lint fires only in the non-test
   build because the item is used only by tests, put the item behind `#[cfg(test)]`.
4. **The lint fires in every target that compiles the code: use `expect`.** Write
   `#[expect(lint, reason = "…")]`. An `expect` that becomes unfulfilled later warns, so stale
   suppressions surface by themselves.
5. **The lint fires in some targets but not others: use `allow` with a reason.** Write
   `#[allow(lint, reason = "…")]`. An `expect` there would be unfulfilled in one target, and
   `-D warnings` turns that into an error.
6. **Narrow the scope.** A module-level `#![allow(lint)]` moves to the narrowest item that needs
   it. Keep it at module level only when most items in the module need it, and say why in the
   reason.

**What a reason is.** A reason states *why this code needs the exception*, not which lint it
is. Good: `reason = "UniFFI hands owned values across the FFI boundary"`,
`reason = "each argument is an independent SQL bind parameter of this migration step"`. Bad:
`reason = "too many arguments"`. One short clause, in English.

**How to tell which case applies.** A practical method:

1. Convert a batch of `allow` to `expect`.
2. Run `cargo clippy --all-targets --workspace -- -D warnings`.
3. Every `unfulfilled_lint_expectations` warning marks a site that is case 1, 3 or 5.
4. Remove that suppression and rerun clippy to tell which.

Work crate by crate so that each run stays small.

## Tasks

**L1 — `reprise-core`, `reprise-view`, `reprise-runtime-protocol`, `reprise-stems`,
`reprise-platform-linux`.** Apply the rule to every suppression. Commit.

**L2 — `reprise-mcp`, `reprise-cli`, `reprise-android-ffi`.** Apply the rule. UniFFI
signatures are a legitimate reason for `needless_pass_by_value`. Commit.

**L3 — `reprise-gnome`, except `ui/mod.rs` and the `strings*.rs` files.** Apply the rule,
including the stale `#![allow(dead_code)]` in `ui/browse/filter_bar.rs`. Commit.

**L4 — `crates/reprise-gnome/src/ui/mod.rs`.** These are the flat re-export alias groups.
- Delete every alias that is unused in every target.
- If an alias is used only by tests, gate it with `#[cfg(test)]`.
- A group needs no suppression once only used aliases remain.
- Do not rewrite call sites to new paths. Retiring the alias layer is wave 3.
- Commit.

**L5 — the `strings*.rs` files.** Apply the rule, including the gettext exception. Commit.

**L6 — make reasons mandatory.** Add `allow_attributes_without_reason = "warn"` to
`[workspace.lints.clippy]` in the root `Cargo.toml`, with a one-line comment in the style of the
existing block. `cargo clippy --all-targets --workspace -- -D warnings` must then be clean.
Commit.

**L7 — `clippy::significant_drop_in_scrutinee`: measure, then decide.** This lint catches a
`RefCell` borrow or lock guard held across a `match` or `if let` body, which is this codebase's
#1 panic class.

1. Run `cargo clippy --all-targets --workspace -- -W clippy::significant_drop_in_scrutinee`, and
   count the distinct sites and the files they sit in.
2. **At most 25 sites:** fix each one by hoisting the value out of the scrutinee into a `let` (copy
   or clone it out) before the `match`. The guard must drop before any arm runs, and the arms'
   observable behaviour must not change. If an arm genuinely needs the guard held (it mutates
   through it), keep that site and mark it `#[expect(clippy::significant_drop_in_scrutinee,
   reason = "…")]`. Then enable the lint as `"warn"` in `[workspace.lints.clippy]`. Commit.
3. **More than 25 sites:** change no code. Record the count and the top ten files in your final
   message instead.

## Ownership

- Every file under `crates/` that carries an `allow` attribute.
- Files whose code is deleted as dead under rule 2.
- Files that need a `#[cfg(test)]` gate under rule 3.
- The root `Cargo.toml`, `[workspace.lints]` only.

Do not touch `scripts/`. The `too_many_arguments_budget` in `scripts/check-architecture.sh`
counts `allow` and `expect` alike, so it stays at 41 unless a suppression is deleted. If you
delete one, lower the budget in the same commit; that block is the only exception to "do not
touch `scripts/`".

## Verification

Run at the end:

```
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo test -p reprise-core -p reprise-view -p reprise-mcp -p reprise-cli
scripts/check-architecture.sh
scripts/tests/gettext-catalogs.sh
```

For `reprise-gnome`, run filtered tests for the modules whose code you deleted or gated. The
orchestrating session runs the unfiltered suites and the merge gate afterwards.

## Parallelität

There is one strand. Every crate carries suppressions, so no disjoint file group exists, and
`Cargo.toml` is shared. The tasks run in order L1 to L7.
