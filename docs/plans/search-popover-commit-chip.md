---
slug: search-popover-commit-chip
worktree: ~/Projects/reprise-search-popover-commit-chip
branch: feature/search-popover-commit-chip
phase: planned
codex_session:
created: 2026-08-09
---

# Search becomes a popover, and the chip becomes a receipt

Replace the sliding `GtkSearchBar` strip with a popover anchored to the header
lens, and stop rendering the search chip while the popover is open. The chip is
created **once, on close** — a commit receipt — instead of being rebuilt on
every keystroke as a live echo of the entry.

Visual reference: `Suchfilter.dc.html` in the Claude Design project *"Suchfilter
und Podcast-Verhalten"* (`d7706c53-fb4f-4a75-ac65-a5fec97f1b02`). Its prototype
originally modelled the landed behaviour with `chipMode = "on-close"` and
`escDiscards = false`. SEARCH-4a/14 were revised on 2026-08-12: Enter still
commits, while Escape now discards through the section's chip-clear path.

---

## Anchoring note — read this before touching anything

This plan was written against **`origin/dev` at `7214a29de1`**, in the worktree
`~/Projects/reprise-search-popover-commit-chip`. Three premises of
the original request are stale against that base and are corrected here:

1. **`browse/browse_bar_chips.rs::chip_labels` and
   `browse/browse_filter_strings.rs::search_chip_label` are not where the search
   chip is built any more.** The chip is built in
   `crates/reprise-gnome/src/ui/filter_bar_layout.rs:121`
   (`replace_scoped_search`), rendered by
   `crates/reprise-gnome/src/ui/filter_bar_strings.rs:25`
   (`scoped_search_chip_label`), whose message lives in
   `crates/reprise-view/src/strings/browse.rs:62` (`search_chip_label_in`).
   `browse_filter_strings.rs` does not exist on `origin/dev`.

2. **The search bar is not a row in a window grid.** It is a second top bar of an
   `adw::ToolbarView` (`library_chrome.rs:59`, `root.add_top_bar(&search_bar)`).
   The window's vertical layout is already what the request asks for: content and
   player bar are siblings in a vertical `GtkBox` with `content.set_vexpand(true)`
   (`crates/reprise-gnome/src/ui/player_bar/library_player_bar.rs:29-40`). **No
   row re-assignment is needed.** Removing the top bar is sufficient, and the
   player bar stays pinned by construction. SEARCH-10 below still tests it,
   because "already correct" and "provably still correct" are different things.

3. **`SEARCH-9` is taken.** It is `[active]` on `origin/dev`
   (`docs/ux-rules.md:2770`, the debounce/viewport rule). The six new rules
   therefore shift by one: the request's SEARCH-9…14 become **SEARCH-10…15**.

**The chip already renders before the facet chips.** `FilterBarLayout` has a
dedicated `Search` slot ordered ahead of `Facets`
(`filter_bar_layout.rs:23-47`). Requirement "chip first" is structural and
already met; SEARCH-12 pins it against regression.

**One behaviour is deliberately dropped** (decided with the user, 2026-08-09):
today `GtkSearchBar::set_key_capture_widget` opens the search by typing directly
into the list (SEARCH-2b). A `GtkPopover` cannot do that on its own, and
hand-rolling the key capture is the classic "popover steals keystrokes" bug. The
design mock opens search only through the lens, and so does Brave's find bar.
**Type-to-open goes away**; SEARCH-2c says so explicitly.

---

## Design tokens from the mock

From `Suchfilter.dc.html`, the `overlayOpen` branch (lines ~54-72):

| Property | Value |
|---|---|
| Panel width | `336px` |
| Panel padding | `9px`, column gap `7px` |
| Panel border | `1px solid` neutral-800, radius `--radius-md`, `--shadow-lg` |
| Panel surface | `--color-surface` (the header/chrome plane, not `.view`) |
| Anchor | `top: 42px; right: 12px` under a `46px` header — i.e. bottom-end, right-aligned to the window's 12px inset, **no arrow** |
| Entry | leading magnifier at `left: 10px`, `padding-left: 32px`, `padding-right: 32px`, `border-color: --color-accent` |
| Clear affordance | in-entry backspace glyph at the right, only when the query is non-empty |
| Caption row | `font-size: 11px`, `--color-neutral-500`, `padding: 0 2px`, `justify-content: space-between` — scope hint left, dismiss hint right |
| Scope hint | `"Searches " + scope` → e.g. `Searches episode titles` |
| Dismiss hint | `Esc to close` |

