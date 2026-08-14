---
slug: list-geometry-invariants
worktree: /home/marvin/Projects/reprise-list-geometry-invariants
branch: feature/list-geometry-invariants
phase: refactored
codex_session:
created: 2026-08-14
---
# ListLayout: collapse the unreachable `Option`, pin the `section_starts` invariant

Source: `docs/plans/queue-anchor-grill-followups.md` §4 A and B — the two production-code
follow-ups the queue-section-anchor landing run excludes by construction (its Codex prompt
forbids touching production code and names `list_geometry_layout.rs` as byte-frozen).

Settled in the grill on 2026-08-14; the alternatives that lost are recorded with their
reasons so the next reader does not re-open them.

## Base and stacking

This branch is **stacked on `feature/queue-section-anchor`**, which is not merged yet.
`crates/reprise-gnome/src/ui/list_geometry_layout.rs` does not exist on `origin/dev` — the
parent branch introduces it — so `dev` is not a possible base.

- base commit: `5c2dc9a482`
- the parent branch keeps moving, but only in test files and plan documents. Its own task
  pins `list_geometry_layout.rs`, `reload_anchor_scroll.rs`, `reload_restore.rs`,
  `track_list_reload.rs`, `track_list_geometry.rs` and `list_geometry.rs` as byte-frozen.
  That is what makes this stack collision-free on every file below.

**Do not touch** `crates/reprise-gnome/src/ui/track_list/queue_section_geometry_display_tests.rs`
or `queue_section_header_display_tests.rs`. Those are the parent branch's live work, being
edited right now in another worktree.

---

## Task A — collapse the unreachable `Option` on `content_height` / `max_scroll`

### The current shape

`list_geometry_layout.rs:20-41` holds two independent fields plus a constructor that rejects
the one combination in which they would disagree:

```rust
pub(in crate::ui) struct ListLayout {
    row_height: RowHeight,
    section_header_height: Option<RowHeight>,
    section_starts: Vec<u32>,
}

/// Returns `None` when sections exist but no header height is known.
pub(in crate::ui) fn new(…) -> Option<Self> {
    if !section_starts.is_empty() && section_header_height.is_none() {
        return None;
    }
    …
}
```

Because that guard exists, `content_height` (`:117-127`) can never take its `Unknown` arm, yet
it still returns `Option<f64>`; `max_scroll` (`:129-132`) inherits it, and `validate`
(`:146-148`) carries a dead `else { NoOpinion }` for the same reason.

The `None` is unreachable at the only real construction site too. `list_geometry.rs:418-428`:

```rust
pub(in crate::ui) fn layout(…, section_starts: Vec<u32>) -> Option<ListLayout> {
    let header_height =
        (!section_starts.is_empty()).then(|| self.section_header_height(db, cache));
    ListLayout::new(row_height, header_height, section_starts)
}
```

`ListGeometry::section_header_height` (`list_geometry.rs:405-416`) returns a plain `RowHeight`,
**not** an `Option`. The header height is therefore present exactly when the starts are
non-empty — literally the condition the constructor tests. The `Option` restates a fact the
call site already guarantees.

### The hard constraint

Implement this **structurally**: carry a type that cannot be absent. **Never** add an
`expect`, `unwrap`, `unreachable!` or `panic!` at the delegation seam in `content_height`.
That seam runs inside GTK reload callbacks; turning an unreachable state into a live panic
path there is a worse trade than the `Option` it removes. A patch that does it that way is
rejected regardless of how green the tests are.

### A1. Bind the header height to the sections

```rust
/// The row positions that carry a section header, together with the height
/// every one of those headers has. Non-empty by construction: a layout with
/// no sections holds `None` instead.
///
/// `starts` is strictly ascending — see `headers_above_in`.
#[derive(Clone, Debug, PartialEq)]
struct SectionBands {
    header_height: RowHeight,
    starts: Vec<u32>,
}

pub(in crate::ui) struct ListLayout {
    row_height: RowHeight,
    sections: Option<SectionBands>,
}
```

`Option<SectionBands>` is a real either/or — a list has sections or it does not — and no state
exists in which starts are present without a height.

Rejected in the grill:
- an enum `ListLayout { RowsOnly, Sectioned }` — equally sound, but all eight methods would
  carry a two-arm match, including the four that never look at sections;
