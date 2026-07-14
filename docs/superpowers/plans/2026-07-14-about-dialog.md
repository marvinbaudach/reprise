# Native About Dialog — Implementation Plan

## Global Constraints

- TDD: observe the focused menu contract fail before implementation.
- English code/UI/commits; German design specification and complete German gettext.
- Never access the real desktop, music, database, cache, accounts, or session bus.
- Keep every substantially edited source file below 800 lines and Core dependency-pure.

## Task 1 — Add the native About dialog

Files: `crates/reprise-gnome/src/ui/about.rs`, `crates/reprise-gnome/src/ui/mod.rs`,
`crates/reprise-gnome/src/ui/primary_menu.rs`, `crates/reprise-gnome/src/ui/strings.rs`,
gettext files, Manual QA, and status documents.

TDD steps:

1. Extend the primary-menu contract to require `win.about`; run the focused test
   and observe RED.
2. Add the `About` string, menu action, and native `AdwAboutDialog` containing
   application name/icon/version, Marvin Baudach as developer and copyright
   holder, GPL-3.0-or-later for the app, and an MIT legal section for the engine
   and Linux platform components.
3. Add an isolated display test for the concrete dialog metadata and complete
   German gettext.
4. Run focused tests, the isolated display test, all project gates, release
   checker, Core-purity proof, file-size proof, and adversarial review.
5. Commit `feat: add native About dialog`.

Expected result: one new non-display test assertion and one ignored display test.
