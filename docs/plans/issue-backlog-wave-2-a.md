---
slug: issue-backlog-wave-2-a
worktree: /home/marvin/Projects/reprise-issue-backlog-wave-2-a
branch: feature/issue-backlog-wave-2-a
phase: planned
codex_session:
created: 2026-08-22
---
# Wave 2, strand A — #475: one geometry model on the adoption path too

Base `origin/dev` = `ada027270a` (after wave 1). Sweep task 13.

Owns, and writes only:

```
crates/reprise-gnome/src/ui/track_list/reload_anchor_scroll.rs
crates/reprise-gnome/src/ui/list_geometry_layout.rs
docs/plans/issue-backlog-wave-2-a.md
```

Everything else is another agent's file. `track_list_reload.rs`,
`track_list_model.rs` and `diagnostic_trail.rs` in particular belong to strand B,
which runs at the same time.

---

## Read this first: the issue is not achievable as written

#475 asks to "replace the parallel section geometry fields with the shared typed
layout model **without changing scroll-adoption behavior**". Measured 2026-08-22
against `ada027270a`, that is impossible in the literal form, for two reasons.

**1. `ListLayout` cannot express what the adoption path computes.**
`ScrollAdoptionGeometry::matches` (`reload_anchor_scroll.rs:86-117`) derives the
section height **backwards from the live `upper`**:

```
section_height = (upper - row_count * row_height) / section_count
guard_top      = guard_position * row_height + preceding_sections * section_height
```

It has to, because it only ever runs when `applied_layout` is `None`
(`reload_anchor_scroll.rs:281`: `(applied_layout.is_none() && section_count > 0)`),
i.e. when no trustworthy header height exists yet — most often because
`geometry.is_settled(...)` was false (`reload_anchor_scroll.rs:558-560`).
`ListLayout::sectioned` (`list_geometry_layout.rs:65`) takes a **known**
`header_height: RowHeight` as a parameter. The two run in opposite directions.

**2. `ListLayout` is not `Copy`.** It is `#[derive(Clone, Debug, PartialEq)]`
(`list_geometry_layout.rs:51`) and carries `SectionBands { starts: Vec<u32>, .. }`.
`ScrollAdoptionGeometry` is `#[derive(Clone, Copy)]` (`reload_anchor_scroll.rs:76`)
and is captured by value in a `connect_value_changed` handler that fires on every
adjustment change (`reload_anchor_scroll.rs:302-322`). A naive swap costs an `Rc`
or a heap clone per signal.

This is also why #477 and #479 — which introduced `ListLayout` and converted
`tag_reload_anchor.rs`, `reload_restore.rs`, `tag_edit_flow.rs` and
`track_list_geometry.rs` to it — **left this one site alone**. That was not an
oversight.

The decision taken by the repository owner on 2026-08-22: **extend `ListLayout`**
so the adoption path can use it, rather than close the issue or half-convert it.

## Task 1 — give `ListLayout` the constructor this path needs

Add a constructor to `list_geometry_layout.rs` that builds a sectioned layout from
what the adoption path actually has: a row height, a section count, a row count,
and the **observed** `upper`. It derives the per-header height the same way
`matches` does today — the remainder of `upper` after the rows, divided evenly
across the sections — and is therefore the one place in the codebase where that
inversion lives.

Requirements on it:

- It returns `Option<Self>` (or an equivalent), because the inputs can be
  degenerate: zero rows, zero sections, a non-finite `upper`, an `upper` smaller
  than the rows alone. Every guard `matches` performs today
  (`reload_anchor_scroll.rs:88-115`) must survive somewhere — either in this
  constructor or at the call site — and none may be silently dropped.
- Name it for what it does (it infers a header height from an observation), and
  say in its doc comment **why** it exists: that the adoption path has no settled
  layout, and that this is the inverse of `sectioned`.
- Do **not** change `sectioned`, `row_top`, `headers_above`, `centered_value` or
  `validate`. Those carry wave 1's freshly proven behaviour and three §4C
  mutations; a change there is out of scope for this strand.

