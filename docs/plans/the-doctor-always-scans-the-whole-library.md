---
slug: the-doctor-always-scans-the-whole-library
worktree: /home/marvin/Projects/reprise-the-doctor-always-scans-the-whole-library
branch: feature/the-doctor-always-scans-the-whole-library
phase: planned
codex_session:
created: 2026-08-30
---
# The Doctor always scans the whole library

The Library Doctor's start page offers a three-way scope choice — Whole Library,
Current View, Selection. It goes away. Every scan started from the start page
covers the whole library, and the page reads as one option (the remote switch),
one verb ("Run Scan Now") and one history line.

The Selection scope survives, but **without a selector**: "Unify spellings" in My
Stats starts its scan directly and goes straight to the running screen, never
through the start page. So the start page keeps exactly one meaning, and the one
workflow that genuinely needed a subset keeps working.

## What this is not

Three things that look in scope and are not:

- **The core scopes stay.** `DoctorScopeRequest::{WholeLibrary, CurrentView,
  Selection}` and `freeze_scope` are untouched. `doctor_actions.rs:469-499`
  builds its own `DoctorViewSnapshot` from the `track_ids` an agent passes, so
  `music_scan_tags` never reads the start page and keeps all three scopes.
- **DOC-9d is a different scope.** `doc_9d_the_footer_states_the_scope_of_the_filter`
  is about the *review page's* filter ("27 of 390 · filtered by Year"), not the
  scan scope. DOC-9d and all seven `doc_9d_*` tests stay exactly as they are.
- **No layout redesign.** Only the scope block is removed. The `column` box keeps
  its spacing of 26 and every other element keeps its place and wording.

## The rulebook comes first

The scope is written into the binding contract, so the code change is downstream
of a rulebook change. AGENTS.md §"UX rules are binding": IDs are append-only,
every `[active]` rule needs a rule-named test, and
`scripts/check-ux-traceability.sh` gates both directions.

**Both rules are amended in place, not replaced.** `[replaced by <ID>]` is for a
rule whose identity changes. DOC-7c's headline claim is "opens in the content
slot, not as a push over `content_nav`" — the scope sentence is one clause that
narrows, and DOC-7c already carries an `*Amended 2026-08-15*` note in exactly
this form. Keeping the IDs keeps all 13 `doc_7c_*` and 3 `doc_2a_*` tests
pointed where they are.

### DOC-7c (`docs/ux-rules.md:4773`, [active] [gtk])

Today the rule says:

> Its start page owns scope, the remote switch, "Run Scan Now" and the only
> "Revert Last Cleanup" in the app. […] Scope is not persistent: Whole Library by
> default, Current View suggested from a filtered view, Selection from a
> selection context.

Append an `*Amended 2026-08-30: …*` note saying: the start page owns no scope
choice — it owns the remote switch, "Run Scan Now" and the only "Revert Last
Cleanup", and every scan it starts covers the whole library. The "Scope is not
persistent" clause is void. The Selection scope survives without a selector:
"Unify spellings" in My Stats starts a scan over exactly its group's tracks and
goes straight to the running screen, never through the start page;
`music_scan_tags` keeps all three scopes per DOC-2a. Add both new test names to
the rule's `*Tests:*` list.

### DOC-2a (`docs/ux-rules.md:4497`, [active] [core])

Core is unchanged, so most of the rule stays true. One clause is not:

> An invalid or emptied invocation context visibly falls back to Whole Library.

What changes is not *whether* the fallback is visible but *what* it does. Append
an `*Amended 2026-08-30: …*` note: the start page can no longer produce an
invalid context, because every scan it starts is Whole Library. A Selection scan
from "Unify spellings" still can — its tracks may go missing between the click
and the freeze — and that case is now named rather than silently retried: the run
ends and says the tracks are gone, instead of falling back to a whole-library
scan nobody asked for. `ScopeFallbackRequired` (`scan.rs:65,90`) stays a core
outcome reported to its caller; the agent surface turns a non-`Completed` outcome
into an error at `doctor_actions.rs:48`.

Two clauses that do **not** change and that tasks 5 and 6 must honour:

- "Selection […] cannot be started while empty" — the Unify path must refuse an
  empty group rather than start a scan that falls back.
- "The last fully completed result survives navigation and restart with
  **scope**, timestamp, options, and provenance" — old summaries keep naming
  their scope.

## Tasks

Test-first per AGENTS.md. For a deletion, the failing test asserts the absence.

**1 — The two named tests.** In
`crates/reprise-gnome/src/ui/library_doctor/tests.rs`:

- `doc_7c_the_start_page_offers_no_scope_choice` — build the start page, assert
  no `adw::ToggleGroup` sits among its children.
- `doc_7c_unify_spellings_scans_its_group_without_the_start_page` — the Stats
  entry point starts a scan over exactly the given IDs and leaves the Doctor on
  the running screen, with the start page never shown.

Run both, watch them fail. They are the amended clauses' named tests and land in
the same commit as the amendment.

If either needs a display, its `#[ignore]` reason must be the exact marker the
existing tests use — `"requires a display; run via xvfb-run"`. Check 3 of
`check-ux-traceability.sh` allows that marker on any rule status but limits every
other `#[ignore]` to `[planned]` rules, and DOC-7c is `[active]`; invented wording
fails the gate.

**2 — Amend the rulebook.** DOC-7c and DOC-2a as worded above, each with its
`*Amended 2026-08-30: …*` note and the new test names.
Then `scripts/check-ux-traceability.sh`.

**3 — Start page.** `start_page.rs`: drop the `scope: adw::ToggleGroup` field
(33), its construction and the `scope_block` box carrying the "Scope" label
(93–122), the `selected_scope`/`set_selected_scope` pair (215–221), and
`self.scope.set_sensitive(!running)` in `set_running` (234–239). The remote row
becomes the column's first child; spacing stays at 26. `refresh()` (241–260) is
untouched — it already counts `ViewSource::Library` with an empty filter, so the
"1999 tracks · about 11 minutes" estimate stays correct and needs no new trigger.

**4 — Coordinator.** `mod.rs`:

- `start_scan` (491–512) splits into two entry points that each name their scope:
  `start_whole_library_scan()` and `start_selection_scan(track_ids)`. **Not** one
  function that infers the scope from a pending `selection_override` — the
  amendment says every scan the start page starts covers the whole library, and
  that invariant must be structural, not a matter of some other code path having
  cleared the override in time. "Run Scan Now" calls the first and can produce
  nothing else.
- Delete `suggested_scope` (771), `scope_choice` (779),
  `scope_selection_to_request` (789), and the `set_selected_scope` calls in
  `open` (422) and the rescan closure (698–705).
- **Keep** `selection_override` (137) and `current_view_snapshot` — the Unify
  path needs the first, and the second still has callers. Verify the second
  claim when the edit is made; if it turns out to have none, delete it.
- The `DoctorScanOutcome::ScopeFallbackRequired` arm (590–605) **keeps a visible
  toast** but loses its `start_scan()` retry. It stays reachable: `selection()`
  (`scope.rs:109-132`) filters on `PRESENT` and silently skips rows, so a
  non-empty ID list whose tracks went missing between the click and the freeze
  yields `FallbackRequired` (`scope.rs:23`) from a GUI-initiated scan. Retrying
  as a whole-library scan would be wrong here — nobody asked for an 11-minute run
  by clicking "Tag spellings" — so the arm ends the job and says the tracks are
  gone. See task 8 for the reworded string.

**5 — The Unify entry point.** `open_for_selection` (443) no longer preselects a
scope and opens the start page. It stores the IDs and starts the scan directly,
landing on the running screen. Per DOC-2a it must refuse an empty group rather
than start a scan that falls back — `group_track_ids` only yields groups with two
or more variants, so an empty list means something upstream is wrong; return
without starting. `window_runtime_wiring.rs:181` keeps calling it unchanged, and
the whole My Stats surface — "Tag spellings", the band-tile and genre edit icons,
the "N spellings merged" hints — stays exactly as it is.

**6 — Rescan.** The review page's rescan (`connect_rescan`, 692–708) reproduces
what the review covered, in two cases instead of three:

- `scope_kind == "whole_library"` → a fresh `WholeLibrary` request. A rescan is a
  new run, and DOC-2a freezes IDs at "Run scan now", so it must re-freeze and
  pick up tracks added since.
- anything else → `Selection { track_ids: scan.track_ids }`. The frozen set *is*
  the definition of what that review covered.

**7 — Old scans keep rendering.** `summary_model.rs:114-120` maps a stored
`scope_kind` to a label. It stays: scans persisted before this change carry
`"current_view"`/`"selection"`, and an agent scan still creates them. This is
DOC-2a compliance, not hygiene. Add a test that a summary built from a scan with
`scope_kind: "selection"` still names Selection.

**8 — Strings.** Exactly one constant loses its last user: `DOCTOR_SCOPE`, the
"Scope" label. `DOCTOR_SCOPE_WHOLE_LIBRARY`, `_CURRENT_VIEW` and `_SELECTION`
**stay** — task 7 needs them.

`DOCTOR_SCOPE_FALLBACK` also stays, but its text is now wrong: "That scope is no
longer available. Scanning the whole library instead." promises a retry that task
4 removes. Reword it to name what happened —
`N_!("Those tracks are no longer in the library.")`. That changes the msgid, and
`de` and `es` are complete locales that may carry no untranslated entries, so
both need a translation in the same commit:

| locale | msgstr |
|--------|--------|
| `de`   | `Diese Titel sind nicht mehr in der Bibliothek.` |
| `es`   | `Esas pistas ya no están en la biblioteca.` |

Remove `DOCTOR_SCOPE` from `strings_library_doctor.rs`, then regenerate
`po/reprise.pot` with the `xgettext` invocation from
`scripts/tests/gettext-catalogs.sh:21-25`, pointed at `--output=po/reprise.pot`.
The gate only compares `.po` against a freshly generated `.pot`, so a stale
committed `.pot` passes silently — regenerate it deliberately. The seven `.po`
files keep the dropped msgids as obsolete entries; `de` and `es` are complete
locales and must stay complete.

**9 — Remove the dead tests.** Delete
`doc_2a_scope_choice_freezes_the_requested_input_shape` and
`review_rescan_restores_the_scanned_scope_choice` (`tests.rs:57,75`), and
`doc_7c_entry_scope_defaults_to_library_and_suggests_filtered_view` (:82). The
traceability gate stays green: DOC-2a keeps `doc_2a_scope_freezes_present_track_ids`
and `doc_2a_last_complete_scan_survives_restart` in core, DOC-7c keeps eleven
existing tests plus the two from task 1. Leave
`invalid_context_requires_visible_scope_fallback` in `core/library_doctor/tests.rs`
alone — it covers the core path the agent surface still uses.

**10 — Look at it.** Take a screenshot of the start page through the environment
`scripts/check-display-tests.sh:228` sets up (`GSK_RENDERER=cairo`,
`GDK_BACKEND=x11`, `dbus-run-session`, `xvfb-run`) and actually look at it. The
absence test proves "no ToggleGroup"; it does not prove the card reads as
simplified, and that was the request. No permanent display test is added for the
start page.

## Gates

```
cargo fmt --all
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace          # never bare `cargo test` — that runs one member
scripts/check-ux-traceability.sh
scripts/tests/gettext-catalogs.sh
```

`scripts/check-merge-readiness.sh` wraps the last two.

## Parallelität

**No cut. One strand.**

The tasks are sequential by compilation, not by preference. Removing
`selected_scope` from `start_page.rs` (task 3) breaks `mod.rs`'s call sites in
the same compile, so tasks 3 and 4 cannot land in separate worktrees. Task 8
depends on the last string use disappearing in tasks 3 and 4; tasks 1, 2 and 9
are what the code change is measured against.

The one part that could have been its own strand — removing the My Stats
affordance across `ui/stats/**` — does not exist any more. The grill settled on
keeping "Unify spellings" and rerouting it, which leaves that crate corner
untouched and `window_runtime_wiring.rs` unchanged. What remains is a few hundred
lines in one directory plus two rulebook amendments. A two-strand cut would put
`start_page.rs` and `mod.rs` — the two files that must change together — on
either side of a merge, and the disjointness check in `/code` would reject it.
