---
slug: responsive-editing-and-one-table-grammar-c
worktree: /home/marvin/Projects/reprise-responsive-editing-and-one-table-grammar-c
branch: feature/responsive-editing-and-one-table-grammar-c
phase: shipped
codex_session:
created: 2026-09-05
---
# Strand C — `structure`: `wire()` becomes an index

Strand C of `docs/plans/responsive-editing-and-one-table-grammar.md`. Read the
mother plan's §1 (goal G7, the Package 3.4 non-goal) and §2 (rules) first; §2
binds. Branch `feature/responsive-editing-and-one-table-grammar-c`. One task.

## File ownership

- Owns: `crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs`,
  `crates/reprise-gnome/src/ui/window/wiring/**` (new).
- Never edits: `ui/window/window_action_wiring.rs` (strand A's), any view
  file (strand B's). The view constructors this strand calls keep their
  signatures; if one seems to need a change, stop and report.
- Does not edit `docs/ux-rules.md`.

## Task C1 — one file per wiring concern

**Files.** `crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs`; new
`crates/reprise-gnome/src/ui/window/wiring/{mod,deferred_sources,library_doctor,
compact_mode,menu,playing_source,nav_back,section_search,clear_all,listeners,
view_session,close,session_restore,deep_link}.rs`.

`wire(args: RuntimeWiring<'_>)` (`:94-735`) has thirteen statement groups with
comment headings (`95-151` deferred source wiring, `153-188` library doctor,
`194-202` compact mode, `215-269` spectrogram and menu, `271-281` playing
source, `283-319` nav back, `328-334` section search, `345-393` clear-all and
routing, `421-461` listeners, `477-499` view session, `508-604` close,
`646-712` session restore, `718-735` deep link and smoke). Each becomes
`pub(super) fn wire_<concern>(w: &RuntimeWiring<'_>)` in its own file, moved
verbatim; `wire()` shrinks to the thirteen calls **in the same order**, with a
comment stating that the order is load-bearing (session restore after
listeners, close after view session). `RuntimeWiring`'s 44 fields do not
change — Package 3.4 does that later, one view at a time.

Locals that cross two groups (the survey did not count them; there will be a
few `Rc` clones) are hoisted into a small `WiringScratch` struct passed by
reference, not turned into fields.

Tests: no new behaviour, so the proof is the existing coverage —
`device_sync_page_external_changes_display_tests.rs` and
`external_changes/tests.rs` name `wire()`; run them individually, one process
each under `xvfb-run -a` where they are display tests. Plus the startup smoke
the merge-readiness gate runs. The mutation check that matters: temporarily
swap two of the thirteen calls and watch a session-restore test go red, then
restore the order and record both runs in the commit message.

## Acceptance for strand C

- `window_runtime_wiring.rs` under 200 lines; no file in `wiring/` over 200.
- Behaviour byte-identical: `git diff --stat` shows only moves, and a
  `git diff --color-moved=dimmed-zebra` reads as pure relocation apart from the
  new function signatures, the `mod` declarations and the `WiringScratch`
  struct.
- The mutation check went red and back to green, both recorded.

## Abort criteria

- A `RefCell` borrow that lived across two groups inside one function becomes
  visible when the groups are separate functions. That is a finding, not an
  obstacle: copy the value out in its own statement and note it in the commit.
- A group that cannot be moved without changing a view constructor's signature
  stays in `wire()` with a comment saying why; report it. Do not edit the view.
