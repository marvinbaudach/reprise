# Place Pill vs Filter Pill Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Model policy (user token strategy):** dispatch every implementation subagent with `model: sonnet`, effort `high`. The orchestrating session only reviews between tasks.

**Goal:** A location (artist/album/genre page) and a filter (search/facet/Hide AI) stop sharing one shape — the location becomes an outlined place pill with a back affordance and place-relative counting, filters keep their filled chips with `×`.

**Architecture:** `filter_restriction.rs` stays the single pure decision module; its scope vocabulary becomes place vocabulary and stops reporting places as restrictions. `browse_bar.rs` grows a two-zone layout (place zone | filter zone); its chip construction moves to a new `browse_bar_chips.rs` first, because the file sits at 788 of the allowed 800 lines. `browse_filter_count.rs` loses the branch that substituted the whole library as the counting base. Everything downstream (`end_of_results.rs`) reads those two modules and follows without its own logic.

**Tech Stack:** Rust, gtk4-rs (GTK 4.22), libadwaita, rusqlite. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-07-31-place-pill-vs-filter-pill-design.md`

## Global Constraints

- Gates before EVERY commit: `cargo fmt --check` · `cargo clippy --locked --all-targets --workspace -- -D warnings` · `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace` · `scripts/check-ux-traceability.sh` · `scripts/check-architecture.sh`.
- ux-rules.md status flips happen IN the implementation commit of the rule — flip only when the rule is FULLY implemented.
- Rule-named tests: `fn fil_<nr><suffix>_…` with exactly ONE primary rule ID, `#[test]` directly above the fn (≤ 5 lines), a `// UX FIL-…:` comment ABOVE the `#[test]` attribute, never between attribute and fn. Rule-named tests for `[active]` rules must be display-free (no `gtk4::init`) so they run un-ignored in the workspace suite.
- Widget-level verifications that need a display are NON-rule-named tests with `#[ignore = "requires a display; run via xvfb-run"]`, run via `scripts/check-display-tests.sh`. The display suite is herd-flaky — **only single-test runs count as evidence**, never a batch result.
- Files < 800 lines. `browse_bar.rs` is at 788 — Task 3 splits it before anything is added.
- All user-visible copy via gettext `N_!` constants/functions (English, typographic quotes “ ” U+201C/U+201D). Comments and identifiers English.
- Immutability: helpers return new `BrowseFilter`/`String` values, never mutate in place. Never hold a `RefCell::borrow()` across a GTK call.
- One commit per task, no attribution footer, no push.
- `reprise-core` stays untouched — the counting change is verified to need no core work.

## Rebase dependency

The parallel branch `fix/scope-chip-survives-sidebar-refresh` (worktree `../reprise-scope-chip`) fixes the bug where any queue mutation throws the user out of an artist page. **It lands first; this branch rebases onto it before Task 1.** Overlap is limited to `window/metadata_navigation.rs`, and there only in tests.

Verify before starting:

```bash
git -C /home/marvin/Projects/reprise fetch origin dev
git -C /home/marvin/Projects/reprise log --oneline origin/dev -5 | rg -i "sidebar|scope"
```

If the fix is not on `origin/dev` yet, stop and report — building the pill while it still vanishes on play makes every manual verification lie.

## File Structure

| File | Responsibility after this plan |
|---|---|
| `browse/filter_restriction.rs` | pure law: what restricts, what carries a place pill, when the row shows |
| `browse/browse_bar_chips.rs` (new) | builds filter chips and the place pill widget |
| `browse/browse_bar.rs` | bar state, two-zone layout, wiring |
| `browse/browse_filter_count.rs` | counting, always relative to the current place |
| `browse/browse_filter_strings.rs` | place/filter copy |
| `track_list/end_of_results.rs` | end-of-results line, follows the two modules above |

## Parallel Execution Map (file ownership per wave)

| Wave | Tasks (parallel) | Files owned (disjoint within wave) |
|---|---|---|
| 1 | Task 1 | `browse/filter_restriction.rs`, `browse/browse_filter_strings.rs` |
| 1 | Task 3 | `browse/browse_bar.rs`, `browse/browse_bar_chips.rs`, `browse/mod.rs` |
| 2 | Task 2 | `browse/browse_filter_count.rs` |
| 2 | Task 4 | `browse/browse_bar.rs`, `browse/browse_bar_chips.rs`, `window/library_shell.rs` |
| 3 | Task 5 | `track_list/end_of_results.rs` |
| 4 | Task 6 | `docs/ux-rules.md` |
| 4 | Task 7 | verification only, no source files |