- a newtype around the height — the unreachability comes from the *pairing* with the starts,
  not from the height's own type.

### A2. Two total constructors, keeping the lazy header lookup

```rust
pub(in crate::ui) fn rows_only(row_height: RowHeight) -> Self;
pub(in crate::ui) fn sectioned(
    row_height: RowHeight,
    header_height: RowHeight,
    starts: Vec<u32>,     // empty ⇒ stores `sections: None`
) -> Self;
```

**No constructor returns `Option` or `Result`.** `ListGeometry::layout` becomes
`-> ListLayout` and branches itself:

```rust
if section_starts.is_empty() {
    ListLayout::rows_only(row_height)
} else {
    ListLayout::sectioned(row_height, self.section_header_height(db, cache), section_starts)
}
```

This is not a stylistic choice. `section_header_height` runs
`list_geometry_header::load_height(db, density, &cache.section_header_height)` — a cache/DB
lookup that today does **not happen at all** for a section-less list, and this plan is
behaviour-preserving. A single `new(row, header, starts)` would run it on every layout build
of every album, artist and search list. Rejected for that reason; a lazy `impl FnOnce`
parameter was rejected as generics-and-closure noise for what two constructors solve directly.

### A3. The delegation seam, split rather than unwrapped

`ListLayout::content_height` delegates to `list_geometry::content_height`
(`list_geometry.rs:191-204`), which returns `ContentHeight::{Known, Unknown}` because it takes
`Option<RowHeight>`. That enum has three callers where the header height is genuinely unknown
— `trusted_content_height` (`:206-222`), `preseed_upper` (`:224-231`),
`ListGeometry::content_height` (`:503`) — so **`ContentHeight` stays exactly as it is**.

Split the arithmetic instead of unwrapping the result:

```rust
pub(in crate::ui) fn rows_content_height(n_rows: usize, row_height: RowHeight) -> f64;
pub(in crate::ui) fn sectioned_content_height(
    n_rows: usize, n_sections: usize, row_height: RowHeight, header: RowHeight,
) -> f64;
```

The existing `content_height(…, Option<RowHeight>) -> ContentHeight` becomes a thin wrapper
over those two, so its three callers are untouched and the equation stays in one place — the
`mul_add` form **moves**, it is not copied. `ListLayout` then calls the infallible helper that
matches its own state, and there is no `Unknown` arm at the seam to handle. That is the whole
point of the split.

Rejected: letting `ListLayout` recompute the equation inline. One `mul_add` in two places is
exactly the duplicated-decision shape that has already drifted apart once in this codebase.

### A4. What the collapse produces

- `content_height(&self, n_rows) -> f64`
- `max_scroll(&self, n_rows, viewport_height) -> f64`
- `validate` loses its `let Some(content_height) = … else { NoOpinion }`. Its other three
  `NoOpinion` paths — non-finite `upper`, non-positive `upper`, an allocation below the
  prediction — stay byte-for-byte as they are.
- `has_sections()` reads `self.sections.is_some()`.
- `row_top`'s `map_or(0.0, …)` becomes the same either/or, expressing "no sections" rather
  than "height missing".

### A5. Call sites — follow the compiler

Do not treat this as a closed list. Change what `cargo check` points at, anywhere inside this
worktree's `crates/reprise-gnome/src/ui/**`. Known ones:

- `list_geometry.rs:418-428` — `ListGeometry::layout` → `-> ListLayout`, branching as in A2.
- `track_list/track_list_geometry.rs:44-49` — drop the `?` on `geometry.layout(…)`.
  **Keep the other two `?`**, on `shared.column_view.vadjustment()` (`:34`) and on
  `row_height` (`:43`): those are real absences, and the function's own `Option<ListLayout>`
  return stays.
- `track_list/reload_restore.rs:150` — `layout.max_scroll(…)?` loses its `?`. Whatever else
  makes the enclosing function `Option` stays untouched.
- `track_list/reload_anchor_scroll.rs:154` (`applied_layout: Option<&ListLayout>`) and `:396`
  (`-> Option<ListLayout>`) — these encode "no layout available at all" and **stay
  `Option`**. Only the `None` arms that this change makes unreachable may collapse.

**Rule for the whole sweep:** collapse only an `Option` whose `None` arm is unreachable *by
type* after this change. An `Option` that still encodes a real absence stays an `Option`.

---

