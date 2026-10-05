---
slug: refactor-wave-2026-10-w3c
worktree: /home/marvin/Projects/reprise-refactor-wave-2026-10-w3c
branch: feature/refactor-wave-2026-10-w3c
phase: coded
codex_session:
created: 2026-10-05
---
# Refactor wave 2026-10, wave 3 — strand C: the shared add-dialog scaffold

Mother plan: `docs/plans/refactor-wave-2026-10-w3.md`, and the "Standing rules for every strand" in
`docs/plans/refactor-wave-2026-10.md` bind this strand. Consolidation package 2.3
(`docs/plans/consolidation-plan.md` §4) and finding D2 (`docs/plans/architecture-consolidation.md`
§4.3) are the origin. **This strand is deliberately smaller than package 2.3** — see Decisions for why
the "one add dialog" cut is not behaviour-preservable on the current tree.

This plan is your only channel. When the code disagrees with a table below, the code wins: keep what
the code does and say so in your final message.

## Purpose

The Podcasts/YouTube add dialog and the Radio add dialog build their dialog chrome — content box,
search entry, status label, footnote, Cancel/primary footer, header bar, `adw::Dialog` — in one shared
place, and guard stale async results with one shared generation type. Every pixel, string, widget
order, focus target, keyboard path and accessibility role stays as it is today, and two new display
tests pin that before a line of production code moves.

## Evidence (origin/dev @ 0322ae01df, 2026-10-05)

Paths below are relative to `crates/reprise-gnome/src/ui/`.

| File | Lines | File | Lines |
| --- | --- | --- | --- |
| `podcasts/add_dialog.rs` | 726 | `radio/add_dialog.rs` | 776 |
| `podcasts/add_dialog_tests.rs` | 773 | `radio/add_dialog_tests.rs` | 775 |
| `podcasts/add_dialog_followers.rs` | 497 | `radio/add_dialog_location.rs` | 93 |
| `podcasts/add_dialog_results.rs` | 447 | `radio/add_dialog_network.rs` | 73 |
| `podcasts/add_dialog_input.rs` | 430 | `radio/add_dialog_rows.rs` | 96 |
| `podcasts/add_dialog_rows.rs` | 235 | `radio/radio_add_input.rs` | 18 |
| `podcasts/add_dialog_chips.rs` | 199 | `radio/radio_chips.rs`, `radio/station_preview.rs` | — |
| `podcasts/add_dialog_subscription.rs` | 90 | | |

Four files are within 30 lines of the 800-line cap. Both test files are declared as
`#[cfg(test)] #[path = "add_dialog_tests.rs"] mod tests;` at the end of their `add_dialog.rs`
(podcasts :724-726, radio :774-776).

**Why the two dialogs do not share a phase machine or a result list today:**

| Aspect | Podcasts (`podcasts/add_dialog.rs`) | Radio (`radio/add_dialog.rs`) |
| --- | --- | --- |
| Phase model | none at runtime. `AddDialogPhase` (:37-46) is `#[cfg(test)]`; its only use is `dialogue_state_names_cover_async_lifecycle` (`add_dialog_tests.rs:677`), which asserts the array has 6 entries. Phase is implicit in the status text and the children of `results`. | runtime `AddDialogState { phase, generation: u64, query, result_label }` (:49-55) with `AddDialogPhase { Idle, Searching, Results(Vec<StationCandidate>), Previewing, Preview(StationPreview), NearYou(NearYouAction), Error(AddFailure) }` (:32-47) and pure transitions `begin` (:76), `begin_chip_search` (:111), `accept` (:92), `can_confirm` (:120); `render(state)` (:617) resets every widget |
| Generation | `Rc<Cell<u64>>` created per `present` (:233); bumped on **every** submit before classify and refusal (:248-249) and by the charts chip (:327-328); compared at :524, :681 and `add_dialog_followers.rs:165` (`apply_if_current`), read at followers :242 | `u64` inside the cloned state; bumped only by `begin`/`begin_chip_search` with `wrapping_add(1)`; compared in `accept` (:93); reset to 0 by `Default` on every `present` (:414) and on an Empty submit (:430); refusals do not bump |
| Result list | `gtk4::Box` (vertical, 8, `margin_end 6`) of section Boxes and row Boxes inside a `ScrolledWindow` | `gtk4::ListBox` (`boxed-list`, `SelectionMode::None`) inside `LocationResults` (`gtk4::Stack` with `results`/`missing-location`/`missing-country` pages) |
| Lifecycle | a fresh `adw::Dialog` per `present` (:188) | one `Rc<RadioAddDialog>` built in `RadioView::new` (`radio/radio_view.rs:255`), re-presented |
| Async delivery | spawn failure **and** receive handled inside `glib::spawn_future_local` (:513-523, :669-679) after a generation check | spawn failure handled **synchronously** (`dispatch` :555-566), receive inside the future (:567-589) |
| Preview and commit | inline preview block; preview Subscribe keeps the dialog open and clears the block | preview card plus footer "Add station"; confirm calls `on_added()` then `dialog.close()` |
| `on_added` | `Rc<dyn Fn(i64, bool)>` | `Rc<dyn Fn()>` |
| Loading | status text only | `gtk4::Spinner` plus status text |
| Error | status text, no CSS class | status text with CSS class `error` |
| Input | `AddInput { Empty, Search, YoutubeUrl, FeedUrl }` via `podcasts::url_detect::detect`; foreign URL refused while typing (SRC-6) | `AddInput { Empty, Search, Url }` by `http://`/`https://` prefix (`radio_add_input.rs`) |