Task 1 and Task 3 touch disjoint files and can run at the same time. Task 2 and Task 4 both consume Task 1's renamed functions, so wave 2 starts only after wave 1 is merged.

---

### Task 1: Place vocabulary and the visibility law

`filter_restriction.rs` decides today that an artist page "restricts". That single boolean is why the row says `FILTER` at a place with no filter set, why the counter reads `3 of 9`, and why `end_of_results` has to special-case scopes away again. This task turns "scope restricts" into "place carries a pill" and leaves restriction to actual filters.

**Files:**
- Modify: `crates/reprise-gnome/src/ui/browse/filter_restriction.rs`
- Modify: `crates/reprise-gnome/src/ui/browse/browse_filter_strings.rs:57-59`

**Interfaces:**
- Produces:
  - `pub(in crate::ui) fn has_place_pill(source: &ViewSource) -> bool` — true for `Artist`/`Album`/`Genre` only.
  - `pub(in crate::ui) fn place_pill_label(source: &ViewSource) -> Option<String>` — the bare place name, no decoration (`"Alpha Artist"`, `"Pain Remains — Lorna Shore"`).
  - `pub(in crate::ui) fn is_restricted(search: &str, browse: &BrowseFilter, exclude_ai: bool) -> bool` — **source parameter dropped**.
  - `pub(in crate::ui) fn row_visible(is_track_source: bool, restricted: bool, has_place_pill: bool, preference_visible: bool) -> bool`
  - `filter_strings::leave_place_label(place: &str) -> String` — replaces `remove_scope_label`.
- Removed: `scope_restricts`, `scope_chip_label`, the old 4-arg `is_restricted`, the old 3-arg `row_visible`.

- [ ] **Step 1: Write the failing tests**

Replace the tests `fil_1c_artist_scope_restricts_and_renders_a_chip`, `fil_1c_genre_scope_restricts_and_renders_its_own_chip`, `fil_1c_playlist_and_queue_carry_no_scope_chip` and `fil_8_recently_added_is_a_removable_library_scope_chip` in `filter_restriction.rs` with:

```rust
    // UX FIL-1c: a place carries a pill but is not a filter — it never turns
    // the row into the restricted state on its own.
    #[test]
    fn fil_1c_places_carry_a_pill_without_restricting() {
        let artist = ViewSource::Artist("Lorna Shore".into());
        assert!(has_place_pill(&artist));
        assert_eq!(place_pill_label(&artist).as_deref(), Some("Lorna Shore"));
        assert!(!is_restricted("", &BrowseFilter::default(), false));
    }

    // UX FIL-1c: album places name album and artist; genre places name the genre.
    #[test]
    fn fil_1c_album_and_genre_places_label_themselves() {
        let album = ViewSource::Album {
            album: "Pain Remains".into(),
            album_artist: "Lorna Shore".into(),
        };
        assert_eq!(
            place_pill_label(&album).as_deref(),
            Some("Pain Remains — Lorna Shore")
        );
        let genre = ViewSource::Genre("Metalcore".into());
        assert_eq!(place_pill_label(&genre).as_deref(), Some("Metalcore"));
    }

    // UX FIL-1c: places reachable through a sidebar row carry no pill — the
    // sidebar already names the location.
    #[test]
    fn fil_1c_sidebar_places_carry_no_pill() {
        for source in [
            ViewSource::Playlist(7),
            ViewSource::Queue,
            ViewSource::Library,
            ViewSource::Missing,
        ] {
            assert!(!has_place_pill(&source));
            assert_eq!(place_pill_label(&source), None);
        }
    }

    // UX FIL-8: Recently added is a sidebar place and loses its pill.
    #[test]
    fn fil_8_recently_added_is_a_sidebar_place_without_a_pill() {
        let source = ViewSource::RecentlyAdded;
        assert!(!has_place_pill(&source));
        assert_eq!(place_pill_label(&source), None);
    }

    // UX FIL-2: the row shows for a place pill even with no filter and the
    // hide preference set.
    #[test]
    fn fil_2_row_shows_for_a_place_pill_without_any_filter() {
        assert!(row_visible(true, false, true, false));
        assert!(!row_visible(true, false, false, false));
        assert!(!row_visible(false, false, true, true));
    }
```

Also update the existing tests that call the changed signatures: `fil_2_row_is_forced_visible_when_restricted_despite_hidden_preference` and `fil_2_row_follows_preference_when_idle` gain a third argument `false` for `has_place_pill`; `fil_2_whitespace_search_does_not_restrict` and `fil_7_exclude_ai_restricts_on_its_own` drop their `&ViewSource::…` argument.

