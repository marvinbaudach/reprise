# Reliable Separate Title Bar — Implementation Plan

## Global Constraints

- TDD: observe the focused display contract fail before implementation.
- English code/UI/commits; German design specification and complete German gettext.
- Never access the real desktop, music, database, cache, accounts, or session bus.
- Keep Chromium-style integrated CSD as the default and preserve `client`/`system` tokens.
- Keep every substantially edited source file below 800 lines and Core dependency-pure.

## Task 1 — Replace the ineffective SSD request with a native separate title bar

Files: `crates/reprise-gnome/src/ui/window_decorations.rs`, decoration preference/gettext files,
current decoration specifications, `docs/agent-workflow/MANUAL-QA.md`, and status documents.

TDD steps:

1. Change the isolated GTK contract to require no separate bar in Client mode, a native
   `GtkHeaderBar` top bar with `Reprise` and title buttons in System mode, hidden integrated
   controls/duplicate Compact titles, stable content, and a complete live roundtrip. Run it under
   the isolated Xvfb/D-Bus/XDG environment and observe RED, including the rejected
   `GtkWindow:set_titlebar` attempt on `AdwApplicationWindow`.
2. Add one reusable outer `AdwToolbarView` and separate title bar to `WindowDecorations`, route
   Library/Compact root changes through its content slot, project title visibility without
   re-entrant borrows, refresh Compact geometry, and remove the ineffective CSS-`ssd`
   confirmation path. Preserve pre-present startup application and live switching.
3. Rename the visible alternative to `Separate title bar`, update truthful explanatory copy and
   German gettext, and revise superseded decoration/fallback documentation and Manual QA.
4. Run focused pure/UI tests, the display contract, a two-start isolated persistence smoke, all
   project gates, release checker, file-size proof and adversarial review.
5. Commit `fix: provide reliable separate title bar`.

Expected result: unchanged non-display test count and one existing isolated decoration contract
expanded to cover the reliable separate-titlebar roundtrip.
