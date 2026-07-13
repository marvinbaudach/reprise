# Unified Chip Filter Bar — Implementation Plan

**Goal:** Replace the three persistent browse searches with the approved compact chip filter bar,
one temporary value search, result count, and reset action while retaining existing Core semantics.

**Baseline:** 704 passed; 30 ignored. Core remains unchanged and dependency-pure. No real database,
music, cache, desktop, or session bus is used.

## Global constraints

- Existing `BrowseFilter` and cascading `query_browse_values` semantics are authoritative.
- The header's all-fields search remains unchanged and combines with browse filters by `AND`.
- Every UI string is gettext-backed and German translation coverage remains complete.
- GTK callbacks clone values out of `RefCell`s before calls that can re-enter.
- Every created or substantially edited file ends below 800 lines.
- Every app run is fully isolated with D-Bus session, Xvfb, scratch XDG data/cache, forced X11,
  unset Wayland display, and fakesink.

## Task 1 — Pure filter-bar projection and gettext extraction

**Files:** create `crates/reprise-gnome/src/ui/browse_filter_strings.rs`; modify
`ui/browse_bar.rs`, `ui/mod.rs`, `ui/strings.rs`, `po/POTFILES.in`, `po/de.po`.

**Interfaces:**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
struct FilterChip { facet: BrowseFacet, label: String, accessible_remove_label: String }

fn filter_chips(filter: &BrowseFilter) -> Vec<FilterChip>;
fn available_facets(filter: &BrowseFilter) -> Vec<BrowseFacet>;
fn remove_filter(filter: &BrowseFilter, facet: BrowseFacet) -> BrowseFilter;
fn value_matches_search(value: &str, search: &str) -> bool;
```

RED tests prove stable chip order and Unknown labels, omission of active facets, cascading removal,
and case-insensitive substring matching. Move all existing browse strings out of edge-tight
`strings.rs`, add Filter/Add filter/Reset/result/remove copy, and update German gettext coverage.

Run the targeted tests, then every project gate. Adversarially review cascade behavior, translation
extraction, and source sizes.

Commit: `feat: add pure chip filter projection`

## Task 2 — Native GTK chip bar, one chooser search, and live counts

**Files:** modify `ui/browse_bar.rs`, `ui/track_list.rs`; add a sibling module only if required by
the source-size gate.

**Interfaces:**

```rust
impl BrowseBar {
    pub fn set_result_count(&self, filtered: usize, total: usize);
}
```

Replace the three DropDowns with a FlowBox of removable chip buttons and one MenuButton. Its popover
first lists available facets, then shows a back action, one SearchEntry, and value rows loaded from
the existing cascading query. Selection, chip removal, and Reset all update the stored filter through
one callback path. Preserve `restore_filter`, `refresh`, source visibility, and the existing smoke's
raw selection seam.

`track_list::reload` updates the bar with the visible count and unfiltered Library total without
affecting non-Library sources. Add an ignored display test covering accessible controls, chip
projection, removal, and Reset. Extend smoke logging with projected chips/result count.

Run targeted tests, every project gate, release checker, core-purity, file-size proof, and the fully
isolated Browse+search smoke. Adversarially review RefCell lifetimes, popover lifecycle, cascading
state, session restore, narrow layout, and duplicate count queries.

Commit: `feat: replace browse dropdowns with chip filter bar`

## Task 3 — Close-out, merge, and lock release

**Files:** update `docs/agent-workflow/STATUS.md`; append the local progress ledger if it exists.

Re-run all gates and isolated Browse+search integration. Perform a whole-branch review against the
design. Record only real-desktop visual spacing/wrapping as manual. Merge the feature branch into
local `main`, release the coordination lock, and do not push.

Commit: `docs: release work lock; finish unified filter bar`