## Task B — one `headers_above` predicate, carrying the invariant

### The second copy

`headers_above` (`list_geometry_layout.rs:59-64`) counts every start `<= position`, so a
duplicate entry is counted twice, `row_top` gains a phantom header band, and every anchor
derived from it drifts by one header height.

`reload_anchor_scroll.rs:182-185` holds the same predicate a second time, character for
character, on the layout-less adoption path that feeds `ScrollAdoptionGeometry`:

```rust
let preceding_sections = section_starts
    .iter()
    .filter(|start| **start <= guard_position)
    .count();
```

An invariant documented only on `ListLayout` would guard the copy that already has a type
around it and leave the bare one unguarded.

### B1. Extract the predicate, put the assert inside it

```rust
/// Counts the section headers at or above `position`.
///
/// `starts` must be **strictly ascending**. Duplicates would be counted twice
/// and shift every row top below them by one header height. The invariant holds
/// by construction: `compose_virtual` (`reprise-view/src/queue.rs:284-311`)
/// pushes each section at `items.len()` behind a non-emptiness guard, so every
/// section contributes at least one row before the next start is taken. The one
/// theoretical violation is the `u32::try_from(...).unwrap_or(u32::MAX)`
/// saturation at `:296` and `:305`, which needs more than `u32::MAX` queue rows.
///
/// The counting itself does not depend on ordering — the assert is deliberately
/// stricter than the arithmetic needs, because ascending order is what the
/// producer actually guarantees and a break in it is a real upstream bug.
pub(in crate::ui) fn headers_above_in(starts: &[u32], position: u32) -> usize {
    debug_assert!(
        starts.windows(2).all(|pair| pair[0] < pair[1]),
        "section starts must be strictly ascending, got {starts:?}"
    );
    starts.iter().filter(|start| **start <= position).count()
}
```

Both `ListLayout::headers_above` and `reload_anchor_scroll.rs:182-185` call it. No behaviour
changes: same predicate, same count, and `ScrollAdoptionGeometry` keeps its current shape and
fields.

Verify the `compose_virtual` reasoning above against the code before committing to the doc
comment. If the producer turns out to guarantee less than strict ascent, assert the strongest
property it does guarantee and say in the comment why the weaker form is the honest one.

### B2. Explicitly not

- **No runtime dedup, sort or filter.** Absorbing a duplicate would silently paper over a real
  bug in the queue model instead of surfacing it. The `debug_assert!` is the whole mechanism.
- **No release-build assertion.** The condition is unreachable in practice, and a panic in a
  GTK callback is precisely what task A removes.
- **`ScrollAdoptionGeometry` is not restructured.** Replacing its raw `section_count` /
  `preceding_sections` / `row_height` fields with a `ListLayout` is the real end state, but it
  is a behaviour-carrying change inside a `connect_value_changed` callback and needs its own
  display verification. File it as a follow-up issue; do not do it here.

---

## Behaviour must not change, and that has to be shown

1. **The existing unit tests are the oracle.** `list_geometry_layout.rs:177-312` already pins
   `headers_above` across four layout shapes, `row_top` against two header heights, `row_at`
   round-tripping, `content_height`/`max_scroll` at the queue's real numbers, and all three
   `validate` outcomes. They must keep passing with only the mechanical edit of dropping
   `Some(…)`/`.unwrap()` from their expectations — `assert_eq!(layout.content_height(2_276),
   Some(77_456.0))` becomes `assert_eq!(layout.content_height(2_276), 77_456.0)`, **the number
   stays**. Do not weaken an assertion to make it compile. If a value has to change, that is a
   behaviour change: stop and report it instead of adjusting the test.
2. `sectioned_layout_requires_a_header_height` (`:309-312`) asserts a state that no longer
   exists. Replace it — do not just delete it — with a test that pins the new total behaviour:
   `sectioned(row, header, vec![])` equals `rows_only(row)` in `row_top`, `content_height` and
   `has_sections`.
3. Add a test for the invariant: under `#[cfg(debug_assertions)]`, a duplicated start trips
   `headers_above_in` — `#[should_panic(expected = "strictly ascending")]`.
4. Add a test that the two `headers_above` call sites agree: the same starts and position
   through `ListLayout::headers_above` and through `headers_above_in` produce the same count.

## Gates the implementer runs

In `/home/marvin/Projects/reprise-list-geometry-invariants`:

