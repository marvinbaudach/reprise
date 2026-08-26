---
slug: the-fill-yields-not-the-accent
worktree: /home/marvin/Projects/reprise/.worktrees/contrast-gate
branch: fix/the-fill-yields-not-the-accent
phase: refactored
codex_session:
created: 2026-08-26
---

# The fill yields, not the accent

## How this plan reached `coded`

Written after the fact. The work was implemented interactively in the main
thread rather than by Codex from a plan, so the `plan` and `code` phases did not
run as the pipeline describes them. This file exists to give the branch the
anchor `land.sh` requires and to record the measurements the phases would
otherwise have carried. The `check` phase runs normally against the committed
diff.

## The report

Three surfaces reported as dark-text-on-dark-green, sampled with an eyedropper:
the "Sync now" action in the device-sync dock (~2.5:1), the shuffle toggle in the
player bar, and the playlist selection checkboxes.

## What the sampled colours actually were

The requested pairing — `#4fdcd4` on `#0d2523` — is the palette the app already
ships. `data/brand/palette.toml` declares `reprise_teal = "#4FDBD4"` and
`accent.rs` declares `APP_ACCENT_FG = "#04140f"`, which measure 11.16:1. So no
colour literal was introduced; the defects were elsewhere.

**Checkbox: no defect.** Only `library_doctor/review_css.rs` styles checkboxes
and it sets geometry alone. The mark is Adwaita's `accent_fg_color` on
`accent_bg_color` = 11.16:1, the highest ratio in the app. The sampled
`#49c0ba` on `#2f3237` is 5.84:1 — the teal *fill* against the surrounding row,
not the mark against the fill. Declined, with the numbers.

## The three real defects

1. **The checked toggle, 2.97:1.** `@reprise_accent_text_color` was derived
   against the filter-chip tint (0.18) while `BTN_CHECKED_FILL_PRESS_ALPHA`
   filled to 0.38. Nothing modelled the surface the label actually landed on.
2. **The disabled primary button.** Adwaita dims filled buttons, leaving the
   near-black accent foreground on a mid-dark tint of the accent. WCAG exempts
   inactive controls; the exemption is not a licence to make them illegible.
3. **The play/pause glyph, 1.69:1.** A literal `#ffffff` on the player accent,
   on the app's most prominent control, where WCAG 1.4.11 owes 3:1.

## The fix

`ACCENT_TINT_CEILING` (0.26) bounds the heaviest accent-tinted background any
rule may paint; `critical_accent_surface` derives the accent foreground against
it, across the white elevation rungs as well as the bare palette. Fills that
exceeded it yield — the precedent `CHIP_BG_ALPHA` already set. The disabled
primary drops its accent surface and neutralises Adwaita's dimming filter. The
play buttons take libadwaita's guaranteed `@accent_bg_color`/`@accent_fg_color`
pair.

Documented as CONTRAST-5a (replacing CONTRAST-5) and BTN-5.

## Evidence

The ceiling is measured, not chosen: swept against the brand teal and the four
extreme system accents `accent::tests` exercises. 0.28 breaks — a heavy tint of
a near-white accent lifts a dark surface to mid-grey, the lightness search
leaves the sRGB gamut, and the monochrome fallback drops the brand hue app-wide
while every ratio still passes. 0.26 is the last value that holds on every rung.

Guards are mutation-proved: reverting the ceiling to 0.30 fires three tests;
reverting the play button rule fires two. The tint-ceiling guard found two
over-budget fills nobody was looking for (stats badge 0.28, layout-preview band
0.35) on its first run.

## Known blast radius

`@reprise_accent_text_color` in dark moves from `#4fdbd4` to `#75fdf5` /
`#74fcf4` / `#6ff6ef` across 50 references in 23 files. Same hue, lifted;
required by AA. Light barely moves. Checked-toggle fills drop 0.22/0.30/0.38 →
0.18/0.22/0.26. This is deliberate and was reported to the user.

## Parallelität

**Not cut.** Every file reads `ACCENT_TINT_CEILING` or the role derived from it,
and the three defects share one mechanism — a token change in `tokens.rs`
propagates through `theme.rs` into every consumer in the same commit. A cut
would put the token and its consumers in different branches, so neither strand's
tests could go green before the merge. Single strand, no suffix files.

## Post-merge cross-checks

None required by a strand cut. Two checks belong to the merge itself:

- The gate must be re-run in an isolated worktree. The interactive run happened
  in a shared checkout carrying 72 unrelated modifications, which a gate run
  measures along with the change.
- `git log --oneline HEAD..origin/dev` before landing: any commit touching
  `crates/reprise-gnome/src/ui/style/` can interact with the ceiling.

## The review's open item, resolved

The `check` phase's widening of the tint guard to `background-image` surfaced
one collision, and the fix for it first shipped as a blanket exclusion of
`@accent_color` gradients — the guard silenced instead of the rule answered.
That axis is wrong twice over: it exempts every future accent gradient,
including one that does carry text, and it says nothing about why the one real
collision is allowed.

The colliding rule is the 2px rail left of the online-children card
(`preference_online_master.rs`), a fade from `alpha(@accent_color, 0.55)` to
transparent. The rail is a bare `gtk4::Box` constructed with no children in
`preference_plugins.rs` and never given any: nothing can sit on it, so the
ceiling — which exists only to keep a *foreground* legible — does not bind it.
Lowering the stop to 0.26 would erase a 2px subordination cue to pay a
legibility tax for text that does not exist.

So the guard now says that out loud. `DECORATIVE_ACCENT_SURFACES` exempts named
rules, never a property or a colour role, and every entry has to argue that its
surface carries no foreground. Three guards hold it honest:

- removing the entry fires `contrast_5a_no_app_surface_tints_past_the_ceiling`
  on the rail, and that dump is the full list — the rail is the only collision
  in the app (verified 2026-08-26);
- changing the rail's fill fires
  `contrast_5a_every_decorative_exemption_still_names_a_live_rule`, so a stale
  exemption cannot quietly become a blind spot;
- `contrast_5a_the_tint_ceiling_guard_catches_a_louder_fill` now pins an
  `@accent_color` gradient too, which is exactly the hole the blanket exclusion
  had opened.