In `browse_filter_strings.rs`, add the test:

```rust
    // UX FIL-1c: the place pill's accessible name says leaving, not removing.
    #[test]
    fn fil_1c_place_pill_label_says_leave_not_remove() {
        assert_eq!(leave_place_label("Lorna Shore"), "Leave Lorna Shore");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd /home/marvin/Projects/reprise-place-pill
cargo test -p reprise-gnome --lib filter_restriction 2>&1 | tail -20
```

Expected: FAIL — `cannot find function has_place_pill in this scope`, plus arity errors on `is_restricted`/`row_visible`.

- [ ] **Step 3: Implement the law**

In `filter_restriction.rs`, replace `scope_restricts`, `scope_chip_label`, `is_restricted` and `row_visible` with:

```rust
/// Whether `source` is a place the user reached from inside the track list
/// and that therefore has no sidebar row naming it. Only these carry a place
/// pill: everywhere else the sidebar selection is the location display, and a
/// pill would be a second one (docs/ux-rules.md K, FIL-1c).
pub(in crate::ui) fn has_place_pill(source: &ViewSource) -> bool {
    matches!(
        source,
        ViewSource::Artist(_) | ViewSource::Album { .. } | ViewSource::Genre(_)
    )
}

/// The bare name of the place, undecorated — the caller adds the pill's back
/// affordance, and `library_shell::scope_title` reuses it for the window title.
pub(in crate::ui) fn place_pill_label(source: &ViewSource) -> Option<String> {
    match source {
        ViewSource::Artist(artist) => Some(artist.clone()),
        ViewSource::Genre(genre) => Some(genre.clone()),
        ViewSource::Album {
            album,
            album_artist,
        } if album_artist.trim().is_empty() => Some(album.clone()),
        ViewSource::Album {
            album,
            album_artist,
        } => Some(format!("{album} — {album_artist}")),
        _ => None,
    }
}

/// A place is not a restriction: only search, facets and the AI-exclude filter
/// withhold rows the location would otherwise show.
pub(in crate::ui) fn is_restricted(search: &str, browse: &BrowseFilter, exclude_ai: bool) -> bool {
    filters_restrict(search, browse, exclude_ai)
}

pub(in crate::ui) fn row_visible(
    is_track_source: bool,
    restricted: bool,
    has_place_pill: bool,
    preference_visible: bool,
) -> bool {
    is_track_source && (restricted || has_place_pill || preference_visible)
}
```

`is_restricted` is now a thin alias of `filters_restrict`; keep it, because callers read better with the intent name and the two may diverge again.

In `browse_filter_strings.rs`, replace `remove_scope_label`:

```rust
pub(in crate::ui) fn leave_place_label(place: &str) -> String {
    formatted(N_!("Leave {place}"), &[("place", place)])
}
```

- [ ] **Step 4: Fix the call sites this breaks**

Four call sites reference the removed names. Leave them compiling by mechanical substitution only — behaviour changes belong to Tasks 2, 4 and 5:

- `browse_bar.rs:447` — `is_restricted(&search, &filter, exclude_ai)` (drop `&source`).
- `browse_bar.rs:448-452` — `row_visible(self.track_source.get(), restricted, super::filter_restriction::has_place_pill(&source), self.preference_visible.get())`.
- `browse_bar.rs:527` — `if let Some(scope) = super::filter_restriction::place_pill_label(&source)`.
- `browse_filter_count.rs:30` — drop the `source` argument.
- `browse_filter_count.rs:51` — `if super::filter_restriction::has_place_pill(source)`.
- `end_of_results.rs:52` — `&& !crate::ui::browse::filter_restriction::has_place_pill(&source)`.
- `library_shell.rs:302` — `place_pill_label(source)`.
- `browse_bar.rs:533` — `filter_strings::leave_place_label(&scope)`.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cargo test -p reprise-gnome --lib filter_restriction 2>&1 | tail -10
cargo test -p reprise-gnome --lib browse_filter_strings 2>&1 | tail -10
```

Expected: PASS, `test result: ok`.

- [ ] **Step 6: Run the gates and commit**

```bash
cargo fmt --check
cargo clippy --locked --all-targets --workspace -- -D warnings
env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace 2>&1 | tail -20
scripts/check-ux-traceability.sh
scripts/check-architecture.sh
git add crates/reprise-gnome/src/ui/browse/filter_restriction.rs \
        crates/reprise-gnome/src/ui/browse/browse_filter_strings.rs \
        crates/reprise-gnome/src/ui/browse/browse_bar.rs \
        crates/reprise-gnome/src/ui/browse/browse_filter_count.rs \
        crates/reprise-gnome/src/ui/track_list/end_of_results.rs \
        crates/reprise-gnome/src/ui/window/library_shell.rs