GTK translation notes:

- `GtkSearchEntry` already ships the leading magnifier and the trailing clear
  icon, so the entry needs CSS for the accent border and width only — do not
  hand-build icons.
- No arrow: `popover.set_has_arrow(false)`.
- Bottom-end: `popover.set_position(gtk4::PositionType::Bottom)` +
  `popover.set_halign(gtk4::Align::End)`. This right-aligns the panel to the
  **lens**, which sits one button left of the sidebar toggle; the mock aligns it
  to the window inset. Accept the lens alignment — it is within a few pixels,
  and it is stable. **Do not hardcode a pixel offset** to chase the mock.
- Keep the existing generic search placeholder. The mock's scoped placeholder
  (`"Search episode titles"`) would repeat the caption line verbatim one
  control below it.

---

## Architecture: where the decision lives

Requirement 5 of the request — keep the "is there a chip?" decision out of the
widgets — is satisfied by making the chip's query **a genuinely different value**
from the filter's query, not by a widget consulting a flag.

- The entry drives **filtering** on every keystroke, exactly as today
  (`SectionSearch::new` → `connect_search_changed` → `apply_to_active`).
  Requirement 2 is untouched: results and the "N of TOTAL" count stay
  incremental.
- The entry drives the **chip** only through a second, separate push: the
  *committed* query. While the popover is open, the committed query is empty.
  While it is closed, it equals the live query.

The whole decision is one pure function in `reprise-view`:

```rust
// crates/reprise-view/src/search_chip.rs
pub enum SearchSurface { Open, Closed }

/// SEARCH-11/12/13: what the filter bar's search slot should show.
pub fn committed_query(query: &str, surface: SearchSurface) -> Option<&str>
```

`None` while `Open`, `None` for an empty or whitespace-only query, otherwise
`Some(trimmed)`. Everything else follows from it, and no GTK type appears in
that decision — `scripts/check-frontend-thinness.sh` stays green because the
logic lands in `reprise-view`, whose line floor rises in the same commit.

---

## Tasks

T1 and T2 are independent of everything else and of each other. T3–T5 interlock
through the same seam and must land in order. T6 depends on T3–T5. Codex should
run them in that order; the grouping is what a parallel run would split on.

### T1 — the pure layer (`reprise-view`)

Files owned: `crates/reprise-view/src/**`, `scripts/check-frontend-thinness.sh`.

1. New `crates/reprise-view/src/search_chip.rs` with `SearchSurface` and
   `committed_query` as above. Register it in `crates/reprise-view/src/lib.rs`.
   Unit tests: open + non-empty → `None`; closed + `"wer"` → `Some("wer")`;
   closed + `"   "` → `None`; closed + `"  wer  "` → `Some("wer")`.