Unifying the result containers would change the accessibility tree (`ListBoxRow` versus `Box`
children); giving Podcasts a runtime phase model is new behaviour, not a move; the delivery helpers
differ in timing on the spawn-failure path. None of that is behaviour-preserving by construction.

**What is identical, line for line** (podcasts `build_surface` :79-186, radio `new` :186-260):

- content `gtk4::Box::new(Vertical, 12)` with all four margins set (18 podcasts, 16 radio)
- `gtk4::SearchEntry::builder().placeholder_text(<hint>).build()`, appended first
- `status = gtk4::Label::new(None)`, `add_css_class("reprise-text-secondary")`, `set_xalign(0.0)`;
  radio also `set_wrap(true)`
- footnote `gtk4::Label::new(Some(<text>))`, classes `caption` and `reprise-text-secondary`,
  `set_xalign(0.0)`, `set_wrap(true)`, appended after the body
- `cancel = gtk4::Button::with_label(<cancel>)`; `primary = gtk4::Button::with_label(<primary>)`,
  `add_css_class("suggested-action")`, `set_sensitive(false)`; footer
  `gtk4::Box::new(Horizontal, 8)`, `set_halign(End)`, cancel then primary, appended last
- `adw::HeaderBar::new()` with `set_title_widget(Some(&adw::WindowTitle::new(<title>, "")))`;
  `adw::ToolbarView::new()`, `add_top_bar(&header)`, `set_content(Some(&content))`
- `adw::Dialog::builder().content_width(W).content_height(H).child(&toolbar)` — podcasts also
  `.title(<title>)` (:170), radio does not (:256-260)
- `cancel.connect_clicked` → `dialog.close()` through a `downgrade()`d handle (podcasts :367-371,
  radio :302-307)
- `dialog.present(Some(parent)); entry.grab_focus();` as the last two statements of `present`
  (podcasts :378-379; radio: check the end of `present`, :411 onwards)

Sizes: podcasts `CONTENT_WIDTH = 620`, `CONTENT_HEIGHT = 560` (:53-54); radio 560 × 620 (:29-30).

**Strings.** All copy comes from `strings_podcasts.rs`, `strings_radio.rs`, `strings_sources.rs`,
`strings_online_sources.rs` through `strings::text(..)`; the dialog files contain no gettext call and
are not listed in `po/POTFILES.in` (only the strings files are). The new module must receive
already-translated `String`s and contain no gettext call either.

**Tests that prove nothing changed** (names are load-bearing: `scripts/check-ux-traceability.sh`
maps `fn <rule>_<n>_…` within five lines after `#[test]` to the rule, and accepts `#[ignore]` only as
the exact line `#[ignore = "requires a display; run via xvfb-run"]`):