## Task 2 — rebuild `ScrollAdoptionGeometry` on it

Replace `section_count`, `preceding_sections` and `row_height` with the layout.
Keep the struct cheap to capture: either keep a small `Copy` projection derived
*from* the layout (`row_height`, `headers_above(guard_position)`) so the closure
stays allocation-free, or hold `Rc<ListLayout>`. State in `## Result` which you
chose **and what it costs per signal emission** — this handler fires on every
adjustment change during a restore, so the answer matters.

`guard_position`, `row_count` and `before` stay as they are.

`matches` then asks the layout for the guard row's top instead of doing its own
arithmetic. The clamp to `[lower, (upper - page_size).max(lower)]` and the
`before`-rejection stay where they are — they are about the adjustment, not the
geometry.

## Task 3 — the free function

`headers_above_in` (`list_geometry_layout.rs:36`) is called directly at
`reload_anchor_scroll.rs:277` to compute `preceding_sections`. If task 2 makes
that call redundant, remove it from the call site. If it is still needed to build
the layout, leave it and say so. Do not delete the function itself — other callers
may exist outside this strand's files.

## Acceptance — this is a refactor, so the bar is "nothing moved"

1. **The existing unit test must still pass unchanged.**
   `adoption_accepts_only_the_value_explained_by_the_requested_guard_row`
   (`reload_anchor_scroll.rs:624`) is the only direct witness to this arithmetic.
   If it needs editing to compile, that edit must be mechanical (constructing the
   struct differently); its **assertions** may not be weakened. Show the before
   and after of the test in `## Result` if you touch it at all.
2. **Add the cases the old test never had.** The guards listed in task 1 —
   zero rows, zero sections, `preceding_sections > section_count`,
   `guard_position >= row_count`, non-finite inputs, `upper < lower`, negative
   `page_size` — each get a displayless test asserting the same *decision* the
   current code makes. Derive the expected answers from the current code, not
   from what seems sensible.
3. **Equivalence proof.** Before changing anything, add a displayless test that
   pins the current `matches` behaviour on a table of concrete inputs, including
   at least one realistic sectioned-queue case with a non-integer row height.
   That test must pass **before and after** the refactor, unchanged. This is the
   only thing that can show the rewrite is behaviour-neutral; a refactor whose
   only evidence is "the suite is still green" has not been measured.
4. **Mutation proof that the new path is load-bearing.** Force the derived header
   height to `0.0`, run the tests, record which go red, revert, record green. If
   *nothing* goes red, the new constructor is not actually being used and the
   refactor is cosmetic — say so rather than reporting success.
5. **Display coverage.** `nav_back_to_a_large_sectioned_queue_never_visits_the_top`
   and `queue_anchor_names_the_row_at_the_viewport_top`
   (`queue_section_geometry_display_tests.rs:508,623`) drive this path end to end.
   Run both before and after and record passed/failed each time. They are the only
   behavioural net; note in `## Result` that they check the outcome, not this
   arithmetic, so they are a weaker witness than item 3.
6. Displayless GNOME suite (`cargo test --locked -p reprise-gnome`, fresh XDG
   roots, `REPRISE_AUDIO_SINK=fakesink`), `cargo fmt --check`, and
   `cargo clippy --all-targets -p reprise-gnome -- -D warnings`. Record counts.

**Forbidden:** changing `sectioned`/`row_top`/`headers_above`/`centered_value`/
`validate` semantics, touching any file outside the ownership list, and claiming
`Fixes #444`.

---

## Result

Implemented on 2026-08-22 against the planned `ada027270a` base.

### Implementation

`ListLayout::sectioned_inferred_from_observed_upper` now owns the one inverse
calculation. It rejects zero rows, zero sections, non-finite `upper`, and an
allocation more than 0.5 px shorter than the row bodies. A shortfall within
that inherited epsilon becomes the old zero-height-header interpretation. Its
doc comment records why the unsettled adoption path needs the inverse of
`sectioned`.

