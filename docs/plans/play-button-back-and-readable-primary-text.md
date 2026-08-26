---
slug: play-button-back-and-readable-primary-text
worktree: /home/marvin/Projects/reprise-play-button-back-and-readable-primary-text
branch: feature/play-button-back-and-readable-primary-text
phase: refactored
codex_session:
created: 2026-08-26
---
# The play button keeps its own face, the primary label earns its ratio

## Goal

Two corrections to #710 (`a69fe8e621`), decided by the user after seeing the
result in 0.1.80:

1. **The play button goes back to what it was before #710.** It is the app's
   playback identity, not a suggested action, and #710 made it read like the
   "Add channel" button.
2. **The genuinely unreadable filled buttons get a light foreground on their
   unchanged fill** — derived from the project's own tokens, never pinned.

## Decisions already taken — do not reopen

- **Full revert for the play button**, chosen with the cost on the table: white
  on `@reprise_player_accent` measures **1.69:1**, and `@reprise_player_accent`
  aliases `@accent_color`, which libadwaita derives as
  `oklab(from accent_bg_color max(l, 0.85))` in dark — a pale standalone tint.
  The user chose this knowingly. Do not re-argue it, do not "improve" it, do not
  quietly keep a middle ground.
- **Derive, never pin.** The hex values from the user's brief (`#eafaf7`,
  `#25786b`, `#1e665b`, `#152020`, `#2f5255`, `#49c0ba`, `#2f3237`) are a target
  picture, **not input**. None of them exist in the tree — verified, zero hits
  across `crates/reprise-gnome/src/`. Colors here are derived from
  `APP_ACCENT` (a build-time `env!("REPRISE_APP_ACCENT")`) through OKLab math.
  A pinned literal would be correct for exactly one accent and would trip the
  CONTRAST-5a foreground guards.
- **No new hue, no fill changes.** Every background, size, spacing and layout
  stays exactly as it is. This is a foreground-only change.

## What is already there — verified, do not rebuild

- `accent::accent_text_color(accent, background, is_dark) -> String`
  (`accent.rs:136`) already derives a foreground guaranteed to clear
  `ACCENT_TEXT_MINIMUM_RATIO = 4.5` (`accent.rs:90`) against a given
  background, via `ensure_contrast_by_lightness` (`color_math.rs:139`), falling
  back to `max_contrast_monochrome` (`color_math.rs:118`) when lightness alone
  cannot get there. **This is the tool. Do not write a second one.**
- It already feeds `@reprise_accent_text_color` (`theme.rs:276`).
- The shuffle toggle's checked state **already uses that derived token**
  (`buttons.rs:209`, `color: @reprise_accent_text_color`). See Task 3.
- `ACCENT_TINT_CEILING = "0.26"` (`tokens.rs:176`) is the heaviest tint any rule
  may paint, and the surface CONTRAST-5a pins its ratios against.

## Task 1 — the play button, reverted

Revert the `PLAY_CSS_CLASS` / `mini-player-play` hunks of `a69fe8e621`:

- `player_bar/player_bar_layout.rs` (~line 464): back to
  `background-color: @reprise_player_accent; color: #ffffff;`
- `compact/compact_player_layouts.rs`: the same pairing for `.mini-player-play`.
- Remove the CONTRAST-5a comment block #710 added above the play rule. Replace
  it with a short one saying the play button deliberately keeps the playback
  accent and a white glyph, that this is a product decision recorded in
  PLAY-16 below, and that it does **not** meet the 3:1 a non-text control would
  otherwise owe. Say it plainly — a reader who finds this later must not think
  it was an oversight.

Everything else #710 changed stays. This reverts one of its three defects, not
the commit.

## Task 2 — the enforcing test

`style/panel_contrast.rs:244`,
`contrast_5a_accent_surfaces_pair_with_the_theme_accent_foreground`, asserts for
both play selectors that the rule contains `@accent_bg_color`/`@accent_fg_color`
and does **not** contain `@reprise_player_accent`. Task 1 makes it fail by
construction.

Do not delete the test. **Narrow it**: keep its accent-surface carve-out for any
*other* accent surface it guards, and drop the two play selectors from its loop,
with a comment naming PLAY-16 as the reason. Its second half — the measured
`GLYPH_MINIMUM_RATIO` assertion on the brand accent — goes with the play
selectors, since nothing else in the loop is a glyph.

If, after removing the two play selectors, the loop has no entries left, then
the test's whole subject was the play button: delete it and say so explicitly in
the commit message rather than leaving an empty loop that asserts nothing.

## Task 3 — measure before touching anything else

**This task is measurement, not code.** The user named three targets; the sweep
shows they are not in the same state, so implementing all three blindly would
change things that are already correct.

Write a `#[test]` that prints (or asserts and reports) the real rendered
foreground/background pairs and their ratios for:

