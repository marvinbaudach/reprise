# FIL Filter Visibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Model policy (user token strategy):** dispatch every implementation subagent with `model: sonnet`. The orchestrating session only reviews between tasks.

**Goal:** Active track-list restrictions become unmissable (ux-rules.md section K, FIL-1a + FIL-2..6): the headerbar search appears as a chip in a permanent per-source filter row, the hit count is accented state, a "Clear all" resets everything in one click, an end-of-results line explains hidden tracks at the list end, the search field carries its own state, and matches are highlighted in the four searched columns.

**Architecture:** The existing `BrowseBar` becomes the permanent list header of every track source. A tiny pure module (`filter_restriction.rs`) is the single visibility law (`is_restricted`, `is_track_source`, `row_visible`); `browse_filter_count.rs` extends totals to all sources; a window-level `win.clear-all-filters` action is the one reset invoked by the "Clear all" chip-row button, the FIL-3 pill, and the FIL-6 empty-state button. FIL-3 is an overlay positioned from the ColumnView's measured content height (virtualization untouched). FIL-5 introduces Pango markup (escaped, ASCII-case-insensitive to mirror SQLite LIKE) into the four text-cell bind closures.

**Tech Stack:** Rust, gtk4-rs (GTK 4.22), libadwaita, rusqlite. No new dependencies.

## Global Constraints

- Gates before EVERY commit: `cargo fmt --check` · `cargo clippy --locked --all-targets --workspace -- -D warnings` · `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace` · `scripts/check-ux-traceability.sh` · `scripts/check-architecture.sh`.
- ux-rules.md status flips (`[geplant] → [aktiv]`) happen IN the implementation commit of the rule — flip only when the rule is FULLY implemented (FIL-2 flips last, in Task 10).
- Rule-named tests: `fn fil_<nr><suffix>_…` with exactly ONE primary rule ID, `#[test]` directly above the fn (≤ 5 lines), a `// UX FIL-…:` comment ABOVE the `#[test]` attribute, never between attribute and fn. Rule-named tests for rules that will flip `[aktiv]` must be display-free (no `gtk4::init`) so they run un-ignored in the workspace suite.
- Widget-level verifications that need a display are written as NON-rule-named tests with `#[ignore = "requires a display; run via xvfb-run"]` (run via `scripts/check-display-tests.sh`).
- Files < 800 lines (browse_bar.rs is at 788 — Task 2 starts with a split). RefCell discipline: never hold a `borrow()` across a GTK call.
- All user-visible copy via gettext `N_!` constants/functions (English, typographic quotes “ ” U+201C/U+201D). Comments/identifiers English.
- One commit per task, no attribution footer, no push.
- reprise-core stays untouched except where a task says otherwise (none do).

## Parallel Execution Map (file ownership per wave)

| Wave | Tasks (parallel) | Files owned (disjoint within wave) |
|---|---|---|
| 1 | T1, T5, T6, T7 | T1: `browse/filter_restriction.rs`, `browse/mod.rs` · T5: `status_bar.rs`, `window/window.rs:283-289`, `strings.rs` (status block only) · T6: `window/library_chrome.rs`, `style/interactions.rs` · T7: `track_list/match_highlight.rs`, `track_list/track_list_columns.rs`, `track_list/mod.rs` |
| 2 | T2 | `browse/browse_bar.rs`, `browse/browse_chooser.rs` (new), `browse/browse_filter_strings.rs`, `browse/mod.rs` |
| 3 | T3, T4, T8 | T3: `browse/browse_filter_count.rs`, `track_list/track_list_reload.rs`, `view_session.rs:135-142` · T4: `track_list/track_list.rs`, `window/window_runtime_wiring.rs` · T8: `track_list/end_of_results.rs` (new), `track_list/track_list_builder.rs`, `track_list/mod.rs`, `strings.rs` (end-of-results block) |
| 4 | T9 | `track_list/track_list_empty_state.rs`, `track_list/track_list_builder.rs` (button field), `track_list/track_list.rs` (scan-widget seam) |
| 5 | T10 | `docs/ux-rules.md` (FIL-2 flip), verification only |

Rule flips: T4 → FIL-1a · T6 → FIL-4 · T7 → FIL-5 · T8 → FIL-3 · T9 → FIL-6 · T10 → FIL-2. FIL-1b stays `[geplant]`.

---

### Task 1: Visibility law — `filter_restriction.rs`

**Files:**
- Create: `crates/reprise-gnome/src/ui/browse/filter_restriction.rs`
- Modify: `crates/reprise-gnome/src/ui/browse/mod.rs` (add `pub(in crate::ui) mod filter_restriction;`)

**Interfaces:**
- Produces: `pub(in crate::ui) fn is_restricted(search: &str, browse: &BrowseFilter) -> bool` · `pub(in crate::ui) fn is_track_source(source: &ViewSource) -> bool` · `pub(in crate::ui) fn row_visible(is_track_source: bool, restricted: bool, preference_visible: bool) -> bool`. Consumed by Tasks 2, 3, 8.

