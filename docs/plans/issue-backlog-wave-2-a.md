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

*(Codex fills this in.)*