git commit -m "refactor(gnome): places carry a pill instead of restricting like a filter"
```

---

### Task 2: Counting relative to the place

**Files:**
- Modify: `crates/reprise-gnome/src/ui/browse/browse_filter_count.rs:44-59`

**Interfaces:**
- Consumes: `filter_restriction::has_place_pill` (Task 1).
- Produces: no new API — `source_total` simply stops overriding its base.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `browse_filter_count.rs`:

```rust
    // UX FIL-2: inside a place the counter relates to that place, never to the
    // whole library — a playlist showing "12 tracks" is the precedent.
    #[test]
    fn fil_2_place_counts_against_itself_not_the_library() {
        let conn = seeded_conn();
        let source = ViewSource::Artist("Alpha Artist".into());
        assert_eq!(source_total(&conn, &source, false, 3, &[]).unwrap(), 3);
        assert_eq!(source_total(&conn, &source, true, 2, &[]).unwrap(), 3);
    }
```

The existing `seeded_conn()` helper must contain at least three tracks by `Alpha Artist` and at least one by another artist. Inspect it and extend its `execute_batch` accordingly so the library total and the artist total differ — otherwise the test passes for the wrong reason.

- [ ] **Step 2: Run the test to verify it fails**

```bash
cargo test -p reprise-gnome --lib browse_filter_count 2>&1 | tail -20
```

Expected: FAIL — the restricted case returns the library total (e.g. `9`) instead of `3`.

- [ ] **Step 3: Delete the override**

Replace the body of `source_total` (`browse_filter_count.rs:44-59`) with:

```rust
fn source_total(
    conn: &Db,
    source: &ViewSource,
    restricted: bool,
    count: usize,
    queue_ids: &[i64],
) -> Result<usize, rusqlite::Error> {
    if !restricted || matches!(source, ViewSource::Queue) {
        return Ok(count);
    }
    // The counting base is always the current place. Substituting the library
    // here is what made an artist page read "3 of 9 tracks" — filter
    // vocabulary at a location that is not a filter (FIL-2).
    queries::query_track_count_browsed(conn, source, "", &BrowseFilter::default(), queue_ids)
        .and_then(|value| {
            usize::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, value))
        })
}
```

`query_track_count_browsed_conn` already dispatches `Artist`/`Album`/`Genre` to their own per-place counters, so no core change is needed.

- [ ] **Step 4: Run the test to verify it passes**

```bash
cargo test -p reprise-gnome --lib browse_filter_count 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 5: Run the gates and commit**

```bash
cargo fmt --check && cargo clippy --locked --all-targets --workspace -- -D warnings
env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace 2>&1 | tail -20
git add crates/reprise-gnome/src/ui/browse/browse_filter_count.rs
git commit -m "fix(gnome): count against the current place instead of the whole library"
```

---

### Task 3: Split chip construction out of browse_bar

Pure move, no behaviour change. `browse_bar.rs` is at 788 of 800 allowed lines and Task 4 adds a zone layout to it.

**Files:**
- Create: `crates/reprise-gnome/src/ui/browse/browse_bar_chips.rs`
- Modify: `crates/reprise-gnome/src/ui/browse/browse_bar.rs`, `crates/reprise-gnome/src/ui/browse/mod.rs`

**Interfaces:**
- Produces: `pub(super) fn append_chip(chips: &gtk4::FlowBox, child: &impl IsA<gtk4::Widget>)`, `pub(super) fn filter_chips(filter: &BrowseFilter) -> Vec<FilterChip>`, `pub(super) struct FilterChip { pub facet: BrowseFacet, pub label: String, pub accessible_remove_label: String }`, plus the helpers `facet_label`, `displayed_value`, `filter_value`, `available_facets`, `remove_filter`, `apply_selection` — all moved verbatim.

- [ ] **Step 1: Move the pure chip helpers**

Create `browse_bar_chips.rs` and move these items unchanged out of `browse_bar.rs`: `FilterChip`, `filter_value`, `facet_label`, `displayed_value`, `filter_chips`, `available_facets`, `remove_filter`, `apply_selection`, `value_matches_search`, `restored_filter`, `append_chip`, `FACETS`, and the `chip_labels` test helper. Change their visibility from `fn`/`pub(super) fn` to `pub(super) fn` where `browse_bar.rs` uses them, and add the module header:

```rust
//! Chip construction for the unified filter bar — pure helpers plus the
//! FlowBox append, split out of `browse_bar.rs` to keep both files under the
//! repository's source-size limit.
```

Register it in `mod.rs`:

```rust
pub(in crate::ui) mod browse_bar_chips;
```

In `browse_bar.rs`, add `use super::browse_bar_chips::{append_chip, apply_selection, available_facets, filter_chips, remove_filter, restored_filter, value_matches_search, FilterChip};` and delete the moved definitions. Re-export what `browse_chooser.rs` imports from `browse_bar` today so its `use` lines keep working; check with:

```bash
rg "browse_bar::" crates/reprise-gnome/src/ui/browse/
```

- [ ] **Step 2: Verify it is a pure move**

```bash
cargo test -p reprise-gnome --lib browse 2>&1 | tail -10
wc -l crates/reprise-gnome/src/ui/browse/browse_bar.rs crates/reprise-gnome/src/ui/browse/browse_bar_chips.rs
```

Expected: PASS with the same test count as before the split, and `browse_bar.rs` well under 800 lines. No test was added or changed in this task — if any test needed editing, the move was not pure and must be redone.

- [ ] **Step 3: Run the gates and commit**

```bash
cargo fmt --check && cargo clippy --locked --all-targets --workspace -- -D warnings
env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace 2>&1 | tail -20
scripts/check-architecture.sh
git add crates/reprise-gnome/src/ui/browse/browse_bar_chips.rs \
        crates/reprise-gnome/src/ui/browse/browse_bar.rs \
        crates/reprise-gnome/src/ui/browse/mod.rs
git commit -m "refactor(gnome): split chip construction out of browse_bar"
```

---

### Task 4: The place pill and the two-zone row

**Files:**
- Modify: `crates/reprise-gnome/src/ui/browse/browse_bar.rs` (construction ~220-280, `sync_visibility` ~438-456, `rebuild_chips` ~511-546)
- Modify: `crates/reprise-gnome/src/ui/browse/browse_bar_chips.rs` (place pill builder)
- Modify: `crates/reprise-gnome/src/ui/browse/browse_bar_tests.rs`

**Interfaces:**
- Consumes: `filter_restriction::{has_place_pill, place_pill_label}` (Task 1); `browse_bar_chips::append_chip` (Task 3).
- Produces: `pub(in crate::ui) const PLACE_PILL_CSS_CLASS: &str = "reprise-place-pill";`, `BrowseBar::place_button(&self) -> Option<gtk4::Button>` (replaces `scope_button`, same `#[cfg(test)]` visibility).

- [ ] **Step 1: Write the failing display test**

In `browse_bar_tests.rs`:

```rust
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn place_pill_is_outlined_and_carries_no_remove_cross() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let bar = BrowseBar::new(conn);
        bar.set_source_context(&ViewSource::Artist("Alpha Artist".into()));

        let pill = bar.place_button().expect("an artist place shows a pill");
        let label = pill.label().expect("the pill is labelled");
        assert!(label.contains("Alpha Artist"));
        assert!(!label.contains('×'), "a place is left, not removed: {label}");
        assert!(pill.has_css_class(PLACE_PILL_CSS_CLASS));
        assert!(!pill.has_css_class(CHIP_CSS_CLASS));
        assert!(
            pill.tooltip_text().is_some_and(|t| t.contains("Leave")),
            "the tooltip names leaving, not removing"
        );
        assert!(pill.width_request() >= 20 && pill.height_request() >= 20);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn filter_section_label_stays_hidden_at_an_unfiltered_place() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let bar = BrowseBar::new(conn);
        bar.set_source_context(&ViewSource::Artist("Alpha Artist".into()));

        assert!(bar.widget().is_visible(), "the pill forces the row visible");
        assert!(
            !bar.section_label_visible(),
            "an unfiltered place must not claim FILTER"
        );
    }
```

Add the accessor `pub(in crate::ui) fn section_label_visible(&self) -> bool { self.section_label.is_visible() }` to `BrowseBar` under `#[cfg(test)]`.

- [ ] **Step 2: Run the tests to verify they fail**

```bash
xvfb-run -a cargo test -p reprise-gnome --lib place_pill_is_outlined_and_carries_no_remove_cross -- --ignored --exact 2>&1 | tail -20
```

