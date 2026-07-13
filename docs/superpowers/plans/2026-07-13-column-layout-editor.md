# Column Layout Editor + Rhythmbox Discovery — Implementation Plan

## Global Constraints

- TDD: RED before implementation, then GREEN.
- English code/comments/UI/commits; German internal design documents.
- Never access real music, Rhythmbox data or the real Reprise database in QA.
- `reprise-core` remains GTK/GStreamer/zbus-free; every touched file stays under 800 lines.
- Every commit passes fmt, strict clippy, workspace tests and audit; frontend completion
  additionally passes Rustdoc, core purity and fully isolated smoke checks.

## Task 1 — Pure layout editing operations and discovery contract

Files: `column_layout.rs`.

Interfaces:

```rust
pub fn set_column_visible(layout: &ColumnLayout, id: ColumnId, visible: bool) -> ColumnLayout;
pub fn move_column(layout: &ColumnLayout, id: ColumnId, target: ColumnId) -> ColumnLayout;
pub fn rhythmbox_layout_available() -> bool;
pub fn should_offer_rhythmbox_import(available: bool) -> bool;
```

TDD steps:

1. Add failing tests proving Cover/Title cannot be hidden or moved, optional columns
   toggle without changing order, movable columns reorder before the target, self/unknown
   boundary moves are no-ops, and discovery is offered exactly when available.
2. Implement immutable normalized transforms and the schema/key availability wrapper.
3. Run targeted tests and all gates.
4. Commit `feat: add editable column layout operations`.

Expected total: 486 passed, 4 ignored.

## Task 2 — Native persistent editor

Files: new `column_layout_editor.rs`; `mod.rs`, `track_list.rs`, `primary_menu.rs`,
`strings.rs`, gettext catalogs.

Interfaces:

```rust
pub fn present(window: &adw::ApplicationWindow, track_list: &Rc<TrackList>);
pub(super) fn current_column_layout(&self) -> ColumnLayout;
```

TDD steps:

1. Add failing pure editor-row tests for fixed/movable row capabilities and drag payload
   parsing, plus a display-only test proving a row owns DragSource/DropTarget controllers.
2. Build an `AdwDialog` with header, PreferencesGroup/ListBox rows, switches, full-row
   drag/drop, Up/Down buttons, and Reset. Apply/persist after every successful change;
   restore the previous working state and toast on failure.
3. Add `win.edit-column-layout` beside the existing import action and translations.
4. Run the targeted test (including isolated Xvfb) and all gates.
5. Commit `feat: add native column layout editor`.

Expected total: at least 489 passed, 5 ignored.

## Task 3 — Conditional Rhythmbox found prompt and close-out

Files: `first_run.rs`, `strings.rs`, gettext catalogs, `MANUAL-QA.md`, `RELEASING.md`.

TDD steps:

1. Add a failing decision test: missing Rhythmbox hides the option; detected Rhythmbox
   shows a default-off import offer.
2. Render `Rhythmbox found` only on detection, while retaining explicit setup-smoke
   injection and the manual menu import.
3. Add an isolated scratch-schema/display smoke that proves the found copy and no
   automatic import; then prove explicit opt-in imports the fixture layout.
4. Run the full gate battery, release checker, core purity and adversarial stage review.
5. Update ledger/STATUS/manual QA and commit `feat: offer detected Rhythmbox layout import`.

Expected total: at least 491 passed, 5 ignored.

