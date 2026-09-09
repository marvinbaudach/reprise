---
slug: the-turning-disc-survives-a-theme-switch
worktree: /home/marvin/Projects/reprise-the-turning-disc-survives-a-theme-switch
branch: feature/the-turning-disc-survives-a-theme-switch
phase: planned
codex_session:
created: 2026-09-09
---
# The turning disc survives a theme switch

## Why

#903 made the disc's turn rate theme-dependent (dark 25 s, light 40 s). A
theme-dependent rate would otherwise change the drawn angle by up to a full turn
on every switch — `TAU·(t/25 mod 1)` against `TAU·(t/40 mod 1)` — and nothing
re-rasters the cover there to hide it. `Phase::retime` re-anchors the running
segment so the angle stays continuous across the rate change.

What is covered today is only the **arithmetic**:
`npp_18_retiming_keeps_the_disc_angle_continuous` calls `Phase::retime`
directly and asserts the angle is unchanged.

What is **not** covered is the **wiring**: that `set_frame_time` actually
observes the new `is_dark`, resolves the new model, and drives the re-anchor. A
regression there — an early return moved above the `StyleManager` read, the
`last_turn_s` cache not updated, the retime skipped for a held phase — would
leave every unit test green and make the disc snap on every theme switch in the
running app.

Nobody has seen the live behaviour. The one attempt to record it headless was
abandoned because the theme looked unswitchable from outside the process. It is
not: `ui::style::set_color_scheme` calls
`adw::StyleManager::default().set_color_scheme()`, which is **process-local** and
needs neither the user's GNOME settings nor a database write.

## Scope

One `#[ignore]`d display test in the GNOME crate. No production code changes.

## Acceptance

- A display test builds the now-playing panel, advances the shimmer's frame
  clock, records the drawn rotation, flips the colour scheme in-process,
  advances again, and asserts the rotation is continuous across the switch.
- It fails if the re-anchor is removed or bypassed — proven by a mutation probe,
  not assumed.
- The unfiltered `-p reprise-gnome --bins` suite stays green.

## Non-goals

- No change to the turn rates or resting opacities. Both arms are owner-approved.
- No video, no screenshot harness, no measurement against the real library.