- `podcasts/add_dialog_tests.rs`: display — `src_6_the_foreign_url_hint_appears_while_typing`,
  `net_3_search_is_disabled_offline_but_a_url_stays_submittable`,
  `src_8_add_dialog_results_scroll_vertically_only`,
  `src_3a_add_dialog_has_fixed_cancel_and_primary_actions`,
  `src_15a_the_library_chip_appears_only_with_a_genre_to_suggest`,
  `src_19_the_apple_dialog_carries_the_charts_chip_and_the_entry_stays_empty`,
  `src_19_the_charts_chip_is_absent_when_online_sources_are_off`,
  `src_18_a_result_row_states_its_freshness_after_the_author`,
  `src_9_candidate_rows_expose_the_existing_root_and_subtitle_for_wave_two`,
  `src_21_highlighted_title_keeps_its_end_ellipsis_and_pango_attributes`,
  `src_22_unexplained_marker_is_accessible_and_keeps_its_space`,
  `src_8_a_long_result_row_never_widens_the_dialog`,
  `src_5_result_rows_use_the_source_artwork_surface`,
  `src_11_result_row_stays_on_the_fallback_when_images_are_not_allowed`,
  `src_7_a_successful_subscribe_acknowledges_the_row_in_place`,
  `the_add_dialog_offers_an_automatic_fill_switch`; pure —
  `src_11_add_dialog_images_allowed_is_the_net_1a_and`,
  `src_19_an_empty_chart_does_not_speak_the_language_of_a_failed_search`,
  `dialogue_state_names_cover_async_lifecycle`, `pod_13_preview_error_never_forwards_a_leaking_payload`,
  `disabling_initial_import_persists_the_previewed_guid_baseline`,
  `new_subscription_uses_the_selected_automatic_fill_choice`,
  `new_subscription_discovery_inherits_the_configured_automatic_fill_default`.
- `podcasts/add_dialog_followers.rs`: display — `src_23_largest_first_is_focusable_while_counts_are_pending`,
  `src_23_terminal_enrichment_failure_makes_largest_first_unavailable`,
  `src_9_wave_two_joins_by_channel_id_and_preserves_query_highlighting`,
  `a_second_search_discards_the_first_searchs_follower_result`,
  `src_23_pending_largest_first_reorders_once_when_counts_arrive`; pure —
  `src_9_the_two_argv_search_path_reaches_the_channel_subtitle`.
- `radio/add_dialog_tests.rs`: display — `src_7_a_successful_radio_add_acknowledges_the_row_in_place`,
  `src_11_radio_search_result_stays_on_the_fallback_when_images_are_not_allowed`,
  `rad_6_radio_results_highlight_only_the_station_name`,
  `src_8_radio_results_scroll_inside_a_bounded_viewport`,
  `rad_5_the_library_chip_appears_from_the_library_and_searches_what_it_says`,
  `src_8_a_long_station_name_never_widens_the_dialog`,
  `rad_5_near_you_click_without_a_location_shows_an_empty_state_and_dispatches_no_search`,
  `rad_5_near_you_with_countryless_location_uses_distinct_honest_copy`,
  `rad_5_location_broadcast_resumes_the_open_near_you_intent`,
  `rad_5_near_you_click_with_a_location_dispatches_a_search_and_never_opens_settings`,
  `net_1a_radio_search_never_reaches_the_directory_while_the_switch_is_off`,
  `net_3_radio_search_is_refused_offline_but_a_url_still_reaches_preview`,
  `net_3_confirming_an_offline_url_preview_persists_the_station`; pure —
  `src_3a_radio_add_dialog_submits_search_or_url_through_one_field`,
  `dialog_state_ignores_stale_results_and_requires_a_valid_preview`,
  `rad_8_radio_favicon_lookup_accepts_only_secure_station_identity`,
  `rad_8_placeholder_is_display_only_not_a_name_claim`, `src_5_radio_search_hides_existing_favorites`,
  `src_11_radio_add_dialog_images_allowed_is_the_net_1a_and`, `src_5_radio_url_preview_hides_an_existing_favorite`,
  `rad_4_playlist_type_is_detected_without_consuming_a_live_stream`,
  `rad_6_only_text_searches_supply_station_name_highlighting`.
- Pure tests in `podcasts/add_dialog_input.rs` (12), `podcasts/add_dialog_results.rs` (9),
  `podcasts/add_dialog_chips.rs` (7), `radio/radio_chips.rs` (6), `source_add_action.rs` (3); display
  tests in `window/window_online_module_effects_tests.rs:198,236` through `RadioTestHandle`.
- `scripts/cua-e2e/source_content.sh:69-167` clicks the visible labels "Add Podcast", "Preview",
  "Subscribe", "Cancel", "Add Channel" and the SRC-6 sentence, types into the dialog **without
  clicking the entry first** (relies on `entry.grab_focus()`), and relies on Subscribe leaving the
  dialog open and Cancel closing it.

