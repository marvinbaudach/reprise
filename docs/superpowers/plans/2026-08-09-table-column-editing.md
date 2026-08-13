---
slug: table-column-editing
worktree: ~/Projects/reprise-table-column-editing
branch: feature/table-column-editing
phase: refactored
codex_session:
created: 2026-08-09
---
# App-Wide Table Column Editing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give every table with a column header the music library's column editing — right-click the header to toggle, reorder and reset, with order, visibility and widths stored per table.

**Architecture:** `reprise-view` gains a `ColumnKey` trait and a `Layout<K>` generic over it, so each table declares its own column enum while layout normalization, serialization and width persistence exist once. `reprise-gnome` grows a `table_columns/` module that works on an erased `{id, label}` descriptor list behind an `EditorModel` trait, so no GTK widget code is generic. Each table contributes a thin adapter binding its enum to its labels, widths and settings key.

**Tech Stack:** Rust, gtk4-rs, libadwaita, SQLite-backed settings via `reprise-core::library::settings`.

Source spec: `docs/superpowers/specs/2026-08-09-table-columns-and-system-dates-design.md` (Part A).

## Global Constraints

- Anchor against `origin/dev`, not the local checkout — it runs hours behind. Branch from `origin/dev`.
- `reprise-view` must never link `gtk4`, `libadwaita`, `glib`, `gstreamer` or `zbus`.
- Every Rust file stays below 800 lines; `window.rs`, `track_list.rs` and `sidebar.rs` below 600. The three files this plan generalizes are already at 653, 629 and 648 lines — code moves out of them, never in.
- `scripts/check-frontend-thinness.sh` treats `view_floor` (currently 1782) as ceiling **and** floor. This plan grows `reprise-view`, so the floor must be raised in the same commit that grows it — Task 14 does this once, at the end, with the measured number.
- The serialized formats do not change: layout is `order;visible` with comma-separated ids, widths are comma-separated `id:width` pairs sorted by id. `ui.column_layout` and `ui.column_widths` keep meaning the music table, so no stored layout needs migrating.
- STYLE-9 stands: every column carries an explicit width and exactly one visible column expands. Never leave a column at `fixed-width = -1`.
- Visibility is `set_visible`, never `remove_column`. Removing a column resets horizontal scroll and drops the sort.
- Run display tests singly: `xvfb-run -a cargo test -p reprise-gnome <name> -- --ignored --exact --test-threads=1`. The display suite is herd-flaky in a batch, and three tests are already red on `origin/dev` itself — check the base before blaming this branch.
- Check the number before `passed` in every result line. A filter matching nothing still prints `ok`.

---

### Task 1: The column key trait

**Files:**
- Create: `crates/reprise-view/src/columns/key.rs`
- Modify: `crates/reprise-view/src/columns.rs` → becomes `crates/reprise-view/src/columns/mod.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `pub enum Pin { Leading, Trailing }` and `pub trait ColumnKey: Copy + Eq + Hash + Sized + 'static` with `as_str(self) -> &'static str`, `parse(&str) -> Option<Self>`, `all() -> &'static [Self]`, `default_visible() -> &'static [Self]`, `pin(self) -> Option<Pin>`.

- [ ] **Step 1: Move the existing file into a directory**

```bash
mkdir -p crates/reprise-view/src/columns
git mv crates/reprise-view/src/columns.rs crates/reprise-view/src/columns/track.rs
```

Create `crates/reprise-view/src/columns/mod.rs`:

```rust
//! Toolkit-independent column identity, layout and persistence.
//!
//! One table's columns are one enum implementing [`ColumnKey`]; everything
//! that operates on a layout — ordering, normalization, the persisted string —
//! is written once, generic over that trait. The GTK, Tauri and Compose
//! surfaces read the same stored value, and two implementations of one format
//! drift.

pub mod key;
pub mod layout;
pub mod track;

pub use key::{ColumnKey, Pin};
pub use layout::Layout;
pub use track::ColumnId;
```

Verify nothing broke: `reprise_view::columns::ColumnId` must still resolve.

- [ ] **Step 2: Write the failing test**

Create `crates/reprise-view/src/columns/key.rs` with only its tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum Probe {
        Lead,
        Free,
        Trail,
    }

    impl ColumnKey for Probe {
        fn as_str(self) -> &'static str {
            match self {
                Self::Lead => "lead",
                Self::Free => "free",
                Self::Trail => "trail",
            }
        }
        fn parse(value: &str) -> Option<Self> {
            match value {
                "lead" => Some(Self::Lead),
                "free" => Some(Self::Free),
                "trail" => Some(Self::Trail),
                _ => None,
            }
        }
        fn all() -> &'static [Self] {
            &[Self::Lead, Self::Free, Self::Trail]
        }
        fn default_visible() -> &'static [Self] {
            &[Self::Free]
        }
        fn pin(self) -> Option<Pin> {
            match self {
                Self::Lead => Some(Pin::Leading),
                Self::Trail => Some(Pin::Trailing),
                Self::Free => None,
            }
        }
    }

    #[test]
    fn every_key_round_trips_through_its_persisted_name() {
        for key in Probe::all() {
            assert_eq!(Probe::parse(key.as_str()), Some(*key));
        }
        assert_eq!(Probe::parse("unknown"), None);
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p reprise-view every_key_round_trips`
Expected: FAIL — `cannot find trait ColumnKey`.

- [ ] **Step 4: Implement**

Above the test module in `key.rs`:

```rust
//! What a table's column identity has to provide.

use std::hash::Hash;

/// A column the user may not move or hide, and where it sits.
///
/// Two kinds exist in Reprise: a leading artwork column that opens the row,
/// and a trailing action column that is the only access to an action on
/// surfaces without a row context menu. Both stay visible, keep their
/// position, and never appear in the column editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pin {
    Leading,
    Trailing,
}

/// The identity of one table's columns.
///
/// `as_str` is a *persisted* name: changing one silently discards every
/// stored layout that mentions it.
pub trait ColumnKey: Copy + Eq + Hash + Sized + 'static {
    fn as_str(self) -> &'static str;
    fn parse(value: &str) -> Option<Self>;
    /// Every column of this table, in the built-in default order.
    fn all() -> &'static [Self];
    /// The columns a fresh layout shows. Pins are visible regardless of
    /// whether they are listed here.
    fn default_visible() -> &'static [Self];
    fn pin(self) -> Option<Pin>;
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p reprise-view every_key_round_trips`
Expected: PASS, `1 passed`.

- [ ] **Step 6: Commit**

```bash
git add crates/reprise-view/src/columns crates/reprise-view/src/lib.rs
git commit -m "feat: a table-independent column key trait"
```

---

### Task 2: The generic layout

**Files:**
- Create: `crates/reprise-view/src/columns/layout.rs`
- Modify: `crates/reprise-view/src/columns/mod.rs`