`ScrollAdoptionGeometry` now holds `Rc<ListLayout>` instead of the parallel
`section_count`, `preceding_sections`, and `row_height` fields. The captured
layout retains the real section starts and the geometry service's current
header estimate only as a topology source. On every adjustment emission,
`matches` derives a temporary observed layout from the live `upper` and asks
that layout for `row_top(guard_position)`. The adjustment clamp and
before-value rejection remain in `matches` unchanged.

`SectionBands::starts` changed from `Vec<u32>` to `Rc<[u32]>`, without changing
the semantics of `sectioned`, `row_top`, `headers_above`, `centered_value`, or
`validate`. The handler allocates one `Rc<ListLayout>` when it is armed. Each
signal emission performs no heap allocation: it increments and later
decrements the non-atomic `Rc<[u32]>` count once while doing the scalar layout
calculation. This retains the old live-`upper` behavior without cloning the
section vector per signal.

The direct `headers_above_in` call was redundant and is gone from
`reload_anchor_scroll.rs`. The function remains in `list_geometry_layout.rs`
for the layout and its other callers.

The formerly independent `preceding_sections > section_count` state is now
unrepresentable after successful construction because both values come from
one section-start collection. `ScrollAdoptionGeometry::new` still rejects a
mismatched expected section count, and the named regression proves that the
same old malformed input decision remains rejection.

### Original witness

The test's assertions are byte-for-byte unchanged. Only construction changed
mechanically because the geometry is now validated and contains an `Rc`.

Before:

```rust
let geometry = ScrollAdoptionGeometry {
    guard_position: 1_101,
    row_count: 2_276,
    section_count: 2,
    preceding_sections: 2,
    row_height: RowHeight::new(34.0).unwrap(),
    before: 37_454.0,
};
```

After:

```rust
let geometry = adoption_geometry(1_101, 2_276, 2, 2, 34.0, 37_454.0).unwrap();
```

Unchanged assertions:

```rust
assert!(geometry.matches(37_488.0, 0.0, 77_438.0, 249.0));
assert!(!geometry.matches(37_454.0, 0.0, 77_438.0, 249.0));
assert!(!geometry.matches(36_000.0, 0.0, 77_438.0, 249.0));
```

### TDD and equivalence evidence

- The new constructor tests first failed to compile with six `E0599` errors
  because `sectioned_inferred_from_observed_upper` did not exist. After the
  constructor was added, both focused tests passed (`2 passed`).
- Before production changed, the adoption filter passed `10/10`: the existing
  witness, the concrete equivalence table, and eight guard tests. The table
  includes a 2,276-row, two-section queue with a fractional 34.5 px row height.
  Its concrete inputs, expected decisions, and assertion loop were retained
  through the refactor; only fallible geometry construction gained a mechanical
  `unwrap`.
- After the refactor, the same adoption filter passed `10/10`. The constructor
  filter independently passed `2/2`.
- The guard tests cover zero rows, zero sections, more preceding than total
  sections, an out-of-range guard row, every non-finite adjustment input,
  `upper < lower`, negative `page_size`, and `upper` more than the inherited
  epsilon below the row bodies. The equivalence table additionally preserves
  the sub-epsilon zero-header decision and both adjustment-edge clamps.

### Mutation proof

The derived header height was temporarily forced to `0.0`.

- Constructor filter: **RED**, `1 passed, 1 failed`.
  `observed_upper_infers_the_header_height_inverse_of_sectioned` reported
  `37984.5` instead of `38056.5`.
- Adoption filter: **RED**, `8 passed, 2 failed`.
  `adoption_accepts_only_the_value_explained_by_the_requested_guard_row` and
  `adoption_match_decisions_are_pinned_across_concrete_inputs` both failed.
- After restoring the formula: constructor **GREEN** at `2/2`; adoption
  **GREEN** at `10/10`.

The constructor is therefore load-bearing on the production adoption path.

### Display evidence

Each test ran alone under private D-Bus and Xvfb with fresh data, cache,
configuration, runtime, and fixture roots, X11/Cairo, unset Wayland, disabled
AT-SPI, and `REPRISE_AUDIO_SINK=fakesink`. `xvfb-orphan-gc --apply` ran after
every display process.