**Source-scanning tests and scripts that touch these files:** no `include_str!` anywhere points at a
file in this strand. `ui/icons.rs:57-80` scans every `.rs` under `src/ui` for `"…-symbolic"` literals
(no new icon names → unaffected). `scripts/check-frontend-thinness.sh` counts gnome production lines;
its awk skips from a column-zero `#[cfg(test)]` to the next column-zero `}`, so the `#[cfg(test)] use`
lines at `podcasts/add_dialog.rs:31,33` already hide lines 32-186 from it; the dialog files contain no
`rusqlite`/`std::fs`/`thread::spawn` token except `add_dialog_subscription.rs:54` (`rusqlite::Error`,
not touched by this strand). `scripts/check-gnome-idioms.sh` strips tests by the exact
`#[cfg(test)] #[path = "…"] mod tests;` shape and bans `.unwrap()` in production (GP-4).
`scripts/check-architecture.sh:272-289` pins `too_many_arguments_budget=29`; two of the 29 are
`#[expect(clippy::too_many_arguments, reason = …)]` on `attach_candidates` (`podcasts/add_dialog.rs:490-493`)
and `preview` (`:585-588`) — they must stay fulfilled.

## Decisions (fixed — do not re-open)

1. **Package 2.3 shrinks to a scaffold.** A shared phase machine would have to be *added* to Podcasts
   (it has none), a shared result list would change widget classes and accessibility roles, and the
   async delivery differs in when a spawn failure is reported. Each of those is a product or
   accessibility decision, recorded in the mother plan for wave 4. What is shared now is exactly what
   is already identical: the chrome, the generation counter's semantics, and one duplicated test helper.
2. **The chrome is a builder with a body closure**, so each dialog keeps its exact child order: the
   builder appends the entry first and the footnote and footer last; the dialog appends everything in
   between, including the status label, where it sits today.