2. In `crates/reprise-view/src/strings/browse.rs`, add the caption message next
   to `search_chip_label_in` (line 62):

   ```rust
   /// SEARCH-2c: the popover names the fields the current view searches.
   pub fn searches_scope(scope: SearchScope) -> Message
   ```

   It must reuse **the same per-scope field noun** that `search_chip_label_in`
   already uses — factor that noun into one private helper and have both call
   it, so the chip ("… in episode titles") and the caption ("Searches episode
   titles") can never drift apart. Cover every `SearchScope` variant in a test,
   mirroring `fil_1d_chip_label_names_the_fields_of_its_view`.
3. Add the GTK-side renderer `searches_scope(scope) -> String` in
   `crates/reprise-gnome/src/ui/filter_bar_strings.rs`, next to
   `scoped_search_chip_label`.
4. `scripts/check-frontend-thinness.sh`: `view_floor=1780` is an equality gate,
   not just a floor. Raise it to the new measured count **in this same commit**,
   or the gate fails in both directions. Verify by running the script.

### T2 — the rulebook (`docs/ux-rules.md`)

Files owned: `docs/ux-rules.md`.

Six new rules. All `[gtk]`. Place SEARCH-10…15 in section Q next to their
siblings; keep the existing "replaced by" convention.

- **SEARCH-10** — Opening and closing search changes no layout. The search
  surface is a popover over the content; the header keeps its height, the
  content area keeps its allocated height, and the player bar stays flush with
  the window's bottom edge in both states. Nothing is inserted into the window's
  vertical layout.
- **SEARCH-11** — While the search popover is open, the entry is the only place
  the query is shown: the filter bar renders no search chip, even though results
  and the "N of TOTAL {unit}" count already reflect the query. Facet chips stay
  visible throughout.
- **SEARCH-12** — Closing with a non-empty query renders exactly one search
  chip, in the filter bar's search slot ahead of the facet chips. It is built
  once, on close, not from the entry's `changed` signal.
- **SEARCH-13** — Closing with an empty or whitespace-only query renders no
  chip and changes nothing.
- **SEARCH-14** — Enter commits the query and closes. Escape clears query, chip
  and filtering through the active section's existing clear path and closes in
  one press. A click outside and the lens still close while keeping the query
  (SEARCH-5/6). The query stays session- and section-scoped — never written
  to `podcasts::config::save_filter` or the radio settings keys, dropped on
  restart, never carried between sections (SEARCH-8a).
- **SEARCH-15** — Reopening the popover while a search chip exists hides that
  chip and pre-fills the entry with its query, caret at the end. The chip is
  never duplicated.

Four supersessions, each needing its `[replaced by …]` line kept in place:

- **SEARCH-1 → SEARCH-1a** — At rest, search is only the header lens. The field
  lives in a popover attached to that lens, not in a second top bar.
- **SEARCH-2b → SEARCH-2c** — Lens and Ctrl+F open the popover; **typing into
  the list no longer opens it**. Opening focuses the entry and puts the caret at
  the end of any existing query; closing returns focus to the list. The panel is
  bottom-end under the lens, without arrow, on the chrome surface, and carries
  the entry plus one muted caption line naming the searched scope and the
  "Esc to close" hint. It reflows nothing (SEARCH-10).
- **SEARCH-4 → SEARCH-4a** — Escape is one stage: it clears query, chip and
  filtering through the active section's existing clear path and closes the
  popover in the same press. Enter keeps its original commit-and-close
  behaviour. With a closed popover, Escape consumes the key only when a search
  chip is active.
- **SEARCH-7 → SEARCH-7a** — The popover autohides. A click outside closes it
  and keeps the query, chip and accent lens per SEARCH-3/5. The held-pointer
  machinery SEARCH-7 needed is gone with the strip: a popover close inserts and
  removes nothing, so nothing below it can move out from under a click.

Leave SEARCH-3, SEARCH-5, SEARCH-6 and SEARCH-8a `[active]` and unedited — they
already say the right thing about the query surviving a collapse; SEARCH-2c
redefines *what* collapses.

### T3 — the popover module (new file, then `library_chrome.rs` shrinks)

Files owned: `crates/reprise-gnome/src/ui/window/search_popover.rs` (new),
`search_popover_tests.rs` (new), `library_chrome.rs`, `library_chrome_css.rs`,
`library_chrome_tests.rs`, `crates/reprise-gnome/src/ui/window/mod.rs`.

`library_chrome.rs` is 322 lines and must not grow past the repo's 800-line gate
— but the real reason for a separate module is that the popover owns a
self-contained job. Put it in `search_popover.rs`, target well under 300 lines.

1. New `SearchPopover` wrapper struct holding the `gtk4::Popover`, the
   `gtk4::SearchEntry` and the scope-caption `gtk4::Label`. API roughly:

   ```rust
   pub(in crate::ui) struct SearchPopover { /* popover, entry, scope_label */ }
   impl SearchPopover {
       fn new(lens: &gtk4::ToggleButton, entry: &gtk4::SearchEntry) -> Self;
       fn is_open(&self) -> bool;
       fn open(&self);      // popup + focus entry + caret to end
       fn close(&self);     // popdown; focus back to the list
       fn set_scope(&self, scope: SearchScope);  // caption text
       fn connect_open_changed(&self, f: impl Fn(bool) + 'static);
   }
   ```

   - `popover.set_parent(lens)`, `set_autohide(true)`, `set_has_arrow(false)`,
     `set_position(PositionType::Bottom)`, `set_halign(Align::End)`.
   - **`set_parent` obliges an `unparent`.** A popover parented by hand is not
     disposed with its parent; connect the lens's `destroy` (or the window's) to
     `popover.unparent()`, or GTK logs a finalize warning for every window.
   - `open()`: `popup()`, then `entry.grab_focus()` and
     `entry.set_position(-1)` so the caret sits at the end of the pre-filled
     query rather than selecting it.
   - `close()`: `popdown()`. Returning focus to the list is what
     `crate::ui::shortcuts` already does elsewhere; reuse the window's
     `set_focus` path rather than inventing a second one.
   - Escape and Enter: a `GtkEventControllerKey` on the entry — `Return` and
     `KP_Enter` commit and close, while `Escape` clears through the active
     section's chip-clear path and then closes. All return `Propagation::Stop`.
     `connect_stop_search` must use the same Escape abort path so toolkit signal
     order cannot change the result.
   - `connect_closed` on the popover feeds the same `open_changed(false)`
     callback as an explicit `close()`, so autohide (click outside) and Escape
     are one path. That is what makes SEARCH-7a free.
