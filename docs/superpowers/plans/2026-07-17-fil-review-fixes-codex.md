# Codex Handoff — FIL Review Fixes (Round 1)

Review of the FIL implementation (`6fad03d..6af5351`) confirmed two defects
and two cheap cleanups. Fix them in this worktree on
`feat/global-search-rework`. The execution protocol from
`2026-07-17-fil-filter-visibility-codex.md` applies unchanged (gates before
every commit, no push, no attribution footers, rule-named tests display-free,
`scripts/check-ux-traceability.sh` must stay green). No ux-rules.md status
changes are needed — the affected rules are already `[aktiv]`.

Deliver as TWO commits: commit 1 = fixes 1+2 (`fix: …`), commit 2 =
cleanups 3+4 (`refactor: …`).

## Fix 1 — Facet chips leak into non-Library sources (FIL-1a/FIL-2 violation)

`BrowseBar.filter` survives source switches, but outside Library the reload
path applies `BrowseFilter::default()` — the bar must render and reason from
that same *effective* view. Today `rebuild_chips` renders facet chips
unconditionally (only `add_filter` is `is_library`-gated,
browse_bar.rs:406-422) and `sync_visibility` computes `restricted` from the
raw stored filter (browse_bar.rs:320-321). Symptom: set a Genre facet in
Library, switch to a playlist → FILTER label + Genre chip + "Clear all ×"
show over an unrestricted list with a neutral count — a lying chip.

Required change (keep the stored filter so returning to Library restores the
facets — do NOT clear it on leave):

- Add a private helper on `BrowseBar`:

```rust
/// The browse filter as the reload path applies it: facets only act in
/// the Library source (track_list_reload::reload uses default elsewhere).
fn effective_filter(&self) -> BrowseFilter {
    if self.is_library.get() {
        self.filter.borrow().clone()
    } else {
        BrowseFilter::default()
    }
}
```

- `sync_visibility`: compute `restricted` from
  `is_restricted(&self.search.borrow(), &self.effective_filter())`.
- `rebuild_chips` (and `refresh`): render facet chips from
  `effective_filter()` — i.e. no facet chips outside Library; the search
  chip is unaffected.
- Extend the pure chip-model fn so the rule test can prove the gate:
  `fn chip_labels(search: &str, filter: &BrowseFilter, is_library: bool) -> Vec<String>`
  (facet labels only when `is_library`). Update the existing
  `fil_1a_search_appears_as_chip_before_facet_chips` test and ADD:

```rust
// UX FIL-1a: facet chips and "+ Add filter" stay Library-only — a facet
// set in Library must not render as a chip in a playlist, where the
// reload path does not apply it.
#[test]
fn fil_1a_facet_chips_are_library_only() {
    let browse = BrowseFilter { genre: Some("Rock".into()), artist: None, album: None };
    assert_eq!(
        chip_labels("falling", &browse, false),
        vec!["⌕ “falling” in any field".to_string()]
    );
    assert!(chip_labels("", &browse, false).is_empty());
}
```

- Verify the knock-on states: with a stale Library facet and empty search, a
  playlist must show NO "FILTER" label, NO "Clear all", and (preference off)
  no force-show — all fall out of the corrected `restricted`.

## Fix 2 — FIL-6: "Show all 0 tracks" leads into a second empty state

`show_all_action_label(Some((0, 0)))` currently yields "Show all 0 tracks";
clicking it lands in the next empty state, violating FIL-6's "führt
garantiert zu Inhalt". Change the fn to return `None` when `total == 0`
(button hidden), and extend its rule test:

```rust
assert_eq!(show_all_action_label(Some((0, 0))), None);
```

## Cleanup 3 — Named constants in end_of_results.rs

Replace the magic offsets `margin + 12` / `margin + 44` in `recompute` with
named constants (e.g. `LINE_TOP_GAP: i32 = 12`, `PILL_TOP_GAP: i32 = 44`)
carrying a one-line comment on what they space.

## Cleanup 4 — Translation-safe bold count in result_count_markup

`result_count_markup` bolds via `plain.replacen(<filtered>, …)` AFTER
translation — a locale that reorders total before filtered would bold the
wrong number. Rebuild instead: pass the already-bolded filtered value into
the same gettext template (numbers are markup-safe), never string-replace
translated output. Keep the existing
`fil_2_count_markup_accents_only_when_restricted` assertions passing
unchanged.

## Completion

Run the full gate set plus `scripts/check-ux-traceability.sh`, then report:
commit hashes, tests added/changed, and any deviation with a one-line reason.
Do not push.
