---
slug: the-wizard-is-the-only-question
worktree: /home/marvin/Projects/reprise-the-wizard-is-the-only-question
branch: feature/the-wizard-is-the-only-question
phase: planned
codex_session:
created: 2026-09-12
---
# The wizard is the only question

## Goal

Remove the online-sources discovery banner completely. After this change the
online-sources question is asked in exactly one place — the first-run wizard —
and the permanent path stays Preferences · Plugins.

## Why removal, not repair

The banner was reported twice over: it keeps standing after the question has been
answered, and it renders as two surfaces with `Not now` clipped against the
window edge. Both are real, and both stop mattering once the widget is gone.

The decisive fact is the target population. `docs/ux-rules.md:2960` (**NET-4**)
defines the banner as the path for an **existing** installation, and
`first_run_tests.rs:523-537` names it exactly:

```rust
settings::set_library_root(&db, "/music").unwrap();
assert_eq!(initial_decision(&db), FirstRunDecision::ExistingLibrary);
assert!(crate::ui::online_discovery_banner::build(&db, || {}).is_some());
```

`FirstRunDecision::ExistingLibrary` means a database that has a library root but
no onboarding record — someone who had a library *before* online sources existed.
Reprise has not been released, so that population is empty. What the banner
actually does today is appear on every fresh install, behind the welcome dialog,
asking the same question the wizard is already asking (`first_run.rs:79`, `NET-4`:
"the wizard *replaces* the discovery banner's question for a fresh install").

So it is not a feature with two defects. It is a migration notice for a migration
that never happened, and it is the only one of its kind that misfires: the
artwork consent banner (`artwork_consent_banner.rs`) carries the identical layout
defect but its key is written only by a migration that fires on legacy settings
(`db_artwork.rs:49-59`), so it never appears at all. It stays untouched —
dormant dead code, a candidate for a later sweep, not part of this plan.

## Decisions taken in the grill (2026-09-12)

- **G1** — Delete the banner rather than repair it. The earlier repair plan
  (live closing, a full-width band, a `FirstRunDecision` guard) is dropped in
  full; there is nothing left to close or to style.
- **G2** — Delete the settings key too, not just the widget. Nothing but the
  banner and `first_run.rs` reads or writes
  `online_sources.discovery_banner_completed`; a key no code consults is a trap
  for the next reader. Rows already stored in a developer database are left
  orphaned, which is harmless.
- **G3** — Retire the rule as **NET-4 → `[replaced by NET-4a]`**, not as a fresh
  `NET-4c`. The wizard rule already exists, is `[active]`, and already carries
  the two tests that assert the behaviour. A new ID would force renaming
  `net_4a_*` to `net_4c_*` for no change in what is asserted.
- **G4** — Accepted consequence: after the release, a database with a library
  root and no onboarding record is never asked at all. It starts with the gate
  shut and Preferences · Plugins is its only route. This is written into NET-4a
  rather than left implicit.
- **G5** — Out of scope, explicitly: the artwork consent banner, and the
  separate question of collapsing the wizard's three source toggles into one
  master with an expander (the Plugins hierarchy). Both are their own plans.

## Tasks

### T1 — Remove the widget and its wiring

- delete `crates/reprise-gnome/src/ui/online_discovery_banner.rs` (177 lines,
  including its own `#[cfg(test)] mod tests`);
- drop `mod online_discovery_banner;` — `crates/reprise-gnome/src/ui/mod.rs:81`;
- drop the build block in `crates/reprise-gnome/src/ui/window/window.rs:456-465`,
  including the `Rc::downgrade(&preferences)` capture and the
  `preferences.present_plugins(&[])` deep link that only this banner used. Check
  whether that leaves the surrounding `preferences` weak clone unused;
- `crates/reprise-gnome/src/ui/strings_online_sources.rs:53-57`: remove
  `ONLINE_DISCOVERY_BANNER_BODY`, `ONLINE_DISCOVERY_REVIEW`,
  `ONLINE_DISCOVERY_NOT_NOW`;
