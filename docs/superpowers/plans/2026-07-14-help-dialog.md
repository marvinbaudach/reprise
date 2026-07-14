# Native Offline Help — Implementation Plan

## Global Constraints

- Follow RED → GREEN TDD and list only shortcuts implemented by the application.
- Keep English code/UI/commits, German design documentation, and complete German gettext.
- Never access the real desktop, music, database, cache, accounts, or session bus.
- Keep every substantially edited Rust file below 800 lines and Core dependency-pure.

## Task 1 — Add native offline Help

Files: `crates/reprise-gnome/src/ui/help.rs`, `crates/reprise-gnome/src/ui/mod.rs`,
`crates/reprise-gnome/src/ui/primary_menu.rs`, `crates/reprise-gnome/src/ui/strings.rs`,
`crates/reprise-gnome/src/ui/window.rs`, gettext files, Manual QA, and status documents.

TDD steps:

1. Extend the persistent-menu contract to require `win.help` immediately before
   `win.about`; run the focused test and observe RED.
2. Add immutable shortcut specifications and a native `AdwShortcutsDialog` for
   Space, Ctrl+F, Ctrl+M, Escape, Return, Shift+F10, and F1; install the weak-window
   Help action and bind F1 to it.
3. Add pure shortcut coverage and an isolated display test for the concrete dialog;
   complete German gettext and Manual QA.
4. Run focused tests, the isolated display test, all project gates, release checker,
   Core-purity proof, file-size proof, and adversarial review.
5. Commit `feat: add native Help dialog`.

Expected result: one expanded menu test, one new pure shortcut test, and one ignored
display test.