- [ ] **Step 1: Write the failing tests** (bottom of the new file):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // UX FIL-2: the hide preference only governs the idle state — an active
    // restriction always forces the row visible.
    #[test]
    fn fil_2_row_is_forced_visible_when_restricted_despite_hidden_preference() {
        assert!(row_visible(true, true, false));
        assert!(row_visible(true, true, true));
    }

    // UX FIL-2: idle visibility follows the preference; panel sources never show.
    #[test]
    fn fil_2_row_follows_preference_when_idle() {
        assert!(row_visible(true, false, true));
        assert!(!row_visible(true, false, false));
        assert!(!row_visible(false, true, true));
    }

    // UX FIL-1a: the row is the track table's header — panel and non-list
    // sources have no row for it to describe.
    #[test]
    fn fil_1a_row_never_shows_for_panel_sources() {
        assert!(!is_track_source(&ViewSource::ImportErrors));
        assert!(!is_track_source(&ViewSource::MyStats));
        assert!(!is_track_source(&ViewSource::Device { serial: "x".into() }));
        assert!(is_track_source(&ViewSource::Library));
        assert!(is_track_source(&ViewSource::Playlist(3)));
        assert!(is_track_source(&ViewSource::Queue));
        assert!(is_track_source(&ViewSource::Missing));
    }

    // UX FIL-2: a whitespace-only search does not restrict (mirrors the
    // trim in reload's has_filter).
    #[test]
    fn fil_2_whitespace_search_does_not_restrict() {
        assert!(!is_restricted("   ", &BrowseFilter::default()));
        assert!(is_restricted("falling", &BrowseFilter::default()));
        let browse = BrowseFilter { genre: Some("Metal".into()), artist: None, album: None };
        assert!(is_restricted("", &browse));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p reprise-gnome filter_restriction -- --nocapture`
Expected: FAIL to compile (module/functions missing).

- [ ] **Step 3: Write the implementation** (top of the same file):

```rust
//! FIL-1a/FIL-2 visibility law for the filter row — pure decisions, no GTK.
//! The row is a permanent list header of every track source; the hide
//! preference only governs the idle state, an active restriction always
//! forces it visible (docs/ux-rules.md K).

use reprise_core::queries::BrowseFilter;
use reprise_core::view_source::ViewSource;

pub(in crate::ui) fn is_restricted(search: &str, browse: &BrowseFilter) -> bool {
    !search.trim().is_empty() || !browse.is_empty()
}

pub(in crate::ui) fn is_track_source(source: &ViewSource) -> bool {
    !matches!(
        source,
        ViewSource::ImportErrors | ViewSource::MyStats | ViewSource::Device { .. }
    )
}

pub(in crate::ui) fn row_visible(
    is_track_source: bool,
    restricted: bool,
    preference_visible: bool,
) -> bool {
    is_track_source && (restricted || preference_visible)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p reprise-gnome filter_restriction`
Expected: 4 passed.

- [ ] **Step 5: Gates, then commit**

```bash
git add crates/reprise-gnome/src/ui/browse/filter_restriction.rs crates/reprise-gnome/src/ui/browse/mod.rs
git commit -m "feat: add filter-row visibility law for FIL-1a/FIL-2"
```

---

### Task 2: BrowseBar rework — permanent quiet header, search chip, accent count, Clear all

**Files:**
- Create: `crates/reprise-gnome/src/ui/browse/browse_chooser.rs` (extracted popover chooser)
- Modify: `crates/reprise-gnome/src/ui/browse/browse_bar.rs`, `crates/reprise-gnome/src/ui/browse/browse_filter_strings.rs`, `crates/reprise-gnome/src/ui/browse/mod.rs`

**Interfaces:**
- Consumes: `filter_restriction::{is_restricted, is_track_source, row_visible}` (Task 1).
- Produces (new `BrowseBar` API, all in addition to the existing one):
  - `pub fn set_source_context(&self, source: &ViewSource)` — stores `track_source: Cell<bool>` + `is_library: Cell<bool>`, syncs visibility. (`set_library_visible` stays as a shim delegating `track_source = is_library = visible` until Task 3 removes its call sites.)
  - `pub fn set_search(self: &Rc<Self>, text: &str)` — stores the live search, rebuilds chips + visibility.
  - `pub fn set_on_search_cleared(&self, cb: impl Fn() + 'static)` — search-chip × clicked.
  - `pub fn set_on_clear_all(&self, cb: impl Fn() + 'static)` — Clear-all button clicked.
  - `pub(in crate::ui) fn result_count(&self) -> Option<(usize, usize)>` — getter over the existing `result_count` Cell (consumed by Tasks 8, 9).
  - strings: `search_chip_label(query) -> "⌕ “{query}” in any field"`, `remove_search_label(query)`, `CLEAR_ALL = N_!("Clear all")`, `result_count_markup(filtered, total) -> (String, bool)`.

- [ ] **Step 1: Mechanical split.** Move `build_chooser`, `wire_chooser`, `chooser_row`, `load_values`, and the `FACET_PAGE`/`VALUE_PAGE`/`POPUP_MIN_HEIGHT` constants plus `browse_popup_min_height` from `browse_bar.rs` into new `browse_chooser.rs` (`pub(super)` items, `use super::browse_bar::BrowseBar;` where needed). Declare `mod browse_chooser;` in `browse/mod.rs`. No behavior change. Run `cargo test -p reprise-gnome browse` → green. Commit `refactor: extract browse value chooser from browse_bar`.

- [ ] **Step 2: Write the failing pure tests** (in `browse_filter_strings.rs` and `browse_bar.rs` test mods):

```rust
// in browse_filter_strings.rs tests
// UX FIL-1a: the headerbar search renders as the first chip.
#[test]
fn fil_1a_search_chip_label_quotes_the_query() {
    assert_eq!(search_chip_label("falling"), "⌕ “falling” in any field");
    assert_eq!(remove_search_label("falling"), "Remove search: falling");
}

// UX FIL-2: the count is accented (bold markup) only under restriction.
#[test]
fn fil_2_count_markup_accents_only_when_restricted() {
    assert_eq!(
        result_count_markup(15, 1664),
        ("<b>15</b> of 1,664 tracks".to_string(), true)
    );
    assert_eq!(result_count_markup(1664, 1664), ("1,664 tracks".to_string(), false));
    assert_eq!(result_count_markup(1, 1), ("1 track".to_string(), false));
}
```

```rust
// in browse_bar.rs tests
// UX FIL-1a: chip order is search first, then the facet cascade.
#[test]
fn fil_1a_search_appears_as_chip_before_facet_chips() {
    let browse = BrowseFilter { genre: Some("Rock".into()), artist: None, album: None };
    let labels = chip_labels("falling", &browse);
    assert_eq!(labels, vec!["⌕ “falling” in any field".to_string(), "Genre: Rock".to_string()]);
    assert!(chip_labels("  ", &BrowseFilter::default()).is_empty());
}
```

- [ ] **Step 3: Run → red.** `cargo test -p reprise-gnome fil_1a fil_2` fails to compile.

- [ ] **Step 4: Implement strings** in `browse_filter_strings.rs` (same `N_!`/`formatted` conventions as the existing fns):

```rust
pub(in crate::ui) const CLEAR_ALL: &str = N_!("Clear all");

pub(in crate::ui) fn search_chip_label(query: &str) -> String {
    formatted(N_!("⌕ “{query}” in any field"), &[("query", query)])
}

pub(in crate::ui) fn remove_search_label(query: &str) -> String {
    formatted(N_!("Remove search: {query}"), &[("query", query)])
}

/// (markup, accented). Numbers are digits/commas only — markup-safe.
pub(in crate::ui) fn result_count_markup(filtered: usize, total: usize) -> (String, bool) {
    if filtered >= total {
        return (result_count(total, total), false);
    }
    let plain = result_count(filtered, total); // "15 of 1,664 tracks"
    let bold = plain.replacen(
        &reprise_core::format::format_thousands(filtered as i64),
        &format!("<b>{}</b>", reprise_core::format::format_thousands(filtered as i64)),
        1,
    );
    (bold, true)
}
```

- [ ] **Step 5: Implement the BrowseBar changes** in `browse_bar.rs`:
  - New fields: `search: RefCell<String>`, `track_source: Cell<bool>`, `is_library: Cell<bool>`, `section_label: gtk4::Label` (move the local into the struct), `clear_all: gtk4::Button`, `on_search_cleared: RefCell<Option<Rc<dyn Fn()>>>`, `on_clear_all: RefCell<Option<Rc<dyn Fn()>>>`.
  - `clear_all` construction in `new()`: `gtk4::Button::with_label(&format!("{} ×", filter_strings::text(filter_strings::CLEAR_ALL)))`, classes `"flat"` + `CHIP_CSS_CLASS`, appended to `root` after `result_label`, `set_visible(false)`, `connect_clicked` → invoke `on_clear_all`.
  - Pure chip model above the struct:

```rust
fn chip_labels(search: &str, filter: &BrowseFilter) -> Vec<String> {
    let mut labels = Vec::new();
    if !search.trim().is_empty() {
        labels.push(filter_strings::search_chip_label(search.trim()));
    }
    labels.extend(filter_chips(filter).into_iter().map(|chip| chip.label));
    labels
}
```

  - `rebuild_chips`: prepend a search-chip button when `!self.search.borrow().trim().is_empty()` (label `format!("{}  ×", filter_strings::search_chip_label(query))`, accessible label `remove_search_label`, click → invoke `on_search_cleared`); then the facet chips as today; append `add_filter` only when `self.is_library.get()`.
  - `sync_visibility` (replaces the old body):

```rust
fn sync_visibility(&self) {
    let restricted = super::filter_restriction::is_restricted(
        &self.search.borrow(),
        &self.filter.borrow(),
    );
    let visible = super::filter_restriction::row_visible(
        self.track_source.get(),
        restricted,
        self.preference_visible.get(),
    );
    self.root.set_visible(visible);
    self.section_label.set_visible(restricted); // FIL-2: no "FILTER" label when idle
    self.clear_all.set_visible(restricted);
    tracing::info!(visible, restricted, "filter row visibility updated");
}
```

  - `set_source_context(&self, source: &ViewSource)` sets both Cells via `filter_restriction::is_track_source(source)` / `matches!(source, ViewSource::Library)` then `sync_visibility()`. Keep `set_library_visible(visible)` as `self.track_source.set(visible); self.is_library.set(visible); self.sync_visibility();`.
  - `set_search(self: &Rc<Self>, text: &str)`: store, `self.refresh()`, `self.sync_visibility()`.
  - `set_result_count(filtered, total)`: use `result_count_markup` — `self.result_label.set_markup(&markup)`; toggle css class `"accent"` (libadwaita built-in accent text class; if unavailable in this adwaita version, add `.reprise-filter-count-accent { color: @accent_color; }` to `css()` and toggle that).
  - `result_count()` getter returning `self.result_count.get()`.
  - Call `sync_visibility()` also from `apply_filter`'s idle refresh (a facet change alters restriction).
- [ ] **Step 6: Run → green.** `cargo test -p reprise-gnome fil_1a fil_2 browse` all pass; also update the existing display-ignored widget test (`widget_projects_removable_chips_without_a_redundant_reset_button`) for the new child order (root children: section_label, chips, result_label, clear_all → count 4).

- [ ] **Step 7: Gates, then commit**

```bash
git add crates/reprise-gnome/src/ui/browse/
git commit -m "feat: filter row becomes quiet permanent header with search chip and clear all (FIL-1a/FIL-2 groundwork)"
```

---

### Task 3: Counts for every track source + reload rewiring

**Files:**
- Modify: `crates/reprise-gnome/src/ui/browse/browse_filter_count.rs`, `crates/reprise-gnome/src/ui/track_list/track_list_reload.rs`, `crates/reprise-gnome/src/ui/view_session.rs:135-142`

**Interfaces:**
- Consumes: `BrowseBar::{set_source_context, set_search, set_result_count, hide_result_count}` (Task 2), `filter_restriction` (Task 1).
- Produces: `pub(in crate::ui) fn update(bar, conn, source, count, search: &str, browse: &BrowseFilter, queue_ids: &[i64])` and DB-level `fn source_total(conn: &Connection, source: &ViewSource, restricted: bool, count: usize, queue_ids: &[i64]) -> Result<usize, rusqlite::Error>` (consumed by tests; totals surface to Tasks 8/9 via `BrowseBar::result_count()`).

- [ ] **Step 1: Write the failing DB tests** (in `browse_filter_count.rs`, in-memory SQLite, display-free):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::view_source::ViewSource;

    fn seeded_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (id,path,title,artist,album,added_at) VALUES
               (1,'/a.flac','Falling Apart','Caskets','X',0),
               (2,'/b.flac','Other','Dead by April','Y',0),
               (3,'/c.flac','Third','Z','Z',0);
             INSERT INTO playlists (id,name) VALUES (7,'P');
             INSERT INTO playlist_tracks (playlist_id,track_id,position) VALUES (7,1,1),(7,2,2);",
        )
        .unwrap();
        conn
    }

    // UX FIL-2: the total pairs the filtered count with the SOURCE's own
    // unfiltered size — a playlist restricted to 1 hit reads "1 of 2".
    #[test]
    fn fil_2_source_total_is_the_unfiltered_source_count() {
        let conn = seeded_conn();
        assert_eq!(source_total(&conn, &ViewSource::Playlist(7), true, 1, &[]).unwrap(), 2);
        assert_eq!(source_total(&conn, &ViewSource::Library, true, 1, &[]).unwrap(), 3);
    }

    // UX FIL-2: without restriction total == count (no second query).
    #[test]
    fn fil_2_source_total_equals_count_when_idle() {
        let conn = seeded_conn();
        assert_eq!(source_total(&conn, &ViewSource::Playlist(7), false, 2, &[]).unwrap(), 2);
    }

    // UX FIL-2: the queue total needs the live queue ids.
    #[test]
    fn fil_2_queue_total_counts_the_queue_snapshot() {
        let conn = seeded_conn();
        assert_eq!(source_total(&conn, &ViewSource::Queue, true, 1, &[1, 2, 3]).unwrap(), 3);
    }
}
```

- [ ] **Step 2: Run → red** (`source_total` missing).

- [ ] **Step 3: Implement.** Replace `update`'s body:

```rust
pub(in crate::ui) fn update(
    bar: &Rc<BrowseBar>,
    conn: &Rc<RefCell<Connection>>,
    source: &ViewSource,
    count: usize,
    search: &str,
    browse: &BrowseFilter,
    queue_ids: &[i64],
) {
    bar.set_source_context(source);
    bar.set_search(search);
    if !super::filter_restriction::is_track_source(source) {
        bar.hide_result_count();
        return;
    }
    let restricted = super::filter_restriction::is_restricted(search, browse);
    let total = {
        let conn = conn.borrow();
        source_total(&conn, source, restricted, count, queue_ids)
    };
    match total {
        Ok(total) => bar.set_result_count(count, total),
        Err(error) => {
            tracing::warn!(%error, "could not load total count for filter row");
            bar.hide_result_count();
        }
    }
}