1. `button.suggested-action` / `.reprise-btn-primary` **resting** — foreground
   `@accent_fg_color`, which for `AccentSource::App` is the static
   `APP_ACCENT_FG = "#04140f"` (`accent.rs:13`). Measure it against the surface
   the button actually sits on, **not** against flat `APP_ACCENT`. The existing
   proof (`accent.rs:234`) only ever checked the flat case — that gap is the
   suspected cause of the user's 2.5:1.
2. `.{TOGGLE_CLASS}:checked` (the shuffle toggle) — already
   `@reprise_accent_text_color` over `alpha(@accent_bg_color, 0.18)`.
3. The disabled primary (`buttons.rs:238`) — `@reprise_secondary_fg_color` on a
   dropped surface, BTN-5's arm from #710.

Report the numbers. **Only the pairs that measure below 4.5:1 get changed in
Task 4.** A target that already clears it is left alone and named as such in the
commit message.

## Task 4 — the fix, where Task 3 says it is needed

For each pair below 4.5:1, raise **only the foreground**, through
`accent_text_color()` against that pair's real surface. Concretely, the expected
shape (confirm against Task 3's numbers before writing it):

- Give the resting filled-primary role a foreground derived against its actual
  surface instead of the static `#04140f`, i.e. route it through the same
  derivation `@reprise_accent_text_color` already uses. Keep `#04140f` as the
  App accent's `accent_fg_color` where it is still correct — the flat-accent
  proof at `accent.rs:234` stays true and its test stays.
- Do not touch any `background-color`, `background-image`, `min-width`,
  `min-height`, `padding`, `margin`, `border-radius` or transition.

Tests that pin the old foreground and will need honest updating rather than
deletion: `accent.rs:198` (`accent_fg(App) == Some("#04140f")`), `accent.rs:234`
(the 11.164 ratio), `theme.rs:321` (the literal `@define-color accent_fg_color
#04140f;`). If the fix leaves `accent_fg_color` itself untouched and works
through a different token, these three stay green — prefer that shape.

## Task 5 — the checkboxes: ask, do not guess

The user asked for "the playlist selection checkboxes". The sweep found **two
different things** and neither matches cleanly:

- `device_sync/device_sync_picker.rs:192` — the literal playlist picker. It is a
  stock `gtk4::CheckButton` with **no Reprise CSS class at all**, so there is no
  mark color to change without first inventing a class for it.
- `library_doctor/review_header.rs:365` — `.doctor-album-check`, styled in
  `library_doctor/review_css.rs:4-6`, checked state
  `background: var(--accent-bg-color); color: var(--window-bg-color);`. These
  are *album* checkboxes, not playlist ones.

**Do not pick one and implement it.** Measure both in Task 3, report which (if
either) falls below 4.5:1, and leave the change for the user to direct. Say so
in the summary.

## Task 6 — the rules

Read `docs/ux-rules.md` and its append-only contract first.

- Add **PLAY-16** `[active]` `[gtk]`: the play button in the player bar and the
  mini player paints the playback accent with a white glyph. It is the playback
  identity, shared with the marker and the EQ bars, and is deliberately exempt
  from the 3:1 that CONTRAST-5a's accent-surface carve-out would impose. Record
  the measured 1.69:1 in the rule text — an exemption that hides its own cost is
  worthless to whoever reads it next.
- Amend **CONTRAST-5a**'s accent-surface carve-out so it no longer claims the
  play button as its case, and point it at PLAY-16.
- If Task 4 changes a foreground role, the rule that describes that role is
  amended in the same commit.

Gate for this task: `scripts/check-ux-traceability.sh`.

## Task 7 — full gate and ledger

From the repo root, in this order, reporting the real output of each:

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo audit                       # only RUSTSEC-2024-0436 is accepted
scripts/check-architecture.sh
scripts/check-ux-traceability.sh
```

`reprise-core` is not touched, so no core purity proof is needed.

Test-count baseline comes from the latest entry in `.superpowers/sdd/progress.md`,
never from AGENTS.md. Append one line in the agreed format, and say plainly how
many tests were added, how many narrowed, and how many changed their assertion.

Do not run the cua-e2e suite — it needs a display session.

## Not in this plan

- Any change to a fill, size, spacing, radius or layout.
- Any new hue or any pinned literal color.
- `reprise-core`, `reprise-runtime`, `reprise-android-ffi`.
- The other two defects #710 closed. Only the play button's arm is reverted.
- The checkbox change itself — Task 5 measures and reports, nothing more.

## Parallelität

**No cut. One strand.**

The attempt, on record: Tasks 1, 2 and 6 all revolve around the same two play
selectors and the rule that describes them; Task 4 depends on Task 3's
measurements, which do not exist until they are taken. A strand owning only
`docs/ux-rules.md` could not go green before the merge in principle, because
`check-ux-traceability.sh` resolves PLAY-16 against a test name Task 1 and 2
produce — the exact failure mode that cost a whole strand in the Flathub wave.

Post-merge cross-checks: none.