Expected: FAIL — `place_button` does not exist; the label still ends in `×`.

- [ ] **Step 3: Build the place pill**

Add to `browse_bar_chips.rs`:

```rust
/// The place pill: outlined, prefixed with a back chevron, and deliberately
/// without a `×`. Its whole surface is the click target rather than a 20 px
/// cross, because leaving a location is a navigation, not a removal
/// (docs/ux-rules.md K, FIL-1c).
pub(super) fn build_place_pill(place: &str) -> gtk4::Button {
    let button = gtk4::Button::with_label(&format!("‹  {place}"));
    button.add_css_class("flat");
    button.add_css_class(super::browse_bar::PLACE_PILL_CSS_CLASS);
    button.set_size_request(20, 20);
    let leave_label = crate::ui::browse_filter_strings::leave_place_label(place);
    button.set_tooltip_text(Some(&leave_label));
    button.update_property(&[gtk4::accessible::Property::Label(&leave_label)]);
    button
}
```

- [ ] **Step 4: Give the row two zones**

In `browse_bar.rs` construction, add a place zone before the existing widgets and a separator between the zones. Replace the `root.append(...)` block (~276-280) with:

```rust
        let place_zone = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        let zone_separator = gtk4::Separator::new(gtk4::Orientation::Vertical);
        zone_separator.set_visible(false);

        root.append(&place_zone);
        root.append(&zone_separator);
        root.append(&section_label);
        root.append(&chips);
        root.append(&result_label);
        root.append(&clear_all);
```

Add `place_zone: gtk4::Box`, `zone_separator: gtk4::Separator` and `place_button: RefCell<Option<gtk4::Button>>` to the struct and its initializer, and remove `scope_button`.

In `rebuild_chips`, delete the scope-chip block (`browse_bar.rs:527-546`) and populate the place zone instead. Add a `rebuild_place_zone` called from `refresh`:

```rust
    fn rebuild_place_zone(self: &Rc<Self>) {
        while let Some(child) = self.place_zone.first_child() {
            self.place_zone.remove(&child);
        }
        self.place_button.borrow_mut().take();
        let source = self.source.borrow().clone();
        let Some(place) = super::filter_restriction::place_pill_label(&source) else {
            self.place_zone.set_visible(false);
            return;
        };
        let button = super::browse_bar_chips::build_place_pill(&place);
        let weak = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            let Some(bar) = weak.upgrade() else {
                return;
            };
            let callback = bar.on_scope_cleared.borrow().clone();
            if let Some(callback) = callback {
                callback();
            }
        });
        self.place_zone.append(&button);
        self.place_zone.set_visible(true);
        *self.place_button.borrow_mut() = Some(button);
    }
```

Keep the callback name `on_scope_cleared` — it is wired in three places (`window_runtime_wiring.rs:513`, `metadata_navigation.rs:235,299`) and renaming it collides with the parallel branch. Rename it in a follow-up once that branch has landed.

- [ ] **Step 5: Gate the FILTER label and the separator on filters only**

In `sync_visibility`, after the existing `restricted` computation:

```rust
        let has_place_pill = super::filter_restriction::has_place_pill(&source);
        let visible = super::filter_restriction::row_visible(
            self.track_source.get(),
            restricted,
            has_place_pill,
            self.preference_visible.get(),
        );
        self.root.set_visible(visible);
        // The FILTER heading describes the filter zone only — a place is not a
        // filter and must not be labelled as one (FIL-1c).
        self.section_label.set_visible(filters_restrict);
        self.zone_separator
            .set_visible(has_place_pill && filters_restrict);
        self.clear_all.set_visible(filters_restrict);
        tracing::info!(visible, restricted, has_place_pill, "filter row visibility updated");
```

Note the change from `restricted` to `filters_restrict` on `section_label`.

- [ ] **Step 6: Add the outlined CSS class**

In `browse_bar.rs`'s `css()`, add next to the chip rules:

```rust
         .{PLACE_PILL_CSS_CLASS} {{ border-radius: 9999px; padding: 2px 10px; \
         border: 1px solid alpha(currentColor, 0.30); background-color: transparent; }} \
         .{PLACE_PILL_CSS_CLASS}:hover {{ background-color: alpha(currentColor, 0.08); }} \
```

Declare `pub(in crate::ui) const PLACE_PILL_CSS_CLASS: &str = "reprise-place-pill";` beside `CHIP_CSS_CLASS`.

- [ ] **Step 7: Run the tests to verify they pass**

