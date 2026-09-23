---
slug: src-10a-plugins-page
worktree: /home/marvin/Projects/reprise-src-10a-plugins-page
branch: feature/src-10a-plugins-page
phase: refactored
codex_session:
created: 2026-09-23
---

# SRC-10a — the module-off button names the page it actually opens

Closes issue #1014. `SRC-10`'s Block-B2 addendum still promises a button that
"opens the Online sources page directly". `SET-10` abolished that Preferences
main page when it folded every optional capability into Plugins. The code has
been right the whole time — the button deep-links into Plugins — so this change
corrects the rulebook and the one code comment that still names a type which no
longer exists. **No user-visible behaviour changes.**

## Verification scope

This change touches `docs/ux-rules.md` and Rust **test names, one comment and
one added assertion** in `crates/reprise-core` and `crates/reprise-gnome`. No
other crate, no Android, no Kotlin, no packaging metadata is involved.

Run exactly these, from the worktree root:

```
cargo fmt --check
cargo clippy -p reprise-core -p reprise-gnome --all-targets -- -D warnings
cargo test -p reprise-core src_10a
cargo test -p reprise-gnome --bin reprise src_10a
scripts/check-ux-traceability.sh
```

Do NOT run: `cargo test --workspace`, `cargo clippy --all-targets --workspace`,
`cargo audit`, `cargo build --release`, `gradlew`, `uniffi-bindgen`, the Android
suite, `scripts/check-merge-readiness.sh`, or any other repo-wide gate script.
**If `AGENTS.md` or another gate document tells you to run the full gate
battery before committing, that instruction does not apply to this run — this
exception is deliberate and stated here.** A previous run in this repo burned
thirty-one minutes on a workspace cargo build for a change of this size and was
killed before it committed anything.

Also do NOT run the unfiltered `reprise-gnome` test binary. It contains the
`settle()` family, which blocks headless runs; the filtered `src_10a` form above
is the one to use. The full suite is run outside this worktree afterwards.

## Task 1 — replace the rule

In `docs/ux-rules.md`, the `SRC-10` bullet currently begins with

```
- **SRC-10** [active] [gtk] — The genuine "nothing added yet" empty state
```

Change that first line, and only that line, to

```
- **SRC-10** [replaced by SRC-10a] [gtk] — The genuine "nothing added yet" empty state
```

Leave the rest of the `SRC-10` body untouched — a replaced rule stays as a
signpost, never as deletable ballast (`docs/ux-rules.md`, introduction).

Directly **after** the last line of the `SRC-10` bullet (it ends with
`…it never masquerades as "nothing subscribed yet".`) insert this new bullet
verbatim:

```
- **SRC-10a** [active] [gtk] — Replaces `SRC-10`. The geometry is unchanged;
  what changes is the page the module-off button names. The genuine "nothing
  added yet" empty state carries the same geometry for Podcasts, YouTube and
  Radio: the glyph of its own sidebar entry in a muted rounded tile, a title, a
  paragraph with one sentence each on *what* lands here and *where it comes
  from*, exactly one primary button with a plus icon, and beneath it, as a quiet
  second line, the URL path — where the source has one of its own; radio has
  none, because the paragraph already names the stream URL. Neither toolbar nor
  filter row nor counter appears in this state, and never "0 of 0": the surface
  looks unused, not broken. Never a generic placeholder graphic, never a spinner
  with nothing to do. As soon as the first subscription lands, this state
  disappears entirely. **Addendum (Block B2):** two siblings extend this
  geometry rather than replacing it. When a source's own module is switched off
  (`G1`/`NET-1a`) and nothing is subscribed yet, the same
  tile/title/body/one-button shape appears as "{Source} is turned off" with an
  "Enable in Preferences" button that opens Preferences → **Plugins** directly:
  `SET-10` folded the former "Online sources" main page into Plugins, and the
  deep link sends the three online-source rows, which arrive focused, expanded
  and briefly highlighted. Existing subscriptions are named as kept. The button
  is never a plus icon here, since there is nothing to add while the source is
  off (`PodcastsEmptyState::ModuleOff`); it carries the network glyph
  `network-server-symbolic`, which names what is being enabled rather than the
  icon of the page it lands on. Existing subscriptions outrank the module gate:
  it only ever replaces the empty case, never an already-populated view. The
  filter-mismatch state ("Nothing matches these filters",
  `PodcastsEmptyState::NoResults` / `RadioEmptyState::NoResults`) and the
  downloads-only state ("Nothing downloaded yet",
  `PodcastsEmptyState::NoDownloads`) are the opposite of the genuine empty
  state: the toolbar and filter row stay visible, with a "Clear filters" action,
  because clearing the filter — not adding a source — is the way out.
  `NoEpisodes` (subscribed, the feed genuinely has nothing yet) is unchanged and
  keeps the filter row hidden. A fetch failure with an existing subscription but
  no cached or downloaded episode uses the same geometry as
  `PodcastsEmptyState::FetchFailed`, with Retry and the collapsed Details block;
  it never masquerades as "nothing subscribed yet".
```

Two sentences differ from `SRC-10`, and nothing else: the page the button opens,
and the icon it carries. Both are stated deliberately.

## Task 2 — re-hang the tests onto the new rule

The rulebook's introduction requires it, and `scripts/check-ux-traceability.sh`
enforces it: a test whose name maps to a `[replaced]` rule fails the gate, and
an `[active]` rule with no test fails it too. So every `src_10_` test function
becomes `src_10a_`, in the same commit as task 1.

Twenty functions, measured on `origin/dev`:

| File | Count |
| --- | --- |
| `crates/reprise-core/src/podcasts/config.rs` | 1 |
| `crates/reprise-gnome/src/ui/source_empty_state.rs` | 6 |
| `crates/reprise-gnome/src/ui/podcasts/podcasts_empty_state.rs` | 3 |
| `crates/reprise-gnome/src/ui/podcasts/podcasts_view_copy.rs` | 2 |
| `crates/reprise-gnome/src/ui/podcasts/podcasts_view_tests.rs` | 5 |
| `crates/reprise-gnome/src/ui/podcasts/podcasts_presentation_tests.rs` | 1 |
| `crates/reprise-gnome/src/ui/radio/radio_view_tests.rs` | 2 |

The prefix rename `src_10_` → `src_10a_` is unambiguous — no other identifier in
the repo starts with `src_10`. Do not change any test body while renaming, and
do not add, remove or merge tests: the `#[test]` count in these files must be
identical before and after, apart from nothing. (Task 5 adds one assertion
inside an existing test, not a new test.)

## Task 3 — point the remaining references at the new ID

`origin/dev` carries sixty `SRC-10` / `src_10_` references outside the rulebook.
Fifty-eight are in `crates/`; every one of them is a doc comment or a test name
describing behaviour that `SRC-10a` now governs, so all of them move. This
mirrors #1011, which left zero `NET-4c` references in `crates/` behind.

Rename `SRC-10` → `SRC-10a` in `crates/` only, and only where the match is the
whole ID. Anchor the boundary: `SRC-100` does not exist today, but a bare
`s/SRC-10/SRC-10a/g` would also rewrite a future one, and it must never touch
`SRC-1`, `SRC-18` or `SRC-20`. Files holding references, measured on
`origin/dev`:

```
crates/reprise-core/src/podcasts/config.rs
crates/reprise-gnome/src/ui/strings.rs
crates/reprise-gnome/src/ui/strings_podcasts.rs
crates/reprise-gnome/src/ui/strings_radio.rs
crates/reprise-gnome/src/ui/source_empty_state.rs
crates/reprise-gnome/src/ui/preferences/preferences.rs
crates/reprise-gnome/src/ui/podcasts/podcasts_empty_state.rs
crates/reprise-gnome/src/ui/podcasts/podcasts_presentation.rs
crates/reprise-gnome/src/ui/podcasts/podcasts_presentation_tests.rs
crates/reprise-gnome/src/ui/podcasts/podcasts_view.rs
crates/reprise-gnome/src/ui/podcasts/podcasts_view_copy.rs
crates/reprise-gnome/src/ui/podcasts/podcasts_view_tests.rs
crates/reprise-gnome/src/ui/radio/radio_view.rs
crates/reprise-gnome/src/ui/radio/radio_view_tests.rs
```

This list is a starting point, not a fence: verify with
`git grep -n -E 'SRC-10([^0-9a-z]|$)' -- crates` that nothing is left, and if a
file outside the list turns up, rename there too and say so in the report.

**Leave the two documentation hits alone**: `docs/plans/open-issue-sweep-2026-08.md`
and `docs/research/p5-surface-scopes.md` are historical records of what was true
when they were written (`docs/plans/README.md`). Do not touch them.

## Task 4 — the comment that names a type which no longer exists

`crates/reprise-gnome/src/ui/podcasts/podcasts_view_copy.rs`, directly above
`button_icon_name: "network-server-symbolic"`:

```rust
        // Matches `PageId::OnlineSources`'s own icon in
        // `preferences_window.rs`, so the button visually points at where
        // it lands.
```

`PageId::OnlineSources` does not exist any more — `PAGE_ORDER` ends at
`PageId::Plugins`, whose icon is `application-x-addon-symbolic`. The rationale
the comment states is therefore false. Replace it with the rationale that holds:

```rust
        // Deliberately not the Plugins page icon this button deep-links into
        // (`application-x-addon-symbolic`): the glyph names what gets enabled,
        // an online source, not the settings page it is reached through
        // (`SRC-10a`).
```

## Task 5 — pin the icon claim with a test

`SRC-10a` now states the icon in the rulebook, so a test has to hold it, or the
rule says something nothing checks.

In `crates/reprise-gnome/src/ui/podcasts/podcasts_view_copy.rs`, the test
renamed in task 2 to `src_10a_the_module_off_button_never_carries_the_add_icon`
already builds the module-off copy. Add one assertion to that existing test:
the module-off copy's `button_icon_name` is `"network-server-symbolic"`. Keep
the existing assertions; do not add a second test function.

## Order and commits

Tasks 1–5 are one logical change and belong in **one commit** — the rulebook and
its tests must never be red between two commits. Commit message: a prose subject
in this repo's style, no `type:` prefix, for example

```
The module-off button names the page it actually opens (#1014)
```

Then the body: what drifted, that the code was already correct, and that the
rule was replaced rather than edited in place.

## Expected gate results

- `scripts/check-ux-traceability.sh` passes, and the number of active rules it
  reports is **the same before and after**. A replacement moves one rule out of
  `[active]` and adds one in, so it nets to zero. Measure the number on the
  branch if you want it; do not aim for a target, and under no circumstance
  reach a number by editing a rule tag or the gate script.
- The filtered `src_10a` test runs are green, and the number of tests they run
  equals the number of `src_10_` tests on `origin/dev`: twenty, split
  1 (`reprise-core`) + 19 (`reprise-gnome`).
- `cargo clippy -p reprise-core -p reprise-gnome --all-targets -- -D warnings`
  is clean.

## Parallelität

**No cut.** Tasks 1–3 all hinge on the same rule ID landing in the same commit,
and tasks 4 and 5 touch a file task 2 and task 3 both rewrite
(`podcasts_view_copy.rs`). A single strand is the correct shape; splitting it
would only invent a merge order for one commit.
