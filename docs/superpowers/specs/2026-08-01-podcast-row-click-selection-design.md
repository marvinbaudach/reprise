# Episode rows select on click

Podcast and YouTube episode rows adopt the music list's selection model:
clicking a row selects it, Ctrl-click toggles, Shift-click extends a range, and
a double click plays. Today a single click plays immediately and the only way to
select is the per-row checkbox.

## Why

Bulk selection landed with `SRC-12` (`podcasts_selection.rs`,
`podcasts_batch_actions.rs`): checkboxes, a selected-count bar, and a
selection-aware context menu carrying download, mark played/unplayed, delete
downloads, remove, Play Next and Add to Queue. The actions are there; reaching
them is the problem. Selecting seven episodes costs seven precise clicks on
20×20 checkboxes, while the music list — same product, same user — selects a
range with one Shift-click.

## The surface as it is

- Rows are hand-built `gtk4::Box` widgets inside `gtk4::Expander` groups
  (`podcasts_groups.rs:289-404`), not a `ColumnView`. There is no GTK selection
  model to inherit Ctrl/Shift behaviour from; the mechanics have to be written.
- `install_row_activation` (`podcasts_row_interaction.rs:81-110`) wires a
  primary-button `GestureClick` that plays on release, plus an
  `EventControllerKey` where **Enter and Space both play**.
- `PodcastSelection` (`podcasts_selection.rs`) is a `BTreeSet<i64>` with
  `set_selected`/`contains`/`selected_ids`/`remove_all`/`retain_available`. It
  has no anchor and no notion of order.
- The channel detail view keeps its own per-channel selection —
  `BTreeMap<i64, BTreeSet<i64>>` (`youtube_channel_detail.rs:42`) — with the
  same checkbox-only interaction.
- Rows already carry a `DragSource` (`podcasts_dnd.rs`) whose `drag_items`
  resolves the target correctly: the whole selection when the dragged row is
  part of it, otherwise just that row.
- The view rebuilds every row on each `render()`, so nothing may live in the
  widgets.

## Design

### 1. Order-aware selection core

`PodcastSelection` gains an `anchor: Option<i64>` and three operations:

| Operation | Behaviour |
|---|---|
| `select_only(id)` | Selection becomes exactly `{id}`; anchor becomes `id`. |
| `toggle(id)` | Adds or removes `id`; anchor becomes `id`. |
| `select_range(order, id)` | Selection becomes the inclusive span between anchor and `id` in `order`; the anchor does not move. Without an anchor, or with an anchor absent from `order`, it degrades to `select_only(id)`. |

`order` is the rendered episode ids in visible order. Range selection is
therefore defined only over what the user can actually see — a collapsed group's
hidden episodes and rows behind a "Show all 27 episodes" window are not swept up
by a Shift-click, and neither are episodes hidden by the active filter.

The range walk is pure `Vec`/`BTreeSet` work with no GTK types, so it is unit
tested without a display.

The channel detail view stores its selection per channel and cannot share
`PodcastSelection` wholesale. The three operations are therefore free functions
over `(&mut BTreeSet<i64>, &mut Option<i64>, &[i64], i64)`, and both
`PodcastSelection` and `YoutubeChannelState` call them. One implementation, two
owners of state.

### 2. Rendered order in view state

`podcasts_view` already computes the rendered groups. Rendering records the
visible episode ids into an `Rc<RefCell<Vec<i64>>>` held by the view, in render
order across all groups, replacing the previous contents. Shift-click reads it.
The existing `retain_available` call keeps the selection itself pruned; the
order vector is rebuilt wholesale and needs no pruning.

### 3. Pointer semantics

`install_row_activation` becomes `install_row_interaction` and reads both
`n_press` and the modifier state of the primary-button gesture:

| Input | Result |
|---|---|
| Click | `select_only` |
| Ctrl-click | `toggle` |
| Shift-click | `select_range` |
| Double click | Play (the first press has already selected the row) |
| Secondary click on a row outside the selection | `select_only`, then the menu |
| Secondary click on a row inside the selection | Selection untouched, menu acts on all of it |