Run each individually — the display suite is herd-flaky and batch results are not evidence:

```bash
xvfb-run -a cargo test -p reprise-gnome --lib place_pill_is_outlined_and_carries_no_remove_cross -- --ignored --exact 2>&1 | tail -10
xvfb-run -a cargo test -p reprise-gnome --lib filter_section_label_stays_hidden_at_an_unfiltered_place -- --ignored --exact 2>&1 | tail -10
```

Expected: `test result: ok. 1 passed` for each.

- [ ] **Step 8: Update the scope-chip display tests in metadata_navigation.rs**

`fil_1c_scope_chip_x_returns_to_the_library_with_history` and `fil_8_recently_added_chip_x_returns_to_the_normal_library_with_history` call `track_list.shared.browse_bar.scope_button()`. Change the first to `place_button()`. The second must be rewritten: `Recently added` no longer has a pill, so assert its absence and drop the click:

```rust
        assert!(track_list.shared.browse_bar.place_button().is_none());
        assert_eq!(track_list.current_source(), ViewSource::RecentlyAdded);
```

Rename it to `fil_8_recently_added_is_a_sidebar_place_without_a_pill_widget`.

- [ ] **Step 9: Run the gates and commit**

```bash
cargo fmt --check && cargo clippy --locked --all-targets --workspace -- -D warnings
env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace 2>&1 | tail -20
scripts/check-display-tests.sh
scripts/check-accessibility-semantics.sh
git add crates/reprise-gnome/src/ui/browse/ crates/reprise-gnome/src/ui/window/metadata_navigation.rs
git commit -m "feat(gnome): give places their own outlined pill in a zone of its own"
```

---

### Task 5: End-of-results follows the place

Today `end_of_results.rs:52` suppresses the line whenever the source is a scope — so searching inside an artist page shows no end-of-results line at all, even though tracks are genuinely hidden. With places no longer counted as restrictions, that guard is wrong in both directions.

**Files:**
- Modify: `crates/reprise-gnome/src/ui/track_list/end_of_results.rs:46-53`

**Interfaces:**
- Consumes: `filter_restriction::filters_restrict` (unchanged), `BrowseBar::result_count()` (now place-relative after Task 2).

- [ ] **Step 1: Write the failing test**

In `end_of_results.rs`'s test module:

```rust
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn end_of_results_appears_for_a_search_inside_a_place() {
        // A place is not a restriction, but a search inside one is — the line
        // must appear and speak about the place's own total.
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        // build a track list at ViewSource::Artist("Alpha Artist") with three
        // tracks, apply the search "track 2", then:
        assert!(line_box.is_visible());
        assert_eq!(pill.label().unwrap(), "Show all 3 tracks");
    }
```

Follow the existing fixture pattern in that module for constructing the track list; if no such fixture exists there, reuse the one from `browse_bar_tests.rs` and seed three `Alpha Artist` tracks plus one other artist via `crate::test_db`.

- [ ] **Step 2: Run it to verify it fails**

```bash
xvfb-run -a cargo test -p reprise-gnome --lib end_of_results_appears_for_a_search_inside_a_place -- --ignored --exact 2>&1 | tail -20
```

Expected: FAIL — the line stays hidden because of the place guard.

- [ ] **Step 3: Drop the place guard**

Replace `end_of_results.rs:48-53` with:

```rust
    // FIL-7: the AI-exclude filter also restricts; the `hidden == 0` guard below
    // handles the nothing-actually-hidden case. A place is not a restriction —
    // but a filter inside one is, and its hidden count is relative to that
    // place (FIL-2).
    let restricted = crate::ui::browse::filter_restriction::filters_restrict(
        &search,
        &browse,
        shared.browse_bar.exclude_ai(),
    );
```

- [ ] **Step 4: Run it to verify it passes**

```bash
xvfb-run -a cargo test -p reprise-gnome --lib end_of_results_appears_for_a_search_inside_a_place -- --ignored --exact 2>&1 | tail -10
```

Expected: `test result: ok. 1 passed`.

- [ ] **Step 5: Run the gates and commit**

```bash
cargo fmt --check && cargo clippy --locked --all-targets --workspace -- -D warnings
env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace 2>&1 | tail -20
git add crates/reprise-gnome/src/ui/track_list/end_of_results.rs
git commit -m "fix(gnome): show the end-of-results line for filters inside a place"
```

---

### Task 6: Rule text

**Files:**
- Modify: `docs/ux-rules.md` section K (FIL-1c ~1150-1159, FIL-2 ~1160-1181, FIL-8 ~1220-1227)

