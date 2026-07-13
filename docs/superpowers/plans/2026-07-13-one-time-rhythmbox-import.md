# One-time Rhythmbox Import — Implementation Plan

## Global Constraints

- TDD: write and run failing tests before implementation, then make the smallest
  correct change and watch those tests pass.
- English code, comments, UI strings and commits; German internal design docs.
- Never access real music, Rhythmbox user data or the real Reprise database in QA.
- `reprise-core` stays GTK/libadwaita/GStreamer/zbus-free; every touched file ends
  below 800 lines.
- Before each commit run fmt, strict clippy, workspace tests and audit. Completion
  also requires Core purity and fully isolated GTK/first-run checks.

## Task 1 — First-run-only import surface

**Files:** `crates/reprise-gnome/src/ui/first_run.rs`,
`crates/reprise-gnome/src/ui/primary_menu.rs`,
`crates/reprise-gnome/src/ui/preference_library.rs`,
`crates/reprise-gnome/src/ui/strings.rs`, `po/de.po`, `po/reprise.pot`,
`docs/agent-workflow/MANUAL-QA.md`.

**Interfaces:**

```rust
fn rhythmbox_offer(
    decision: FirstRunDecision,
    available: bool,
) -> Option<RhythmboxImportChoices>;

struct RhythmboxImportChoices {
    column_layout: bool,
}

fn primary_menu_entries() -> Vec<(String, &'static str)>;
```

1. RED: change the Rhythmbox offer test to require `ShowWizard + detected`, prove
   `AlreadyCompleted`, `ExistingLibrary` and missing detection return no offer,
   and prove the default column-layout choice is off. Add a primary-menu policy
   test that rejects `win.import-rhythmbox-columns`.
2. Run the focused tests and observe the expected compile/assertion failures.
3. GREEN: implement the policy, render a detected-only `Import from Rhythmbox`
   group with a `Column layout` switch, and feed its explicit state into the
   existing hidden import action. Remove the menu item and Preferences row while
   retaining the internal action and smoke seam.
4. Add/update English and German gettext strings and manual QA. Add a display-only
   widget regression for the detected selection group.
5. Run focused pure tests and the isolated display test, then fmt, strict clippy,
   workspace tests, audit, Core purity and touched-file size checks.
6. Adversarially review all visible and programmatic entry points: only first run
   may present the import selection; no automatic import; no Rhythmbox writes.
7. Commit `fix: limit Rhythmbox import to first-run setup`.

Expected test delta: at least two new pure assertions and one display-only GTK test.

## Task 2 — Isolated lifecycle close-out

**Files:** first-run smoke/tests as required, `docs/agent-workflow/MANUAL-QA.md`,
`.superpowers/sdd/progress.md` during final integration.

1. RED/GREEN only if the existing smoke cannot distinguish detected selection,
   explicit import and second-start suppression; add the smallest observable hook.
2. Run a fully isolated scratch-schema first start with explicit column-layout
   selection and prove the imported serialized layout plus onboarding completion.
3. Run a second start against the same scratch data and prove the first-run dialog
   is not presented again.
4. Re-run all gates, Core purity, file sizes and whole-feature review.
5. Commit any required smoke correction as `test: cover one-time Rhythmbox import lifecycle`;
   otherwise record the successful existing smoke without an empty commit.

