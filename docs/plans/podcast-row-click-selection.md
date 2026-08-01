---
slug: podcast-row-click-selection
worktree: /home/marvin/Projects/reprise-row-selection
branch: feature/podcast-row-click-selection
phase: coded
codex_session:
created: 2026-08-01
spec: docs/superpowers/specs/2026-08-01-podcast-row-click-selection-design.md
---
# Episode rows select on click — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:subagent-driven-development`
> or `superpowers:executing-plans` and work task by task. Steps are checkboxes.

**Goal:** Episode rows in the Podcasts/YouTube surfaces select like track rows —
click selects, Ctrl-click toggles, Shift-click extends, double click plays.

**Architecture:** The selection mechanics become order-aware free functions in
`podcasts_selection.rs` that both selection owners call (the grouped view's
`PodcastSelection`, the detail view's per-channel `BTreeSet`). Rows report
clicks through one new `podcasts.select-row` action carrying `(episode_id,
mode)`. Applying a selection stops going through `render()` and instead updates
the affected row widgets in place, which is what makes keyboard selection
possible at all.

**Tech Stack:** Rust, gtk4-rs, libadwaita. No new dependencies.

## Global Constraints

- `docs/ux-rules.md` rule ids are **append-only**. The new rule is **SRC-14**
  (`SRC-13` is the highest in use). A new `[active]` rule and the code
  implementing it land in the **same commit**, and `check-ux-traceability.sh`
  requires at least one test whose name carries `src_14`.
- `scripts/check-input-parity.sh` fails any new `GestureClick`/`GestureDrag`/
  `DragSource`/`DropTarget` under `crates/reprise-gnome/src/ui` without an
  `// input-parity: ACC-8 keyboard=<tested-partner>` comment on the line
  directly above it, and the named partner needs a real test.
- Files stay under 800 lines. `podcasts_groups.rs` is 404 and
  `youtube_channel_detail.rs` 624 — put new logic in
  `podcasts_selection.rs`/`podcasts_row_interaction.rs`, do not grow the
  detail view.
- No new user-visible strings. This change adds no `po/` work; if a step seems
  to need a new string, stop and re-read the step.
- Display tests do not run in a headless sandbox without Xvfb. Write them; do
  not claim to have run them unless Xvfb is present. Per-file runs only —
  `cargo test -p reprise-gnome` in one batch is flaky on this project.
- Immutability: the shared selection functions take `&mut` state deliberately
  (they are the state owner's mutators) but must not mutate their `order`
  input.

## File map

| File | Responsibility after this change |
|---|---|
| `podcasts_selection.rs` | Selection state **plus** the shared anchor/range mechanics both surfaces call |
| `podcasts_row_interaction.rs` | Pointer and keyboard wiring for a row, including the new modes |
| `podcasts_groups.rs` | Records the rendered order; applies the selected CSS class; registers row widgets for in-place updates |
| `podcasts_view.rs` | Holds `selection_widgets`; derives the rendered order; `apply_selection()` |
| `podcasts_view_actions.rs` | The `podcasts.select-row` action |
| `podcasts_context_menu.rs` | Unchanged logic; the secondary click sets the target first |
| `youtube_channel_detail.rs` | Calls the same shared mechanics |
| `css.rs` | `.reprise-podcast-episode-selected` |
| `docs/ux-rules.md` | `SRC-14`, appended to section AF |

---

### Task 1: Order-aware selection mechanics

**Files:**
- Modify: `crates/reprise-gnome/src/ui/podcasts/podcasts_selection.rs`
- Test: same file's `#[cfg(test)] mod tests` (add one if absent; otherwise
  append)

**Interfaces:**
- Produces:
  - `pub(super) enum SelectMode { Only, Toggle, Range }` with
    `pub(super) const fn as_u8(self) -> u8` and
    `pub(super) const fn from_u8(value: u8) -> Option<Self>`
    (`0 => Only, 1 => Toggle, 2 => Range`)
  - `pub(super) fn apply_select(selected: &mut BTreeSet<i64>, anchor: &mut Option<i64>, order: &[i64], episode_id: i64, mode: SelectMode)`
  - `PodcastSelection::apply(&mut self, order: &[i64], episode_id: i64, mode: SelectMode)`
- Consumes: nothing.

- [ ] **Step 1: Write the failing tests**

Append to the tests module in `podcasts_selection.rs`:

```rust
    fn ids(selected: &BTreeSet<i64>) -> Vec<i64> {
        selected.iter().copied().collect()
    }

    #[test]
    fn src_14_only_replaces_the_selection_and_moves_the_anchor() {
        let mut selected = BTreeSet::from([7, 8]);
        let mut anchor = Some(7);
        apply_select(&mut selected, &mut anchor, &[7, 8, 9], 9, SelectMode::Only);
        assert_eq!(ids(&selected), vec![9]);
        assert_eq!(anchor, Some(9));
    }

    #[test]
    fn src_14_toggle_adds_then_removes_and_moves_the_anchor() {
        let mut selected = BTreeSet::new();
        let mut anchor = None;
        apply_select(&mut selected, &mut anchor, &[7, 8, 9], 8, SelectMode::Toggle);
        assert_eq!(ids(&selected), vec![8]);
        assert_eq!(anchor, Some(8));
        apply_select(&mut selected, &mut anchor, &[7, 8, 9], 8, SelectMode::Toggle);
        assert!(selected.is_empty());
        assert_eq!(anchor, Some(8));
    }

    #[test]
    fn src_14_range_spans_the_rendered_order_in_both_directions() {
        let order = [1, 2, 3, 4, 5];
        let mut selected = BTreeSet::new();
        let mut anchor = None;
        apply_select(&mut selected, &mut anchor, &order, 4, SelectMode::Only);
        apply_select(&mut selected, &mut anchor, &order, 2, SelectMode::Range);
        assert_eq!(ids(&selected), vec![2, 3, 4], "a backwards range still spans");
        assert_eq!(anchor, Some(4), "a range never moves the anchor");
        apply_select(&mut selected, &mut anchor, &order, 5, SelectMode::Range);
        assert_eq!(ids(&selected), vec![4, 5], "the range is re-taken from the anchor");
    }

    #[test]
    fn src_14_range_without_a_usable_anchor_selects_only_the_clicked_row() {
        let mut selected = BTreeSet::from([1]);
        let mut anchor = None;
        apply_select(&mut selected, &mut anchor, &[1, 2, 3], 3, SelectMode::Range);
        assert_eq!(ids(&selected), vec![3]);
        assert_eq!(anchor, Some(3));

        // An anchor that is no longer rendered (its group was collapsed) is
        // not a usable anchor either.
        let mut selected = BTreeSet::from([9]);
        let mut anchor = Some(9);
        apply_select(&mut selected, &mut anchor, &[1, 2, 3], 2, SelectMode::Range);
        assert_eq!(ids(&selected), vec![2]);
        assert_eq!(anchor, Some(2));
    }

    #[test]
    fn src_14_a_row_outside_the_rendered_order_is_still_selectable() {
        let mut selected = BTreeSet::new();
        let mut anchor = None;
        apply_select(&mut selected, &mut anchor, &[1, 2], 99, SelectMode::Only);
        assert_eq!(ids(&selected), vec![99]);
    }

    #[test]
    fn src_14_select_modes_survive_the_action_round_trip() {
        for mode in [SelectMode::Only, SelectMode::Toggle, SelectMode::Range] {
            assert_eq!(SelectMode::from_u8(mode.as_u8()), Some(mode));
        }
        assert_eq!(SelectMode::from_u8(3), None);
    }
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cargo test -p reprise-gnome --lib podcasts::podcasts_selection
```

Expected: compile error — `SelectMode` and `apply_select` do not exist.

- [ ] **Step 3: Implement the mechanics**

In `podcasts_selection.rs`, above `impl PodcastSelection`:

```rust
/// What a click means for the selection. Crossing the action boundary as a
/// `u8` keeps one action where three would otherwise be needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelectMode {
    Only,
    Toggle,
    Range,
}

impl SelectMode {
    pub(super) const fn as_u8(self) -> u8 {
        match self {
            SelectMode::Only => 0,
            SelectMode::Toggle => 1,
            SelectMode::Range => 2,
        }
    }

    pub(super) const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(SelectMode::Only),
            1 => Some(SelectMode::Toggle),
            2 => Some(SelectMode::Range),
            _ => None,
        }
    }
}

/// The selection mechanics both episode surfaces share.
///
/// `order` is the episode ids as they are rendered right now, top to bottom
/// and across group boundaries. A range is defined only over that order:
/// episodes inside a collapsed group, behind a "Show all N" window, or hidden
/// by the active filter are not rendered, so a Shift-click never sweeps them
/// up. `PodcastSelection` and the channel detail view own their state
/// differently, which is why this takes the pieces rather than a receiver.
pub(super) fn apply_select(
    selected: &mut BTreeSet<i64>,
    anchor: &mut Option<i64>,
    order: &[i64],
    episode_id: i64,
    mode: SelectMode,
) {
    match mode {
        SelectMode::Only => {
            selected.clear();
            selected.insert(episode_id);
            *anchor = Some(episode_id);
        }
        SelectMode::Toggle => {
            if !selected.remove(&episode_id) {
                selected.insert(episode_id);
            }
            *anchor = Some(episode_id);
        }
        SelectMode::Range => {
            let span = anchor
                .and_then(|anchor| position(order, anchor))
                .zip(position(order, episode_id));
            let Some((from, to)) = span else {
                // No anchor, or an anchor that is no longer on screen: the
                // honest fallback is the row the user actually clicked.
                return apply_select(selected, anchor, order, episode_id, SelectMode::Only);
            };
            selected.clear();
            selected.extend(order[from.min(to)..=from.max(to)].iter().copied());
        }
    }
}

fn position(order: &[i64], episode_id: i64) -> Option<usize> {
    order.iter().position(|candidate| *candidate == episode_id)
}
```

And on `impl PodcastSelection`:

```rust
    pub(super) fn apply(&mut self, order: &[i64], episode_id: i64, mode: SelectMode) {
        apply_select(&mut self.selected, &mut self.anchor, order, episode_id, mode);
    }
```

Add the field to the struct:

```rust
pub(super) struct PodcastSelection {
    selected: BTreeSet<i64>,
    anchor: Option<i64>,
}
```

`#[derive(Default)]` stays; `Option` defaults to `None`.

- [ ] **Step 4: Run the tests**

```bash
cargo test -p reprise-gnome --lib podcasts::podcasts_selection
```

Expected: all six new tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/podcasts/podcasts_selection.rs
git commit -m "feat(podcasts): add order-aware selection mechanics"
```

---

### Task 2: The rendered order

**Files:**
- Create: `crates/reprise-gnome/src/ui/podcasts/podcasts_rendered_order.rs`
- Modify: `crates/reprise-gnome/src/ui/podcasts/mod.rs` (declare the module)
- Test: `crates/reprise-gnome/src/ui/podcasts/podcasts_rendered_order_tests.rs`

**Interfaces:**
- Consumes: `SourceGroup` from `reprise_core::podcasts`,
  `podcasts_episode_window::visible_count`.
- Produces:
  `pub(super) fn rendered_episode_ids(groups: &[SourceGroup], expanded_sources: &BTreeSet<i64>, expanded_episode_sources: &BTreeSet<i64>) -> Vec<i64>`

The order is computed from the same inputs `podcasts_groups::render` uses, so
it can be tested without a display. A collapsed group contributes nothing: its
rows exist as widgets but the user cannot see them, and a Shift-click must not
reach through a closed expander.

- [ ] **Step 1: Write the failing test**

Create `podcasts_rendered_order_tests.rs`:

```rust
//! The rendered order a Shift-click ranges over.

use std::collections::BTreeSet;

use super::rendered_episode_ids;
use crate::ui::podcasts::podcasts_groups_tests::support::{group, episode};

#[test]
fn src_14_a_collapsed_group_contributes_no_rows() {
    let groups = vec![group(1, &[episode(10), episode(11)]), group(2, &[episode(20)])];
    let expanded = BTreeSet::from([2]);
    assert_eq!(
        rendered_episode_ids(&groups, &expanded, &BTreeSet::new()),
        vec![20]
    );
}

#[test]
fn src_14_the_order_runs_across_groups_in_render_order() {
    let groups = vec![group(1, &[episode(10), episode(11)]), group(2, &[episode(20)])];
    let expanded = BTreeSet::from([1, 2]);
    assert_eq!(
        rendered_episode_ids(&groups, &expanded, &BTreeSet::new()),
        vec![10, 11, 20]
    );
}

#[test]
fn src_14_a_windowed_group_contributes_only_its_visible_ten() {
    let episodes = (0..12).map(|index| episode(100 + index)).collect::<Vec<_>>();
    let groups = vec![group(1, &episodes)];
    let expanded = BTreeSet::from([1]);
    let windowed = rendered_episode_ids(&groups, &expanded, &BTreeSet::new());
    assert_eq!(windowed.len(), 10, "the preview window caps the group at ten");
    assert_eq!(windowed.first(), Some(&100));

    let all = rendered_episode_ids(&groups, &expanded, &BTreeSet::from([1]));
    assert_eq!(all.len(), 12, "'Show all' puts every episode in range");
}
```

If `podcasts_groups_tests` has no reusable `group`/`episode` builders, write
the two helpers locally in this test file instead of exporting new test
support — check first with:

```bash
grep -n "fn group\|fn episode" crates/reprise-gnome/src/ui/podcasts/podcasts_groups_tests.rs
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cargo test -p reprise-gnome --lib podcasts::podcasts_rendered_order
```

Expected: the module does not exist.

- [ ] **Step 3: Implement**

Create `podcasts_rendered_order.rs`:

```rust
//! The episode order a range selection is defined over.
//!
//! `podcasts_groups::render` decides what is on screen; this reads the same
//! inputs so the two cannot disagree. Rows the user cannot see — a collapsed
//! group, everything past a group's ten-episode preview window — are not part
//! of the order, so a Shift-click never selects invisible episodes.

use std::collections::BTreeSet;

use reprise_core::podcasts::SourceGroup;

pub(super) fn rendered_episode_ids(
    groups: &[SourceGroup],
    expanded_sources: &BTreeSet<i64>,
    expanded_episode_sources: &BTreeSet<i64>,
) -> Vec<i64> {
    groups
        .iter()
        .filter(|group| expanded_sources.contains(&group.subscription_id))
        .flat_map(|group| {
            let visible = super::podcasts_episode_window::visible_count(
                group.episodes.len(),
                expanded_episode_sources.contains(&group.subscription_id),
            );
            group
                .episodes
                .iter()
                .take(visible)
                .map(|episode| episode.id)
        })
        .collect()
}

#[cfg(test)]
#[path = "podcasts_rendered_order_tests.rs"]
mod tests;
```

Declare it in `mod.rs` next to the other `podcasts_*` modules:

```rust
mod podcasts_rendered_order;
```

- [ ] **Step 4: Run the tests**

```bash
cargo test -p reprise-gnome --lib podcasts::podcasts_rendered_order
```

Expected: three tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/podcasts/podcasts_rendered_order.rs \
        crates/reprise-gnome/src/ui/podcasts/podcasts_rendered_order_tests.rs \
        crates/reprise-gnome/src/ui/podcasts/mod.rs
git commit -m "feat(podcasts): derive the rendered episode order"
```

---

### Task 3: Apply a selection without rebuilding the list

**Files:**
- Modify: `crates/reprise-gnome/src/ui/podcasts/podcasts_view.rs`
- Modify: `crates/reprise-gnome/src/ui/podcasts/podcasts_groups.rs`
- Modify: `crates/reprise-gnome/src/ui/podcasts/podcasts_view_actions.rs:124-138`
- Test: `crates/reprise-gnome/src/ui/podcasts/podcasts_view_tests.rs`

**Interfaces:**
- Consumes: Task 1's `SelectMode`/`PodcastSelection::apply`, Task 2's
  `rendered_episode_ids`.
- Produces on `PodcastsView`:
  - field `selection_widgets: RefCell<BTreeMap<i64, SelectionRowWidgets>>`
  - `fn rendered_order(&self) -> Vec<i64>`
  - `fn apply_selection(&self)`
  - `fn select_row(&self, episode_id: i64, mode: SelectMode)`

**Do not cache the rendered order in a field.** Expanding or collapsing a group
mutates `expanded_sources` from `connect_expanded_notify`
(`podcasts_groups.rs:120-126`) **without** calling `render()`, so an order
recorded at render time is wrong the moment a user opens a group. Compute it on
each selection instead — it is a walk over ids and runs once per click.
- Produces in `podcasts_groups`:
  `pub(super) struct SelectionRowWidgets { pub row: gtk4::Box, pub checkbox: gtk4::CheckButton }`

**Why in place:** every selection change currently calls `render()`, which
rebuilds every row widget and drops keyboard focus. That is survivable when
selection is checkbox-only and mouse-driven; it makes keyboard selection
impossible, because the focused row ceases to exist after the first `Space`.
`download_widgets` already establishes the pattern of holding per-episode
widgets for targeted updates — follow it.

- [ ] **Step 1: Write the failing test**

In `podcasts_view_tests.rs` (a display test — follow the file's existing
`#[gtk4::test]`/display-gate convention exactly; copy the attribute and any
guard from a neighbouring test in the same file):

```rust
#[gtk4::test]
fn src_14_selecting_a_row_keeps_the_focused_widget_alive() {
    let view = view_with_two_episodes();
    let before = view.selection_widgets.borrow().get(&1).map(|widgets| widgets.row.clone());
    view.select_row(1, SelectMode::Toggle);
    let after = view.selection_widgets.borrow().get(&1).map(|widgets| widgets.row.clone());
    assert_eq!(
        before, after,
        "selecting must not rebuild the row — the focused widget has to survive"
    );
    assert!(view.selection.borrow().contains(1));
    assert!(after.unwrap().has_css_class("reprise-podcast-episode-selected"));
}
```

Reuse whatever fixture the file already has for building a view with episodes;
if none exists, add `fn view_with_two_episodes()` next to the existing helpers
following their shape.

- [ ] **Step 2: Run it and watch it fail**

```bash
xvfb-run -a cargo test -p reprise-gnome --lib podcasts::podcasts_view_tests::src_14 -- --test-threads=1
```

Expected: `select_row` / `selection_widgets` do not exist.

- [ ] **Step 3: Register the row widgets while rendering**

In `podcasts_groups.rs`, add the type and collect into it, mirroring
`download_widgets`:

```rust
/// The widgets a selection change has to touch on a row, held per episode so a
/// selection can be applied without rebuilding the list — see
/// `PodcastsView::apply_selection`. `toggled` is the checkbox's own handler id,
/// blocked while the state is pushed back into the checkbox so the push cannot
/// re-enter through `podcasts.set-selected`.
pub(super) struct SelectionRowWidgets {
    pub(super) row: gtk4::Box,
    pub(super) checkbox: gtk4::CheckButton,
    pub(super) toggled: gtk4::glib::SignalHandlerId,
}
```

`episode_row` already builds `root` and `selected`; have the render path insert
`SelectionRowWidgets { row: root.clone(), checkbox: selected.clone() }` into a
`&mut BTreeMap<i64, SelectionRowWidgets>` threaded alongside `download_widgets`,
and apply the class at build time:

```rust
    if context.selected_ids.contains(&row.id) {
        root.add_css_class("reprise-podcast-episode-selected");
    }
```

- [ ] **Step 4: Add the state and the two methods**

In `podcasts_view.rs`, add to the struct and to the constructor:

```rust
    selection_widgets: RefCell<BTreeMap<i64, podcasts_groups::SelectionRowWidgets>>,
```

`render()` fills `selection_widgets` the same way it already fills
`download_widgets` — clear it first, then collect while building rows. Then:

```rust
    /// The episode order a range selection ranges over, read fresh on every
    /// use: a group's expander writes `expanded_sources` directly without a
    /// re-render, so a cached order would go stale the moment a user opens or
    /// closes a group.
    fn rendered_order(&self) -> Vec<i64> {
        podcasts_rendered_order::rendered_episode_ids(
            &self.groups.borrow(),
            &self.expanded_sources.borrow(),
            &self.expanded_episode_sources.borrow(),
        )
    }


```rust
    /// Push the current selection onto the rows that are already on screen.
    ///
    /// Deliberately not a `render()`: rebuilding every row would drop keyboard
    /// focus, and keyboard selection (`Space`) would then be a one-row affair.
    pub(in crate::ui) fn apply_selection(&self) {
        let selection = self.selection.borrow();
        for (episode_id, widgets) in self.selection_widgets.borrow().iter() {
            let selected = selection.contains(*episode_id);
            if selected {
                widgets.row.add_css_class("reprise-podcast-episode-selected");
            } else {
                widgets.row.remove_css_class("reprise-podcast-episode-selected");
            }
            if widgets.checkbox.is_active() != selected {
                // The checkbox's `toggled` handler fires `podcasts.set-selected`,
                // which lands back here. Blocking it keeps this a one-way push.
                widgets.checkbox.block_signal(&widgets.toggled);
                widgets.checkbox.set_active(selected);
                widgets.checkbox.unblock_signal(&widgets.toggled);
            }
        }
        self.selection_controls.update(&selection.selected_ids());
    }

    pub(in crate::ui) fn select_row(&self, episode_id: i64, mode: SelectMode) {
        let order = self.rendered_order();
        self.selection.borrow_mut().apply(&order, episode_id, mode);
        self.apply_selection();
    }
```

For the signal block: `podcasts_selection::episode_checkbox` must return the
`SignalHandlerId` alongside the checkbox (change its return type to
`(gtk4::CheckButton, glib::SignalHandlerId)`) and `SelectionRowWidgets` must
carry it as a third field `toggled: glib::SignalHandlerId`. Do not use a
`Cell<bool>` re-entrancy flag — a blocked signal states the intent at the
place it applies.

- [ ] **Step 5: Route `set-selected` through the same path**

Replace the body of the `set-selected` handler
(`podcasts_view_actions.rs:127-137`) so the checkbox no longer re-renders:

```rust
        set_selected.connect_activate(move |_, target| {
            let Some(view) = weak.upgrade() else { return };
            let Some((episode_id, selected)) = target.and_then(glib::Variant::get::<(i64, bool)>)
            else {
                return;
            };
            view.selection.borrow_mut().set_selected(episode_id, selected);
            view.apply_selection();
        });
```

- [ ] **Step 6: Run the test**

```bash
xvfb-run -a cargo test -p reprise-gnome --lib podcasts::podcasts_view_tests::src_14 -- --test-threads=1
```

Expected: pass. Then check nothing else broke:

```bash
xvfb-run -a cargo test -p reprise-gnome --lib podcasts:: -- --test-threads=1
```

- [ ] **Step 7: Commit**

```bash
git add crates/reprise-gnome/src/ui/podcasts/
git commit -m "feat(podcasts): apply selection without rebuilding rows"
```

---

### Task 4: Click, Ctrl-click, Shift-click, double click

**Files:**
- Modify: `crates/reprise-gnome/src/ui/podcasts/podcasts_row_interaction.rs:81-110`
- Modify: `crates/reprise-gnome/src/ui/podcasts/podcasts_view_actions.rs`
- Modify: `docs/ux-rules.md` (append `SRC-14` to section AF, after `SRC-13`)
- Test: `crates/reprise-gnome/src/ui/podcasts/podcasts_row_interaction_tests.rs`

**Interfaces:**
- Consumes: Task 1's `SelectMode`, Task 3's `PodcastsView::select_row`.
- Produces: action `podcasts.select-row` with variant type `(i64, u8)`;
  `install_row_activation` renamed to `install_row_interaction` with the same
  signature `(&gtk4::Box, i64)`.

- [ ] **Step 1: Add the rule**

Append after `SRC-13` in `docs/ux-rules.md`:

```markdown
- **SRC-14** [active] [gtk] — **Episode rows select like track rows.** A click
  selects the row alone, Ctrl-click toggles it, Shift-click extends the
  selection from the anchor across the rendered order, and playback takes a
  double click or Enter. Space toggles the focused row's selection and
  Shift+Space extends from the anchor. A secondary
  click on a row outside the selection makes that row the selection before the
  menu opens, so a menu never acts on rows the pointer is not on. A range
  covers only rendered rows: a collapsed group, the episodes past a preview
  window and rows hidden by the filter stay out of it. Applying a selection
  never rebuilds the list, so keyboard focus survives it.
```

- [ ] **Step 2: Write the failing test**

In `podcasts_row_interaction_tests.rs`, following the file's existing style
for driving gestures (read the current tests first — they already exercise
activation):

```rust
#[gtk4::test]
fn src_14_a_plain_click_selects_and_does_not_play() {
    let harness = row_harness();
    harness.click_row(ModifierType::empty(), 1);
    assert_eq!(harness.selected_calls(), vec![(1, SelectMode::Only.as_u8())]);
    assert!(harness.played().is_empty(), "a single click must not play");
}

#[gtk4::test]
fn src_14_modifiers_choose_the_selection_mode() {
    let harness = row_harness();
    harness.click_row(ModifierType::CONTROL_MASK, 1);
    harness.click_row(ModifierType::SHIFT_MASK, 1);
    assert_eq!(
        harness.selected_calls(),
        vec![(1, SelectMode::Toggle.as_u8()), (1, SelectMode::Range.as_u8())]
    );
}

#[gtk4::test]
fn src_14_a_double_click_plays() {
    let harness = row_harness();
    harness.click_row(ModifierType::empty(), 2);
    assert_eq!(harness.played(), vec![7]);
}
```

`row_harness` installs the row into a widget carrying a `SimpleActionGroup`
named `podcasts` with recording `select-row` and `play` actions. If the file
has no such harness, build one there — it stays local to this test file.

- [ ] **Step 3: Run it and watch it fail**

```bash
xvfb-run -a cargo test -p reprise-gnome --lib podcasts::podcasts_row_interaction -- --test-threads=1
```

Expected: fails — a single click currently plays.

- [ ] **Step 4: Rewrite the pointer wiring**

Replace `install_row_activation`'s click controller:

```rust
/// `SRC-14`: the row's primary-button behaviour. The first press of a double
/// click still selects — that is what `ColumnView` does, and it keeps the
/// selection honest if the second press never arrives.
pub(super) fn install_row_interaction(root: &gtk4::Box, episode_id: i64) {
    // input-parity: ACC-8 keyboard=episode-row-enter-space
    let click = gtk4::GestureClick::new();
    let clicked_root = root.downgrade();
    click.connect_released(move |gesture, n_press, _, _| {
        if gesture.current_button() != 1 {
            return;
        }
        let Some(root) = clicked_root.upgrade() else {
            return;
        };
        if n_press >= 2 {
            activate_play(&root, episode_id);
            return;
        }
        let state = gesture.current_event_state();
        let mode = if state.contains(gtk4::gdk::ModifierType::CONTROL_MASK) {
            SelectMode::Toggle
        } else if state.contains(gtk4::gdk::ModifierType::SHIFT_MASK) {
            SelectMode::Range
        } else {
            SelectMode::Only
        };
        select(&root, episode_id, mode);
    });
    root.add_controller(click);
    // … the key controller from Task 5 goes here …
}

fn select(root: &gtk4::Box, episode_id: i64, mode: SelectMode) {
    let target = (episode_id, mode.as_u8()).to_variant();
    if let Err(error) = root.activate_action("podcasts.select-row", Some(&target)) {
        tracing::debug!(%error, episode_id, "podcast row selection did not reach the action");
    }
}
```

Update the call site in `podcasts_groups.rs:333` to the new name.

- [ ] **Step 5: Register the action**

In `podcasts_view_actions.rs`, next to `set-selected`:

```rust
        let select_row =
            gio::SimpleAction::new("select-row", Some(&<(i64, u8)>::static_variant_type()));
        let weak = Rc::downgrade(self);
        select_row.connect_activate(move |_, target| {
            let Some(view) = weak.upgrade() else { return };
            let Some((episode_id, mode)) = target.and_then(glib::Variant::get::<(i64, u8)>) else {
                return;
            };
            let Some(mode) = SelectMode::from_u8(mode) else {
                tracing::debug!(mode, "unknown podcast selection mode");
                return;
            };
            view.select_row(episode_id, mode);
        });
        group.add_action(&select_row);
```

- [ ] **Step 6: Run the tests**

```bash
xvfb-run -a cargo test -p reprise-gnome --lib podcasts:: -- --test-threads=1
bash scripts/check-input-parity.sh
bash scripts/check-ux-traceability.sh
```

Expected: green. The traceability check must find `src_14` tests for the new
rule — they exist from Task 1 onward.

- [ ] **Step 7: Commit**

```bash
git add crates/reprise-gnome/src/ui/podcasts/ docs/ux-rules.md
git commit -m "feat(podcasts): select episode rows by click, play on double click"
```

---

### Task 5: Keyboard — Space selects, Enter plays

**Files:**
- Modify: `crates/reprise-gnome/src/ui/podcasts/podcasts_row_interaction.rs`
- Test: `crates/reprise-gnome/src/ui/podcasts/podcasts_row_interaction_tests.rs`

**Interfaces:**
- Consumes: Task 4's `select()` helper and `podcasts.select-row`.
- Produces: nothing new; this is the ACC-8 partner the gesture marker names.

- [ ] **Step 1: Write the failing tests**

```rust
#[gtk4::test]
fn src_14_space_toggles_the_focused_row_and_enter_plays() {
    let harness = row_harness();
    harness.press_key(gtk4::gdk::Key::space, ModifierType::empty());
    assert_eq!(harness.selected_calls(), vec![(7, SelectMode::Toggle.as_u8())]);
    assert!(harness.played().is_empty(), "Space must not play any more");

    harness.press_key(gtk4::gdk::Key::Return, ModifierType::empty());
    assert_eq!(harness.played(), vec![7]);
}

#[gtk4::test]
fn src_14_shift_space_extends_the_selection() {
    let harness = row_harness();
    harness.press_key(gtk4::gdk::Key::space, ModifierType::SHIFT_MASK);
    assert_eq!(harness.selected_calls(), vec![(7, SelectMode::Range.as_u8())]);
}
```

Shift+Space is the keyboard partner for Shift-click and needs no focus
bookkeeping: the anchor is already in the selection state and focus moves with
the usual arrow/Tab navigation. Do **not** add Shift+Arrow — it would have to
move focus across widgets this view rebuilds, and Shift+Space covers the
range case without that machinery.

- [ ] **Step 2: Run them and watch them fail**

```bash
xvfb-run -a cargo test -p reprise-gnome --lib podcasts::podcasts_row_interaction -- --test-threads=1
```

Expected: fails — `Space` currently plays.

- [ ] **Step 3: Rewrite the key controller**

```rust
    let keys = gtk4::EventControllerKey::new();
    let keyed_root = root.downgrade();
    keys.connect_key_pressed(move |_, key, _, state| {
        let Some(root) = keyed_root.upgrade() else {
            return gtk4::glib::Propagation::Proceed;
        };
        match key {
            gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter => {
                activate_play(&root, episode_id);
                gtk4::glib::Propagation::Stop
            }
            // `SRC-14`: Space is the keyboard partner for Ctrl-click, and
            // Shift+Space for Shift-click. Playing moved to Enter, which is
            // what the track list does.
            gtk4::gdk::Key::space | gtk4::gdk::Key::KP_Space => {
                let mode = if state.contains(gtk4::gdk::ModifierType::SHIFT_MASK) {
                    SelectMode::Range
                } else {
                    SelectMode::Toggle
                };
                select(&root, episode_id, mode);
                gtk4::glib::Propagation::Stop
            }
            _ => gtk4::glib::Propagation::Proceed,
        }
    });
    root.add_controller(keys);
```

Leave the row's accessible label (`PLAY_OR_PAUSE`, `podcasts_groups.rs:310-312`)
unchanged: the row still plays on Enter, so the label stays true, and changing
it would add a user-visible string this plan has ruled out.

- [ ] **Step 4: Run the tests**

```bash
xvfb-run -a cargo test -p reprise-gnome --lib podcasts::podcasts_row_interaction -- --test-threads=1
bash scripts/check-input-parity.sh
```

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/podcasts/podcasts_row_interaction.rs \
        crates/reprise-gnome/src/ui/podcasts/podcasts_row_interaction_tests.rs
git commit -m "feat(podcasts): Space selects the focused episode row, Enter plays"
```

---

### Task 6: A secondary click never acts on rows the pointer is not on

**Files:**
- Modify: `crates/reprise-gnome/src/ui/podcasts/podcasts_groups.rs:380-395`
  (the menu-button/gesture site)
- Test: `crates/reprise-gnome/src/ui/podcasts/podcasts_view_tests.rs`

**Interfaces:**
- Consumes: `PodcastsView::select_row`, `SelectMode::Only`.
- Produces: nothing new.

**The defect, and what is already done:** `build_for_selection` targeted the
whole selection whenever more than one episode was selected — including a menu
opened on a row outside it. **The targeting half shipped separately in PR #203**
(`fix/podcast-menu-target`): the menu now widens only when the row is a member
of the selection, so no action can reach rows the pointer is not on. **Rebase
this branch onto `dev` once #203 lands** and do not re-fix it.

What remains here is the visible half: with three rows selected and the menu
opened on a fourth, the menu is correctly a single-row menu, but three rows
still *look* selected. Making the pointed-at row the selection resolves that
mismatch — and once it does, `build_for_selection`'s membership test is a
belt-and-braces guard rather than the only thing standing between the user and
a wrong "Remove". Keep both.

- [ ] **Step 1: Write the failing test**

```rust
#[gtk4::test]
fn src_14_a_menu_on_an_unselected_row_takes_over_the_selection() {
    let view = view_with_three_episodes();
    view.select_row(1, SelectMode::Only);
    view.select_row(2, SelectMode::Toggle);

    view.prepare_context_menu(3);
    assert_eq!(view.selection.borrow().selected_ids(), vec![3]);

    view.prepare_context_menu(3);
    assert_eq!(
        view.selection.borrow().selected_ids(),
        vec![3],
        "a menu inside the selection leaves it alone"
    );
}
```

- [ ] **Step 2: Run it and watch it fail**

```bash
xvfb-run -a cargo test -p reprise-gnome --lib podcasts::podcasts_view_tests::src_14_a_menu -- --test-threads=1
```

- [ ] **Step 3: Implement**

On `PodcastsView`:

```rust
    /// `SRC-14`: right-clicking a row outside the selection makes that row the
    /// selection, so the menu that opens acts on what the pointer is on. A
    /// right-click inside the selection keeps it — that is how a batch action
    /// is reached.
    pub(in crate::ui) fn prepare_context_menu(&self, episode_id: i64) {
        if !self.selection.borrow().contains(episode_id) {
            self.select_row(episode_id, SelectMode::Only);
        }
    }
```

Call it from the row's menu trigger before the menu is built. The row's menu
button already knows its episode id; route it through a new
`podcasts.prepare-menu` target action (same shape as the existing
`add_target_action` helpers) so the widget stays free of view access.

- [ ] **Step 4: Run the tests**

```bash
xvfb-run -a cargo test -p reprise-gnome --lib podcasts:: -- --test-threads=1
```

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/podcasts/
git commit -m "fix(podcasts): a context menu acts on the row it was opened on"
```

---

### Task 7: The selected row looks selected

**Files:**
- Modify: `crates/reprise-gnome/src/ui/podcasts/css.rs`
- Test: `crates/reprise-gnome/src/ui/podcasts/css.rs`'s own test module (it
  already asserts on rule presence — see `css.rs:104`)

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn src_14_a_selected_row_has_its_own_style() {
        let css = stylesheet();
        assert!(css.contains(".reprise-podcast-episode-row.reprise-podcast-episode-selected"));
    }
```

Match the existing test's accessor for the stylesheet string.

- [ ] **Step 2: Run it and watch it fail**

```bash
cargo test -p reprise-gnome --lib podcasts::css
```

- [ ] **Step 3: Add the style**

Next to the `.reprise-podcast-playing` block (`css.rs:73`):

```css
.reprise-podcast-episode-row.reprise-podcast-episode-selected {
  background-color: alpha(@accent_bg_color, 0.28);
  border-radius: 8px;
}
```

`alpha(@accent_bg_color, …)` keeps it legible in both themes and distinct from
`reprise-hover`'s tint. A row can be selected, hovered and loaded at once —
check all three together against `docs/ux-rules.md` section U (contrast) before
settling on the alpha value.

- [ ] **Step 4: Run the test**

```bash
cargo test -p reprise-gnome --lib podcasts::css
```

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/podcasts/css.rs
git commit -m "feat(podcasts): style the selected episode row"
```

---

### Task 8: The channel detail view uses the same mechanics

**Files:**
- Modify: `crates/reprise-gnome/src/ui/podcasts/youtube_channel_detail.rs`
- Test: `crates/reprise-gnome/src/ui/podcasts/podcasts_view_tests.rs` or the
  detail view's own test module — whichever already covers
  `YoutubeChannelState`

**Interfaces:**
- Consumes: Task 1's `apply_select`, Task 2's `rendered_episode_ids` (single
  group), Task 4's `podcasts.select-row`.
- Produces: `YoutubeChannelState::apply_select(&mut self, subscription_id: i64, order: &[i64], episode_id: i64, mode: SelectMode)`

The detail view keeps its per-channel `BTreeMap<i64, BTreeSet<i64>>` and gains
`anchors: BTreeMap<i64, i64>`. It must not grow a second implementation of the
range walk — it calls `apply_select` with its own state.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn src_14_the_detail_view_ranges_over_its_own_channel() {
    let mut state = YoutubeChannelState::default();
    state.apply_select(5, &[1, 2, 3], 1, SelectMode::Only);
    state.apply_select(5, &[1, 2, 3], 3, SelectMode::Range);
    assert_eq!(state.selected_ids(5), vec![1, 2, 3]);
    assert!(
        state.selected_ids(6).is_empty(),
        "one channel's range never reaches another channel"
    );
}
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cargo test -p reprise-gnome --lib youtube_channel_detail
```

- [ ] **Step 3: Implement**

```rust
    /// `SRC-14`: the detail view's selection is per channel, so it owns the
    /// state and the anchor — but the mechanics are
    /// `podcasts_selection::apply_select`, shared with the grouped view, so
    /// the two surfaces cannot drift apart.
    pub(super) fn apply_select(
        &mut self,
        subscription_id: i64,
        order: &[i64],
        episode_id: i64,
        mode: SelectMode,
    ) {
        let selected = self.selected.entry(subscription_id).or_default();
        let mut anchor = self.anchors.get(&subscription_id).copied();
        podcasts_selection::apply_select(selected, &mut anchor, order, episode_id, mode);
        match anchor {
            Some(anchor) => {
                self.anchors.insert(subscription_id, anchor);
            }
            None => {
                self.anchors.remove(&subscription_id);
            }
        }
        if selected.is_empty() {
            self.selected.remove(&subscription_id);
            self.anchors.remove(&subscription_id);
        }
    }
```

Wire the detail view's rows through `install_row_interaction` exactly as the
grouped view does, and give its rows the same
`reprise-podcast-episode-selected` class. Its `select-row` action handler needs
the channel's rendered order — the detail view renders one channel's episode
list, so its order is that list's ids in render order.

- [ ] **Step 4: Run the tests**

```bash
xvfb-run -a cargo test -p reprise-gnome --lib podcasts:: -- --test-threads=1
```

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/podcasts/youtube_channel_detail.rs
git commit -m "feat(podcasts): the channel detail view selects rows like the library"
```

---

### Task 9: Gates

**Files:** none new.

- [ ] **Step 1: Formatting and lints**

```bash
cargo fmt --check
cargo clippy --locked --all-targets --workspace -- -D warnings
```

- [ ] **Step 2: The project's own checks**

```bash
bash scripts/check-architecture.sh
bash scripts/check-accessibility-semantics.sh
bash scripts/check-input-parity.sh
bash scripts/check-frontend-thinness.sh
bash scripts/check-ux-traceability.sh
bash scripts/check-motion-tokens.sh
```

`check-frontend-thinness.sh` is the one to watch: it rejects domain logic in
the frontend. Range selection over rendered widget order is view mechanics, not
podcast domain logic, but if the check disagrees, do not weaken the check —
report it and stop.

- [ ] **Step 3: Tests**

```bash
cargo test --locked --workspace
```

Per this project's history the gnome display tests are flaky **as a batch** —
re-run any failure as a single test before believing it:

```bash
xvfb-run -a cargo test -p reprise-gnome --lib <one::test::name> -- --test-threads=1
```

- [ ] **Step 4: Audit**

```bash
cargo audit
```

Only `RUSTSEC-2024-0436` is an accepted advisory.

- [ ] **Step 5: Verify against the running app**

Build and drive the real app — several row-interaction bugs in this project
only reproduce in an installed build, never under `cargo test`. Confirm by
hand: click selects without playing; double click plays; Ctrl-click adds;
Shift-click spans a group boundary; Shift-click does not reach into a collapsed
group; the count in the selection bar matches; right-click outside the
selection reduces it to that row; Space then Space then Space selects three
rows in a row without the focus jumping.

- [ ] **Step 6: Open the PR**

```bash
git push -u origin feature/podcast-row-click-selection
gh pr create --base dev --title "feat(podcasts): select episode rows by click" \
  --body "Episode rows adopt the track list's selection model: click selects,
Ctrl-click toggles, Shift-click extends across the rendered order, double click
or Enter plays, Space toggles the focused row. Adds SRC-14. Also fixes a
context menu that acted on the selection when opened on a row outside it.

Test plan: the SRC-14 unit tests, the display tests listed in the plan, plus the
by-hand pass in Task 9 Step 5 against an installed build."
```

## Notes for whoever implements this

- `podcasts_dnd.rs` already puts a `DragSource` on every row. A `GestureClick`
  and a `DragSource` coexist on the same widget today (that is how click-to-play
  works next to drag-to-queue), so no gesture-group juggling is needed — but if
  a click starts registering as a drag, that is where to look.
- `episode_checkbox`'s return type changes in Task 3 (it must hand back its
  signal handler id). Every caller has to be updated in the same commit.
- `SRC-12` stays as it is. It describes bulk actions; `SRC-14` describes how a
  selection is made. Neither replaces the other.