3. **Every difference is a spec field, never a fix.** Podcasts' dialog carries `.title(..)` and radio's
   does not (so radio's dialog has no accessible name today); radio's status label wraps and podcasts'
   does not; margins 18 versus 16. Fixing radio's title is an accessibility change for wave 4 under an
   ACC rule, not a drive-by here.
4. **`Generation` is a `Copy` newtype over `u64`** with `next()` = `wrapping_add(1)` and derived
   equality and `Default`, so radio's pure state stays pure and podcasts' `Rc<Cell<_>>` stays a cell.
5. **No new strings, no UX-rule edits, no test renames, no change to `too_many_arguments` counts.**
6. **The pinning tests are display tests without a rule prefix**, in new sibling files (the existing
   test files are at 773 and 775 lines). They are not part of the `--rule-named` merge gate; you run
   them locally, one process each.

## Owns

This list is a starting point, not a fence. Stop only if the contract itself turns out wrong.

- New: `crates/reprise-gnome/src/ui/source_add_dialog/{mod,chrome,generation,test_support}.rs`
- `crates/reprise-gnome/src/ui/mod.rs` — one `mod source_add_dialog;` line (strand A does not touch
  this file; strand B does not touch `reprise-gnome`)
- `crates/reprise-gnome/src/ui/podcasts/add_dialog.rs`, `podcasts/add_dialog_followers.rs`
  (generation type only), `podcasts/add_dialog_tests.rs` (helper import only),
  new `podcasts/add_dialog_chrome_tests.rs`
- `crates/reprise-gnome/src/ui/radio/add_dialog.rs`, `radio/add_dialog_tests.rs` (generation literals
  and helper import), new `radio/add_dialog_chrome_tests.rs`

Not owned: `podcasts/add_dialog_subscription.rs` (strand A adds a one-line adapter there; you rebase
onto `dev` after A lands and keep it), every other `podcasts/*` and `radio/*` file, `strings_*.rs`,
`po/`, `docs/ux-rules.md`, every script.

## The new module — exact shapes

`crates/reprise-gnome/src/ui/source_add_dialog/mod.rs`:

```rust
//! Shared scaffolding for the "add a source" dialogs (Podcasts/YouTube and Radio): the dialog
//! chrome every source builds the same way, and the request generation that drops stale async
//! results. The phase model, the result list and the preview stay per source.
pub(in crate::ui) mod chrome;
pub(in crate::ui) mod generation;
#[cfg(test)]
pub(in crate::ui) mod test_support;
```

Declare it in `ui/mod.rs` with the same visibility `source_add_action` uses there. No re-exports
(an unused re-export is an `unused_imports` error under `-D warnings`).

`chrome.rs`:

```rust
/// Everything that differs between the add dialogs' chrome. Already translated text only.
pub(in crate::ui) struct ChromeSpec {
    pub title: String,          // header `adw::WindowTitle`
    pub dialog_title: bool,     // also `.title(..)` on the `adw::Dialog` (Podcasts: true, Radio: false)
    pub hint: String,           // SearchEntry placeholder
    pub footnote: String,       // may contain '\n'
    pub cancel_label: String,
    pub primary_label: String,
    pub content_width: i32,
    pub content_height: i32,
    pub margin: i32,            // all four margins of the content box
    pub status_wraps: bool,
}

pub(in crate::ui) struct SourceAddChrome {
    pub dialog: adw::Dialog,
    pub content: gtk4::Box,
    pub entry: gtk4::SearchEntry,
    pub status: gtk4::Label,
    pub footnote: gtk4::Label,
    pub cancel: gtk4::Button,
    pub primary: gtk4::Button,
    pub footer: gtk4::Box,
}

impl SourceAddChrome {
    /// Builds the chrome. `body` appends the source-specific middle (it receives the content box and
    /// the status label, which it must append itself, where the source wants it).
    pub(in crate::ui) fn build(spec: ChromeSpec, body: impl FnOnce(&gtk4::Box, &gtk4::Label)) -> Self;
    /// `dialog.present(Some(parent))` then `entry.grab_focus()` — the two statements every add
    /// dialog ends `present` with.
    pub(in crate::ui) fn present(&self, parent: &impl IsA<gtk4::Widget>);
}
```

`build` does exactly this, in this order: create `content` (Vertical, 12; four margins = `margin`);
create `entry` (`SearchEntry::builder().placeholder_text(&hint).build()`); create `status`
(`Label::new(None)`, class `reprise-text-secondary`, `set_xalign(0.0)`, `set_wrap(status_wraps)` only
when true — do not call `set_wrap(false)`); create `footnote` (`Label::new(Some(&footnote))`, classes
`caption` then `reprise-text-secondary`, `set_xalign(0.0)`, `set_wrap(true)`); create `cancel`
(`Button::with_label`); create `primary` (`Button::with_label`, class `suggested-action`,
`set_sensitive(false)`); create `footer` (Horizontal, 8, `set_halign(End)`, append cancel, append
primary); `content.append(&entry)`; `body(&content, &status)`; `content.append(&footnote)`;
`content.append(&footer)`; header (`adw::HeaderBar::new()`, `set_title_widget(Some(&adw::WindowTitle::new(&title, "")))`);
toolbar (`adw::ToolbarView::new()`, `add_top_bar`, `set_content(Some(&content))`); dialog
(`adw::Dialog::builder().content_width(..).content_height(..).child(&toolbar)` plus `.title(&title)`
when `dialog_title`); wire `cancel.connect_clicked` to `dialog.close()` through a downgraded handle.
No `.unwrap()`, no `expect` (GP-4 bans them in production frontend code).

`generation.rs`:

```rust
/// Which request an async result belongs to. A dialog bumps it when it starts a new request and drops
/// every result that carries an older value. Wraps on overflow, like the `u64` counters it replaces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) struct Generation(u64);

impl Generation {
    #[must_use]
    pub(in crate::ui) fn next(self) -> Self { Self(self.0.wrapping_add(1)) }
}
```

`test_support.rs` (`#[cfg(test)]`): `pub(in crate::ui) fn find_scroller(widget: &gtk4::Widget) -> Option<gtk4::ScrolledWindow>`,
moved from `podcasts/add_dialog_tests.rs:4` (identical to `radio/add_dialog_tests.rs:418`).

## Spec values per dialog (must match the code; the code wins)

| Field | Podcasts (`build_surface`) | Radio (`RadioAddDialog::new`) |
| --- | --- | --- |
| `title` | `strings::text(dialog_title(kind))` | `strings::text(strings::RADIO_DIALOG_TITLE)` |
| `dialog_title` | `true` | `false` |
| `hint` | `strings::text(dialog_hint(kind))` | `strings::text(strings::RADIO_DIALOG_HINT)` |
| `footnote` | `strings::text(strings::SOURCE_SUBSCRIBED_DROP_OUT)` | `format!("{}\n{}", strings::text(strings::RADIO_COMMUNITY_FOOTNOTE), strings::text(strings::SOURCE_SUBSCRIBED_DROP_OUT))` |
| `cancel_label` | `strings::text(strings::PODCAST_CANCEL)` | `strings::text(strings::RADIO_CANCEL)` |
| `primary_label` | `strings::text(strings::PODCAST_SEARCH)` | `strings::text(strings::RADIO_ADD)` |
| `content_width` / `content_height` | `CONTENT_WIDTH` 620 / `CONTENT_HEIGHT` 560 | `CONTENT_WIDTH` 560 / `CONTENT_HEIGHT` 620 |
| `margin` | 18 | 16 |
| `status_wraps` | `false` | `true` |
| `body` appends, in order | the optional suggestion chip (created before `build`), `status`, the `ScrolledWindow` around `results` | `chips.root`, `spinner`, `status`, `location_results.widget()`, `preview`, `fetch_row` |

Everything the body needs (chip, results, scroller, chips, spinner, preview, fetch_row,
`LocationResults`) is created **before** `build` exactly as today and moved into the closure.

## Tasks (in order, one commit each)

**C1 — pin the chrome first (green before any production change).** Create
`podcasts/add_dialog_chrome_tests.rs` and `radio/add_dialog_chrome_tests.rs`, declared from each
`add_dialog.rs` directly below the existing `mod tests;` declaration with the same three-line shape
(`#[cfg(test)]`, `#[path = "add_dialog_chrome_tests.rs"]`, `mod chrome_tests;`). Each file holds one
test, `#[ignore = "requires a display; run via xvfb-run"]`, calling `gtk4::init()` the way the
neighbouring display tests do, building the dialog the way they do (podcasts:
`build_surface(PodcastKind::Rss, Connectivity::Online, true, "DE", None)`; radio:
`RadioAddDialog::new(conn, Rc::new(Cell::new(Connectivity::Online)), || {})` with the same `Db` the
existing tests use), then walking the real widget tree from `dialog.child()` →
`adw::ToolbarView::content()` → the content box, and asserting in order:

- podcasts `add_dialog_chrome_is_built_in_the_documented_order`: children are exactly
  `[SearchEntry, Button, Label, ScrolledWindow, Label, Box]` (the `Button` is the charts chip for Rss
  online; if the chip is absent for your arguments, drop it from the expectation and say so); entry
  placeholder == `strings::text(strings::PODCAST_DIALOG_HINT)`; status has class
  `reprise-text-secondary`, `xalign() == 0.0`, `!wraps()`; footnote text ==
  `strings::text(strings::SOURCE_SUBSCRIBED_DROP_OUT)`, has classes `caption` and
  `reprise-text-secondary` (compare as a set), `wraps()`, `xalign() == 0.0`; footer `halign() == End`,
  children `[Button, Button]` with labels `PODCAST_CANCEL` / `PODCAST_SEARCH`, second has
  `suggested-action` and `!is_sensitive()`; content `spacing() == 12` and all four margins 18;
  `dialog.title()` == `strings::text(strings::PODCAST_DIALOG_TITLE)`, `content_width() == 620`,
  `content_height() == 560`.
- radio `add_dialog_chrome_is_built_in_the_documented_order`: children are exactly
  `[SearchEntry, Box, Spinner, Label, Stack, Box, Box, Label, Box]`; entry placeholder ==
  `text(RADIO_DIALOG_HINT)`; the second child has class `reprise-radio-chips`; status `wraps()`,
  `xalign() == 0.0`, class `reprise-text-secondary`; the sixth child has class `card` and
  `!is_visible()`; footnote text == `format!("{}\n{}", text(RADIO_COMMUNITY_FOOTNOTE), text(SOURCE_SUBSCRIBED_DROP_OUT))`
  with the two classes; footer children labels `RADIO_CANCEL` / `RADIO_ADD`, second `suggested-action`
  and insensitive; margins 16, spacing 12; **`dialog.title()` is empty** (pins today's absence);
  `content_width() == 560`, `content_height() == 620`.

Run both under xvfb (command in Verification). Both must pass on the untouched code. Commit.

**C2 — the module.** Create `source_add_dialog/{mod,chrome,generation,test_support}.rs` as specified
and the `ui/mod.rs` line. Add pure tests next to `generation.rs` (sibling `generation_tests.rs`,
declared `#[cfg(test)]`): `default_is_zero_like_the_counters_it_replaces` (`Generation::default() == Generation(0)`
— make the field `pub(super)` or add a `cfg(test)` constructor), `next_increments_and_wraps_at_the_maximum`
(`Generation(u64::MAX).next() == Generation(0)`), `equality_is_by_value`. `cargo test -p reprise-gnome source_add_dialog`
must report these tests, not "0 tests". Commit.

**C3 — podcasts.** In `build_surface`: create `chip_action`/`suggestion_chip` (the chip itself is built
and `add_css_class("pill")`/`set_halign(Start)` exactly as today, but **not appended yet**), `results`
and `scroller` as today; then `SourceAddChrome::build(spec, |content, status| { if let Some(chip) = &suggestion_chip { content.append(chip); } content.append(status); content.append(&scroller); })`.
Keep the SRC-15a/SRC-19, SRC-8 and SRC-7 comments next to the code they explain. After `build`, the
rest of `build_surface` (`set_status_hint`, `entry.connect_changed`) is unchanged and uses
`chrome.entry`/`chrome.status`/`chrome.primary`. Fill `AddDialogSurface` from the chrome by cloning
the widget handles (`dialog: chrome.dialog.clone()`, …) so every existing test keeps reading
`surface.primary`, `surface.cancel`, `surface.entry`, `surface.status`, `surface.dialog` unchanged;
keep the `chrome` itself as a new field if `present` needs it. Delete the cancel wiring at :367-371
(the chrome did it). Replace the last two statements of `present` (:378-379) with
`surface.chrome.present(parent)` (or keep the two calls if you keep no chrome field — the behaviour
is identical either way). Generation: `Rc<Cell<u64>>` → `Rc<Cell<Generation>>`; `:233` becomes
`Rc::new(Cell::new(Generation::default()))`; `:248-249` and `:327-328` become
`let next = generation.get().next(); generation.set(next);`; the `request_generation: u64` parameters
of `search`, `load_charts`, `attach_candidates`, `preview`, the `SearchContext.generation` field and
`add_dialog_followers.rs` (`apply_if_current`'s two generation parameters, the read at :242) become
`Generation`. **Keep every parameter count unchanged** — the two `#[expect(clippy::too_many_arguments)]`
must stay fulfilled, and `too_many_arguments_budget` stays 29. Run the C1 test and the podcast display
tests. Commit.

**C4 — radio.** In `new`: create `chips`, `spinner`, `results`, `location_results`, `preview`,
`fetch_label`/`fetch_metadata`/`fetch_row` exactly as today, then
`SourceAddChrome::build(spec, |content, status| { content.append(&chips.root); content.append(&spinner); content.append(status); content.append(location_results.widget()); content.append(&preview); content.append(&fetch_row); })`.
Keep the RAD-5 and SRC-7 comments. Fill `DialogWidgets` from the chrome by cloning handles
(`confirm` is the chrome's `primary`); delete the local cancel wiring (:302-307). If `present` ends
with `dialog.present(Some(parent)); entry.grab_focus();` as adjacent statements, replace them with
`self.widgets.chrome.present(parent)` (add the field); otherwise leave `present` alone and say so.
Generation: `AddDialogState.generation: Generation` (derive `Default` still yields zero); `begin`
and `begin_chip_search` use `.next()`; `accept(mut self, generation: Generation, ..)`;
`dispatch(.., generation: Generation, ..)` and the locals in `submit`, `run_chip_search`,
`submit_url_offline` follow. Adapt the pure test `dialog_state_ignores_stale_results_and_requires_a_valid_preview`
only where it names a `u64` literal (use the generation the state returns, or `Generation::default()`);
its assertions stay. Run the C1 test and the radio display tests. Commit.

**C5 — one `find_scroller`.** Move the helper into `source_add_dialog/test_support.rs` and import it
in both `add_dialog_tests.rs` files (and the chrome tests if they use it). Both test files shrink.
Commit.

## Known traps

- **Size cap.** `radio/add_dialog.rs` 776, `radio/add_dialog_tests.rs` 775, `podcasts/add_dialog_tests.rs`
  773, `podcasts/add_dialog.rs` 726. C1 adds three lines to each `add_dialog.rs`; C3/C4 remove more.
  Never add the pinning tests to the existing test files. Check `wc -l` after every commit.
- **The `#[cfg(test)] #[path = "…"] mod …;` shape** is what `scripts/check-gnome-idioms.sh` and
  `scripts/check-frontend-thinness.sh` key on. Keep the three attributes on three lines at column
  zero, exactly like the existing declaration above them.
- **Widget construction order is focus order.** The body closure is the only place the middle widgets
  are appended; copy today's `content.append` sequence verbatim. Nothing else in `build` may reorder.
- **CSS class order is not semantic** but `css_classes()` returns insertion order; compare as sets in
  tests. Styling is unaffected either way.
- **Do not improve anything.** No `.title()` for radio, no `wrap` for podcasts' status, no shared
  margins, no spinner for podcasts. Every one of those is a visible change.
- **`significant_drop_in_scrutinee`** is a workspace lint: keep radio's
  `let state = self.state.borrow().clone().accept(..); self.render(state);` as two statements — never
  put a `borrow()` in a `match`/`if let` scrutinee.
- **`allow_attributes_without_reason`** is on; any new suppression needs `reason`. CI clippy is 1.99,
  local 1.97: never delete an existing suppression as unfulfilled without checking it under both.
- **Feature gates.** `reprise-gnome` has no feature that touches these files, but the workspace command
  `cargo clippy --all-targets --workspace --all-features -- -D warnings` must still pass.
- **No gettext in the new module**, no new icon literal, no `.unwrap()`/`.expect()` in production code,
  no `std::thread::sleep`, no `#[strong]` in `clone!` (`check-gnome-idioms.sh`).
- **Display tests need one process each.** Filtering several `#[ignore]` GTK tests into one `cargo test`
  process aborts in `gtk_init`. Run them one at a time with `--exact` and the **full path** from
  `cargo test -p reprise-gnome -- --ignored --list` (a short name with `--exact` runs 0 tests and
  reports success). Every run must say `1 passed`.
- **Headless isolation is mandatory** for any GTK process you start (AGENTS.md): the command must
  contain `dbus-run-session`, `xvfb-run -a`, `XDG_DATA_HOME`, `XDG_CACHE_HOME`, `GDK_BACKEND=x11`,
  `WAYLAND_DISPLAY=` and `REPRISE_AUDIO_SINK=fakesink`.
- **Never cite a `docs/plans/…` path from code**: the architecture gate fails when a cited plan is
  deleted on landing, and wave plans are deleted on landing.
- **Strand A lands first** and adds a one-line adapter in `podcasts/add_dialog_subscription.rs`; rebase
  onto `dev` before opening the pull request and keep that line.
- **English everywhere**, focused commits, no agent attribution lines.

## Verification

```
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo clippy --all-targets --workspace --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo test -p reprise-gnome source_add_dialog        # the new pure tests; must list > 0 tests
cargo test -p reprise-gnome podcasts::add_dialog      # every non-ignored podcast dialog test
cargo test -p reprise-gnome radio::add_dialog         # every non-ignored radio dialog test
cargo test -p reprise-gnome radio_chips
cargo test -p reprise-gnome source_add_action
scripts/check-architecture.sh                         # too_many_arguments still 29; size caps
scripts/check-frontend-thinness.sh                    # every budget unchanged
scripts/check-gnome-idioms.sh
scripts/check-accessibility-semantics.sh
scripts/check-input-parity.sh
scripts/check-motion-tokens.sh
scripts/check-ux-traceability.sh
scripts/tests/gettext-catalogs.sh                     # no msgid may have changed
```

Display tests — every `#[ignore]` test named in "Tests that prove nothing changed" plus the two C1
tests and the two `window_online_module_effects_tests.rs` tests, one process each:

```
name=<full path from: cargo test -p reprise-gnome -- --ignored --list | rg 'add_dialog|source_add|online_module_effects'>
dbus-run-session -- xvfb-run -a env \
  XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
  cargo test -p reprise-gnome "$name" -- --ignored --exact
```

Grep your own command for `XDG_DATA_HOME` before running it. Never run the unfiltered `reprise-gnome`
suite. The orchestrator runs `scripts/check-merge-readiness.sh` (which includes
`scripts/check-display-tests.sh --rule-named`) after the code phase. Report: the two pinning tests'
results before and after, every spec value where the code disagreed with the table, the line counts of
the four near-cap files, and whether `present` was unified in both dialogs.