- [ ] **Step 1: Rewrite FIL-1c**

Replace the FIL-1c bullet with:

```markdown
- **FIL-1c** [active] [gtk] — Artist, album and genre pages are **places**,
  not filters, and are marked as such: a place pill sits in the filter row's
  own left zone, outlined rather than filled, prefixed with "‹", and without
  a × — its whole surface is the click target (≥ 20 px), and its tooltip and
  accessible name say leaving ("Leave <place>"), never removing. Leaving
  happens through the regular NAV-2 history push to the Library, where the
  remembered search and facets are restored. A place carries a pill exactly
  when no sidebar row already names it: artist, album and genre pages qualify,
  Library, Recently added, playlists, Smart, Queue and Missing do not. The
  "FILTER" heading, the chips and "Clear all" describe the filter zone only
  and never appear for a place alone. Counting follows FIL-2. (Revised
  2026-07-31: the original rule rendered places as removable scope chips under
  the FILTER heading — one shape for two meanings, which measurably read as a
  filter that turned out to be a navigation.)
```

- [ ] **Step 2: Extend FIL-2 with the counting base**

Append to the FIL-2 bullet:

```markdown
  The counting base is always the current place: inside an artist, album or
  genre page "X of Y" relates to that place's own total, never to the whole
  library — the same way a playlist reports its own length. The row is visible
  when a filter is active, when a place pill is due, or when the preference
  asks for it. (Counting base revised 2026-07-31 together with FIL-1c.)
```

- [ ] **Step 3: Adjust FIL-8**

Replace FIL-8's second sentence with:

```markdown
  The source initially sorts by `added_at` descending and carries no place
  pill: it is a sidebar place, and the sidebar row already names it (FIL-1c,
  revised 2026-07-31). Selecting another sidebar row leaves it, like any other
  sidebar place.
```

- [ ] **Step 4: Verify traceability and commit**

```bash
scripts/check-ux-traceability.sh
git add docs/ux-rules.md
git commit -m "docs(ux): places are not filters — revise FIL-1c, FIL-2 and FIL-8"
```

---

### Task 7: Re-measure the original report

The rig from the design measurement is reusable; this repeats it against the built change.

**Files:** none — verification only.

- [ ] **Step 1: Build and start the isolated rig**

```bash
cd /home/marvin/Projects/reprise-place-pill
cargo build -p reprise-gnome --bin reprise
```

Then run the measurement rig (private Xvfb, private D-Bus, throwaway XDG profile, `REPRISE_AUDIO_SINK=fakesink`, nine fixture tracks across three artists) as described in the spec's measurement section, pointing `BIN` at this worktree's `target/debug/reprise`.

- [ ] **Step 2: Walk the reported path and record evidence**

1. Right-click a track → "Go to artist". Screenshot: outlined pill `‹ Alpha Artist` in the left zone, **no** `FILTER` heading, counter `3 tracks`.
2. Double-click a track to play. Screenshot: the pill is **still there** (this is the parallel branch's fix; if it vanishes, that branch did not land and the rebase dependency was skipped).
3. Type a search that matches one track. Screenshot: pill on the left, separator, `FILTER` + search chip + `Clear all ×` on the right, counter `1 of 3 tracks`, end-of-results pill reading `Show all 3 tracks`.
4. Click the place pill. Expect log `scope chip cleared`, view returns to the library, playback continues, queue unchanged.
5. Press Next past the end of the queue. Expect log `queue exhausted on manual next; refilled from the visible view refill_len=9`.

- [ ] **Step 3: Tear the rig down**

Kill only this rig's app, D-Bus, openbox and Xvfb PIDs from its `env.sh` — other sessions run their own `target/debug/reprise` processes and must not be touched.

- [ ] **Step 4: Report**

Post the five screenshots and the matching log lines. No commit.

---

## Self-Review

**Spec coverage:** Model → Task 1. Row layout → Task 4 (steps 4-5). Shape and gesture → Task 4 (steps 3, 6). Counting → Task 2. Row visibility → Tasks 1 and 4. Follow-on copy → Tasks 1 (strings) and 5. Rule changes → Task 6. Files table → Tasks 1-5. Testing → each task's own steps plus Task 7. Core support → verified in Task 2, no task needed.

**Known gap, deliberate:** the callback `on_scope_cleared` keeps its old name through this plan (Task 4, step 4) because renaming it touches three files the parallel branch also edits. It is a follow-up, not a gap in behaviour.