Actions carry `(i64 episode_id, u8 mode)` through one new
`podcasts.select-row` action rather than three near-identical actions. The
existing `podcasts.set-selected` stays for the checkboxes, which remain visible
and keep working as the touch and screen-reader route.

Play moves from single to double click. That is the point of the change: a
single click that both selects and plays cannot express "select these four".

### 4. Keyboard

Enter and KP_Enter keep playing the focused row. **Space stops playing and
toggles the focused row's selection** — the keyboard partner for Ctrl-click, and
what `ColumnView` does. Shift+Up/Down extends the selection along the same
rendered order, moving focus with it.

`scripts/check-input-parity.sh` requires every new gesture to name a tested
keyboard partner in an `// input-parity: ACC-8 keyboard=<partner>` marker, and
the partner needs a real test.

### 5. Correcting the context menu target

`podcasts_context_menu.rs:135-146` targets the whole selection whenever more
than one episode is selected — including a right-click on a row that is not part
of it. With three rows selected, right-clicking a fourth and choosing "Remove"
removes the three, not the one under the pointer. `podcasts_dnd::drag_items`
already gets this right; the menu adopts the same rule via §3's secondary-click
behaviour, which makes the pointed-at row part of the selection before the menu
is built. The `<= 1` fallback inside the menu builder stays as the last line of
defence.

### 6. Visible selection

Selected rows carry `reprise-podcast-episode-selected`, defined in
`podcasts/css.rs` next to `reprise-podcast-playing`. It must read as distinct
from both the hover tint (`reprise-hover`) and the loaded-row treatment, since a
row can be selected, hovered and loaded at once. Selection uses the platform
accent fill; the loaded row keeps its existing marker and dimmed thumbnail.

## Rules and tests

New rule, next free id (`SRC-13` is the highest in use):

> **SRC-14** [active] [gtk] — Episode rows select like track rows: a click
> selects the row alone, Ctrl-click toggles it, Shift-click extends the
> selection across the rendered order, and playback needs a double click or
> Enter. Space toggles the focused row. A secondary click on a row outside the
> selection makes that row the selection before the menu opens, so a menu never
> acts on rows the pointer is not on. Range selection covers only rows that are
> actually rendered.

`scripts/check-ux-traceability.sh` fails an `[active]` rule with no test naming
its id, and the rule and its implementation must land in one commit.

Tests:

- Range selection over a rendered order, including a reversed drag direction
  (anchor after the target) and an anchor that is no longer rendered.
- Range selection ignores episodes that are not in the rendered order.
- `select_only` and `toggle` maintain the anchor.
- Selection survives a `render()` (the widgets are rebuilt every time).
- A double click plays; a single click does not.
- Space toggles selection and does not play; Enter plays.
- Right-clicking a row outside a multi-selection reduces the selection to that
  row; right-clicking inside it leaves the selection alone.
- Both surfaces exercise the shared range function, so the detail view cannot
  drift.
- A display test carrying `SRC-14`.

## Not in this change

- No rubberband drag across rows and no `ColumnView` migration.
- No change to what the batch actions do, to the queue, or to the drag source's
  payload.
- No touching the dead `podcasts_model.rs` / `podcasts_columns.rs`.

## Files

- `crates/reprise-gnome/src/ui/podcasts/podcasts_selection.rs` — anchor, the
  three shared operations
- `crates/reprise-gnome/src/ui/podcasts/podcasts_row_interaction.rs` — pointer
  and keyboard wiring
- `crates/reprise-gnome/src/ui/podcasts/podcasts_groups.rs` — selected CSS
  class, rendered-order recording
- `crates/reprise-gnome/src/ui/podcasts/podcasts_view.rs`,
  `podcasts_view_actions.rs` — rendered-order state, `podcasts.select-row`
- `crates/reprise-gnome/src/ui/podcasts/podcasts_context_menu.rs` — secondary
  click target
- `crates/reprise-gnome/src/ui/podcasts/youtube_channel_detail.rs` — same
  mechanics on the detail surface
- `crates/reprise-gnome/src/ui/podcasts/css.rs` — the selected-row style
- `docs/ux-rules.md` — `SRC-14`, appended