fn source_total(
    conn: &Connection,
    source: &ViewSource,
    restricted: bool,
    count: usize,
    queue_ids: &[i64],
) -> Result<usize, rusqlite::Error> {
    if !restricted {
        return Ok(count);
    }
    queries::query_track_count_browsed(conn, source, "", &BrowseFilter::default(), queue_ids)
        .and_then(|value| {
            usize::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, value))
        })
}
```

  In `track_list_reload.rs::reload`, change the call to `browse_filter_count::update(&shared.browse_bar, &shared.conn, &source, count, &filter, &browse, &queue_ids);` and REMOVE the now-redundant `shared.browse_bar.set_library_visible(...)` from `set_source_and_reload` (update handles source context). In `view_session.rs::finish_track_source` replace `set_library_visible(matches!(source, ViewSource::Library))` with `set_source_context(&source)`. Finally delete the `set_library_visible` shim from `browse_bar.rs` (no call sites remain).

- [ ] **Step 4: Run → green.** `cargo test -p reprise-gnome fil_2 browse_filter_count`.

- [ ] **Step 5: Gates, then commit**

```bash
git add crates/reprise-gnome/src/ui/browse/ crates/reprise-gnome/src/ui/track_list/track_list_reload.rs crates/reprise-gnome/src/ui/view_session.rs
git commit -m "feat: pair filtered counts with per-source totals in every track source (FIL-2 groundwork)"
```

---

### Task 4: Clear-all plumbing + chip callbacks — flips FIL-1a

**Files:**
- Modify: `crates/reprise-gnome/src/ui/track_list/track_list.rs`, `crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs`, `docs/ux-rules.md` (FIL-1a → `[aktiv]`)

**Interfaces:**
- Consumes: `BrowseBar::{set_on_search_cleared, set_on_clear_all}` (Task 2).
- Produces: `pub fn TrackList::clear_all_restrictions(&self)` · `pub fn TrackList::set_on_search_cleared(&self, cb: impl Fn() + 'static)` / `set_on_clear_all` (forwarders to `shared.browse_bar`) · window action `win.clear-all-filters` (name is load-bearing for Tasks 8/9 buttons).

- [ ] **Step 1: Implement `TrackList` methods** (in `track_list.rs`, next to `set_filter`):

```rust
/// FIL-1a/FIL-6: one action resets search AND browse facets in a single
/// reload. The caller additionally clears the headerbar entry text; the
/// debounced search handler then early-returns (filter already empty).
pub fn clear_all_restrictions(&self) {
    *self.shared.browse_filter.borrow_mut() = reprise_core::queries::BrowseFilter::default();
    self.shared.browse_bar.restore_filter(&reprise_core::queries::BrowseFilter::default());
    set_filter_and_reload(&self.shared, "");
}

pub fn set_on_search_cleared(&self, callback: impl Fn() + 'static) {
    self.shared.browse_bar.set_on_search_cleared(callback);
}

pub fn set_on_clear_all(&self, callback: impl Fn() + 'static) {
    self.shared.browse_bar.set_on_clear_all(callback);
}
```

- [ ] **Step 2: Wire the window action and chip callbacks** in `window_runtime_wiring.rs` (after the existing `nav-back` action block, following the same `SimpleAction` pattern; order matters — set track-list filter BEFORE clearing entry text so the debounce early-returns):

```rust
let clear_all = gtk4::gio::SimpleAction::new("clear-all-filters", None);
{
    let track_list = track_list.clone();
    let search_entry = search_entry.clone();
    clear_all.connect_activate(move |_, _| {
        track_list.clear_all_restrictions();
        search_entry.set_text(""); // cascades to the album grid filter too
    });
}
window.add_action(&clear_all);
```

  Then the chip callbacks:

```rust
{
    let inner = track_list.clone();
    let entry = search_entry.clone();
    track_list.set_on_search_cleared(move || {
        inner.set_filter("");
        entry.set_text("");
    });
}
{
    let window = window.clone();
    track_list.set_on_clear_all(move || {
        gtk4::prelude::ActionGroupExt::activate_action(&window, "clear-all-filters", None);
    });
}
```

- [ ] **Step 3: Display-level verification (non-rule-named, ignored).** Add to `track_list.rs` tests:

```rust
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn clear_all_restrictions_resets_search_and_browse_in_one_pass() {
    gtk4::init().unwrap();
    let conn = Connection::open_in_memory().unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    conn.execute_batch(
        "INSERT INTO tracks (path,title,artist,album,genre,added_at) VALUES
           ('/a.flac','Falling Apart','Caskets','X','Metal',0),
           ('/b.flac','Other','Dead by April','Y','Rock',0);",
    )
    .unwrap();
    let track_list = TrackList::new(
        Rc::new(RefCell::new(conn)),
        Box::new(|_ids, _pos, _source| {}),
        |_, _, _, _| {},
        || crate::ui::track_list::queue_sections::QueueViewModel::default(),
        crate::ui::cover_download_worker::setup(),
    );
    track_list.set_filter("falling");
    *track_list.shared.browse_filter.borrow_mut() = BrowseFilter {
        genre: Some("Metal".into()),
        artist: None,
        album: None,
    };
    track_list.reload();

    track_list.clear_all_restrictions();
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
    assert_eq!(track_list.shared.filter.borrow().as_str(), "");
    assert_eq!(*track_list.shared.browse_filter.borrow(), BrowseFilter::default());
    assert_eq!(track_list.shared.browse_bar.result_count(), Some((2, 2)));
}
```

  (Adjust the constructor argument shapes to the actual `OnActivate` / `QueueViewModel` definitions in `track_list_callbacks.rs` / `queue_sections.rs` — the assertions are the contract.)

- [ ] **Step 4: Flip FIL-1a.** In `docs/ux-rules.md` change `**FIL-1a** [geplant] [gtk]` → `**FIL-1a** [aktiv] [gtk]` (coverage: `fil_1a_*` tests from Tasks 1/2). Run `scripts/check-ux-traceability.sh` → ok.

- [ ] **Step 5: Gates, then commit**

```bash
git add crates/reprise-gnome/src/ui/track_list/track_list.rs crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs docs/ux-rules.md
git commit -m "feat: one-click clear-all for search and facets, search chip removal (FIL-1a aktiv)"
```

---

### Task 5: Status overlay goes permanently neutral

**Files:**
- Modify: `crates/reprise-gnome/src/ui/status_bar.rs`, `crates/reprise-gnome/src/ui/window/window.rs:283-289`, `crates/reprise-gnome/src/ui/strings.rs` (status block)

**Interfaces:**
- Produces: `pub fn StatusBar::refresh(&self, conn: &Rc<RefCell<Connection>>)` (drops `filter`/`browse` params). `refresh_for_source_count` unchanged.

- [ ] **Step 1: Write the failing test** (in `status_bar.rs` tests):

```rust
// UX FIL-2: the status overlay always describes the whole library — the
// "X of Y" variant is gone; the filter row owns restriction state.
#[test]
fn fil_2_status_line_copy_is_always_neutral() {
    let text = format_status_text(1704, 4 * 24 * 3_600_000 + 6 * 3_600_000);
    assert!(text.starts_with("1,704 tracks"));
    assert!(!text.contains(" of "));
}
```

- [ ] **Step 2: Run → red** (signature mismatch).

- [ ] **Step 3: Implement.** `format_status_text(track_count: i64, total_duration_ms: i64) -> String` (delete the `filtered_count` parameter and its branch). `refresh` body queries neutrally:

```rust
pub fn refresh(&self, conn: &Rc<RefCell<Connection>>) {
    if !self.enabled.get() {
        return;
    }
    let stats = {
        let conn = conn.borrow();
        queries::query_library_stats_browsed(&conn, "", &BrowseFilter::default())
    };
    // match arms unchanged, but call format_status_text(stats.track_count, stats.total_duration_ms)
}
```

  In `window.rs` the `on_reload` closure becomes:

```rust
move |source, count, _filter, _browse| {
    if matches!(source, ViewSource::Library) {
        status_bar.refresh(&conn_for_status);
    } else {
        status_bar.refresh_for_source_count(count as i64);
    }
},
```

  Delete `strings::status_filtered_of_total` (now unused; keep the neutral status strings). Update any existing `status_bar` module tests to the new signature.

- [ ] **Step 4: Run → green.** `cargo test -p reprise-gnome status`.

- [ ] **Step 5: Gates, then commit**

```bash
git add crates/reprise-gnome/src/ui/status_bar.rs crates/reprise-gnome/src/ui/window/window.rs crates/reprise-gnome/src/ui/strings.rs
git commit -m "feat: status overlay always shows neutral library stats (FIL-2 groundwork)"
```

---

### Task 6: Search entry carries its state — flips FIL-4

**Files:**
- Modify: `crates/reprise-gnome/src/ui/window/library_chrome.rs`, `crates/reprise-gnome/src/ui/style/interactions.rs`, `docs/ux-rules.md` (FIL-4 → `[aktiv]`)

**Interfaces:**
- Produces: `pub(in crate::ui) const SEARCH_ACTIVE_CLASS: &str = "reprise-search-active"` and `pub(in crate::ui) fn search_accent_active(text: &str) -> bool` in `library_chrome.rs`.

- [ ] **Step 1: Write the failing tests:**

```rust
// in library_chrome.rs tests
// UX FIL-4: the field is marked as soon as it carries real text — also
// unfocused; whitespace-only never claims state (mirrors is_restricted).
#[test]
fn fil_4_search_accent_tracks_trimmed_text() {
    assert!(search_accent_active("falling"));
    assert!(!search_accent_active(""));
    assert!(!search_accent_active("   "));
}
```

```rust
// in style/interactions.rs tests
// UX FIL-4: the accent styling for a non-empty search field is part of the
// installed app stylesheet.
#[test]
fn fil_4_css_defines_the_active_search_class() {
    assert!(css().contains(".reprise-search-active"));
    assert!(css().contains("@accent_color"));
}
```

- [ ] **Step 2: Run → red.**

- [ ] **Step 3: Implement.** In `library_chrome.rs`:

```rust
pub(in crate::ui) const SEARCH_ACTIVE_CLASS: &str = "reprise-search-active";

pub(in crate::ui) fn search_accent_active(text: &str) -> bool {
    !text.trim().is_empty()
}

pub(in crate::ui) fn style_header(header: &adw::HeaderBar, search: &gtk4::SearchEntry) {
    header.set_centering_policy(adw::CenteringPolicy::Strict);
    search.set_width_request(SEARCH_WIDTH);
    search.set_hexpand(false);
    search.connect_search_changed(|entry| {
        if search_accent_active(&entry.text()) {
            entry.add_css_class(SEARCH_ACTIVE_CLASS);
        } else {
            entry.remove_css_class(SEARCH_ACTIVE_CLASS);
        }
    });
}
```

  (Session restore sets the entry text through `view_session::restore`, which fires `search_changed` → the class follows automatically.)

  In `style/interactions.rs` append to the existing `css()` format string (tokens already imported there):

```rust
".reprise-search-active { \
   border: 1px solid alpha(@accent_color, 0.5); \
   background-color: alpha(@accent_bg_color, 0.16); }\n"
```

  (Plain string concat — no token interpolation needed. The interactions section already contributes a marker to `style/mod.rs`'s marker test; adding rules to it needs no marker change.)

- [ ] **Step 4: Run → green**, flip `**FIL-4**` to `[aktiv]` in `docs/ux-rules.md`, `scripts/check-ux-traceability.sh` → ok.

- [ ] **Step 5: Gates, then commit**

```bash
git add crates/reprise-gnome/src/ui/window/library_chrome.rs crates/reprise-gnome/src/ui/style/interactions.rs docs/ux-rules.md
git commit -m "feat: accent the search entry whenever it carries text (FIL-4 aktiv)"
```

---

### Task 7: Match highlighting in the four searched columns — flips FIL-5

**Files:**
- Create: `crates/reprise-gnome/src/ui/track_list/match_highlight.rs`
- Modify: `crates/reprise-gnome/src/ui/track_list/track_list_columns.rs` (both bind closures), `crates/reprise-gnome/src/ui/track_list/mod.rs`, `docs/ux-rules.md` (FIL-5 → `[aktiv]`)

**Interfaces:**
- Produces: `pub(in crate::ui) fn highlight_markup(text: &str, needle: &str, foreground: Option<&str>) -> Option<String>` and `pub(in crate::ui) fn accent_foreground(widget: &impl IsA<gtk4::Widget>) -> Option<String>`.

- [ ] **Step 1: Write the failing tests** (in `match_highlight.rs`):

```rust
// UX FIL-5: matching mirrors SQLite LIKE — ASCII-case-insensitive substring.
#[test]
fn fil_5_highlight_matches_are_ascii_case_insensitive() {
    assert_eq!(
        highlight_markup("Falling Apart", "falling", None),
        Some("<b>Falling</b> Apart".to_string())
    );
}

// UX FIL-5: every occurrence is highlighted, not only the first.
#[test]
fn fil_5_all_occurrences_are_highlighted() {
    assert_eq!(
        highlight_markup("la la", "la", None),
        Some("<b>la</b> <b>la</b>".to_string())
    );
}

// UX FIL-5: cell text is Pango-escaped — markup metacharacters stay literal.
#[test]
fn fil_5_highlight_escapes_pango_markup() {
    assert_eq!(
        highlight_markup("Rock & <Roll>", "rock", None),
        Some("<b>Rock</b> &amp; &lt;Roll&gt;".to_string())
    );
}

// UX FIL-5: no needle or no match → no markup (caller uses set_text).
#[test]
fn fil_5_no_markup_when_needle_empty_or_absent() {
    assert_eq!(highlight_markup("Falling", "  ", None), None);
    assert_eq!(highlight_markup("Falling", "xyz", None), None);
}

// UX FIL-5: with a resolved accent, matches are accent bold.
#[test]
fn fil_5_accent_color_wraps_the_match() {
    assert_eq!(
        highlight_markup("Falling", "fall", Some("#2ec8a6")),
        Some("<span foreground=\"#2ec8a6\" weight=\"bold\">Fall</span>ing".to_string())
    );
}
```

- [ ] **Step 2: Run → red.**

- [ ] **Step 3: Implement** `match_highlight.rs`:

```rust
//! FIL-5: accent-bold highlighting of the live-search needle inside cell
//! text. Matching is ASCII-case-insensitive on purpose — it mirrors the
//! SQLite LIKE semantics of the search query (queries/clauses.rs), so a
//! highlighted row and a matching row are the same set.

use gtk4::glib;
use gtk4::prelude::*;

pub(in crate::ui) fn highlight_markup(
    text: &str,
    needle: &str,
    foreground: Option<&str>,
) -> Option<String> {
    let needle = needle.trim();
    if needle.is_empty() {
        return None;
    }
    let hay = text.to_ascii_lowercase();
    let ndl = needle.to_ascii_lowercase();
    let (open, close) = match foreground {
        Some(hex) => (format!("<span foreground=\"{hex}\" weight=\"bold\">"), "</span>"),
        None => ("<b>".to_string(), "</b>"),
    };
    let mut out = String::new();
    let mut cursor = 0usize;
    let mut found = false;
    while let Some(pos) = hay[cursor..].find(&ndl) {
        let start = cursor + pos;
        let end = start + ndl.len();
        out.push_str(&glib::markup_escape_text(&text[cursor..start]));
        out.push_str(&open);
        out.push_str(&glib::markup_escape_text(&text[start..end]));
        out.push_str(close);
        cursor = end;
        found = true;
    }
    if !found {
        return None;
    }
    out.push_str(&glib::markup_escape_text(&text[cursor..]));
    Some(out)
}

/// Resolves the theme accent for markup at bind time. `lookup_color` is
/// deprecated upstream but is the only theme-correct way to get a literal
/// color into Pango markup; scoped allow with this rationale.
#[allow(deprecated)]
pub(in crate::ui) fn accent_foreground(widget: &impl IsA<gtk4::Widget>) -> Option<String> {
    let (found, rgba) = gtk4::prelude::StyleContextExt::lookup_color(
        &widget.as_ref().style_context(),
        "accent_color",
    );
    found.then(|| {
        format!(
            "#{:02x}{:02x}{:02x}",
            (rgba.red() * 255.0) as u8,
            (rgba.green() * 255.0) as u8,
            (rgba.blue() * 255.0) as u8
        )
    })
}
```

  (Byte indices transfer between `text` and `hay` because `to_ascii_lowercase` maps bytes 1:1. If the gtk4-rs `lookup_color` signature differs — it may return `Option<gdk::RGBA>` — adapt the destructuring; the fallback path `None → <b>` keeps everything working.)

- [ ] **Step 4: Wire into the bind closures** in `track_list_columns.rs`. In `append_column`'s `connect_bind` (currently `label.set_text(&render(&track))`):

```rust
let raw = render(&track);
let needle = shared_for_bind.filter.borrow().clone();
match super::match_highlight::highlight_markup(
    &raw,
    &needle,
    super::match_highlight::accent_foreground(&label).as_deref(),
) {
    Some(markup) => label.set_markup(&markup),
    None => label.set_text(&raw),
}
```

  Same replacement in `append_title_column`'s bind for `label.set_text(&track.title)` (title keeps its ellipsize — Pango ellipsizes attributed text fine). This covers title, artist, album AND genre — all four searched columns — because artist/album/genre all render through `append_column`.

- [ ] **Step 5: Run → green.** `cargo test -p reprise-gnome fil_5`. Flip `**FIL-5**` to `[aktiv]`, traceability ok.

- [ ] **Step 6: Gates, then commit**

```bash
git add crates/reprise-gnome/src/ui/track_list/ docs/ux-rules.md
git commit -m "feat: highlight search matches in title, artist, album and genre cells (FIL-5 aktiv)"
```

---

### Task 8: End-of-results line — flips FIL-3

**Files:**
- Create: `crates/reprise-gnome/src/ui/track_list/end_of_results.rs`
- Modify: `crates/reprise-gnome/src/ui/track_list/track_list_builder.rs` (wrap the scrolled page in an Overlay + install), `crates/reprise-gnome/src/ui/track_list/mod.rs`, `crates/reprise-gnome/src/ui/strings.rs` (end-of-results block), `docs/ux-rules.md` (FIL-3 → `[aktiv]`)

**Interfaces:**
- Consumes: `BrowseBar::result_count()` (Task 2), `filter_restriction::is_restricted` (Task 1), action name `win.clear-all-filters` (Task 4 — string only, no compile dependency).
- Produces: `pub(in crate::ui) fn end_line_margin(content_height: f64, viewport_height: f64, scroll_value: f64) -> Option<i32>` · `pub(in crate::ui) fn install(shared: &Rc<Shared>, overlay: &gtk4::Overlay, scrolled: &gtk4::ScrolledWindow)` · strings `end_of_results_hidden_by_search(hidden, query)`, `end_of_results_hidden_by_filters(hidden)`, `end_of_results_hidden_by_both(hidden)`, `show_all_tracks_label(total)` (reused by Task 9).

- [ ] **Step 1: Write the failing tests** (in `end_of_results.rs`):

```rust
// UX FIL-3: with a short list the line sits directly under the last row,
// never at the viewport bottom (grilled acceptance case).
#[test]
fn fil_3_line_sits_under_the_last_row_of_a_short_list() {
    assert_eq!(end_line_margin(300.0, 800.0, 0.0), Some(300));
}

// UX FIL-3: with a long list the line only exists once the list end
// scrolls into the viewport.
#[test]
fn fil_3_line_appears_only_when_the_end_scrolls_into_view() {
    assert_eq!(end_line_margin(5000.0, 800.0, 3000.0), None);
    assert_eq!(end_line_margin(5000.0, 800.0, 4300.0), Some(700));
    assert_eq!(end_line_margin(5000.0, 800.0, 4200.0), Some(800));
}

// UX FIL-3: degenerate geometry never yields a position.
#[test]
fn fil_3_no_line_without_geometry() {
    assert_eq!(end_line_margin(0.0, 800.0, 0.0), None);
    assert_eq!(end_line_margin(300.0, 0.0, 0.0), None);
}
```

  And in `strings.rs` tests (or a small test mod near the new block):

```rust
// UX FIL-3: the copy counts the hidden tracks and names the search.
#[test]
fn fil_3_hidden_copy_counts_and_names_the_search() {
    assert_eq!(
        end_of_results_hidden_by_search("1,649", "falling"),
        "End of results — 1,649 tracks hidden by search “falling”"
    );
    assert_eq!(show_all_tracks_label("1,664"), "Show all 1,664 tracks");
}
```

- [ ] **Step 2: Run → red.**

- [ ] **Step 3: Implement the pure parts.** In `end_of_results.rs`:

```rust
//! FIL-3: the end-of-results line. An overlay positioned from the
//! ColumnView's measured content height — NOT a widget inside the
//! ScrolledWindow (that would defeat row virtualization) and NOT a sticky
//! bar (it only exists where the list actually ends).

pub(in crate::ui) fn end_line_margin(
    content_height: f64,
    viewport_height: f64,
    scroll_value: f64,
) -> Option<i32> {
    if content_height <= 0.0 || viewport_height <= 0.0 {
        return None;
    }
    let end_in_viewport = content_height - scroll_value;
    if end_in_viewport > viewport_height {
        return None;
    }
    Some(end_in_viewport.max(0.0) as i32)
}
```

  In `strings.rs` add a `// End-of-results line (src/ui/track_list/end_of_results.rs).` block:

```rust
pub fn end_of_results_hidden_by_search(hidden: &str, query: &str) -> String {
    formatted(
        N_!("End of results — {hidden} tracks hidden by search “{query}”"),
        &[("hidden", hidden), ("query", query)],
    )
}

pub fn end_of_results_hidden_by_filters(hidden: &str) -> String {
    formatted(N_!("End of results — {hidden} tracks hidden by active filters"), &[("hidden", hidden)])
}

pub fn end_of_results_hidden_by_both(hidden: &str) -> String {
    formatted(
        N_!("End of results — {hidden} tracks hidden by search and filters"),
        &[("hidden", hidden)],
    )
}

pub fn show_all_tracks_label(total: &str) -> String {
    formatted(N_!("Show all {total} tracks"), &[("total", total)])
}
```

- [ ] **Step 4: Build + install the overlay.** In `track_list_builder.rs` wrap the list page:

```rust
let list_overlay = gtk4::Overlay::new();
list_overlay.set_child(Some(&scrolled));
// stack page swap:
stack.add_named(&list_overlay, Some(STACK_PAGE_LIST));
```

  In `end_of_results.rs`, `install(shared, &list_overlay, &scrolled)` (called from the builder tail, after `Shared` exists):
  - Build `line: gtk4::Label` (classes `dim-label`, `caption`, `halign Center`) and `pill: gtk4::Button` (class `pill`, `halign Center`, `set_action_name(Some("win.clear-all-filters"))`). Wrap the label in a `gtk4::Box` (`valign Start`, `halign Fill`) with `set_can_target(false)` — scroll events over the line must reach the list; the pill is a SEPARATE overlay child (`valign Start`, `halign Center`, `can_target` default true) so it stays clickable. `overlay.add_overlay(&line_box); overlay.add_overlay(&pill);` Both start `set_visible(false)`.
  - `fn recompute(shared, scrolled, line_box, line, pill)`:

```rust
let source = shared.source.borrow().clone();
let browse = if matches!(source, reprise_core::view_source::ViewSource::Library) {
    shared.browse_filter.borrow().clone()
} else {
    reprise_core::queries::BrowseFilter::default()
};
let search = shared.filter.borrow().clone();
let restricted = crate::ui::browse::filter_restriction::is_restricted(&search, &browse);
let counts = shared.browse_bar.result_count();
let filtered = shared.model.n_items() as usize;
let Some((_, total)) = counts.filter(|_| restricted && filtered >= 1) else {
    line_box.set_visible(false);
    pill.set_visible(false);
    return;
};
let hidden = total.saturating_sub(filtered);
if hidden == 0 {
    line_box.set_visible(false);
    pill.set_visible(false);
    return;
}
let vadj = scrolled.vadjustment();
let (_, natural) = shared.column_view.preferred_size();
let margin = end_line_margin(natural.height() as f64, vadj.page_size(), vadj.value());
match margin {
    Some(margin) => {
        let hidden_str = reprise_core::format::format_thousands(hidden as i64);
        let query = search.trim().to_string();
        let text = match (query.is_empty(), browse.is_empty()) {
            (false, true) => crate::ui::strings::end_of_results_hidden_by_search(&hidden_str, &query),
            (true, false) => crate::ui::strings::end_of_results_hidden_by_filters(&hidden_str),
            _ => crate::ui::strings::end_of_results_hidden_by_both(&hidden_str),
        };
        line.set_text(&text);
        pill.set_label(&crate::ui::strings::show_all_tracks_label(
            &reprise_core::format::format_thousands(total as i64),
        ));
        line_box.set_margin_top(margin + 12);
        pill.set_margin_top(margin + 44);
        line_box.set_visible(true);
        pill.set_visible(true);
    }
    None => {
        line_box.set_visible(false);
        pill.set_visible(false);
    }
}
```

  - Recompute triggers (all call the same closure, weak-`Shared`): `vadj.connect_value_changed`, `vadj.connect_changed` (covers resize + content growth), and `shared.selection.connect_items_changed` wrapped in `glib::idle_add_local_once` (model repopulation needs an allocation pass before `preferred_size` is meaningful).

- [ ] **Step 5: Run → green** (`cargo test -p reprise-gnome fil_3`), flip `**FIL-3**` to `[aktiv]`, traceability ok.

- [ ] **Step 6: Gates, then commit**

```bash
git add crates/reprise-gnome/src/ui/track_list/ crates/reprise-gnome/src/ui/strings.rs docs/ux-rules.md
git commit -m "feat: end-of-results line explains hidden tracks at the list end (FIL-3 aktiv)"
```

---

### Task 9: Zero-hits empty state gets its one guaranteed step — flips FIL-6

**Files:**
- Modify: `crates/reprise-gnome/src/ui/track_list/track_list_empty_state.rs`, `crates/reprise-gnome/src/ui/track_list/track_list_builder.rs` (Shared fields), `crates/reprise-gnome/src/ui/track_list/track_list.rs` (`set_empty_scan_widget` seam + Shared struct), `docs/ux-rules.md` (FIL-6 → `[aktiv]`)

**Interfaces:**
- Consumes: `strings::show_all_tracks_label` (Task 8), `BrowseBar::result_count()` (Task 2/3 — `update` runs BEFORE `apply_empty_state` in `reload`, so counts are fresh), action `win.clear-all-filters` (Task 4).
- Produces: Shared fields `show_all_button: gtk4::Button`, `empty_scan_widget: RefCell<Option<gtk4::Widget>>`.

- [ ] **Step 1: Write the failing tests** (in `track_list_empty_state.rs`):

```rust
// UX FIL-6: zero hits under restriction is the NoResults state in every
// source — the state that carries the single "Show all" action.
#[test]
fn fil_6_zero_hits_with_restriction_selects_no_results_state() {
    assert_eq!(empty_state_for(0, true, &ViewSource::Library), EmptyState::NoResults);
    assert_eq!(empty_state_for(0, true, &ViewSource::Playlist(7)), EmptyState::NoResults);
    assert_eq!(empty_state_for(0, true, &ViewSource::Queue), EmptyState::NoResults);
}

// UX FIL-6: the action names the outcome with the full count.
#[test]
fn fil_6_show_all_action_names_the_full_count() {
    assert_eq!(show_all_action_label(Some((0, 1664))), Some("Show all 1,664 tracks".to_string()));
    assert_eq!(show_all_action_label(None), None);
}
```

- [ ] **Step 2: Run → red** (`show_all_action_label` missing).

- [ ] **Step 3: Implement.**
  - Pure fn in `track_list_empty_state.rs`:

```rust
/// FIL-6: label for the single next step, None when no count is known
/// (then the button is hidden rather than lying).
pub(in crate::ui) fn show_all_action_label(counts: Option<(usize, usize)>) -> Option<String> {
    counts.map(|(_, total)| {
        crate::ui::strings::show_all_tracks_label(&reprise_core::format::format_thousands(
            total as i64,
        ))
    })
}
```

  - Shared fields (declare in `track_list.rs`, init in `track_list_builder.rs`):

```rust
pub(in crate::ui) show_all_button: gtk4::Button,
pub(in crate::ui) empty_scan_widget: RefCell<Option<gtk4::Widget>>,
```

  builder init:

```rust
let show_all_button = gtk4::Button::new();
show_all_button.add_css_class("pill");
show_all_button.set_halign(gtk4::Align::Center);
show_all_button.set_action_name(Some("win.clear-all-filters"));
```

  - `apply_empty_state` child management: in the `NoResults` arm, before flipping the stack:

```rust
match show_all_action_label(shared.browse_bar.result_count()) {
    Some(label) => {
        shared.show_all_button.set_label(&label);
        shared.empty_page.set_child(Some(&shared.show_all_button));
    }
    None => shared.empty_page.set_child(gtk4::Widget::NONE),
}
```

  In the `EmptyLibrary` arm restore the scan widget: `shared.empty_page.set_child(shared.empty_scan_widget.borrow().as_ref());` — and in `EmptyQueue`/`NothingHere` arms `shared.empty_page.set_child(gtk4::Widget::NONE);`. Change `TrackList::set_empty_scan_widget` to store the widget into `shared.empty_scan_widget` AND set it as child (preserves current first-run behavior, EmptyLibrary is the state shown then).

- [ ] **Step 4: Run → green** (`cargo test -p reprise-gnome fil_6 empty_state`), flip `**FIL-6**` to `[aktiv]`, traceability ok.

- [ ] **Step 5: Gates, then commit**

```bash
git add crates/reprise-gnome/src/ui/track_list/ docs/ux-rules.md
git commit -m "feat: zero-hit empty state offers one guaranteed step back to content (FIL-6 aktiv)"
```

---

### Task 10: FIL-2 flip, full gates, headless acceptance

**Files:**
- Modify: `docs/ux-rules.md` (FIL-2 → `[aktiv]`)

- [ ] **Step 1: Confirm FIL-2 coverage exists un-ignored** (from Tasks 1/2/3/5): `grep -rn "fn fil_2_" crates/ | wc -l` ≥ 6.
- [ ] **Step 2: Flip `**FIL-2**` to `[aktiv]`** in `docs/ux-rules.md`. Run `scripts/check-ux-traceability.sh` → "UX traceability ok".
- [ ] **Step 3: Full merge gate:** `scripts/check-merge-readiness.sh --no-fetch` → all green.
- [ ] **Step 4: Display suite:** `scripts/check-display-tests.sh` → the new/updated display-ignored tests pass under Xvfb.
- [ ] **Step 5: Headless acceptance walkthrough** (grilled acceptance criteria; NEVER open a window on the user's desktop — Xvfb only, per TESTING.md's env pattern):
  - `REPRISE_SMOKE_VIEW_SESSION='library|falling|~|~|~|title|asc' xvfb-run -a <app>` → tracing shows the restored search, the filter row visible with the search chip, accented count, and `browse smoke`-style `result_count` pairing filtered with total.
  - Repeat with `playlist:<id>` as source → chip row present in the playlist, count pairs against the playlist total.
  - `REPRISE_SMOKE_FILTER=zzz-no-hits` → NoResults StatusPage with a single "Show all N tracks" child.
  - Verify no `[geplant]`-referencing `#[ignore]` remains on FIL tests.
- [ ] **Step 6: Commit**

```bash
git add docs/ux-rules.md
git commit -m "docs(ux): flip FIL-2 to aktiv — filter row state complete"
```

---

## Self-review notes (already applied)

- FIL-1b (Albums/Artists chips) deliberately stays `[geplant]` — out of scope, documented gap.
- NAV-6 untouched (Esc two-stage lives in shortcuts.rs; the search chip's × goes through `set_on_search_cleared`, not through stop-search).
- `browse_filter_count::update` runs before `apply_empty_state` in `reload` — Task 9's count read is ordered correctly.
- Task 2 keeps `set_library_visible` as a shim so Wave 2 compiles standalone; Task 3 removes it with the call sites.
- All rule-named tests introduced here are display-free (pure fns or in-memory SQLite); widget-level checks are non-rule-named and display-ignored.
