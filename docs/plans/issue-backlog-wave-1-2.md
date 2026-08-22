---
slug: issue-backlog-wave-1-2
worktree: /home/marvin/Projects/reprise-issue-backlog-wave-1-2
branch: feature/issue-backlog-wave-1-2
phase: reviewed
codex_session:
created: 2026-08-22
---
# Strand 2 — #404: sorting becomes reachable without a mouse

Mother plan: `docs/plans/issue-backlog-wave-1.md`. Base `origin/dev` = `1515487599`.
Sweep task 5 (`docs/plans/open-issue-sweep-2026-08.md`).

This strand owns, and writes **only**:

```
crates/reprise-gnome/src/ui/browse/**
crates/reprise-gnome/src/ui/track_list/track_list_sort.rs
crates/reprise-gnome/src/ui/strings.rs
docs/ux-rules.md
docs/plans/issue-backlog-wave-1-2.md
```

`crates/reprise-gnome/src/ui/track_list/track_list.rs` is written by **nobody** in
this wave. Task 2's design exists to keep it that way — read `shared.sort` when
the popover opens instead of adding an observer to `Shared`.

---

## The finding this strand acts on

Sorting the track table has exactly one control today: clicking a column header
(`crates/reprise-gnome/src/ui/track_list/track_list_sort.rs:61-114`,
`wire_sort_clicks` → `on_sorter_changed`). The header widget is a
`GtkColumnViewTitle`, which reports AT-SPI role `filler` and carries no action,
and GTK offers no API to change that. So for assistive technology the table
cannot be sorted at all — 52 occurrences over both seeds of the sweep that filed
#404.

The GTK defect gets its own upstream report; that is **not** part of this strand.
What this strand does is give sorting a second, properly labelled control that
writes the same state.

## Task 1 — the rule first

`docs/ux-rules.md` today covers column behaviour under **STYLE-13** [active]
("Columns belong to the user, in every table"), which describes the header sorter
and the header-band column editor. Two general accessibility rules — **GP-1** and
**GP-10** — are `[planned]`, not `[active]`.

