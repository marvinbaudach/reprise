# Remove Compact Bar Layout — Implementation Plan

## Global Constraints

- TDD: observe focused Core RED and focused UI RED before minimal implementation.
- English code/comments/UI/commits; German internal design specification and complete gettext.
- Never access real music, database, cache, desktop, or session bus.
- Preserve Cover, Pill and Card behavior and migrate persisted `bar` values safely to Card.
- Keep `reprise-core` GTK/GStreamer/zbus-free and every edited source file below 800 lines.

## Task 1 — Remove Bar and make Card the compatible default

Files: `crates/reprise-core/src/library/settings.rs`, compact settings tests,
`crates/reprise-gnome/src/ui/compact_player*.rs`, related GTK contracts and fixtures,
gettext catalogs, current compact specifications, `docs/agent-workflow/MANUAL-QA.md`, and status
documents.

TDD steps:

1. Add Core regressions proving Card is the default and invalid-value fallback, only
   Cover/Pill/Card round-trip, and legacy persisted `bar` reads as Card. Run them and observe RED.
2. Remove `CompactLayout::Bar`, default typed reads to Card, retain the narrow legacy reader, and
   run focused Core tests GREEN plus the Core purity proof.
3. Change UI regressions to require exactly Cover/Pill/Card, reject the removed `bar` UI token,
   initialize Card, and cover the three remaining GTK roots. Run them and observe RED before
   removing the Bar factory, menu entry, strings and fixtures.
4. Update gettext, current design/manual QA references, then run focused tests, isolated GTK/app
   smoke coverage, every project gate, the release checker, file-size proof and adversarial review.
5. Commit `refactor: remove compact bar layout`.

Expected result: one fewer selectable layout and one fewer ignored Bar-only display contract; all
non-display coverage remains green with added legacy migration assertions.