| Test | Before | After |
| --- | --- | --- |
| `nav_back_to_a_large_sectioned_queue_never_visits_the_top` | `1 passed`; samples `first=49381 min=49347 max=49381` | `1 passed`; samples `first=49381 min=49347 max=49381` |
| `queue_anchor_names_the_row_at_the_viewport_top` | `1 passed` | `1 passed` |

These tests verify the rendered restore outcome end to end, not the inverse
arithmetic itself. They are therefore a weaker witness than the unchanged
displayless equivalence table.

### Final verification

- Prescribed displayless GNOME suite with fresh XDG roots and fake audio:
  `1988 passed, 0 failed, 778 ignored`; GNOME conformance integration tests:
  `10 passed, 0 failed`.
- An earlier diagnostic run put `TMPDIR` on the worktree filesystem and
  reported `1983 passed, 5 failed, 778 ignored`; all five failures were the
  simulated-MTP fixtures rejecting that nonstandard root as read-only. The
  prescribed rerun above used their normal temporary filesystem and passed.
- `cargo fmt --check`: passed.
- `cargo clippy --all-targets -p reprise-gnome -- -D warnings`: passed.
- `cargo clippy --all-targets --workspace -- -D warnings`: passed.
- Isolated `cargo test --locked --workspace`: passed, including Core at
  `2576 passed, 3 ignored`, GNOME at the counts above, Android FFI at
  `172 passed`, Linux platform at `158 passed, 2 ignored`, and every remaining
  workspace and documentation suite.
- `cargo audit`: passed over 482 dependencies; the only finding was the
  accepted `RUSTSEC-2024-0436` warning for `paste`.
- Project Python, YAML, and Markdown source quality plus all three contract
  tests passed with task-local writable uv cache/tool roots. The first attempt
  stopped before linting because the managed session could not lock the
  read-only default `~/.cache/uv`; the isolated rerun supplied the verdict.
- Strict rustdoc, architecture, accessibility, input-parity, frontend-thinness,
  UX-traceability, motion, AppStream, Flatpak, GNOME-idiom, AI-hygiene,
  device-sync GStreamer, runtime-service-install, worktree-GC, and scheduled-
  workflow gates passed. The private-D-Bus runtime-service inventory passed
  `25/25`; the serial Linux-platform suite passed `158/158` with two ignored.
- The clean-tree merge-readiness wrapper could not fetch because the managed
  host rejects the permissions on
  `/etc/ssh/ssh_config.d/20-systemd-ssh-proxy.conf`. A live read confirmed
  `origin/dev` remained `ada027270a`, was an ancestor of `HEAD`, and the
  worktree was clean. The documented `--no-fetch` rerun passed branch-diff
  validation and then stopped on unchanged
  `scripts/cua-e2e/responsive_window.sh:72` (`SC2154`). Individual gates were
  therefore run directly.
- Two other individual aggregate checks expose unchanged baseline failures
  outside this strand: gettext reports five missing Arabic messages, and the
  QA-linter fixture `fresh-install-skip-before.json` has no `snapshot_id`.
  This branch changes none of the reported files.
- A broad `scripts/check-display-tests.sh --rule-named` follow-up completed its
  test-worker processes repeatedly, but the managed PTY never returned the
  helper's final balance sheet after it concatenated all per-test Cargo logs.
  The wrappers were stopped only after process inspection showed no remaining
  test, Xvfb, or reporting process. This broad result is therefore unproven and
  is not reported as green. The two required end-to-end display tests above
  have exact before/after pass evidence. Final Xvfb cleanup removed nine stale
  locks and nine orphaned sockets.
- Edited code sizes: `list_geometry_layout.rs` 525 lines and
  `reload_anchor_scroll.rs` 799 lines, both below the 800-line limit.

No Core file, user data, music file, live desktop, sibling-owned path, version
metadata, remote branch, or issue state was changed. No `Fixes #444` claim was
made.