- `cargo fmt --check`
- `cargo clippy --all-targets --workspace -- -D warnings`
- `cargo test --workspace`
- `scripts/check-architecture.sh`
- `cargo test -p reprise-gnome --bins --no-run`

**Display tests cannot be run by the implementer** — they need a real X display. Do not
attempt them and do not claim display coverage.

## The display pass, run separately — twice

Each test in its own process, own XDG roots, `dbus-run-session`, `xvfb-run -a`, and
`GDK_BACKEND=x11 WAYLAND_DISPLAY= GSK_RENDERER=cairo REPRISE_AUDIO_SINK=fakesink`.
Judge on the `^test result:` line **and its count** — a selector matching nothing still prints
`ok. 0 passed`, so a zero count is a miss, not a pass. Finish with `xvfb-orphan-gc --apply`.

Selectors:

- the four `navback_anchor_display_tests` controls, starting with
  `nav_back_lands_on_the_anchored_row`
- `queue_anchor_names_the_row_at_the_viewport_top`
- `que_1_queue_section_headers_share_one_height`
- `browse_4_the_title_link_leaves_the_viewport_at_the_revealed_track`

**Run 1 — the control arm, on this branch before landing.** It measures against the known
parent state and answers one question: does *this* change break the anchors? Without it, a red
result after the rebase cannot be attributed between the parent and this branch.

**Run 2 — after the rebase**, on the tree that actually lands. Run 1's tree no longer exists at
that point, so run 2 is a re-measurement and not a re-confirmation.

`nav_back_to_a_large_sectioned_queue_never_visits_the_top` belongs to the parent branch and may
be red in run 1. That is inherited state: **name it, do not fix it.**

## Landing

The parent lands first. Then, and only then:

```
git fetch origin dev
git rebase --onto origin/dev 5c2dc9a482 feature/list-geometry-invariants
```

so only this branch's own commits sit on the squashed `dev`. Open the PR against `dev`
afterwards — not before, or it carries the parent's nine commits — run display pass 2, then
`land.sh <pr>`. The status block below is what `land.sh` matches on `branch:`.

## Out of scope

From `queue-anchor-grill-followups.md` §6, unchanged:

- per-section header heights in `ListLayout`;
- excluding, renaming or deleting any test;
- `uniform()`'s 0.5 px tolerance and `CONTENT_HEIGHT_EPSILON`;
- item C (the #444 mutation gate) and item D (the full ignored-suite pre-landing run) — they
  belong to the parent branch's landing. **Nothing here may claim `Fixes #444`.**

Plus, from the grill: the `ScrollAdoptionGeometry` restructure (B2), to be filed as an issue.

---

## Parallelität

**The work cannot be cut. One strand.**

Reason: A and B both change `crates/reprise-gnome/src/ui/list_geometry_layout.rs`, and B's
`headers_above_in` is extracted from the very type A restructures — B1's assert would sit in a
function A is rewriting. Two strands would edit the same lines of the same file and conflict on
merge for no wall-clock gain: the whole change is one compile unit plus its call-site sweep.

- **Purpose:** make the unreachable geometry states unrepresentable, and give the one
  remaining unguarded assumption a single home with a debug-only assert.
- **File ownership (globs):**
  - `crates/reprise-gnome/src/ui/list_geometry_layout.rs`
  - `crates/reprise-gnome/src/ui/list_geometry.rs`
  - `crates/reprise-gnome/src/ui/track_list/*.rs` — **except** the two
    `queue_section_*_display_tests.rs` files, which the parent branch owns
  - `docs/plans/list-geometry-invariants.md`
  - plus whatever else inside `crates/reprise-gnome/src/ui/**` the compiler points at. Do not
    stop at the paths named above if a call site lives elsewhere.
- **Merge order:** not applicable within the strand. Across branches, this one lands **after**
  `feature/queue-section-anchor`.
- **Post-merge cross-checks:**
  1. the rebase itself, then the full gate list again — a silent interaction with the parent's
     final test edits shows up there and nowhere else;
  2. display pass run 2, on the rebased tree;
  3. `git grep -n 'ContentHeight::Unknown'` still finds its legitimate callers in
     `list_geometry.rs`, and finds nothing in `list_geometry_layout.rs`;
  4. `git grep -n 'filter(|start| \*\*start <= '` finds exactly one predicate in the tree.
