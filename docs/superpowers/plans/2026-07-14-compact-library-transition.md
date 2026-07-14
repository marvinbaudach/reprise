# Compact-to-Library Transition — Implementation Plan

## Global Constraints

- Follow RED → GREEN TDD and preserve the single-window/single-player architecture.
- Keep English code/comments/commits and German internal design documentation.
- Never access the real desktop, music, database, cache, accounts, or session bus.
- Keep every substantially edited Rust file below 800 lines and Core dependency-pure.

## Task 1 — Mount Library content before requesting full geometry

Files: `crates/reprise-gnome/src/ui/minimal_view.rs`, matching design/Manual QA, and status docs.

TDD steps:

1. Add an isolated GTK regression that starts with a Compact placeholder mounted and observes the
   first `width-request` notification during `restore_library`. Require the Library root to be the
   mounted host content at that instant; run it and observe RED with the current geometry-first order.
2. Reorder `restore_library` so the Library root is installed before any full-size request while
   preserving resizable state, stored dimensions, maximization, and geometry tracking.
3. Run the focused display regression, existing transition/decorations tests, an isolated
   Compact-to-Library smoke, all workspace gates, release checker, file-size proof, and adversarial
   review.
4. Commit `fix: stabilize compact view restoration`.

Expected result: one new ignored display regression; all non-display counts remain unchanged.
