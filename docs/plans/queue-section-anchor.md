---
slug: queue-section-anchor
worktree: /home/marvin/Projects/reprise-queue-section-anchor
branch: feature/queue-section-anchor
phase: planned
codex_session:
created: 2026-08-13
---
# The sectioned Queue's scroll anchor counts section headers as zero

**Goal:** After a Back navigation into the Queue, the restored viewport shows the row the user left, not a row two positions away that happens to produce the same pixel value. The anchor's `(track id, offset)` pair means what its doc comment says — a row and a distance into that row — in a sectioned list as well as an unsectioned one. After this, exactly one module knows that a row's top edge is `position × row + headers_above × header`, and both the capture and the restore half read it from there.

**Origin:** GitHub issue #444, diagnosed on `dev` @ `a10cd59d60` over six bit-identical runs. `reload_restore.rs::scroll_target()` models a list as rows only; a sectioned list's headers occupy real content height. `list_geometry.rs::content_height()` has known how to count them since `queue-section-preseed` landed — the correction reached the *measuring* side and never the *offset* side.

---

## Decisions

1. **Both halves are fixed, and the defect they hide is latent.** Capture and restore are exact inverses of the same rows-only model, so their errors cancel: the plain Back looks right today. The defect surfaces only when something moves between capture and restore — the section layout changes while you are away, rows are reordered or deleted, or a caller means "put this track at the top" and passes `offset = 0.0`. That made the scope a real decision rather than an obvious yes. It was taken deliberately: the anchor must name a row again instead of reproducing a pixel, and doing nothing was never available, because the display test is red on `dev` and has to be resolved either way. **A restore-only fix is a regression, not a partial fix** — it un-cancels the pair and drifts the round trip by 72 px, ~2 rows, every time. The 2026-08-10 navback handover's warning against "another single fix" describes exactly that failure.

2. **The header term belongs in a value object, not in a parameter.** `scroll_target()` gets a `&ListLayout` — row height, section-header height, section starts — instead of a bare `row_height: f64`. Passing `n_headers_above` and `header_height` as two more scalars would put the counting rule at every call site, which is the same duplicate-predicate drift the fix exists to end. Passing the raw ranges would move the rule into the callee but leave the clamp needing the section count separately. One object answers all three questions.

3. **The codec is bidirectional and ships in one piece.** `row_top(position)` and its inverse `row_at(content_y)` live in the same type, because today's two opposite copies are precisely what cancels.

4. **`n_sections == 0` reduces to today's arithmetic exactly, by construction.** `headers_above(p)` counts section starts `<= p`, which is `0` for an empty start list; `row_top(p)` is then `p × row_height`; `row_at(y)` is `(floor(y / row_height), y − floor(…) × row_height)`, character for character what the unsectioned sites compute now. Every existing unsectioned test keeps its literal expected values, and any that changes is a defect in the change.

5. **The clamp calls `list_geometry::content_height()`, it does not restate it.** `ListLayout::content_height(n_rows)` delegates and maps `ContentHeight::Unknown` to `None`. No second `n_sections × header + rows` expression survives this plan.

6. **The section starts come from `shared.queue_sections`,** the same source `n_sections` already comes from at every geometry call site (`reload_anchor_scroll.rs:224`, `view_state_memory.rs:104`, `track_list_geometry.rs:29`). Reading them from the widget's `SectionModel` instead would introduce a second source that can disagree with the pre-seeded `upper` for one allocation frame — the precise hazard `list_geometry.rs`'s module doc opens with.

7. **The header height is reached through a method on `ListGeometry`, not through public fields on `ListGeometryCache`.** The cache's `Cell<f64>`s carry an encoding (negative = `Assumed`, `-1.0` = invalidated, `0.0` = unloaded) that only `load_trusted_height` should interpret — its own doc comment says *"the same decision living in two functions is how this codebase has produced drift before."* Add `ListGeometry::section_header_height(db, cache) -> RowHeight`, symmetric with the existing `row_height(db, cache)`, and keep the fields private.