- `crates/reprise-gnome/src/ui/first_run.rs:78-90`: remove the third write in
  `persist_completion` (`set_online_discovery_banner_completed`) and rewrite the
  doc comment, which currently explains the banner it closes. What remains is
  "everything the wizard persists on both exits: onboarding completion and the
  source selection";
- tests that assert the banner: `first_run_tests.rs:94-101`
  (`both_exits_close_onboarding_and_the_discovery_banner` — the name and the
  banner assertion both go; keep whatever it asserts about onboarding),
  `first_run_tests.rs:164`, and
  `first_run_tests.rs:523-537` (`existing_library_keeps_the_online_discovery_banner`
  in full).

### T2 — Remove the settings key (G2)

- `crates/reprise-core/src/library/settings.rs:27` `ONLINE_DISCOVERY_BANNER_COMPLETED_KEY`
  and the two private accessors at `:160-167`;
- `crates/reprise-core/src/library/settings_api.rs:11,21,79-90` — the re-exports
  and the two public wrappers;
- `crates/reprise-core/src/library/settings_tests.rs:139-146`
  (`online_discovery_banner_completed_typed_accessors_round_trip`).

No migration is involved: nothing in `reprise-core` writes this key, so no schema
version changes and no upgrade path is affected.

### T3 — Retire NET-4 in the rulebook (G3, G4)

`docs/ux-rules.md`:

- **NET-4** (`:2960-2970`) becomes `- **NET-4** [replaced by NET-4a] — …`. Keep
  the sentence that explains what it used to require; the rulebook's own process
  rule (`:18-21`) calls a replaced rule "a signpost, never deletable ballast".
  The form the gate parses is `^- \*\*NET-4\*\* \[replaced` — see
  `scripts/check-ux-traceability.sh:24`.
- **NET-4a** (`:2971-2981`) loses both banner clauses: "and close the discovery
  banner of `NET-4`, so the question is never asked twice" and the closing
  sentence "An existing library never sees the wizard and keeps the banner."
  In their place, state G4 explicitly: an existing library never sees the wizard,
  is never asked, starts with the gate shut, and reaches the sources only through
  Preferences · Plugins.
- **NET-4b** (`:2982-2990`, Android) loses its comparison sentence "Unlike the
  `NET-4` banner it enables directly instead of pointing at the settings page".
  The Android banner itself is unaffected — it is shown whenever the gate is off
  and the question unsettled, not on a migration path, so it keeps its rule and
  its behaviour. Rewrite the clause so it stands without naming NET-4.

**This must land in the same commit as T1's test deletion.** The rulebook demands
it (`:21`) and the gate enforces it in both directions:
`check-ux-traceability.sh:84` fails when a test still references a replaced rule,
and `:96` fails when an `[active]` rule has no test. NET-4's only test is the one
inside the deleted file (`online_discovery_banner.rs:142`,
`net_4_discovery_banner_persists_review_and_dismiss_actions_before_hiding`), so
either half alone is red.

### T4 — Un-wire the two e2e harnesses

Both drive the banner by screen coordinates and will not fail gracefully.

- `scripts/ptr-e2e/run.sh:436-450`: `dismiss_onboarding_banner()` clicks
  `$DISCOVERY_BANNER_NOT_NOW_X/$DISCOVERY_BANNER_NOT_NOW_Y`, asserts the DB value
  `online_sources.discovery_banner_completed == 1`, and on failure calls
  `log_fail` and `exit 1` — "refusing to run coordinate flows". Remove the
  function, its two coordinate definitions, and its call site(s). The assertion
  targets the key T2 deletes, so leaving it behind kills the whole suite.
- `scripts/cua-e2e/selection_anchor.sh`: remove `dismiss_discovery_banner()`
  (`:87`) and its two call sites (`:133`, `:159`), the explanatory comment at
  `:52`, and the two evidence names `anchor-01b-banner-dismissed` /
  `anchor-10b-banner-dismissed`. Renumbering the remaining evidence steps is
  optional; if the numbering is left with gaps, say so in the script's comment so
  the next reader does not hunt for a missing screenshot.