2. `library_chrome.rs`:
   - Delete `wire_search_focus_collapse`, `wire_search_focus_collapse_with`,
     `should_collapse_search_after_focus_change` and the whole held-pointer
     `EventControllerLegacy` block (lines 96-204). The popover's autohide
     replaces all of it — that machinery existed only because collapsing a top
     bar moved the widgets under the pointer.
   - Delete `update_preserved_query` and the `preserved_query` stash in
     `wire_search_toggle` (lines 206-210, 229-277). `GtkSearchBar` wiped its
     connected entry on collapse and the stash existed to undo that; a popover
     wipes nothing. Keep only the part of `wire_search_toggle` that syncs the
     lens's `:checked` state via `search_toggle_active` (SEARCH-3) — it must now
     be driven by `connect_open_changed` plus the entry's `changed`.
   - `LibraryChrome` loses `search_bar: gtk4::SearchBar` and gains
     `search: SearchPopover`. Drop `root.add_top_bar(&search_bar)`.
   - `search_toggle_active(search_mode, query)` survives unchanged, now fed the
     popover's open state.
3. `library_chrome_css.rs`: replace the `.reprise-search-strip` rule with
   `.reprise-search-popover` per the token table above (surface, 1px border,
   `--radius-md`, shadow, 336px, 9px padding, 7px gap) plus
   `.reprise-search-popover-caption` (11px, dimmed). The `style_guard` test at
   `library_chrome.rs:299` iterates `[".reprise-library-header",
   ".reprise-search-strip"]` and asserts `background-color:` **and**
   `border-bottom:` — a popover has a full border, not a bottom edge. Update the
   guard to assert `background-color:` for both and `border-bottom:` only for
   the header, with a comment saying why the popover differs.

### T4 — the state seam (`section_search.rs`, `section_search_wiring.rs`, `shortcuts.rs`)

Files owned: `crates/reprise-gnome/src/ui/window/section_search.rs`,
`section_search/tests.rs`, `section_search/tests/**`,
`section_search_wiring.rs`, `section_search_reroute_tests.rs`,
`crates/reprise-gnome/src/ui/shortcuts.rs`,
`crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs`,
`crates/reprise-gnome/src/ui/window/window.rs`.

1. `SectionSearch` swaps `search_bar: WeakRef<SearchBar>` for the popover
   handle, and `collapse_bar()` becomes `close_popover()`. `sync_affordance`
   (line 329) loses its `set_key_capture_widget` calls — with type-to-open gone
   there is no capture widget. It keeps making the lens insensitive with the
   `nothing_to_filter` tooltip and force-closing the popover for
   `SearchScope::Unsupported`, and it now also pushes the scope caption via
   `SearchPopover::set_scope`.