8. **A guessed header height must never be written as a scroll value.** The header height is `Assumed` — the 36 px CSS floor — and has never been measured end to end (`queue-section-preseed`'s own record says the measured path was never exercised). In the test the guess happens to equal the allocation exactly, which is why the arithmetic closes; on a desktop whose header allocates 40 px it would not, and the anchor would land 8 px off with nothing to notice it. So `ListLayout` carries a `validate(upper)` — `n_rows × row_height + n_sections × header_height` against the live `upper`, tolerance one row height. On disagreement the geometry counts as **not ready and the restore is deferred**, through the existing `None` path in `reload_anchor_scroll::apply`, not through a new mechanism. A guess that agrees with the allocation is evidence; a guess that disagrees is a guess. The check lives in the layout type, not at the call sites — same single-source reasoning as Decision 2.

9. **The display test splits in two.** `nav_back_to_a_large_sectioned_queue_never_visits_the_top` keeps exactly what its name promises — the journey (the sampled minimum never drops to the top) and the settle, now against a header-aware oracle built from *measured* heights. The two semantic assertions get their own display test in the same file: that the captured anchor names the row that was topmost on screen, and that the anchor row sits at the same on-screen y after the restore. Two reasons. The old name and its exact module path are cited by #444, this plan and the pipeline's reproduction command, so it must not move. And a red run then names which half broke instead of offering four candidates — which matters here, because the journey test can pass while the semantic one fails, and that asymmetry is the signature of a half-fix.

10. **`scroll_center` stays rows-only in this change.** `centered_scroll_value_with_height` (`scroll_center.rs:49,56`) has the same blind spot for centring, and fixing it would pull in the reveal, glide and delete-follow display suites, none of which the implementer can run — the verification cost lands entirely on the human. Task 5 files the follow-up issue, and the new module's doc comment points at the remaining copy so the next reader does not assume the model is single-sourced everywhere.

11. **No new UX rule.** BROWSE-2 ("ID-plus-offset anchor … Back/Forward restores exactly", `docs/ux-rules.md`) already binds this behaviour and is `[active]`. This is a defect against an existing rule.

---

## What is wrong

### The model

A sectioned list's content is not `n_rows × row_height`. With ranges `[(0,1), (1,2276)]`, `row_height = 34`, `header_height = 36`:

```
[0, 36)        header "Now Playing"
[36, 70)       row 0
[70, 106)      header "Play Next"
[106, …)       rows 1 … 2275
```

so `row_top(p) = p × 34 + headers_above(p) × 36`, where `headers_above(p)` counts section starts `<= p` — 1 for `p = 0`, 2 for every `p >= 1`. Content height is `2276 × 34 + 2 × 36 = 77456`, exactly the `upper` the probe measured, because `content_height()` already pre-seeds it correctly.

### Four sites, two of them inverses of the other two

| Site | File | What it computes | Blind to |
|---|---|---|---|
| restore | `reload_restore.rs:150-153` `scroll_target` | `position × row + offset`; clamp `len × row − viewport` | headers above, headers in the clamp |
| restore | `reload_restore.rs:127` `prepaint_guard_position` | `ceil((target + page) / row) − 1` | headers above |
| **capture** | `view_state_memory.rs:115-119` `capture` | `floor(value / row)`, `value − index × row` | headers above |
| **capture** | `track_list_reload.rs:188-192` `capture_reload_anchor` | identical expression | headers above |

Plus one producer of offsets the fixed `scroll_target` will consume: `reload_restore.rs:184` `reanchor_on_track`, which derives an offset as `(anchor_position − track_position) × row_height` — correct only when no header sits between the two rows.

### The measured round trip

Probe output from #444 (`dev` @ `a10cd59d60`):

```
upper=77456  page=249  rows=2276  value=37454
anchor_pos=1101  row_offset=20  ranges=[(0,1), (1,2276)]  headers_above=2
cached_row_h=34   cached_header_h=-36   (negative = Assumed)
```

At `value = 37454` the row genuinely at the top edge is **1099**, 16 px into it (`37454 − 106 = 37348`, `37348 / 34 = 1098.47`). Capture recorded **1101 / 20** — `floor(37454 / 34)`. Restore reproduced `1101 × 34 + 20 = 37454`, the value it started from. The errors cancel, which is why the picture looks right and the anchor is nonetheless meaningless: BROWSE-2's ID-plus-offset anchor has degenerated into the absolute scroll value the module doc says it must never be.

### The prediction

With an honest capture the pair becomes `(1099, 16)` and the target is `1099 × 34 + 2 × 36 + 16 = 37454` — the same pixel, now for the right reason, with the right row named.

| | today | after this plan | restore-only fix |
|---|---|---|---|
| captured anchor id | `1102` (position 1101) | `1100` (position 1099) | `1102` |
| final `adjustment.value()` | `37454` | `37454` | `37526` |
| anchor row's on-screen y | unchanged across the trip | unchanged across the trip | 72 px higher |

**A final value of 37526 is the signature of a half-fix**, not of success. Record whichever number appears.

---

## Global Constraints

- **The file lists are a floor, not a ceiling.** Every task names anchor files; find the rest with `grep`. If a change needs a neighbouring file — a test pinning a literal, a `mod` line, a signature two calls up — change it and name it in the report. Do not stop because a path is missing from a list.
- **Tasks 2 and 3 are one logical change.** Correcting production alone leaves the display test red (its oracle is wrong in the same direction); correcting the oracle alone asserts a defect. Commit them separately if you prefer, but report neither as verified before the human's display pass in Task 5.
- **Language everywhere is English** (AGENTS.md, non-negotiable) — code, comments, doc comments, commit messages.
- **The 800-line ceiling** (`scripts/check-architecture.sh`). At the base commit `list_geometry.rs` is **764** lines. Adding two methods will come close. If it breaches, move part of its `mod tests` into a sibling `#[path]` test file — `list_geometry_cache_tests.rs` is the existing precedent — and **do not trim doc comments to fit** (AGENTS.md). `track_list.rs` must stay below 600; it is 576 at the base commit.
- **RefCell discipline.** `shared.queue_sections` is a `RefCell`. Copy the section starts out in their own statement before any GTK call, the way `let n_sections = shared.queue_sections.borrow().len();` already does at every existing site. A `Ref` held across `ListGeometry::configure` or `adjustment.set_value` is the #1 recurring panic class in this repo.
- **`cargo test -p reprise-gnome --lib` runs nothing.** The crate has no `[lib]`, only `[[bin]] name = "reprise"`. Use `--bins` or `--bin reprise`.
- **Judge every test run on `^test result:` and its count, never on the word `ok`.** With `--exact` and a wrong module path cargo prints `ok` and `0 passed` — a green-looking run that executed nothing. The path in question contains `track_list_reload::`; omitting it matches nothing and still looks like a pass.
- **You cannot run display tests.** There is no Xvfb in this sandbox. Every `#[ignore]`d test here is compiled and listed by you and *executed by the human* in Task 5. Say so plainly rather than implying coverage you did not obtain.
- **File ownership.** AGENTS.md lists the "list geometry service" ownership as ACTIVE over `list_geometry*.rs`, `reload_restore.rs`, `view_state_memory.rs`, `track_list_geometry.rs` and the queue-section display tests. Both its tracks shipped; this branch is their successor. Do not edit the AGENTS.md ownership tables — that file belongs to another strand. Note the overlap in the report.

### Known red before this plan — not caused by it

The bar is **"no new failure against the state before this plan"**, not "green".

- display: `preferences_are_a_dialog_with_a_page_sidebar`
- display: `handle_queue_drop_dispatches_ids_to_the_wired_callback`
- display: `nav_back_to_a_large_sectioned_queue_never_visits_the_top` — the subject of this plan
- the `arch` gate has been reported red for `track_list.rs has 601 lines`. At `a10cd59d60` that file measures **576** and the size section passes. If your baseline prints 601, you are not on the base commit — stop and say so.

A *different* failure than the ones Task 0 records: stop and report it, do not absorb it.

### Sandbox limits — also not a branch failure

- `cargo audit` may not be able to lock `~/.cargo/advisory-db`; skip and note it. The only accepted advisory is RUSTSEC-2024-0436.
- Tests writing through `dirs::cache_dir()` fail with `ReadOnlyFilesystem`; point `XDG_CACHE_HOME` into the worktree.
- `scripts/ci-quality.sh` executes display suites and needs a clean integration worktree; it is the human's, not yours.

---

## Task 0: Record the starting state

- [ ] **Step 1: Baseline**

```bash
cd "$WORKTREE"
git rev-parse HEAD            # must be a10cd59d60ffb40beeafdd1a4ea12531c25303dc
cargo test --workspace 2>&1 | tee /tmp/q444-baseline.txt | tail -40
grep -c '^test result' /tmp/q444-baseline.txt
grep -E '^test result: FAILED' /tmp/q444-baseline.txt
scripts/check-architecture.sh    > /tmp/q444-arch-before.txt   2>&1 || true
scripts/check-ux-traceability.sh > /tmp/q444-trace-before.txt  2>&1 || true
```

Record every failing test by name. That list is the reference for Task 5.

**The display suite belongs in the baseline, and this is why.** `cargo test --workspace` does not start it — display tests are `#[ignore]`d and only run through `scripts/check-display-tests.sh` under `xvfb-run`. A baseline without it records zero display failures, and every test already red on `dev` then looks like this branch's regression when the suite finally runs. That is not hypothetical: it fired on a previous plan for two tests in files that branch never touched. **You cannot start the suite here.** State that, list the three known-red display tests as unattributed, and treat every display result in Task 5 as the human's evidence rather than yours.

- [ ] **Step 2: Prove the target test is discoverable at its exact path**

```bash
cargo test -p reprise-gnome --bins -- --ignored --list | grep queue_section_geometry_display_tests
```

The path must read `ui::track_list::track_list_reload::queue_section_geometry_display_tests::nav_back_to_a_large_sectioned_queue_never_visits_the_top`. Repeat after Tasks 2 and 3 — it is the only guard against silently breaking the human's reproduction command.

- [ ] **Step 3: Record the displayless neighbours that must not move**

```bash
cargo test -p reprise-gnome --bins -- reload_restore     2>&1 | tail -20
cargo test -p reprise-gnome --bins -- view_state_memory  2>&1 | tail -20
cargo test -p reprise-gnome --bins -- list_geometry      2>&1 | tail -20
cargo test -p reprise-gnome --bins -- scroll_center      2>&1 | tail -20
```

All green today, all green at the end, and every *unsectioned* expected value unchanged — see Decision 4.

---

## Task 1: One geometry codec, displayless and test-first

**Files:** add `crates/reprise-gnome/src/ui/list_geometry_layout.rs`; modify `crates/reprise-gnome/src/ui/mod.rs` (the `mod` line, next to `list_geometry_header`).

The `list_geometry_` prefix is deliberate: a `grep -rn list_geometry` must find every part of the row/header model in one search.

- [ ] **Step 1: Write the tests first**

Against this shape (adjust names freely, keep the semantics):

```rust
/// Content-space geometry of a list that may carry section headers: the one
/// place that knows a row's top edge is
/// `position * row_height + headers_above(position) * section_header_height`.
///
/// Deliberately GTK-free. `scroll_center::centered_scroll_value_with_height`
/// still models a list as rows only; it centres rather than anchors and is
/// tracked separately — it is the last remaining copy of this model.
pub(in crate::ui) struct ListLayout { … }

impl ListLayout {
    /// `None` when `section_starts` is non-empty and no header height is known:
    /// a sectioned list whose header height is unknown has no content model,
    /// exactly as `list_geometry::content_height` returns `Unknown` for it.
    fn new(row_height: RowHeight, section_header_height: Option<RowHeight>,
           section_starts: Vec<u32>) -> Option<Self>;
    fn rows_only(row_height: RowHeight) -> Self;

    fn headers_above(&self, position: u32) -> usize;
    fn row_top(&self, position: u32) -> f64;
    /// Inverse of `row_top`: the largest `position` whose top edge is at or
    /// above `content_y`, and the distance from that edge down to `content_y`.
    /// Never clamped to the row count — a `content_y` past the end yields a
    /// position past the end, which callers reject through `track_at`.
    fn row_at(&self, content_y: f64) -> (u32, f64);
    /// The last row whose top edge lies strictly above `content_y`: the row at
    /// the viewport's lower edge for a `scroll_to` guard.
    fn last_row_above(&self, content_y: f64) -> Option<u32>;
    /// Delegates to `list_geometry::content_height`; `None` for `Unknown`.
    fn content_height(&self, n_rows: usize) -> Option<f64>;
    fn max_scroll(&self, n_rows: usize, viewport_height: f64) -> Option<f64>;
    /// Decision 8: does this layout describe the live allocation? Compares
    /// `content_height(n_rows)` with `upper`, tolerance one row height. A
    /// layout that fails this must not be used to write a scroll value.
    fn validate(&self, n_rows: usize, upper: f64) -> bool;
}
```

Required coverage. **A test that exercises one section layout cannot tell a value from a constant** — `headers_above` returning the literal `2` would pass a single-layout suite:

- `headers_above` across **four** layouts: `[]`, `[0]`, `[0, 1]` (the Queue's own), `[0, 12, 40]`; at position 0, at each start, at a start − 1, and past the last start.
- **Two different header heights** (36 and 20) with the same starts, asserting the results differ by the expected multiple. A silently ignored header height passes every single-height test.
- `row_top(0) == 0.0` for `rows_only`, `== header_height` when a section starts at 0.
- Round trip over several layouts and every position in a small model: `row_at(row_top(p)) == (p, 0.0)`.
- Reconstruction over sampled `content_y`, **including values inside a header band**: for `(p, o) = row_at(y)`, `row_top(p) + o == y` exactly. Inside a header band the offset legitimately exceeds one row height; below `row_top(0)` it is legitimately negative (`reanchor_on_track` already documents negative offsets as meaningful).
- `last_row_above(k × row) == k − 1` and `last_row_above(k × row + 0.5) == k` for `rows_only` — pins today's `ceil(…) − 1` boundary behaviour, which a naive `row_at`-based implementation gets wrong by one at exact boundaries.
- `content_height` equals `list_geometry::content_height`'s `Known` value for the same inputs, including the Queue's real numbers: 2276 rows, starts `[0, 1]`, 34/36 → `77456`.
- **`validate`, both ways** (Decision 8): true for `2276/34/36/[0,1]` against `upper = 77456`; false against an `upper` that implies a 40 px header (`77464`), i.e. a disagreement larger than one row height. Both cases named explicitly — a guard that is never seen to reject proves nothing.
- **The reduction**: for `rows_only`, `row_top(p) == p × h`, `row_at(y) == (floor(y / h), y − floor(y / h) × h)`, `content_height(n) == n × h`. Its own named test — it is the contract Decision 4 rests on.

- [ ] **Step 2: Implement**

`headers_above` counts starts `<= position`, so it does not depend on the starts being sorted. One workable shape for `row_at`, given at most a handful of sections: for `h` from `section_starts.len()` down to `0`, take `p = floor((content_y − h × header) / row)` and accept the first candidate whose `headers_above(p) == h`; if none matches, `content_y` lies above `row_top(0)` — return `(0, content_y − row_top(0))`. Implement it however you like; the property tests are the specification.

- [ ] **Verification**

```bash
cargo test -p reprise-gnome --bins -- list_geometry_layout 2>&1 | grep -E '^test result'
cargo clippy --all-targets --workspace -- -D warnings
```

The `test result:` line must show a non-zero count. Zero passed is a failure, not a pass.

---

## Task 2: Both halves of the anchor move onto the codec

**Files (a floor):**
- `crates/reprise-gnome/src/ui/list_geometry.rs` — add `ListGeometry::section_header_height(db, cache) -> RowHeight` (via `list_geometry_header::load_height` with the private `density()` and cache cell) and `ListGeometry::layout(db, cache, section_starts) -> Option<ListLayout>`. **Only consult the header height when `section_starts` is non-empty** — `load_height` writes the cache and the settings row on a cold read, and calling it for unsectioned views would be a behaviour change. Watch the 764-line budget.
- `crates/reprise-gnome/src/ui/track_list/track_list_geometry.rs` — the TrackList adapter, e.g. `layout(shared, captured_row_height: Option<RowHeight>, n_rows: usize) -> Option<ListLayout>`: copy the section starts out of `shared.queue_sections` in one statement, take `captured_row_height.or_else(|| geometry.observed_row_height(…))` so each caller keeps the height source it has today, build through `ListGeometry::layout`, and **return `None` when `validate(n_rows, adjustment.upper())` is false** (Decision 8). This file is 85 lines and is already the TrackList↔geometry seam.
- `crates/reprise-gnome/src/ui/track_list/reload_restore.rs`, `reload_anchor_scroll.rs`, `view_state_memory.rs`, `track_list_reload.rs`
- `crates/reprise-gnome/src/ui/tag_edit/tag_reload_anchor.rs`, `tag_edit_flow.rs`
- `crates/reprise-gnome/src/ui/track_list/reveal_track_display_tests.rs`, `tag_mutation_refresh_display_tests.rs` (call sites in test helpers)

- [ ] **Step 1: The restore side**

Found with `grep -rn 'scroll_target\|prepaint_guard_position' crates/`:

| Call site | Change |
|---|---|
| `reload_restore.rs:140` `scroll_target` | `row_height: f64` → `layout: &ListLayout`. Body: `layout.row_top(position) + offset`, clamped to `0.0 ..= layout.max_scroll(current_ids.len(), viewport_height)?`. |
| `reload_restore.rs:120` `prepaint_guard_position` | Same parameter change; `let last = layout.last_row_above(target + viewport_height)?.min(len − 1)` — drop the local `ceil` division. |
| `reload_restore.rs:184` `reanchor_on_track` | `&ListLayout`. Offset becomes `layout.row_top(anchor_position) − layout.row_top(track_position) + anchor_offset`. Keep the NaN guard's *intent*: `ListLayout` can only hold a finite positive `RowHeight`, so the guard moves into the constructor — say so in the doc comment rather than deleting the reasoning. |
| `reload_anchor_scroll.rs:240` (`apply`) | Build once via the adapter with `captured_row_height`, `current_ids.len()`; pass `adjustment.page_size()`. Return `false` when the adapter yields `None` — which now also covers a failed `validate`, so a mismatched geometry defers instead of writing a guess. |
| `reload_anchor_scroll.rs:62` (`scroll_to_anchor`) | Today it re-derives from `captured_row_height.zip(page)`. Thread the *same* `ListLayout` down from `schedule`/`refine_once`, or rebuild it through the adapter — but it must be the same layout `apply` used, or the guard row and the written value disagree. |
| `view_state_memory.rs:239` (`apply_restored_scroll`) | Adapter with `captured_row_height = None`; pass `page`. |
| `view_state_memory.rs:90` (`#[cfg(test)]` wrapper) | Takes `&ListLayout`; its one test (`browse_2_anchor_survives_resort`, `:315`) passes `ListLayout::rows_only(20.0)` and **keeps `Some(66.0)`**. |
| `reveal_track_display_tests.rs:71` (`anchor_target`) | Unsectioned Library view: `ListLayout::rows_only(RowHeight::new(upper / len)?)`. Value unchanged. |
| `reload_restore.rs` own unit tests | Pass `rows_only`; every existing expected value stays (`145.0`, `10.0`, `0.0`, the `None`s). Sectioned cases come in Step 4. |
| `tag_reload_anchor.rs:31,46` + `tag_edit_flow.rs:505-513` + `tag_mutation_refresh_display_tests.rs:379` | `&ListLayout`, built through the adapter at the `tag_edit_flow` site. In scope because `reanchor_on_track` *produces* an offset the now-header-aware `scroll_target` consumes; leaving it rows-only re-opens the inconsistency this task closes. Byte-identical for the unsectioned views these tests use. |

- [ ] **Step 2: The capture side**

- `view_state_memory.rs:113-120` (`capture`): replace `let index = (scroll_value / height).floor().max(0.0) as u32; … (track.id, scroll_value − index × height)` with `let (position, offset) = layout.row_at(scroll_value.max(0.0));` then the same `shared.model.track_at(position)`. **Keep the `.max(0.0)` at the caller**, not inside `row_at` — it preserves today's unsectioned behaviour exactly, and `row_at` must stay a pure inverse.
- `track_list_reload.rs:186-193` (`capture_reload_anchor`): identical expression, identical replacement. Its `capture_row_height` (`:141`) is unchanged — it still supplies the height the adapter prefers over the observed one.
- `track_list_reload.rs:125-136` (`pending_reveal_anchor`) is **not** changed here: it expresses a centring, not a viewport-top anchor, and belongs with `scroll_center` (Decision 10). Leave a one-line comment naming that.

- [ ] **Step 3: Confirm the reduction empirically**

```bash
cargo test -p reprise-gnome --bins -- reload_restore view_state_memory list_geometry scroll_center 2>&1 | grep -E '^test result'
git diff --stat
```

Every previously-passing displayless test must still pass **with its original expected values**. If an unsectioned expectation had to move, stop: Decision 4 is violated and the codec is wrong, not the test.

- [ ] **Step 4: Sectioned unit tests in `reload_restore.rs`**

At least: the Queue's own layout (2276 rows, starts `[0, 1]`, 34/36) asserting `scroll_target(Some((id, 16.0)), …, position 1099) == Some(37454.0)`; the same anchor under a **different** layout (starts `[0]`, or a 20 px header) producing a different, hand-computed value; and the clamp near the end of the list, where the correct bound is `77456 − page` and the old one was 72 px short.

- [ ] **Verification**

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace 2>&1 | tee /tmp/q444-task2.txt | tail -40
diff <(grep -oE '^test [a-z0-9_:]+ \.\.\. FAILED' /tmp/q444-baseline.txt | sort) \
     <(grep -oE '^test [a-z0-9_:]+ \.\.\. FAILED' /tmp/q444-task2.txt   | sort)
scripts/check-architecture.sh
cargo test -p reprise-gnome --bins -- --ignored --list | grep -c display_tests
```

The diff must be empty in the new-failure direction. The `--list` count must not drop — a display test that stopped compiling is invisible to `cargo test --workspace`.

---

## Task 3: The display test splits, and its oracle becomes evidence

**Files:** `crates/reprise-gnome/src/ui/track_list/queue_section_geometry_display_tests.rs`.

**Why an arithmetic oracle cannot be the primary evidence.** Whatever formula the test uses, it reads `captured_anchor` — so it moves with the capture side:

| production state | header-aware arithmetic oracle |
|---|---|
| both halves fixed | `expected = 37454`, value `37454` → passes ✓ |
| neither fixed (today) | `expected = 37526` (from the captured 1101), value `37454` → fails ✓ |
| restore-only fix | `expected = 37526`, value `37526` → **passes ✗** |
| capture-only fix | `expected = 37454`, value `37382` → fails ✓ |

The restore-only row is the one that matters: it is what the three earlier point fixes did, and an arithmetic oracle waves it through while the user's viewport lands two rows low. Only comparing the restored picture against the captured picture catches it — an observation, not a formula.

- [ ] **Step 1: Two tree-reading helpers**

Follow the walk already in this file (`rendered_queue_headers`, `:192`) and in `crates/reprise-gnome/src/ui/scroll_probe.rs` (which matches on `type_().name().contains("ColumnViewRow")`).

```rust
/// Every realized track row, as (title, top edge in ColumnView coordinates),
/// sorted by y. Rows are matched by carrying a `gtk4::Label` whose text starts
/// with "Track " — that is what excludes the ColumnView's own column-title row,
/// which is a `ColumnViewRow` widget too.
fn rendered_rows(column_view: &gtk4::ColumnView) -> Vec<(String, f32)>;

/// The rendered height of a section header and of a track row, measured from
/// the widget tree rather than from the geometry cache.
fn rendered_band_heights(column_view: &gtk4::ColumnView) -> Option<(f32, f32)>;
```

**Both helpers must discard zero-height widgets.** Measured on 2026-08-13: a walk that took every label with the `queue-section-header` class returned two entries, both `y=25, height=0` — unrealized widgets out of GTK's recycling pool, not the rendered headers. The codebase already solves this in `list_geometry::RowMeasurement::from_widget_heights` ("Zero-height widgets are unrealized and do not describe a bound row"). Apply the same filter, and say why in a comment, or these assertions will read garbage and look like anchor bugs.

Use `compute_bounds(column_view)` — established prior art in this repo, including in display tests. GTK allocates visible children at `content_y − adjustment.value()`, so a child's y in the ColumnView's space is viewport-relative; the column-title bar contributes a constant that cancels in every comparison below, so no assertion may depend on its absolute value.

- [ ] **Step 2: The existing test keeps the journey**

`nav_back_to_a_large_sectioned_queue_never_visits_the_top` keeps its name, its path and its two assertions — the sampled minimum (`minimum > expected − 2 × row_h`) and the settle (`|value − expected| < row_h`) — but the oracle changes. Replace `let row_height = adjustment.upper() / restored_ids.len() as f64;` (`:279`) with:

- `(row_h, header_h)` from `rendered_band_heights`;
- **a precondition**: `restored_ids.len() × row_h + queue_ranges.len() × header_h` equals `adjustment.upper()` within a row. This is #444's own identity, `2276 × 34 + 2 × 36 = 77456`, and it validates the measured inputs against the pre-seeded range. A failure here is a geometry finding (the rendered list does not match the range), not necessarily an anchor bug — say so in the message;
- `expected = anchor_position × row_h + headers_above × header_h + captured_anchor.row_offset`, where `headers_above` counts the starts in the test's own `queue_ranges` that are `<= anchor_position`. The test owns `queue_ranges` already (`:219`); it must **not** import `ListLayout` — the point is a second, independently-sourced computation, not a call into the code under test.

- [ ] **Step 3: A new display test owns the semantics**

Same file, own name — say what it asserts, e.g. `queue_anchor_names_the_row_at_the_viewport_top`. Two assertions:

1. **Capture** — right after `let captured_anchor = …` (`:247`), while the sectioned Queue is still showing: the topmost rendered row's title equals `format!("Track {:04}", captured_anchor.track_id)`. Today this reports `Track 1102` against a topmost `Track 1100` — the two rows the missing 72 px buys. Print the first few rendered rows in the failure message: if the helper's coordinate assumption is wrong, the human sees which rows were found and where, instead of an unexplained mismatch.
2. **Round trip** — after the restore has settled: the anchor row's on-screen y is within 1 px of the y it had before the navigation. Same widget, same coordinate space, same run — every constant cancels and no geometry model appears in the assertion at all. This is the user-visible content of BROWSE-2 and the guard against "the jump moved rather than went away".

- [ ] **Step 4: Extend the probe line**

Keep `QUEUEPROBE` (`:296`) — it is printed on the green path on purpose and `queue-section-preseed` cites its wording verbatim. Add measured `row_h` and `header_h`, `headers_above`, the captured `(track_id, row_offset)`, the two on-screen y values, and `expected` vs `final`. The human must be able to fill in the prediction table from one line of output.

- [ ] **Verification (compile and discoverability only — you cannot run it)**

```bash
cargo test -p reprise-gnome --bins -- --ignored --list \
  | grep queue_section_geometry_display_tests
cargo clippy --all-targets --workspace -- -D warnings
```

The old test's exact path must be unchanged and the new one must appear beside it. Report that both compile and list, and that neither was executed.

---

## Task 4: Mutation proofs

*An assertion never made to fail proves only its own arithmetic.*

- [ ] **Step 1: The displayless mutation — you run this**

```bash
# make headers_above always answer 0 — the pre-fix model
$EDITOR crates/reprise-gnome/src/ui/list_geometry_layout.rs   # or a targeted sed
cargo test -p reprise-gnome --bins -- list_geometry_layout 2>&1 \
  | grep -E '^test result' || echo INCONCLUSIVE
git checkout crates/reprise-gnome/src/ui/list_geometry_layout.rs
cargo test -p reprise-gnome --bins -- list_geometry_layout 2>&1 \
  | grep -E '^test result' || echo INCONCLUSIVE
```

Expected: `FAILED` with the mutation, `ok` with a non-zero count without it. **`INCONCLUSIVE` counts as a failure** — a run that printed no `test result:` line executed nothing. Paste both outputs into the report.

- [ ] **Step 2: The same against the production sites**

Repeat with the header term removed from `scroll_target`'s call into `row_top` (a restore-only regression) and confirm the sectioned `reload_restore` tests from Task 2 Step 4 go red. This is the displayless stand-in for the display mutation the human runs.

- [ ] **Step 3: Also mutate the guard**

Make `validate` always return `true` and confirm its rejecting unit test from Task 1 fails. A guard that cannot be seen to reject is decoration.

- [ ] **Step 4: Write the display mutations down for the human**

Specify them precisely enough to run without re-deriving anything:

1. **Header term reverted** (`headers_above` → `0`): both display tests must **fail**.
2. **Restore-only fix** (capture back to `floor(value / row_height)`): the new semantic test must **fail** on the round-trip assertion with `y_after − topmost_y ≈ 72`, while the journey test may still pass. That asymmetry is the whole point of the split.

---

## Task 5: Verification, the human's display pass, and the report

- [ ] **Step 1: The gates you can run**

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace 2>&1 | tee /tmp/q444-after.txt | tail -40
diff <(grep -oE '^test [a-z0-9_:]+ \.\.\. FAILED' /tmp/q444-baseline.txt | sort) \
     <(grep -oE '^test [a-z0-9_:]+ \.\.\. FAILED' /tmp/q444-after.txt    | sort)
grep -c '^test result' /tmp/q444-after.txt      # compare with the Task 0 count
scripts/check-architecture.sh
scripts/check-ux-traceability.sh
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'   # must be empty
```

Filter on `^test result: FAILED`; a bare `FAILED` also matches summary lines of passing runs.

- [ ] **Step 2: File the follow-up issue**

`scroll_center::centered_scroll_value_with_height` and `track_list_reload::pending_reveal_anchor` keep the rows-only model for centring (Decision 10). File an issue referencing #444 and this plan, and put its number in the report — an out-of-scope note nobody files is a note nobody reads.

- [ ] **Step 3: The display pass — the human, not you**

Risk-targeted, six runs, each with the question it answers. Not the full display gate: it carries more known redness than information here. Per AGENTS.md every headless command carries its own XDG roots, its own D-Bus session and `WAYLAND_DISPLAY=`.

```bash
cd <worktree>
R=$(mktemp -d); mkdir -p "$R"/{config,data,cache,state}
env XDG_CONFIG_HOME=$R/config XDG_DATA_HOME=$R/data XDG_CACHE_HOME=$R/cache XDG_STATE_HOME=$R/state \
    GDK_BACKEND=x11 WAYLAND_DISPLAY= GSK_RENDERER=cairo REPRISE_AUDIO_SINK=fakesink \
    dbus-run-session -- xvfb-run -a cargo test -p reprise-gnome --bins -- \
      --ignored --exact --nocapture <full::module::path> \
  2>&1 | tee /tmp/q444-display.txt
grep -E '^test result|QUEUEPROBE' /tmp/q444-display.txt
```

| # | Run | Question it answers |
|---|---|---|
| 1 | `nav_back_to_a_large_sectioned_queue_never_visits_the_top` | Does the journey still avoid the top, and does it settle on a header-aware oracle? |
| 2 | the new semantic test | Does the anchor name the right row, and does the row come back to the same height? |
| 3 | mutation "header term reverted" | Both red — is the header term load-bearing? |
| 4 | mutation "restore-only fix" | Semantic test red with `≈ 72` px, journey test possibly green — is the split doing its job? |
| 5 | `navback_anchor_display_tests` | Unsectioned lists unchanged — the real-display proof of Decision 4. |
| 6 | `queue_section_header_display_tests` and `reveal_track_display_tests` | The two suites sharing the sectioned geometry and the `offset = 0.0` caller. |

Judge each on `^test result:` **and the count**. Without `track_list_reload::` in the path, `--exact` matches nothing and prints `ok` with `0 passed`. Run `xvfb-orphan-gc --apply` afterwards.

- [ ] **Step 4: The report**

Name every file touched beyond the task lists. Include both mutation outputs from Task 4, the failure diff from Step 1, the suite counts before and after, the follow-up issue number, and — filled in from `QUEUEPROBE` — the prediction table: captured anchor id, final `adjustment.value()`, and the two on-screen y values. **If the final value is 37526 rather than 37454, say so plainly**: that is a restore-only fix and the capture half did not land. Where a number in this plan turned out wrong once the display run computed it, give the run's number and say which conclusion it changes.

---

## Verification of the whole strand

- One module knows the row/header model; `grep -rn 'headers_above\|row_top' crates/` finds it in one file, and the only remaining rows-only model is the named, documented one in `scroll_center`.
- `list_geometry::content_height` has exactly one caller-side re-implementation: none.
- Every unsectioned expected value in the displayless suites is unchanged from the Task 0 baseline.
- The codec's tests drive at least four section layouts and two header heights, and go red when `headers_above` is reduced to a constant.
- `validate` has a test that sees it reject, and that test goes red when the guard is stubbed to `true`.
- The new semantic test's capture assertion is red on the base commit and green after — that is the assertion carrying the defect.
- The round-trip assertion is red for a restore-only fix, by ≈ 72 px.
- No new failure against the Task 0 baseline, display suite included.

---

## Out of scope

- **`scroll_center::centered_scroll_value_with_height`** (`scroll_center.rs:49,56`), its caller `reload_restore::centered_track_scroll_target`, and `track_list_reload::pending_reveal_anchor`. Same blind spot, centring rather than anchoring semantics, and a blast radius across display suites the implementer cannot run. Followed up by the issue filed in Task 5 Step 2.
- **The header height's provenance.** It is still `Assumed` (the 36 px CSS floor) in every run recorded so far; `queue-section-preseed`'s own record says the measured/persisted path was never exercised end to end. Decision 8's guard makes a wrong guess visible instead of silent, which is what this plan owes; measuring it properly is its own strand.
- **`upper` pre-seeding and the allocation ordering** (`reload_anchor_scroll::arm_refinement`, `REPRISE_RESTORE_AFTER_ALLOCATION`). Already correct — the pre-seeded `upper` of 77456 matches the sectioned content height exactly. This plan changes where the anchor lands inside that range, not the range.
- **Renaming the display test to a `browse_2_…` rule-named form.** More traceable, and it would break the reproduction command #444, this plan and the pipeline all cite. Its own commit, after this one is green.