Write the contract before building it: extend STYLE-13 (or add a rule adjacent to
it, whichever fits that document's shape) to say that **sorting is reachable
without a pointer**: the sort field and direction are available from a labelled
control in the browse bar, that control and the column header write the same sort
state, and neither is a second source of truth. Name the issue in the rule text
the way this document names issues elsewhere.

If the new behaviour makes GP-1 or GP-10 partly true, do **not** flip their status
in this strand — a `[planned]` rule becomes `[active]` when it holds everywhere,
not when one more widget complies.

## Task 2 — the control

Add a sort `MenuButton` to `crates/reprise-gnome/src/ui/browse/browse_bar.rs`,
beside the existing `add_filter` button (`browse_bar.rs:99-110`).

Shape it after `add_filter`, which is the local idiom: a `gtk4::MenuButton` with
a label child, the `pill` style class, styling delegated to
`filter_bar_layout`, and an explicit accessible label via `update_property`
(`browse_bar.rs:108-110`). It must match `+ Add Filter` in height and style
classes — it sits next to it and a mismatched twin is worse than no twin.

Contents: the sort **field** and the sort **direction**. The fields are the ones
the table already accepts — derive them from the existing whitelist
(`queries::SORT_WHITELIST` / `ColumnId::from_sort_field`) rather than typing a
second list; a hand-kept copy will drift the first time a column is added.

Labels come from `crates/reprise-gnome/src/ui/strings.rs` through the module's own
`text`/`formatted`/`plural` helpers and the `N_!` macro, never from literals in
the widget code. Reuse the column-header strings where they already exist rather
than inventing a second wording for the same field.

**One truth.** Choosing an entry must write `shared.sort` and trigger the reload
through the path that already exists — `track_list_sort.rs`'s
`sort_by_column(view, column, order)` (`track_list_sort.rs:50-57`) drives the
`ColumnView`'s own sorter, whose change observer then writes `shared.sort` and
calls `reload`. Go through that, so a menu choice and a header click are the same
event downstream. Do not write `shared.sort` from the browse bar directly, and do
not call `reload` twice.

**The mark, without a new observer.** The menu shows which field and direction are
current. Read `shared.sort.borrow()` when the popover is about to be shown and set
the marks then. `Shared` has no change signal for `sort` and this strand must not
add one — a popover that is closed has nothing to display, so reading on open is
both sufficient and simpler. Note this reasoning in a comment so the next reader
does not "fix" it into an observer.

There is no radio-style `GMenu` precedent in this codebase — the only stateful
action is a boolean in
`crates/reprise-gnome/src/ui/compact/compact_player_menu.rs:74-89`, and
`gio::ActionEntry` is unused repo-wide. Either build the radio group with
`gio::SimpleAction::new_stateful` carrying a string state, or build the popover
the way `add_filter` does — a plain `gtk4::Popover` with list rows. Pick the one
that yields a **labelled, activatable** element per choice in the accessibility
tree; that is the whole point of the change. Whichever you pick, every row or menu
item carries an accessible label and the current choice is exposed as state, not
only as a drawn checkmark.

## Task 3 — keyboard reach

A control that only a pointer can open does not discharge this issue. The new
button must be reachable by keyboard on the same tab path as `+ Add Filter`, its
popover navigable with the arrow keys, and closable with Escape. If `add_filter`
already sits in a working tab order, place the new button so it inherits it
rather than building a second focus model.

## Acceptance

The claim to be proven is "sorting is reachable without a pointer, and there is
still only one sort state" — not "a menu was added".

1. **A displayless test that a menu choice and a header click converge.** Drive
   the new control's activation handler for a given field and direction, and
   assert `shared.sort` afterwards equals what the header click produces for the
   same column — and that `reload` ran exactly **once**. The existing display test
   `column_headers_update_sort_state_and_reload_once`
   (`track_list_sort.rs:391-454`) already counts reloads; follow its shape. If the
   handler cannot be reached without a display, make it a display test.
2. **The list does not drift.** A test that the menu's field list is exactly the
   accepted sort whitelist. It must fail if a field is added to one and not the
   other.
3. **The a11y shape.** Assert what this GTK binding permits: `accessible_role()`
   readback works (see `track_list_columns_tests.rs:71,95,148` for the idiom),
   `Property::Label` has no getter in gtk4-rs 0.11.4. So assert the roles and the
   activatability of each choice, and cover the labelling through
   `scripts/check-accessibility-semantics.sh` — extend it if the new widget's
   shape is not already covered, and record a red run with the labels removed and
   a green run with them present.
4. **Mutation proof for the single-truth claim.** Make the menu write
   `shared.sort` directly instead of going through the sorter, run the tests, and
   record that the "reload ran once" assertion goes **red**. Revert. A test that
   stays green under that mutation is not testing the claim.
5. **Nothing regressed in the browse bar.** `browse_bar_tests.rs` currently has 20
   tests; run them and record the count before and after.
6. Run the displayless GNOME suite (`cargo test --locked -p reprise-gnome`, fresh
   XDG roots, `REPRISE_AUDIO_SINK=fakesink`) and record passed/failed.

**Not in this strand:** the upstream GTK report about `GtkColumnViewTitle`, any
change to the header sorter's own behaviour, and #411's busy indicator — which
lands in the same file later and must not be anticipated here.

---

## Result

Implemented on `feature/issue-backlog-wave-1-2` against the recorded
`1515487599` base. The browse bar now places a labelled `Sort` menu button in
the same action box as `+ Add Filter`. Its field and direction choices are
GTK radio groups, use the existing column labels and `ColumnId::from_sort_field`
mapping, and expose explicit accessible labels and checked state. Opening the
popover reads `shared.sort`; choosing either kind of entry drives the existing
`ColumnView` sorter. The existing sorter observer remains the only writer of
`shared.sort` and the only reload trigger. STYLE-13 names Issue #404 and records
that contract; GP-1 and GP-10 remain `[planned]`.

### TDD and mutation evidence

Every display run used a fresh data, cache, config, and runtime directory with
the following isolation envelope (one GTK test per process):

```sh
dbus-run-session -- xvfb-run -a env \
  XDG_DATA_HOME="$profile_root/data" XDG_CACHE_HOME="$profile_root/cache" \
  XDG_CONFIG_HOME="$profile_root/config" XDG_RUNTIME_DIR="$profile_root/runtime" \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
  GIO_USE_VFS=local GTK_USE_PORTAL=0 GSK_RENDERER=cairo \
  cargo test --locked -p reprise-gnome "$test_name" -- --exact --include-ignored
```

- Initial feature red: the first focused compile produced 11 `E0609` errors
  because `BrowseBar::sort_control` did not exist. The production control then
  made the new tests compile.
- Single-truth mutation: replacing the sorter call with a direct
  `shared.sort` assignment made
  `style_13_browse_sort_and_header_click_converge_and_reload_once` fail its
  exact-once assertion (`left: 3`, `right: 4`). Restoring the sorter path made
  the same test pass; field and direction activation each reload exactly once.
- Open-time mark mutation: forcing the direction mark to ascending made
  `style_13_header_sort_is_marked_when_the_sort_popover_opens` fail at
  `the header-selected descending direction is marked on open`. Restoring the
  read from `shared.sort.dir` made it pass.
- Labelling mutation: removing both the radio labels and their semantic marker
  made `scripts/check-accessibility-semantics.sh` exit 1 and made
  `style_13_sort_choices_are_keyboard_radio_actions` fail the GTK
  `AccessibleProperty::Label` assertion (`0 passed; 1 failed`). Restoring them
  made the script and test green (`1 passed; 0 failed`). The test also verifies
  `Radio` roles, activation, GTK `Checked` state, mutual exclusion, the tab-path
  peer placement, and the matching browse-bar button styling.
- Final focused STYLE-13 control run: all five rule-named tests passed in their
  own processes (`5 passed; 0 failed`).

### Regression counts

The plan's stated 20-test browse baseline was stale. A live pre-edit count found
19 tests: the ordinary filtered run reported `11 passed; 0 failed; 8 ignored`,
and all eight ignored display tests passed individually. A single-process
attempt with all display tests produced `12 passed; 7 failed` because GTK was
initialized from different Rust test threads; this was a harness-shape failure,
so all display evidence thereafter used one process per test.

After the implementation, `browse_bar_tests.rs` has 21 tests. The ordinary run
reported `11 passed; 0 failed; 10 ignored`, and all ten ignored display tests
passed individually. No pre-existing browse-bar test was lost. One intermediate
run caught an actual regression: a `GtkSeparator` inside the closed sort popover
violated `fil_2a_music_has_no_filter_caption_or_zone_separator`; removing that
unnecessary separator restored the test.

The requested displayless GNOME suite used fresh XDG roots:

```sh
env XDG_DATA_HOME="$profile_root/data" XDG_CACHE_HOME="$profile_root/cache" \
  REPRISE_AUDIO_SINK=fakesink cargo test --locked -p reprise-gnome
```

Result: the crate suite passed 1,961 tests with 0 failures and 770 ignored
display tests; `tests/gnome_conformance.rs` passed another 10 with 0 failures.

### Project gates

- `cargo fmt --check` — passed.
- `cargo clippy --all-targets --workspace -- -D warnings` — passed.
- `cargo test --locked --workspace` with fresh XDG roots and `fakesink` —
  aggregate `5,466 passed; 0 failed; 800 ignored` across 59 test/doc-test
  results.
- `cargo audit` could not create `/home/marvin/.cargo/advisory-db..lock` because
  the sandbox mounts Cargo home read-only. `cargo audit --no-fetch` loaded the
  cached 1,225-advisory database, scanned all 482 locked dependencies, and
  reported only the allowed `RUSTSEC-2024-0436` warning for `paste`.
- `scripts/check-ux-traceability.sh` — passed, 406 active rules covered.
- `scripts/check-accessibility-semantics.sh`, `scripts/check-architecture.sh`,
  `scripts/check-appstream.sh`, and `scripts/check-ai-hygiene.sh` — passed.
- `scripts/check-gnome-idioms.sh` and
  `scripts/check-flatpak-manifest.sh` exited 0 with existing `[planned]`-rule
  warnings. The latter could not allocate a Flatpak linter instance.
- `scripts/check-project-quality.sh --project --showroom` — passed after
  redirecting XDG, uv cache, and uv tool roots to a writable temporary tree.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` —
  passed.
- `cargo test --locked -p reprise-platform-linux -- --test-threads=1` —
  `159 passed; 0 failed; 27 ignored` across its unit/integration results.
  The private-session-bus follow-up passed all 25 ignored runtime-service
  integration tests.
- `DISPLAY_TEST_JOBS=4 scripts/check-display-tests.sh --rule-named` —
  `548 passed; 0 failed`; one measurement-only tool was correctly skipped.
  An earlier serial attempt was interrupted after it stopped producing
  observable progress; it did not produce a pass/fail balance sheet.
- Worktree-GC, Worktree-GC-schedule, device-sync GStreamer, input-parity,
  runtime-service-install, frontend-thinness, and motion-token checks passed.
- `git diff --check` — passed. Every substantially edited Rust file remains
  below 800 lines; no Core file changed, so the Core-purity proof does not
  apply.

The clean-tree aggregate
`MERGE_READINESS_BASE_REF=origin/dev scripts/check-merge-readiness.sh --no-fetch`
could not finish because its first ShellCheck gate stops on the unchanged
`scripts/cua-e2e/responsive_window.sh:72` (`SC2154`: `repo_root` referenced but
not assigned). `scripts/tests/qa-linters.sh` independently stopped because the
CUA fixture output carried no `snapshot_id`; every file under `scripts/` and
`.github/` is byte-identical to `origin/dev` on this branch.

One branch-caused gate remains red and cannot be corrected inside the strand's
exclusive ownership: `scripts/tests/gettext-catalogs.sh` found all five new
msgids missing when it reached `po/ar.po` (`Sort`, `Sort by`, `Direction`,
`Ascending`, and `Descending`). The plan requires those labels to be extracted
through `N_!`, while the mother plan permits this strand to write only
`strings.rs` and forbids every `po/*.po` path. Landing therefore needs an
explicit catalogue-ownership handoff or an expanded path grant; this branch did
not cross that boundary.

### Deferred evidence

The implementation was not driven in a real desktop session and therefore its
rendered geometry and live AT-SPI presentation remain visually unverified. The
mother plan reserves the merged-product accessibility sweep for post-merge CUA.
The upstream `GtkColumnViewTitle` report and Issue #411 remain outside this
strand, as required.
