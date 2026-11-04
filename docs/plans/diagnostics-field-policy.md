---
slug: diagnostics-field-policy
worktree: /home/marvin/Projects/reprise-diagnostics-field-policy
branch: feature/diagnostics-field-policy
phase: planned
codex_session:
created: 2026-11-04
---
# Diagnostics keep the harmless fields and name every field they emit

## Goal

The run log and Copy Diagnostics (#1310) replace almost every structured
tracing field with `$REDACTED`. The allow-list `is_safe_structured_field`
(`crates/reprise-core/src/diagnostics/redact.rs:173`) names 12 keys. The code
emits 397 distinct keys, and about 313 of them are harmless: durations,
counts, enum states, booleans and internal integer ids. Those are the fields
that make a log diagnosable. After this change they survive, identifying
fields stay redacted, and every emitted key is classified so a new one cannot
silently go either way.

## Decisions (made with the maintainer, 2026-11-04)

- **D1: a closed list of exact key names.** No suffix or prefix rules
  (`*_ms`, `*_id`, `is_*`). A suffix rule is fail-open, because a future
  `foo_id` that carries a string would leak. Unknown keys stay redacted
  (fail-closed), exactly as today.
- **D2: one predicate for both passes.** This already holds.
  `EventFields::record_value` (`crates/reprise-gnome/src/ui/diagnostics.rs:152-157`)
  and the free-text `redact_sensitive_token` (`redact.rs:137-171`) both call
  `is_safe_structured_field`. Keep it that way: extend the list, do not add a
  second mechanism, and do not pass typed values through by value shape.
- **D3: a classification check.** Every key emitted by a `tracing` macro in
  non-test code under `crates/` must appear in exactly one of two lists, KEEP
  or REDACT. The check fails on an unclassified key, on a key in both lists,
  and on a listed key that no longer exists, so the lists cannot rot.
- **D4: these stay redacted.** `error`, `message`, `detail`, `reason`,
  `source`, `path`, `dir`, `file`, `root`, `url`, `uri`, `name`, `title`,
  `artist`, `album`, `playlist`, `playlist_name`, `device_id`, `device_name`,
  `value`, `command`, `from`, `to`, `key`, `item`, and every class-b or
  class-c key in the table. `error_kind` call sites are a possible follow-up,
  out of scope here.

## Inputs

`docs/plans/diagnostics-field-policy.keys.md` holds the sweep: 397 keys with
class, site count and one example site each. **Its class column is a
keyword/shape heuristic, not an audit.** Every key that goes into KEEP has to
be verified at **every** non-test emitting site at info level or above, and at
debug and trace too, because the filter can be raised. A KEEP value is an
integer, float, bool, duration, a `Debug`/`Display` of a fieldless enum or a
closed set of static labels, a crate version, or an internal database or
sequence id. A string that could come from a file, tag, URL, network response,
user input or foreign library message is not a KEEP value. If one site of a
key fails this, the key goes to REDACT; rename that site's key only when the
other sites are worth keeping and the rename is trivial.

## Tasks

1. **Policy module (test-first).** Create
   `crates/reprise-core/src/diagnostics/field_policy.rs` with
   `pub const KEEP_FIELDS: &[&str]` and `pub const REDACT_FIELDS: &[&str]`,
   both sorted and documented. `is_safe_structured_field` becomes a lookup
   in `KEEP_FIELDS`. Keep its path and signature, or re-export it from
   `redact.rs`, so callers do not change. Carry the existing
   `musicbrainz_error` justification comment over verbatim. Unit tests: the
   lists are sorted and disjoint; unknown keys are not safe; the existing
   diagnostics tests (`crates/reprise-core/src/diagnostics/tests.rs:83,125,157,187`,
   `crates/reprise-gnome/src/ui/diagnostics/tests.rs:32`,
   `file_mirror.rs:99`, `extended_tests.rs:57`) keep passing unchanged.
2. **Populate the lists** from the key table under the verification rule in
   "Inputs". Record each non-obvious KEEP decision in a one-line comment.
3. **Classification check** (D3). Implement it as a reprise-core test, for
   example `field_policy_sweep_tests.rs`, that walks `crates/*/src/**/*.rs`
   from `CARGO_MANIFEST_DIR/../..`, skips test modules and `*_tests.rs`, and
   extracts field keys from `tracing::{trace,debug,info,warn,error}!`,
   `event!` and their bare imported forms. That covers the `key = expr`,
   `key = %expr`, `key = ?expr` and shorthand `key`/`%key`/`?key` forms;
   a dotted key such as `a.b` must be handled too. Before relying on it, prove
   the extractor finds at least the 397 keys of the table, and seed one
   deliberately unclassified key to see the check fail. The failure message
   names the key, one site, and which list to add it to.
4. **Rust-visible proof of the outcome.** Add a test that renders a
   representative event (for example `elapsed_ms=12 state=Playing count=3
   error=<path-bearing text>`) through both the writer path and the
   Copy Diagnostics re-redaction (`render.rs:114`). The kept fields arrive
   verbatim, and `error` is `$REDACTED` in both.
5. **Docs.** Update the module doc in
   `crates/reprise-gnome/src/ui/diagnostics/file_mirror.rs:1-11` to point at
   the policy lists. No UX-rule status change: FB-19 and START-6 stay
   `[planned]`, and their wording ("no library path reaches the clipboard")
   still holds.

## Out of scope

- `error_kind` fields at the ~674 `error = %error` sites.
- `reprise-android-ffi`: it logs ~25 `error = %error` fields and never calls
  the redactor. Note it in the PR body as a separate finding; do not change
  it here.
- Free-text messages that contain `key=multi word`: only the first word is
  checked. Existing behaviour, not widened by this change.

## Gates

The full merge-readiness gate. Additionally:
`cargo test -p reprise-core diagnostics` and
`cargo test -p reprise-gnome diagnostics` run during development. Every code
file stays under 800 lines; the lists live in their own module for that
reason.