2. `SectionHandlers` (line 54) gains a third sink:

   ```rust
   struct SectionHandlers {
       apply: Rc<dyn Fn(&str)>,        // live: re-filters, every keystroke
       commit: Rc<dyn Fn(&str)>,       // chip: the committed query only
       clear_facets: Rc<dyn Fn()>,
   }
   ```

   `register()` grows the matching parameter. All six production call sites are
   in `section_search_wiring.rs` (lines 48, 66, 97, 123, 149, 175) plus one test
   stub at line 239 — a single file, so this is one edit, not a sweep.
3. One invariant, in one place. `SectionSearch` holds
   `surface: Cell<SearchSurface>` and every path that applies a query also
   commits it:

   ```rust
   fn apply_to_scope(&self, scope: SearchScope, query: &str) {
       // … existing apply …
       self.commit_to_scope(scope, search_chip::committed_query(query, self.surface.get()));
   }
   ```

   Opening the popover sets `surface = Open` and re-commits (chip disappears);
   closing sets `Closed` and re-commits (chip appears). Wire both from
   `SearchPopover::connect_open_changed`. Because the commit is recomputed from
   the live query every time, there is no second copy of the query to keep in
   sync — this is the whole reason the chip cannot drift from the filter.
4. `shortcuts.rs` (lines 216-270): `win.focus-search` toggles the popover
   instead of `search_bar.set_search_mode`. `next_search_mode(current)` stays as
   the pure toggle helper — it is already tested and rule-named. The
   `pending_focus` dance existed because a `GtkSearchBar`'s entry is unmapped
   until the reveal animation runs; `SearchPopover::open()` focuses after
   `popup()`, so drop `pending_focus` if it has no remaining reader, and say so
   in the commit message.
5. `window.rs` is **599 lines against a 600-line hard gate**
   (`scripts/check-architecture.sh:26`). It must not gain a single net line.
   `window.rs:462` and `window.rs:556` are the only touch points — swap the
   field, do not add wiring there. Put any new wiring in
   `window_runtime_wiring.rs` (line 44 declares `search_bar: &gtk4::SearchBar`,
   lines 90/412/544 pass it on).

### T5 — the filter bars read the committed query

Files owned: `crates/reprise-gnome/src/ui/podcasts/podcasts_filter_bar.rs`,
`radio/radio_filter_bar.rs`, `releases/releases_filter_bar.rs`,
`concerts/concerts_filter_bar.rs`, `browse/browse_bar.rs`,
`crates/reprise-gnome/src/ui/filter_bar_layout.rs`.

Each of the five scoped bars gets a `committed_query: RefCell<String>` and a
`set_committed_query(&self, query: &str)` that stores it and re-runs the bar's
existing chip rebuild. That method is the `commit` sink registered in T4.

Then exactly one line changes in each bar's rebuild — the argument handed to
`replace_scoped_search`:

| File | Line | today → after |
|---|---|---|
| `podcasts/podcasts_filter_bar.rs` | 284 | `&filter.query` → `&self.committed_query()` |
| `radio/radio_filter_bar.rs` | 388 | `&filter.query` → `&self.committed_query()` |
| `releases/releases_filter_bar.rs` | 228 | `&query` → `&self.committed_query()` |
| `concerts/concerts_filter_bar.rs` | 335 | `&query` → `&self.committed_query()` |
| `browse/browse_bar.rs` | 411 | `&query` (from `self.search`) → `&self.committed_query()` |

`filter_bar_layout::replace_scoped_search` keeps its signature. Its doc comment
gains one line: the query it receives is the **committed** query, and blank
means "no chip" (which it already implements at line 128). The `on_clear`
closures stay exactly as they are — the chip's × still clears the section's live
query, which now also clears the commit through T4's invariant, so
`replace_scoped_search` empties the slot on the next rebuild.

`preferences/preferences_search.rs:368` uses the unscoped `replace_search` for
the Preferences search, which is a different surface with its own entry. **Do
not touch it.**

### T6 — tests

New rule-named tests. The traceability gate
(`scripts/check-ux-traceability.sh`) requires ≥1 test per `[active]` rule and
**forbids any test naming a `[replaced]` ID**, so the renames below are
mandatory, not cosmetic.

New, in `search_popover_tests.rs` unless noted:

- `search_10_opening_search_changes_no_allocated_height` — realize a window with
  the chrome and the player-bar shell, measure the content area's allocated
  height and the player bar's bottom edge against the window's, open the
  popover, pump the main loop, measure again, close, measure again. All three
  equal. Needs a display; mark `#[ignore]` in the repo's display-test style and
  make sure `scripts/check-display-tests.sh --rule-named` picks it up.
  **Guard against a false green**: assert the popover actually reports
  `is_visible()` in the middle measurement, or a popover that never opened would
  pass this test trivially.
- `search_11_open_popover_shows_no_chip_while_the_count_already_filters` — with
  the popover open and `"wer"` typed, the layout's search slot is empty while
  the section's apply sink has already seen `"wer"`. Assert both halves; the
  count reflecting the query is the half that proves filtering stayed
  incremental.
- `search_12_closing_commits_exactly_one_chip_before_the_facets` — close with
  `"wer"`; assert the search slot has exactly one child and
  `layout.slot_order()` puts `FilterBarSlot::Search` before
  `FilterBarSlot::Facets` with both populated.
- `search_13_closing_with_a_blank_query_commits_nothing` — `""` and `"   "`,
  no chip, and the facet chips are untouched.
- `search_14_escape_discards_while_enter_commits_the_filtered_result_set` —
  Escape closes with an empty apply sink and no chip; Enter closes while the
  apply sink still holds `"wer"` and the chip is present.
- `search_15_reopening_hides_the_chip_and_prefills_the_entry` — with a committed
  chip present, open; the search slot empties, the entry text equals the chip's
  query, and the slot still has exactly one child after closing again (not two).

Renames and rewrites of existing tests (`library_chrome_tests.rs`,
`shortcuts.rs`):

| today | after |
|---|---|
| `search_1_idle_is_icon_not_field` | `search_1a_…` |
| `search_2b_bar_reveals_flush_under_headerbar` | `search_2c_popover_floats_without_reflowing` |
| `search_2b_ctrl_f_reveals_and_focuses` (shortcuts.rs) | `search_2c_ctrl_f_opens_and_focuses` |
| `search_4_escape_clears_then_collapses` (shortcuts.rs) | `search_4a_escape_closes_and_discards_the_query` — one press clears and closes |
| `search_4_escape_release_wins_over_late_search_bar_reopen` (shortcuts.rs) | delete — it guards a race that only a revealed top bar had |
| `search_4_explicit_clear_discards_the_preserved_query` | `search_4a_the_entrys_clear_icon_discards_the_query` |
| `search_7_*` (4 tests) | replace with one `search_7a_clicking_outside_closes_and_keeps_the_filter`; the held-pointer trio dies with the strip |
| `search_5_*`, `search_6_*` (5 tests) | keep the names and the assertions; only the harness moves from `SearchBar` to `SearchPopover` |

`section_search/tests.rs:31` builds a `FilterBarLayout` and calls
`replace_scoped_search` directly (line 49) — that harness now has to route
through the commit sink, or it will assert on a chip the production path no
longer builds there.

### T7 — gates

Run and make green, in the worktree:

```
cargo test -p reprise-view
cargo test -p reprise-gnome
scripts/check-frontend-thinness.sh
scripts/check-ux-traceability.sh
scripts/check-architecture.sh
scripts/check-display-tests.sh --rule-named
```

Two known traps from earlier runs in this repo:

- **Display tests are flaky in a herd.** A failure in the batch is not evidence
  on its own; re-run the failing test alone before believing it, and check the
  same test against the base commit before blaming this change.
- **Some display tests are already red on `origin/dev`.** Establish the baseline
  in a clean checkout of `7214a29de1` before reporting any red as a regression.

---

## Definition of done

- No `gtk4::SearchBar` remains in `crates/reprise-gnome/src` outside of test
  harnesses that legitimately test something else (`rg -n 'SearchBar' crates`).
- `docs/ux-rules.md` carries SEARCH-10…15 `[active]`, and SEARCH-1/2b/4/7 marked
  `[replaced by …]`, with every test renamed accordingly.
- `window.rs` is still under 600 lines; every touched file is under 800.
- The chip is built from a `commit` sink, and `rg -n 'connect_changed'` shows no
  path from a keystroke to `replace_scoped_search`.
