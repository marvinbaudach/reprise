# Rhythmbox Import Navigation — Implementation Plan

Design: `docs/superpowers/specs/2026-07-14-rhythmbox-import-navigation-design.md`

## Global Constraints

- Follow RED → GREEN TDD; preserve the existing read-only, conservative and
  explicitly triggered Rhythmbox import behavior.
- Keep English code/comments/UI source strings/commits and German internal docs.
- Never access real music, Rhythmbox data, Reprise data, cache, desktop or session bus.
- Keep Core dependency-pure and every substantially edited file below 800 lines.
- Before committing run fmt, strict Clippy, workspace tests and audit; completion
  also requires Core purity, gettext, isolated GTK/application checks and review.

## Task 1 — Push the import chooser inside Preferences

**Files:** `crates/reprise-gnome/src/ui/preference_rhythmbox.rs`,
`crates/reprise-gnome/src/ui/preferences.rs`, `crates/reprise-gnome/src/ui/strings.rs`,
`po/de.po`, `po/reprise.pot`, `docs/agent-workflow/MANUAL-QA.md`, design/plan/status.

**Interfaces:**

```rust
struct ImportPageSurface {
    page: adw::NavigationPage,
    rows: Vec<adw::SwitchRow>,
    import_button: gtk4::Button,
}

fn build_import_page() -> ImportPageSurface;
fn build_import_row(path: &Path) -> Option<ImportRowSurface>;
fn open_rhythmbox_import(self: &Rc<PreferencesContext>);
```

1. RED: change the display regressions to require an activatable navigation row
   with a forward indicator and no embedded Import button, plus a poppable detail
   page containing all six options, existing defaults and a plain `Import` action.
2. Run the focused display tests in isolated Xvfb and observe the expected compile
   or assertion failure with the current `AdwAlertDialog` chooser.
3. GREEN: build the `AdwNavigationPage`, push it through the existing weak
   Preferences navigation seam, wire its Import button to the unchanged import
   worker, and retain the direct isolated smoke start.
4. Add the `Import` gettext source/German translation and update manual QA to
   require the same-window second level and Back behavior.
5. Run focused pure/display tests, the existing Scratch Rhythmbox application
   smoke, gettext validation, full gates, Core purity and touched-file size proof.
6. Adversarially review navigation lifetime, duplicate starts, RefCell discipline,
   default choices, missing navigation, absence of the former chooser dialog and
   preservation of read-only data semantics; fix findings and rerun affected gates.
7. Commit `fix: embed Rhythmbox import in preferences` and update/release the
   coordination entry without touching the active main-work lock.

Expected result: the Rhythmbox chooser is a native second-level Preferences page;
all existing import behavior and safety contracts remain unchanged.