**Interfaces:**
- Consumes: `ColumnKey`, `Pin` (Task 1).
- Produces: `pub struct Layout<K> { pub order: Vec<K>, pub visible: HashSet<K> }` with `impl Default`, plus free functions `normalize<K>(Vec<K>, HashSet<K>) -> Layout<K>`, `serialize<K>(&Layout<K>) -> String`, `parse<K>(&str) -> Option<Layout<K>>`, `set_visible<K>(&Layout<K>, K, bool) -> Layout<K>`, `move_before<K>(&Layout<K>, K, K) -> Layout<K>`, `move_after<K>(&Layout<K>, K, K) -> Layout<K>`.

- [ ] **Step 1: Write the failing tests**

In `crates/reprise-view/src/columns/layout.rs`, reusing the `Probe` enum from
Task 1 (move it into a `#[cfg(test)] pub(crate) mod probe;` sibling so both
files share one definition rather than two that can drift):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::columns::probe::Probe;

    #[test]
    fn normalize_places_pins_around_the_free_band() {
        let layout = normalize(
            vec![Probe::Trail, Probe::Free, Probe::Lead],
            [Probe::Free].into_iter().collect(),
        );
        assert_eq!(layout.order, vec![Probe::Lead, Probe::Free, Probe::Trail]);
    }

    #[test]
    fn normalize_forces_every_pin_visible() {
        let layout = normalize(Probe::all().to_vec(), std::collections::HashSet::new());
        assert!(layout.visible.contains(&Probe::Lead));
        assert!(layout.visible.contains(&Probe::Trail));
        assert!(!layout.visible.contains(&Probe::Free));
    }

    #[test]
    fn normalize_appends_a_column_the_stored_value_never_mentioned() {
        // A column added in a later release must not become unreachable.
        let layout = normalize(vec![Probe::Lead], [Probe::Lead].into_iter().collect());
        assert!(layout.order.contains(&Probe::Free));
    }

    #[test]
    fn a_layout_round_trips_through_its_persisted_string() {
        let layout = set_visible(&Layout::<Probe>::default(), Probe::Free, false);
        let serialized = serialize(&layout);
        assert_eq!(parse::<Probe>(&serialized), Some(layout));
    }

    #[test]
    fn parse_skips_an_unknown_id_without_losing_the_rest() {
        let layout = parse::<Probe>("lead,gone,free,trail;free").expect("parses");
        assert_eq!(layout.order, vec![Probe::Lead, Probe::Free, Probe::Trail]);
        assert!(layout.visible.contains(&Probe::Free));
    }

    #[test]
    fn a_pin_can_be_neither_hidden_nor_moved() {
        let hidden = set_visible(&Layout::<Probe>::default(), Probe::Lead, false);
        assert!(hidden.visible.contains(&Probe::Lead));
        let moved = move_after(&Layout::<Probe>::default(), Probe::Lead, Probe::Trail);
        assert_eq!(moved.order, Layout::<Probe>::default().order);
    }

    #[test]
    fn moving_a_free_column_reorders_only_the_free_band() {
        let layout = Layout::<Probe>::default();
        let moved = move_before(&layout, Probe::Free, Probe::Free);
        assert_eq!(moved.order, layout.order, "moving onto itself is a no-op");
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p reprise-view columns::layout`
Expected: FAIL — module does not exist.

- [ ] **Step 3: Implement**

```rust
//! Column order and visibility, and the string they are stored as.
//!
//! The persisted shape is `order;visible`: two comma-separated id lists, the
//! second a subset of the first. Unchanged from the music library's original
//! format, so no stored layout needs migrating.

use std::collections::HashSet;

use super::key::{ColumnKey, Pin};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout<K: ColumnKey> {
    pub order: Vec<K>,
    pub visible: HashSet<K>,
}

impl<K: ColumnKey> Default for Layout<K> {
    fn default() -> Self {
        normalize(
            K::all().to_vec(),
            K::default_visible().iter().copied().collect(),
        )
    }
}

/// Brings any order and visibility set into the one shape the table renders:
/// leading pins, then the free columns in the user's order, then trailing
/// pins, with every pin visible and every unmentioned column appended.
pub fn normalize<K: ColumnKey>(order: Vec<K>, mut visible: HashSet<K>) -> Layout<K> {
    for key in K::all() {
        if key.pin().is_some() {
            visible.insert(*key);
        }
    }
    let mut normalized: Vec<K> = K::all()
        .iter()
        .copied()
        .filter(|key| key.pin() == Some(Pin::Leading))
        .collect();
    for key in order.into_iter().chain(K::all().iter().copied()) {
        if key.pin().is_none() && !normalized.contains(&key) {
            normalized.push(key);
        }
    }
    normalized.extend(
        K::all()
            .iter()
            .copied()
            .filter(|key| key.pin() == Some(Pin::Trailing)),
    );
    Layout {
        order: normalized,
        visible,
    }
}

pub fn serialize<K: ColumnKey>(layout: &Layout<K>) -> String {
    let layout = normalize(layout.order.clone(), layout.visible.clone());
    let order = join(&layout.order);
    let visible: Vec<K> = layout
        .order
        .iter()
        .copied()
        .filter(|key| layout.visible.contains(key))
        .collect();
    format!("{order};{}", join(&visible))
}

pub fn parse<K: ColumnKey>(value: &str) -> Option<Layout<K>> {
    let (order, visible) = value.split_once(';')?;
    Some(normalize(
        parse_ids::<K>(order),
        parse_ids::<K>(visible).into_iter().collect(),
    ))
}

pub fn set_visible<K: ColumnKey>(layout: &Layout<K>, key: K, visible: bool) -> Layout<K> {
    let mut next = layout.clone();
    if visible || key.pin().is_some() {
        next.visible.insert(key);
    } else {
        next.visible.remove(&key);
    }
    normalize(next.order, next.visible)
}

pub fn move_before<K: ColumnKey>(layout: &Layout<K>, key: K, target: K) -> Layout<K> {
    move_relative(layout, key, target, false)
}

pub fn move_after<K: ColumnKey>(layout: &Layout<K>, key: K, target: K) -> Layout<K> {
    move_relative(layout, key, target, true)
}

fn move_relative<K: ColumnKey>(layout: &Layout<K>, key: K, target: K, after: bool) -> Layout<K> {
    if key == target || key.pin().is_some() {
        return layout.clone();
    }
    let mut order = layout.order.clone();
    let Some(source) = order.iter().position(|candidate| *candidate == key) else {
        return layout.clone();
    };
    order.remove(source);
    let Some(index) = order.iter().position(|candidate| *candidate == target) else {
        return layout.clone();
    };
    order.insert(index + usize::from(after), key);
    normalize(order, layout.visible.clone())
}

fn join<K: ColumnKey>(keys: &[K]) -> String {
    keys.iter()
        .map(|key| key.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

/// Unknown ids are skipped rather than failing the whole parse: a layout
/// written by a newer build must not cost an older one its other columns.
/// A repeated id keeps its first occurrence.
fn parse_ids<K: ColumnKey>(value: &str) -> Vec<K> {
    let mut seen: Vec<K> = Vec::new();
    for token in value.split(',') {
        if let Some(key) = K::parse(token.trim()) {
            if !seen.contains(&key) {
                seen.push(key);
            }
        }
    }
    seen
}
```

Add `pub mod layout;` and the `pub use` to `columns/mod.rs`. Note the
behaviour change from the original: the music library's `parse_layout`
returned `None` on an unknown id, discarding the whole layout. Skipping is
strictly kinder and is what the test above pins.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p reprise-view columns::layout`
Expected: PASS, `7 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-view/src/columns
git commit -m "feat: one column layout, generic over the table"
```

---

### Task 3: The four column enums

**Files:**
- Modify: `crates/reprise-view/src/columns/track.rs` (implement the trait on the existing `ColumnId`)
- Create: `crates/reprise-view/src/columns/release.rs`, `concert.rs`, `radio.rs`
- Modify: `crates/reprise-view/src/columns/mod.rs`

**Interfaces:**
- Consumes: `ColumnKey`, `Pin` (Task 1).
- Produces: `ColumnId` (unchanged names) plus `ReleaseColumn { Cover, Date, Title, Artist, Type, Status, Buy }`, `ConcertColumn { Date, Artist, City, Venue, Distance, Tickets }`, `RadioColumn { Artwork, State, Station, Genre, Bitrate, Country, NowPlaying }`.

- [ ] **Step 1: Write the failing tests**

In each new file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::columns::{ColumnKey, Layout, Pin};

    #[test]
    fn release_columns_round_trip_and_pin_their_fixed_ones() {
        for key in ReleaseColumn::all() {
            assert_eq!(ReleaseColumn::parse(key.as_str()), Some(*key));
        }
        assert_eq!(ReleaseColumn::Cover.pin(), Some(Pin::Leading));
        assert_eq!(ReleaseColumn::Status.pin(), Some(Pin::Trailing));
        assert_eq!(ReleaseColumn::Buy.pin(), Some(Pin::Trailing));
        assert_eq!(ReleaseColumn::Date.pin(), None);
    }

    /// NR-25: the named text columns keep their order; the cover leads them.
    #[test]
    fn nr_25_the_default_release_layout_leads_with_the_cover() {
        let layout = Layout::<ReleaseColumn>::default();
        assert_eq!(
            layout.order,
            vec![
                ReleaseColumn::Cover,
                ReleaseColumn::Date,
                ReleaseColumn::Title,
                ReleaseColumn::Artist,
                ReleaseColumn::Type,
                ReleaseColumn::Status,
                ReleaseColumn::Buy,
            ]
        );
    }
}
```

Write the analogous pair for `concert.rs` and `radio.rs`, and for
`track.rs` a test asserting `ColumnId::Cover.pin() == Some(Pin::Leading)` and
that `Layout::<ColumnId>::default().order` still begins with `Cover` followed
by `Title`.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p reprise-view columns::`
Expected: FAIL — the enums do not exist.

- [ ] **Step 3: Implement**

`release.rs`, as the pattern for all three:

```rust
//! The releases table's columns.

use super::key::{ColumnKey, Pin};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReleaseColumn {
    Cover,
    Date,
    Title,
    Artist,
    Type,
    Status,
    Buy,
}

const ALL: [ReleaseColumn; 7] = [
    ReleaseColumn::Cover,
    ReleaseColumn::Date,
    ReleaseColumn::Title,
    ReleaseColumn::Artist,
    ReleaseColumn::Type,
    ReleaseColumn::Status,
    ReleaseColumn::Buy,
];

const DEFAULT_VISIBLE: [ReleaseColumn; 4] = [
    ReleaseColumn::Date,
    ReleaseColumn::Title,
    ReleaseColumn::Artist,
    ReleaseColumn::Type,
];

impl ColumnKey for ReleaseColumn {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cover => "cover",
            Self::Date => "date",
            Self::Title => "title",
            Self::Artist => "artist",
            Self::Type => "type",
            Self::Status => "status",
            Self::Buy => "buy",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        ALL.iter().copied().find(|key| key.as_str() == value)
    }

    fn all() -> &'static [Self] {
        &ALL
    }

    fn default_visible() -> &'static [Self] {
        &DEFAULT_VISIBLE
    }

    /// Releases has no row context menu, so Status and Buy are the only access
    /// to hiding a release and to its purchase link. They are pinned for the
    /// same reason Cover is: hiding them would make a function unreachable.
    fn pin(self) -> Option<Pin> {
        match self {
            Self::Cover => Some(Pin::Leading),
            Self::Status | Self::Buy => Some(Pin::Trailing),
            _ => None,
        }
    }
}
```

`ConcertColumn`: `Tickets` is `Trailing`, everything else free, all five free
columns visible by default. `RadioColumn`: `Artwork` and `State` are
`Leading`, the rest free and visible by default. For `ColumnId` in
`track.rs`, add the `ColumnKey` impl with `Cover => Some(Pin::Leading)`,
everything else `None`, `all()` returning the existing `DEFAULT_ORDER` and
`default_visible()` returning the seven columns the current `Default` impl
lists — copy both lists verbatim from
`crates/reprise-gnome/src/ui/track_list/column_layout.rs:25-36` and `:112-128`
so the music library's defaults do not shift.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p reprise-view columns::`
Expected: PASS. Check the count.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-view/src/columns
git commit -m "feat: column identities for releases, concerts and radio"
```

---

### Task 4: Generic column widths

**Files:**
- Modify: `crates/reprise-view/src/column_widths.rs`

**Interfaces:**
- Consumes: `ColumnKey` (Task 1).
- Produces: `serialize_widths<K: ColumnKey>(&[(K, i32)]) -> String` and `parse_widths<K: ColumnKey>(&str) -> Vec<(K, i32)>` — same names, same format, now generic.

The file stays at its current path rather than moving into `columns/`: moving
it would churn every call site for no gain.

- [ ] **Step 1: Write the failing test**

Add to its test module:

```rust
    #[test]
    fn widths_round_trip_for_a_second_table() {
        use crate::columns::ReleaseColumn;
        let widths = vec![(ReleaseColumn::Artist, 260), (ReleaseColumn::Date, 160)];
        let serialized = serialize_widths(&widths);
        assert_eq!(serialized, "artist:260,date:160");
        assert_eq!(parse_widths::<ReleaseColumn>(&serialized), widths);
    }
```

Note the expected string is sorted by id, as the existing doc comment promises.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p reprise-view widths_round_trip_for_a_second_table`
Expected: FAIL — the function is not generic.

- [ ] **Step 3: Implement**

Replace `use crate::columns::ColumnId;` with `use crate::columns::ColumnKey;`
and make both functions generic over `K: ColumnKey`, leaving every body
otherwise untouched — the sort key becomes `key.as_str()`, which it already
effectively was. Update the module doc's first line to say "a table's
columns" rather than the track columns.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p reprise-view column_widths && cargo build -p reprise-gnome`
Expected: PASS and a clean build — the music call sites infer `ColumnId`.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-view/src/column_widths.rs
git commit -m "refactor: column widths serialize for any table"
```

---

### Task 5: The erased editor model

**Files:**
- Create: `crates/reprise-gnome/src/ui/table_columns/mod.rs`
- Create: `crates/reprise-gnome/src/ui/table_columns/descriptor.rs`
- Modify: `crates/reprise-gnome/src/ui/mod.rs`

**Interfaces:**
- Consumes: nothing from GTK yet.
- Produces: `pub(in crate::ui) struct ColumnDescriptor { pub id: String, pub label: String }` and `pub(in crate::ui) trait EditorModel: 'static` with `title(&self) -> String`, `columns(&self) -> Vec<ColumnDescriptor>`, `is_visible(&self, id: &str) -> bool`, `set_visible(&self, id: &str, visible: bool)`, `move_column(&self, id: &str, target: &str, after: bool)`, `reset(&self)`. Every later task in this plan takes an `Rc<dyn EditorModel>`.

- [ ] **Step 1: Write the failing test**

In `descriptor.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Fake {
        hidden: RefCell<Vec<String>>,
    }

    impl EditorModel for Fake {
        fn title(&self) -> String {
            "Edit column layout".to_owned()
        }
        fn columns(&self) -> Vec<ColumnDescriptor> {
            vec![ColumnDescriptor {
                id: "date".to_owned(),
                label: "Date".to_owned(),
            }]
        }
        fn is_visible(&self, id: &str) -> bool {
            !self.hidden.borrow().iter().any(|hidden| hidden == id)
        }
        fn set_visible(&self, id: &str, visible: bool) {
            if visible {
                self.hidden.borrow_mut().retain(|hidden| hidden != id);
            } else {
                self.hidden.borrow_mut().push(id.to_owned());
            }
        }
        fn move_column(&self, _id: &str, _target: &str, _after: bool) {}
        fn reset(&self) {
            self.hidden.borrow_mut().clear();
        }
    }

    #[test]
    fn an_editor_model_reports_and_flips_visibility() {
        let model = Fake {
            hidden: RefCell::new(Vec::new()),
        };
        assert!(model.is_visible("date"));
        model.set_visible("date", false);
        assert!(!model.is_visible("date"));
        model.reset();
        assert!(model.is_visible("date"));
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p reprise-gnome an_editor_model_reports`
Expected: FAIL — the trait does not exist.

- [ ] **Step 3: Implement**

```rust
//! What the column editor needs to know about a table — and nothing more.
//!
//! The editor is one widget tree serving four tables whose column identities
//! are four different Rust types. Making the widget code generic over them
//! would monomorphise every closure, gesture and drag payload four times for
//! no benefit, so the type disappears at this boundary: the editor sees ids
//! and labels, and the per-table adapter behind this trait turns an id back
//! into its typed key.

pub(in crate::ui) struct ColumnDescriptor {
    pub id: String,
    pub label: String,
}

pub(in crate::ui) trait EditorModel: 'static {
    fn title(&self) -> String;
    /// The editable columns, in their current order. Pinned columns are never
    /// listed — they cannot be moved or hidden, so a row for them would be a
    /// row that does nothing.
    fn columns(&self) -> Vec<ColumnDescriptor>;
    fn is_visible(&self, id: &str) -> bool;
    fn set_visible(&self, id: &str, visible: bool);
    fn move_column(&self, id: &str, target: &str, after: bool);
    fn reset(&self);
}
```

`mod.rs` declares `pub(in crate::ui) mod descriptor;` and re-exports both
items. Declare `mod table_columns;` in `ui/mod.rs`.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p reprise-gnome an_editor_model_reports`
Expected: PASS, `1 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/table_columns crates/reprise-gnome/src/ui/mod.rs
git commit -m "feat: an erased column editor model"
```

---

### Task 6: Move the editor surface

**Files:**
- Create: `crates/reprise-gnome/src/ui/table_columns/editor.rs`
- Create: `crates/reprise-gnome/src/ui/table_columns/editor_dnd.rs`
- Modify: `crates/reprise-gnome/src/ui/track_list/column_layout_editor.rs` (shrinks to the music entry points)

**Interfaces:**
- Consumes: `EditorModel`, `ColumnDescriptor` (Task 5).
- Produces: `pub(in crate::ui) struct EditorSurface { pub toolbar: adw::ToolbarView, pub list: gtk4::ListBox }` and `pub(in crate::ui) fn build_surface(model: &Rc<dyn EditorModel>, show_reset: bool) -> EditorSurface`; `pub(in crate::ui) fn present_dialog(window: &adw::ApplicationWindow, model: &Rc<dyn EditorModel>)`; `pub(in crate::ui) fn build_navigation_page(model: &Rc<dyn EditorModel>) -> adw::NavigationPage`.

- [ ] **Step 1: Move the code**

Read `crates/reprise-gnome/src/ui/track_list/column_layout_editor.rs` in full
(653 lines) before touching it. Move into `editor.rs` everything that builds
the surface — the row construction, the toggle wiring, the Reset action,
`build_surface`, `build_dialog`, `build_navigation_page`, `present` — and
into `editor_dnd.rs` the row drag-and-drop: `wire_row_drag_and_drop`,
`set_drop_indicator`, `is_after_half`, `parse_drag_payload`,
`keyboard_reorder_offset`, and the `DROP_BEFORE_CLASS`, `DROP_AFTER_CLASS`,
`ROW_CLASS`, `HANDLE_CLASS`, `HANDLE_REST_OPACITY`, `HANDLE_ACTIVE_OPACITY`,
`DRAG_GHOST_OPACITY` constants. The split exists because the original is 653
lines against an 800-line gate and this task adds callers, not because the
two halves are unrelated.

Three substitutions turn the moved code table-independent:

- `Rc<TrackList>` → `Rc<dyn EditorModel>`.
- `ColumnId` → `String` id; `column_layout::column_label(id)` →
  `descriptor.label`; `editor_lists_column(id)` disappears, because
  `EditorModel::columns` already returns only listable columns.
- `column_layout::set_column_visible` / `move_column` / `move_column_after`
  and the layout write-back → `model.set_visible`, `model.move_column(id,
  target, after)`, `model.reset()`.

`row_capabilities` disappears with it: every listed row is toggleable and
draggable, which the original already documented and the pin model now
guarantees.

- [ ] **Step 2: Leave the music entry points behind**

`track_list/column_layout_editor.rs` keeps only what the rest of the app calls
by name today — `SMOKE_ENV`, `arm_smoke_column_layout_editor`'s hook,
`present`, `build_navigation_page`, `install_header_popover` — each delegating
to the new module with the music adapter from Task 9. Until Task 9 lands, have
them construct the adapter inline; the compiler will point at the gap.

- [ ] **Step 3: Verify the file sizes**

Run: `bash scripts/check-architecture.sh`
Expected: passes. Confirm with `wc -l` that `editor.rs` and `editor_dnd.rs`
each stay well under 800 and that `column_layout_editor.rs` shrank.

- [ ] **Step 4: Run the existing tests**

Run: `cargo test -p reprise-gnome column_layout`
Expected: PASS — this task changes structure, not behaviour.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/table_columns crates/reprise-gnome/src/ui/track_list/column_layout_editor.rs
git commit -m "refactor: lift the column editor out of the track list"
```

---

### Task 7: The generic registry and its persistence

**Files:**
- Create: `crates/reprise-gnome/src/ui/table_columns/registry.rs`
- Create: `crates/reprise-gnome/src/ui/table_columns/width_persistence.rs`
- Modify: `crates/reprise-gnome/src/ui/track_list/column_layout.rs` (shrinks to the music adapter in Task 9)

**Interfaces:**
- Consumes: `Layout<K>`, `ColumnKey` (Tasks 1–2), `EditorModel` (Task 5).
- Produces:
  - `pub(in crate::ui) struct ColumnRegistry<K: ColumnKey>` with `new(view: &gtk4::ColumnView, conn: Db, keys: TableKeys, columns: Vec<(K, gtk4::ColumnViewColumn)>) -> Rc<Self>`, `apply(&self, &Layout<K>)`, `column(&self, K) -> Option<&gtk4::ColumnViewColumn>`, `is_visible(&self, K) -> bool`, `reset(&self)`, `layout(&self) -> Layout<K>`.
  - `pub(in crate::ui) struct TableKeys { pub layout: &'static str, pub widths: &'static str }`.
  - `impl<K: ColumnKey> EditorModel for ColumnRegistry<K>` — this is the bridge between the typed core and the erased surface.
  - `pub(in crate::ui) fn wire(registry: &Rc<ColumnRegistry<K>>, label: impl Fn(K) -> String, width: impl Fn(K) -> i32, filler: K)` in `width_persistence.rs`.

- [ ] **Step 1: Write the failing test**

In `registry.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use reprise_view::columns::ReleaseColumn;

    /// STYLE-10: the filler role is not welded to one column. Hiding the
    /// filler moves it to the first visible free column, or the table stops
    /// absorbing its own slack — which is the gap the music library has had
    /// since Title became hideable.
    #[test]
    fn style_10_the_filler_moves_when_it_is_hidden() {
        let layout = reprise_view::columns::layout::set_visible(
            &reprise_view::columns::Layout::<ReleaseColumn>::default(),
            ReleaseColumn::Title,
            false,
        );
        assert_eq!(
            filler_for(&layout, ReleaseColumn::Title),
            Some(ReleaseColumn::Date),
            "with Title hidden, Date is the first visible free column"
        );
        assert_eq!(
            filler_for(
                &reprise_view::columns::Layout::<ReleaseColumn>::default(),
                ReleaseColumn::Title
            ),
            Some(ReleaseColumn::Title),
            "a visible preferred filler keeps the role"
        );
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p reprise-gnome style_10_the_filler_moves`
Expected: FAIL — `cannot find function filler_for`.

- [ ] **Step 3: Implement**

Move `ColumnRegistry`, `restore_stored_widths`, `save_widths_now`,
`wire_width_persistence`, `wire_order_persistence`, `reset_widths`,
`WIDTH_SAVE_DEBOUNCE_MS` and the `syncing_order` / `syncing_width` cells out of
`track_list/column_layout.rs`, making each generic over `K: ColumnKey` and
replacing `COLUMN_LAYOUT_KEY` / `COLUMN_WIDTHS_KEY` with the `TableKeys` the
registry was constructed with. Keep every existing comment — the reasoning
about `syncing_order` muting the rebuild, about the mid-mutation snapshot, and
about Title's fill-expand flip is load-bearing and applies unchanged.

Add the filler rule:

```rust
/// The column that should absorb the table's leftover width: the preferred
/// filler while it is visible, otherwise the first visible free column in
/// order. STYLE-9 wants exactly one such column per table, and the preferred
/// one can be hidden by the user.
pub(in crate::ui) fn filler_for<K: ColumnKey>(layout: &Layout<K>, preferred: K) -> Option<K> {
    if layout.visible.contains(&preferred) {
        return Some(preferred);
    }
    layout
        .order
        .iter()
        .copied()
        .find(|key| key.pin().is_none() && layout.visible.contains(key))
}
```

`apply` calls it after setting visibility, turns `set_expand(true)` on the
returned column and `set_expand(false)` on every other, and skips the whole
step while `syncing_width` is set — otherwise the fill flip would be read as a
manual resize by the very listener that watches for one.

`impl EditorModel for ColumnRegistry<K>` maps ids through `K::parse`, filters
`pin().is_some()` out of `columns()`, and writes each change back through
`layout::set_visible` / `move_before` / `move_after` followed by `apply` and a
`settings::set_setting` of `layout::serialize`.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p reprise-gnome style_10_the_filler_moves && cargo build -p reprise-gnome`
Expected: PASS and a clean build.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/table_columns crates/reprise-gnome/src/ui/track_list/column_layout.rs
git commit -m "feat: one column registry for every table"
```

---

### Task 8: Header popover and header drag

**Files:**
- Create: `crates/reprise-gnome/src/ui/table_columns/header_popover.rs`
- Create: `crates/reprise-gnome/src/ui/table_columns/header_dnd.rs`
- Modify: `crates/reprise-gnome/src/ui/track_list/column_header_dnd.rs` (becomes the music binding)

**Interfaces:**
- Consumes: `EditorModel` (Task 5), `build_surface` (Task 6).
- Produces: `pub(in crate::ui) fn install_header_popover(view: &gtk4::ColumnView, model: &Rc<dyn EditorModel>)` and `pub(in crate::ui) fn install_header_drag(view: &gtk4::ColumnView, model: &Rc<dyn EditorModel>)`.

- [ ] **Step 1: Move and generalize**

`header_popover.rs` takes `install_header_popover` from
`column_layout_editor.rs:395-431` verbatim, swapping `Rc<TrackList>` for
`Rc<dyn EditorModel>` and `track_list.column_view_widget()` for the passed
view. **Keep the capture-phase comment intact.** It records that
`GtkColumnViewTitle`'s own click gesture claims every press at the target, so
a bubble-phase ancestor never sees a header right-click — the same claim race
that broke GTK's native column drag. Someone will otherwise "simplify" the
propagation phase and silently lose the feature.

`header_dnd.rs` takes the 629-line `column_header_dnd.rs` the same way. Its
resolution from a dropped column back to a layout position goes through
`EditorModel::move_column` with string ids instead of `ColumnId`.

- [ ] **Step 2: Verify the existing header-drag tests still pass**

Run: `cargo test -p reprise-gnome column_header_dnd`
Expected: PASS. The music table is the only caller so far, so any change in
behaviour here is a regression, not a feature.

- [ ] **Step 3: Verify file sizes**

Run: `bash scripts/check-architecture.sh`
Expected: passes.

- [ ] **Step 4: Commit**

```bash
git add crates/reprise-gnome/src/ui/table_columns crates/reprise-gnome/src/ui/track_list/column_header_dnd.rs
git commit -m "refactor: header popover and header drag serve any table"
```

---

### Task 9: The music adapter — proving nothing changed

**Files:**
- Modify: `crates/reprise-gnome/src/ui/track_list/column_layout.rs`
- Modify: `crates/reprise-gnome/src/ui/track_list/column_layout_tests.rs`
- Modify: `crates/reprise-gnome/src/ui/window/window.rs:265`

**Interfaces:**
- Consumes: everything above.
- Produces: `pub(in crate::ui) fn registry(track_list: &Rc<TrackList>) -> Rc<ColumnRegistry<ColumnId>>` and `pub(in crate::ui) fn model(track_list: &Rc<TrackList>) -> Rc<dyn EditorModel>`.

The music table's visible behaviour must not change at all. This task is
finished when the existing tests pass unmodified except for import paths.

- [ ] **Step 1: Write the failing regression test**

In `column_layout_tests.rs`:

```rust
    /// STYLE-10: the music library is the table this concept came from. After
    /// generalising it, its default layout, widths and filler must be
    /// bit-identical — a silent shift here is a regression for every existing
    /// user, whose stored layout was written against these defaults.
    #[test]
    fn style_10_the_music_defaults_are_unchanged() {
        let layout = reprise_view::columns::Layout::<ColumnId>::default();
        assert_eq!(
            reprise_view::columns::layout::serialize(&layout),
            "cover,title,artist,album,year,added,duration,rating,play-count,track-number,genre;\
cover,title,artist,album,year,duration,rating"
        );
    }
```

Derive the expected string from the current `DEFAULT_ORDER` and `Default`
impl before running — if it disagrees, the enum lists in Task 3 were copied
wrong, and that is what this test is for.

- [ ] **Step 2: Run to verify it fails or passes for the right reason**

Run: `cargo test -p reprise-gnome style_10_the_music_defaults`
Expected: PASS once Task 3's lists are correct; a FAIL here means the
defaults shifted and must be fixed in `track.rs`, not in the test.

- [ ] **Step 3: Build the adapter**

`column_layout.rs` keeps only what is music-specific: `column_label`,
`cell_alignment`, `column_width_policy`, `apply_column_width_policy`,
`is_width_persistable`, the column construction, and the two functions above.
Everything generic now lives in `table_columns/`. `window.rs:265` becomes
`table_columns::header_popover::install_header_popover(track_list.column_view_widget(), &column_layout::model(&track_list));`
plus the matching `install_header_drag` call.

- [ ] **Step 4: Run the whole music column suite**

Run: `cargo test -p reprise-gnome column_layout && cargo test -p reprise-gnome column_widths`
Expected: PASS, with the same counts as on `origin/dev`. Compare them.

- [ ] **Step 5: Run the music display tests singly**

Run each of the display-gated tests in `column_layout_tests.rs` with
`xvfb-run -a … -- --ignored --exact --test-threads=1`.
Expected: PASS. Check `origin/dev` first for any that are already red.

- [ ] **Step 6: Commit**

```bash
git add crates/reprise-gnome/src/ui/track_list crates/reprise-gnome/src/ui/window/window.rs
git commit -m "refactor: the music table becomes one column adapter among four"
```

---

### Task 10: Releases — adapter and cover column

**Files:**
- Create: `crates/reprise-gnome/src/ui/releases/releases_column_layout.rs`
- Modify: `crates/reprise-gnome/src/ui/releases/releases_columns.rs`
- Modify: `crates/reprise-gnome/src/ui/releases/releases_view.rs:92-130`
- Modify: `crates/reprise-gnome/src/ui/updates/release_cover.rs`
- Modify: `crates/reprise-core/src/library/settings.rs` (add the three keys)

**Interfaces:**
- Consumes: `ColumnRegistry<ReleaseColumn>` (Task 7), `install_header_popover`, `install_header_drag` (Task 8).
- Produces: `LazyReleaseCover::set_release(&self, release_group_mbid: &str, artist: &str)`; `releases_column_layout::model(...) -> Rc<dyn EditorModel>`.

- [ ] **Step 1: Add the settings keys**

In `crates/reprise-core/src/library/settings.rs`, beside `COLUMN_LAYOUT_KEY`:

```rust
pub const RELEASES_COLUMN_LAYOUT_KEY: &str = "ui.column_layout.releases";
pub const RELEASES_COLUMN_WIDTHS_KEY: &str = "ui.column_widths.releases";
pub const CONCERTS_COLUMN_LAYOUT_KEY: &str = "ui.column_layout.concerts";
pub const CONCERTS_COLUMN_WIDTHS_KEY: &str = "ui.column_widths.concerts";
pub const RADIO_COLUMN_LAYOUT_KEY: &str = "ui.column_layout.radio";
pub const RADIO_COLUMN_WIDTHS_KEY: &str = "ui.column_widths.radio";
```

`ui.column_layout` and `ui.column_widths` stay exactly as they are and keep
meaning the music table — that is what makes this change migration-free.

- [ ] **Step 2: Write the failing rebinding test**

In `release_cover.rs`:

```rust
    /// STYLE-10: `ColumnView` recycles row widgets, so a cover cell is bound
    /// to a second release without ever being constructed again. Latching the
    /// MBID at construction — right for the popover this widget was built for
    /// — would show the previous row's artwork here.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn style_10_releases_cover_rebinds_when_the_row_changes() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let cover = LazyReleaseCover::new(40);
        cover.set_release("11111111-1111-1111-1111-111111111111", "Falling Leaves");
        let first = cover.initials_text();
        cover.set_release("22222222-2222-2222-2222-222222222222", "Air");
        assert_ne!(cover.initials_text(), first, "the cell kept the old row");
        assert!(!cover.shows_image(), "a rebound cell must clear its picture");
    }
```

Add the two small accessors the test needs (`initials_text`, `shows_image`)
next to `widget()`.

- [ ] **Step 3: Run to verify it fails**

Run: `xvfb-run -a cargo test -p reprise-gnome style_10_releases_cover_rebinds -- --ignored --exact --test-threads=1`
Expected: FAIL — `LazyReleaseCover::new` takes three arguments.

- [ ] **Step 4: Make the cover rebindable**

`LazyReleaseCover::new(edge: i32)` builds the tile with no release. A new
`set_release(mbid, artist)` sets the initials, hides and clears the picture,
resets the `started` cell, stores the mbid — and, when
`reprise_core::cover_download::release_group_cover_path(mbid)` already
resolves, sets the file **synchronously** rather than through
`one_shot_task`. Without that, every scroll pass flashes the initials tile
before the cached image reappears. `connect_map` keeps arming the lazy fetch
for the uncached case, reading the mbid from the cell rather than from the
closure's capture.

- [ ] **Step 5: Add the column and the adapter**

In `releases_columns.rs`, prepend a cover column pinned to 40 px via
`widths::pin` (matching the music library's Cover width), bound through
`set_release(&entry.release_group_mbid, &entry.artist_name)` on `bind` and
cleared on `unbind`. Update `column_contract()` and the two tests that assert
it — `nr_25_table_has_the_five_named_columns` and
`nr_20_table_adds_a_bandcamp_purchase_column` — so the named text columns are
checked at their new offset rather than deleted.

`releases_column_layout.rs` holds the adapter: labels from
`strings_releases.rs`, widths from the `widths::` constants already used in
`releases_columns.rs`, `TableKeys` from Step 1, preferred filler
`ReleaseColumn::Title`.

In `releases_view.rs`, after `append_columns`, build the registry, apply the
stored layout, and install the popover and header drag on the view.

- [ ] **Step 6: Write the failing surface test**

```rust
    /// STYLE-10: what the user actually does — right-click the header band,
    /// uncheck a column, and find it gone and still gone after a rebuild.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn style_10_releases_header_right_click_edits_the_table() {
        // Build the view as `releases_view.rs` does, right-click at (x, 4)
        // inside the header band, assert a popover is realised, toggle the
        // Type row, and assert the Type column is no longer visible while
        // Cover, Status and Buy still are.
    }
```

- [ ] **Step 7: Run the tests singly**

Run each new test with `xvfb-run -a … -- --ignored --exact --test-threads=1`.
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/reprise-core/src/library/settings.rs crates/reprise-gnome/src/ui/releases crates/reprise-gnome/src/ui/updates/release_cover.rs
git commit -m "feat: releases gets a cover column and an editable table"
```

---

### Task 11: Concerts — adapter

**Files:**
- Create: `crates/reprise-gnome/src/ui/concerts/concerts_column_layout.rs`
- Modify: `crates/reprise-gnome/src/ui/concerts/concerts_columns.rs`
- Modify: `crates/reprise-gnome/src/ui/concerts/concerts_view.rs`

**Interfaces:**
- Consumes: `ColumnRegistry<ConcertColumn>` (Task 7), Task 10's settings keys.
- Produces: `concerts_column_layout::model(...) -> Rc<dyn EditorModel>`.

- [ ] **Step 1: Write the failing test**

```rust
    /// STYLE-10: concerts learns the same gesture. Tickets is pinned — it is
    /// the only access to the ticket link on a surface with no row context
    /// menu — so it must survive a hide-everything attempt.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn style_10_concerts_header_right_click_edits_the_table() {
        // Build the view as `concerts_view.rs` does, open the header popover,
        // hide City, assert City is invisible and Tickets is still visible.
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `xvfb-run -a cargo test -p reprise-gnome style_10_concerts_header -- --ignored --exact --test-threads=1`
Expected: FAIL.

- [ ] **Step 3: Implement**

The adapter mirrors Task 10's: labels from `strings_concerts.rs`, widths from
the `widths::Sizing` values already in `concerts_columns.rs`, `TableKeys` for
concerts, preferred filler `ConcertColumn::Venue` — concerts has no `filler`
column today, every column is `pinned`, so this is the one behavioural
addition: give Venue the expand so the table stops leaving slack unclaimed.
`append_columns` returns its `SortColumns` unchanged; wire the registry after
it in `concerts_view.rs`.

- [ ] **Step 4: Run singly**

Expected: PASS. Also rerun `style_9_concert_columns_keep_their_width_when_the_rows_change` — giving Venue the expand must not reintroduce width drift.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/concerts
git commit -m "feat: concerts gets an editable table"
```

---

### Task 12: Radio — adapter

**Files:**
- Create: `crates/reprise-gnome/src/ui/radio/radio_column_layout.rs`
- Modify: `crates/reprise-gnome/src/ui/radio/radio_columns.rs`
- Modify: `crates/reprise-gnome/src/ui/radio/radio_view.rs:115`

**Interfaces:**
- Consumes: `ColumnRegistry<RadioColumn>` (Task 7), Task 10's settings keys.
- Produces: `radio_column_layout::model(...) -> Rc<dyn EditorModel>`.

- [ ] **Step 1: Write the failing test**

```rust
    /// STYLE-10: radio's artwork and state columns lead every row and are
    /// pinned; Station is its filler and hideable, which is exactly the case
    /// that moves the filler role.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn style_10_radio_header_right_click_edits_the_table() {
        // Open the header popover, hide Station, assert Genre now expands and
        // Artwork and State are still visible.
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `xvfb-run -a cargo test -p reprise-gnome style_10_radio_header -- --ignored --exact --test-threads=1`
Expected: FAIL.

- [ ] **Step 3: Implement**

Labels from `strings.rs`'s `RADIO_*` constants, widths from the existing
`widths::` calls, preferred filler `RadioColumn::Station`. Note
`radio_columns.rs` has a test that reads its own source text
(`nav_10b_…split_once("pub(super) fn append_columns")`) — check whether the
refactor moves that anchor and update it if so.

- [ ] **Step 4: Run singly**

Expected: PASS, plus `style_9_radio_columns_keep_their_width_when_the_rows_change`.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/radio
git commit -m "feat: radio gets an editable table"
```

---

### Task 13: The menu entry follows the active view

**Files:**
- Modify: `crates/reprise-gnome/src/ui/primary_menu.rs:135-147`
- Modify: `crates/reprise-gnome/src/ui/window/window.rs`
- Modify: each of the four views, to register themselves on becoming active

**Interfaces:**
- Consumes: `Rc<dyn EditorModel>` from all four adapters.
- Produces: `pub(in crate::ui) struct ActiveTable(RefCell<Option<Rc<dyn EditorModel>>>)` with `set(&self, model: Option<Rc<dyn EditorModel>>)` and `get(&self) -> Option<Rc<dyn EditorModel>>`, owned by the window and shared with the primary menu.

- [ ] **Step 1: Write the failing test**

```rust
    /// STYLE-10: the keyboard route to the editor must address the table the
    /// user is looking at, and must not pretend to work where there is no
    /// table.
    #[test]
    fn style_10_the_menu_action_follows_the_active_table() {
        let active = ActiveTable::default();
        assert!(active.get().is_none(), "no table, no target");
        active.set(Some(fake_model("Releases")));
        assert_eq!(active.get().expect("a table").title(), "Releases");
        active.set(None);
        assert!(active.get().is_none());
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p reprise-gnome style_10_the_menu_action_follows`
Expected: FAIL — `ActiveTable` does not exist.

- [ ] **Step 3: Implement**

`ACTION_EDIT_COLUMN_LAYOUT` reads `ActiveTable::get()` and presents the dialog
for whatever it returns; when it returns `None` the action is disabled via
`set_enabled(false)`, so the menu item greys out rather than opening the wrong
table. Each view sets it on becoming visible and clears it on leaving. Find
the view-switch point in `window.rs` — the same place that already decides
`matches!(source, ViewSource::Library)` for the status bar — and drive it from
there rather than adding four independent hooks.

Preferences → Layout keeps opening the music table: inside Preferences there
is no active view to follow. Leave `preferences.rs:500` alone.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p reprise-gnome style_10_ && cargo test -p reprise-gnome primary_menu`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/primary_menu.rs crates/reprise-gnome/src/ui/window crates/reprise-gnome/src/ui/releases crates/reprise-gnome/src/ui/concerts crates/reprise-gnome/src/ui/radio
git commit -m "feat: edit column layout addresses the table on screen"
```

---

### Task 14: The rule, the amendment, and the gates

**Files:**
- Modify: `docs/ux-rules.md` (section U after STYLE-9 at line 2820; NR-25 at line 2286)
- Modify: `scripts/check-frontend-thinness.sh` (`view_floor`)

**Interfaces:**
- Consumes: everything above.
- Produces: nothing further.

- [ ] **Step 1: Write STYLE-10**

Insert after STYLE-9 in `docs/ux-rules.md`:

```markdown
- **STYLE-10** [active] [gtk] — **Columns belong to the user, in every
  table.** A right-click anywhere on a table's header band opens the column
  editor popover: toggle visibility, drag to reorder, reset. The same editor
  is reachable without a pointer through the primary menu's "Edit column
  layout…", which addresses the table of the active view and is insensitive
  where no table is shown. Order, visibility and header-dragged widths are
  stored per table and survive a restart. A table may declare fixed columns —
  a leading artwork column, a trailing action column on a surface without a
  row context menu — which stay visible, keep their position and never appear
  in the editor; every other column belongs to the user. Exactly one visible
  column is the filler (STYLE-9); when the user hides it, the filler role
  moves to the first visible free column in the table's order. Hiding the
  sorting column does not change the sort, because hiding is a visibility
  flip and never removes the column from the view. **Test rule:** one
  rule-named display test per table, plus a measured filler test.
```

- [ ] **Step 2: Amend NR-25**

NR-25 reads "The gap view remains the table `Date · Title · Artist · Type ·
Status`". Add after the first sentence: "A fixed cover column leads them, and
the `Buy` column of NR-20 trails them; both follow STYLE-10's fixed-column
rule. The named text columns are otherwise unchanged in name and order."

- [ ] **Step 3: Raise the thinness floor**

Run: `bash scripts/check-frontend-thinness.sh`
Expected: FAIL with "shared view layer: production lines are up to N (floor
still says 1782)". Set `view_floor=N` to the number the script printed. Do not
guess it.

- [ ] **Step 4: Run every gate**

```bash
bash scripts/check-architecture.sh
bash scripts/check-frontend-thinness.sh
bash scripts/check-ux-traceability.sh
cargo test -p reprise-view
cargo test -p reprise-core
cargo test -p reprise-gnome
```

Expected: all pass. Four gates are already red on `origin/dev` itself
(thinness, catalogues, `nav_10a`, arch lint) — check the base before treating
any failure as caused by this branch.

- [ ] **Step 5: Commit**

```bash
git add docs/ux-rules.md scripts/check-frontend-thinness.sh
git commit -m "docs: STYLE-10 binds column editing to every table"
```

---

## Parallelism and file ownership

Tasks 1–4 (`reprise-view`) are one chain, one agent. Tasks 5–8 (the shared GTK
module) are a second chain that starts once Task 4 has landed. Task 9 must
follow Task 8 and precede nothing — but it is the regression proof, so it is
worth its own gate.

Tasks 10, 11 and 12 are independent of each other and own disjoint files.
Three agents may run them concurrently once Task 9 is green. Record this
ownership in `AGENTS.md` in the worktree before dispatching, not only here —
an agent that never reads this plan still must not stray:

| Task | Owns |
|---|---|
| 10 | `ui/releases/`, `ui/updates/release_cover.rs`, `core/library/settings.rs` |
| 11 | `ui/concerts/` |
| 12 | `ui/radio/` |

Task 10 adds the settings keys for all three tables, so 11 and 12 must not
touch `settings.rs`. Tasks 13 and 14 run last, alone: 13 touches all four
views, 14 touches `docs/ux-rules.md`, which every parallel agent would
otherwise conflict on.

## Self-review

- **Spec coverage.** §A.1 core → Tasks 1–4. §A.2 surface → Tasks 5–8. §A.3
  the fixed/free table → Task 3's enums. §A.4 persistence → Task 10 Step 1
  plus Task 7's `TableKeys`. §A.5 reachability → Tasks 8 and 13. §A.6 edge
  cases: filler → Task 7, sorting → Task 7's `set_visible` constraint stated
  in the Global Constraints, everything-hidden → Task 3's pins, narrow window
  → untouched by construction. §A.7 the cover column → Task 10. STYLE-10 and
  the NR-25 amendment → Task 14.
- **Not covered here by design.** Part B of the spec (dates) is a separate
  plan, `2026-08-09-system-date-format.md`.
- **Type consistency.** `Layout<K>` with fields `order` and `visible` is used
  under that name in Tasks 2, 3, 7 and 9. The layout functions are
  `set_visible` / `move_before` / `move_after` throughout — the music
  library's originals were `set_column_visible` / `move_column` /
  `move_column_after`, and Task 6 Step 1 names that rename explicitly so the
  call sites are updated rather than silently left dangling. `EditorModel`'s
  six methods are listed identically in Tasks 5, 6, 7 and 13. `TableKeys {
  layout, widths }` is introduced in Task 7 and consumed in Tasks 10–12.
- **Known soft spots, stated rather than papered over.** Task 11 adds an
  expand to Venue that concerts does not have today; the step says so and
  re-runs the STYLE-9 test that would catch the consequence. Task 12 flags a
  test that reads its own file's source text and may need its anchor updated.
  Tasks 6, 7 and 8 move large bodies of code whose exact final line counts
  cannot be predicted here — each ends with an explicit
  `scripts/check-architecture.sh` run rather than an assumption.