### T5 — i18n catalogues

`scripts/tests/gettext-catalogs.sh:21-30` regenerates `reprise.pot` with
`xgettext --files-from=po/POTFILES.in` and compares every catalogue against it
with `msgcmp --use-fuzzy --use-untranslated`. So:

- remove the three msgids from `po/reprise.pot` and from all seven catalogues
  (`po/ar.po`, `bn`, `de`, `es`, `fr`, `hi`, `zh_CN`);
- check `po/POTFILES.in` for an entry pointing at the deleted file and remove it
  if present;
- re-run `scripts/tests/gettext-catalogs.sh` — it is the authority on whether the
  catalogues are consistent, not a manual read of the diff.

### T6 — Gate

Cut the worktree from `origin/dev` (this session's branch is behind — dev is at
`2eb70d5152`, #926 — and the banner files are identical on dev, so there is
nothing to rebase around).

Run, with output redirected to a file and the question answered by `grep`, never
by reading a verdict through a pipe:

- the workspace test suite for `reprise-core` and `reprise-gnome`;
- `scripts/check-ux-traceability.sh` — the rule change's own gate;
- `scripts/tests/gettext-catalogs.sh` — T5's gate;
- a compile check of the two shell harnesses (`bash -n`) plus a grep proving no
  reference to `discovery_banner`, `online_discovery`, or "can now follow
  podcasts" survives anywhere outside `docs/plans/`.

That last grep is the completeness check for the whole change: the sweep that
found T4 and T5 is the same sweep that proves the removal is total.

## Risks

- **A half-landed change is red, not merely incomplete.** T1+T3 are coupled by
  the traceability gate, T2+T4 by the ptr-e2e DB assertion, T1+T5 by `msgcmp`.
  This is one commit's worth of work; splitting it across commits inside the
  branch is fine, splitting it across branches is not.
- **The `present_plugins(&[])` deep link** in `window.rs` may be this banner's
  only caller of that empty-target form. Removing the block could leave an unused
  import or an unused weak clone — the compiler will say so, but check that the
  *other* deep link a few lines down (`lyrics_view().set_on_settings`) is not
  accidentally caught in the same deletion.
- **Evidence-name gaps in `selection_anchor.sh`** could be read by a later run as
  a missing screenshot rather than a removed step.
- **An orphaned settings row** stays in developer databases. Deliberate (G2), and
  invisible: nothing reads the key any more.

## Parallelität

**One strand. The plan cannot be cut.**

The cut fails on the gates, not on the file lists. The file groups do look
disjoint at first sight — T1/T2 are Rust, T3 is `docs/ux-rules.md`, T4 is two
shell scripts, T5 is `po/` — but each pair is coupled by a check that goes red in
either intermediate state:

- T1 deletes NET-4's only test while T3 changes NET-4's status.
  `check-ux-traceability.sh` fails on T1 alone (`[active]` rule with no test,
  `:96`) *and* on T3 alone (a test referencing a replaced rule, `:84`). The
  rulebook mandates the same-commit coupling in its own process rules (`:21`).
- T2 deletes the key that `ptr-e2e/run.sh:442` asserts, and that assertion exits
  the suite rather than reporting a failure. T2 without T4 is a dead harness.
- T1 removes the three strings that T5 removes from the catalogues; `msgcmp`
  compares a freshly extracted `.pot` against the checked-in catalogues, so
  either half alone leaves the catalogue check red.

A three-way cut would therefore produce three branches of which none can go green
alone — the exact failure mode the `Parallelität` rule exists to prevent. The
whole change is a deletion of roughly 200 lines plus six call sites; serial is
also simply faster here than three worktrees and a merge order.

- **Merge order:** n/a.
- **Post-merge cross-checks:** n/a.
