---
slug: ux-rules-motion
worktree: ~/Projects/reprise/.worktrees/transitions
branch: feat/ux-rules-motion
phase: shipped
codex_session:
created: 2026-07-17
---
# UX motion (section O) — final implementation plan

This shipped plan is retained because tracked files still reference it as durable documentation.

**Status:** grilled 2026-07-17, ready for /code
**Branch:** feat/ux-rules-motion (from the worktree `.worktrees/transitions`, see Operational notes)
**Date:** 2026-07-17

> All decisions are final (decision table below). This document is the
> complete instruction for the headless code phase. **The code phase
> implements EXCLUSIVELY phase 1 (section 4.2) and then stops.**
> Phase 2 starts only after the hard gate check in section 4.3 — never
> "just keep going" within the same run.

---

## 1. Decisions (grilling 2026-07-17, all final)

| No. | Question | Decision |
|---|---|---|
| G1 | Section letter & integration order | Section **O**. `docs/ux-rules.md` is only touched in this branch after CTX ("N. Track context menu", feature/context-menu-unification) is on main and main has been integrated. An HTML comment in the section preamble documents the letter situation (M tooltips on main, N claimed by CTX). |
| G2 | Token cut | **Micro 150 ms ease-out** · **Standard 250 ms ease-out-cubic**. Existing surfaces deliberately migrate 150→250; the risk "the app feels different" remains as an explicit acceptance note. |
| G2i | Play/Pause icon crossfade | **Micro halves (2×75 ms)** — no special number. |
| G2ii | Accent fade 400 ms | Fourth token **"Ambient" 400 ms** for atmospheric, non-interactive transitions (accent-color crossfade; in future e.g. the artist hero glow). |
| G3 | Adw-internal animations in the MOT-1 wording | Wording "every animation configured by Reprise itself"; Adw-internal animations without a duration API (OverlaySplitView, NavigationSplitView, ToastOverlay, Banner, Dialog, Popover) count as system-given and are exempt — including the push/pop slides of the settings subpages. NO clause requiring them to approximate the tokens. |
| G4 | Spatial token | Stays in the rule text ("AdwSpringAnimation, Adw default spring parameters, starting with the first directed navigation case"); added in `ui/motion.rs` only with the first consumer (YAGNI). |
| G5 | Widget choice for the left sidebar | The left sidebar becomes **`adw::OverlaySplitView` (position start)** — exactly the widget of the right column. Port the `apply_sidebar_visibility`/`manually_hidden`/breakpoint(<800 px) logic; extend the tests with breakpoint cases. |
| G6 | Lint mechanics | `scripts/check-motion-tokens.sh` forbids, outside `motion.rs`/`tokens.rs`: `TimedAnimation::new` with an integer-literal duration, and `set_transition_duration(`/`.transition_duration(` with a literal. Ticks/CSS remain a review matter. |
| G7 | EQ permanent loops | `reprise-eq` and `mini-eq-bar` = a named MOT-5 exception: "EQ indicators run only during active playback." |
| G8 | MOT-7 scope | Gate centrally: Adw animations via the follow-enable-animations property + our own tick callbacks (waveform position smoothing → set the position hard; progress interpolation) + pulse timers. `gtk::Spinner` and GTK CSS mechanics = system behavior, not gated. T-V (CSS behavior under `gtk-enable-animations=false`) is a mandatory verification; a negative finding goes back to grilling, not a silent decision. |
| G9 | Scope cut | **Core cut.** This branch: section O + `ui/motion.rs` + lint + sidebar symmetry (MOT-3 incl. sentence 2) + token migration (MOT-1) + MOT-7 + the MOT-6 skip + MOT-2. The new MOT-5 behavior (scale pulse, waveform crossfade, pause desaturation) and the queue drop/remove animation (MOT-4 exception) = follow-up branch; MOT-5 stays `[planned]` with a flip-criterion comment (tooltip pattern TIP-1b/2b). |
| G10 | Pause desaturation | Wording: "pause slightly desaturates the waveform fill (at draw time), play reverses it" — the accent pipeline (`cover_accent`) stays untouched. |
| G-Pfad | Module location | **`ui/motion.rs`** (UI root level, next to `nav_history.rs`/`notifications.rs`): token constants, the `timed()` constructor (sets follow-enable-animations), the gate helper `animations_enabled()`, the slot helper `replace_animation()` with `skip()`. `tokens.rs` keeps the CSS `TRANSITION` constant and will consume the Micro duration from `motion.rs`. |
| G-Sym | MOT-3 sentence 2 | Stays: the inner Tracks/Albums/Artists stack + the StatusPage⇄list stacks crossfade with the Standard token like the outer Library/Stats/Device stack. |
| G-Seq | Sequencing | **Two-phase** after a verified ownership finding: phase 1 immediately and conflict-free (T0, T2, T7, T8, T3); phase 2 GATED behind the merge of feat/missing-import-errors AND feature/context-menu-unification into main (T1, T4, T5, T6, T9, T10). ALL status flips lie in phase 2 (they need the section). Details in section 4. |

---

## 2. Audit inventory (condensed, self-verified)

No `.ui`/`.blp`/`.css` files exist — the UI is entirely imperative, CSS inline
as strings (`ui/style/mod.rs::app_css()` → `CssProvider::load_from_string`).

### 2.1 Explicitly configured widget transitions

| Location | Widget / transition | Trigger | Target token |
|---|---|---|---|
| `window/window.rs:368-369` | outer stack (Library/Stats/Device), crossfade 150 ms | user | Standard (phase 2, T4) |
| `compact/compact_player_layouts.rs:140-141` | revealer crossfade 150 ms (hover overlay mini-player, 1000 ms linger) | user | Micro (T7) |
| `scan/scan_progress.rs:155-156` | revealer crossfade 150 ms (scan card) | **background** | Standard (phase 2, T5) |
| `sidebar/sidebar_device_card.rs:250-254` | 2× stack + 2× revealer, 150 ms or 0 with enable-animations=false (dynamic, lines 248-255) | **background** (sync) | Standard (phase 2, T5) |
| `info_panel/info_panel.rs:148` | stack crossfade, default duration (~200 ms) | user (tab) | Standard (T8) |
| `lyrics/lyrics_view.rs:82` | stack crossfade, default | **background** (loading state) | Standard (T8) |
| `browse/browse_chooser.rs:28` | stack SlideLeftRight, default | user | Standard (T8) |
| `preferences/preference_rhythmbox.rs:323` | stack SlideLeft, default | user | Standard (T8) |

**Without a transition (hard cuts):** the inner Tracks/Albums/Artists stack
(`library_shell.rs:43-66` — `gtk4::Stack::new()`, nothing set), the
StatusPage⇄list stack of the track table, the left sidebar
(`window_navigation.rs:10-27`: `sidebar_page.set_visible(false)` — GTK never
animates `visible`; the header toggle path, lines 72-85, is purely structural
via `set_collapsed`/`set_show_content`), mini⇄full (`minimal_view.rs`:
`ToolbarView::set_content` swap).

**Adw-internal, no duration API in the bindings (system-given, G3):**
OverlaySplitView (right column, `information_column.rs:20-28`),
NavigationSplitView, ToastOverlay, Banner, Dialog, Popover/PopoverMenu — plus
the app's only genuine `AdwNavigationView` pushes: the settings/preferences
subpages (`preferences.rs:363`, `preference_sync.rs:150`,
`preferences_window.rs:305`). The animation is hardcoded in C in each case; it
respects `gtk-enable-animations` natively.

### 2.2 Adw animations and hand-built ticks

| Location | What | Duration | Gated today | Target |
|---|---|---|---|---|
| `player_bar/player_bar.rs:281-339` | track crossfade cover+title+artist, one slot | 125+125 ms | yes (line 282) | Standard halves 2×125 (T7) |
| `player_bar/player_bar.rs:371-397` | Play/Pause icon crossfade, one slot | **60+60 ms** (the doc comment claims 120) | yes (line 372) | Micro halves 2×75 (T7, G2i) |
| `compact/compact_player.rs:403-428` | mini-player title/artist crossfade | 125+125 ms (`CROSSFADE_HALF_MS`) | yes (line 133) | Standard halves (T7) |
| `style/cover_accent.rs:317-343` | accent crossfade, **global single slot** `CURRENT_ANIMATION` | 400 ms | yes (line 322) | Ambient (T7, G2ii) |
| `sidebar/sidebar_device_card.rs:339-372` | tick: progress interpolation, hand-built ease-out-cubic | 150 ms | yes (line 342) | Micro + central gate (phase 2, T5) |
| `player_bar/waveform_seek.rs:398ff` | tick: peaks build-up with a per-bar stagger | 300 ms (`BUILD_DURATION_S`), stagger 2 ms | yes (line 314) | Ambient (T7; the stagger stays an implementation detail) |
| `player_bar/waveform_seek.rs:398ff` | tick: position smoothing (velocity-based) | continuous | **no** | gate: set the position hard (T7, G8) |
| `scan/scan_progress.rs:298ff` | `ProgressBar::pulse()` every 100 ms (`PULSE_INTERVAL`) | permanent loop during a scan | **no** | gate: the timer does not start (phase 2, T5, G8) |

`AdwAnimation::skip()` is called **nowhere**; new animations silently replace
the old slot handle (the old Adw animation keeps running, because Adw
references it itself during `play()`) → MOT-6, T7.

### 2.3 CSS (inline)

- Central token `style/tokens.rs:61`:
  `TRANSITION = "150ms cubic-bezier(0.16, 1, 0.3, 1)"` — used in ~10 CSS
  sections (hover/focus app-wide). Stays 150 ms = Micro; will consume
  `motion::MICRO_MS` (T8).
- Press-scale: `transform 120ms ease-out`, `:active scale(0.94)`
  (`player_bar_layout.rs:273-278`) → Micro cascade via `tokens.rs` (T7).
- Two `@keyframes` permanent loops, both only during playback:
  `reprise-eq` 1100 ms infinite (now-playing row; the file is
  **`ui/eq_bars.rs`**) and `mini-eq-bar` 650 ms infinite alternate
  (`player_bar_layout.rs:305-309`) — a named MOT-5 exception (G7), the
  durations stay (permanent loops are not transition tokens).
- **Unverified:** whether GTK's CSS transitions/`@keyframes` respect
  `gtk-enable-animations` → mandatory verification **T-V in T2**
  (phase 1, so that a negative finding goes back to grilling BEFORE phase 2).

### 2.4 Load-bearing findings

1. **The existing code is a 150 ms world** — migrating to Standard 250 is a
   deliberate, app-wide tempo change (G2, acceptance note).
2. **Spatial has zero consumers today** (no album-detail push; the main view's
   NavigationView is static; mini⇄full is an unanimated content swap) → G4.
3. **A central gating API exists:** libadwaita 0.9 (feature `v1_9`,
   `Cargo.toml`) binds `AnimationExt::set_follow_enable_animations_setting`
   (the property's default is **false** per the Adw docs — which is why all
   six places gate by hand today; T2 verifies this in a test).
4. **A lint on `Duration::from_millis` would be ineffective** (59 false
   positives, the real durations are raw `u32`) → the G6 cut.
5. **Parallel situation (verified, as of 2026-07-17):** see the blocklist in
   section 4.1 — the basis for the two-phase cut (G-Seq).

---

## 3. Final section text for `docs/ux-rules.md` (phase 2, T1)

All rules start `[planned]`; flips only in the named task commits. Level
tags: `[gtk]` where a widget state is checkable headless (model
`sidebar_device_card.rs:579ff` with `set_gtk_enable_animations`); `[manual]`
only where honestly nothing mechanical applies (the MOT-4 visual check).

```markdown
## O. Motion & Transitions

<!-- Section letter: M (tooltips) is assigned on main; N is claimed by
     feature/context-menu-unification ("N. Track context menu").
     Motion therefore takes O; the letter situation was verified against
     the main state when this section was inserted. -->

Motion illustrates, it never informs exclusively: every transition confirms
a state change that would also be fully visible without it —
`gtk-enable-animations=false` is the proof (MOT-7). Animations follow direct
user actions; background processes switch hard or fade in place (MOT-2, the
motion reading of P-4).

- **MOT-1** [planned] [gtk] — Four tokens, no free-floating numbers: every
  animation configured by Reprise itself uses one of four tokens from
  `ui/motion.rs`: **Micro** 150 ms ease-out for control state (icon swap
  Play⇄Pause, hover pills, chips, rating, press-scale; icon crossfades run
  as two Micro halves of 75 ms each) · **Standard** 250 ms ease-out-cubic
  for surfaces (sidebar/panel reveal, toast in, card collapse, crossfades
  cover/StatusPage⇄list) · **Ambient** 400 ms ease-out-cubic for
  atmospheric, non-interactive transitions (accent-color crossfade) ·
  **Spatial** = AdwSpringAnimation with Adw default spring parameters for
  directed navigation, added in code starting with the first directed
  navigation case. Ease-in only for what is leaving (toast out, Micro
  duration); linear only for genuine progress bars. Adw-internal widget
  animations without a duration API (OverlaySplitView, NavigationSplitView,
  ToastOverlay, Banner, Dialog, Popover — e.g. the push/pop slides of the
  settings subpages) count as system-given and are exempt from the token
  requirement.
  <!-- Flip criterion MOT-1: all call sites from the motion plan's audit
       inventory consume tokens; scripts/check-motion-tokens.sh is strict
       and without a leftover allowlist. -->
- **MOT-2** [planned] [gtk] — User action animates, background never:
  transitions follow direct user actions. Scan/watcher/mount/sync switch
  hard or fade without displacement (P-4 in motion language). Exception:
  the process card started by the user may fill/pulse.
- **MOT-3** [planned] [gtk] — Symmetry: same pattern = same widget + same
  token. Specifically: the left library sidebar uses exactly the same
  widget and thus exactly the same transition as the right info column
  (`adw::OverlaySplitView`, position start — the trigger for this
  section); the inner Tracks/Albums/Artists switch and the StatusPage⇄list
  stacks crossfade with the Standard token like the outer
  Library/Stats/Device stack.
- **MOT-4** [planned] [manual] — Lists do not move: no stagger/fade-in per
  row (windowed model, 200-item window, libraries beyond 1,600 rows).
  Allowed: a crossfade of the entire surface on a view change; named
  exception: the queue may animate DnD drop and single remove.
  <!-- The queue exception is permissive, not mandatory; its
       implementation lives in the follow-up branch and does not block the
       MOT-4 flip. -->
- **MOT-5** [planned] [gtk] — The player bar lives, but quietly: Play→Pause
  = icon crossfade (two Micro halves) + scale pulse (1.0→0.92→1.0, Micro);
  track change = cover/title crossfade; the waveform crossfades to the new
  track instead of dropping to 0; pause slightly desaturates the waveform
  fill (at draw time), play reverses it — the accent pipeline
  (`cover_accent`) stays untouched. The EQ indicators (track list,
  mini-player) run only during active playback; the idle bar is static — no
  permanent loop without playback.
  <!-- Flip criterion MOT-5 (follow-up branch, pattern TIP-1b/2b): scale
       pulse, waveform crossfade and pause desaturation are implemented and
       covered by a [gtk] test. Icon and track crossfade already exist
       tokenized; they alone do not flip the rule. -->
- **MOT-6** [planned] [gtk] — Nothing blocks: the model changes at frame 0,
  the animation only illustrates. A second action during a running
  animation jumps to the end state via `AdwAnimation::skip()` and then
  starts the new one; animation slots (track crossfade, icon crossfade,
  accent fade) call `skip()` instead of silently dropping the old handle.
- **MOT-7** [planned] [gtk] — `gtk-enable-animations=false` wins without
  exception: every token degrades centrally in `ui/motion.rs` to a hard
  switch (`follow-enable-animations-setting` or the central gate helper
  `animations_enabled()`), not at 30 call sites. Also applies to our own
  tick callbacks (waveform position smoothing: set the position hard;
  progress interpolation) and pulse timers. `gtk::Spinner` and
  GTK-internal CSS mechanics are system behavior and are not gated.
```

---

## 4. Task plan — two phases

> Format: one commit per task, TDD wherever a test is named, flips only in
> the named task commit, the commit title carries the rule ID.
>
> **HARD INSTRUCTION TO THE CODE PHASE: implement ONLY phase 1
> (T0, T2, T7, T8, T3 in this order/wave), then STOP.**
> Phase 2 (T1, T4, T5, T6, T9, T10) is locked until the gate check in
> 4.3 returns `GATE OPEN` AND main has been integrated AND the ownership
> scan has been repeated. No phase 2 task may be "pulled forward" — not
> partially either, and not as "just the test".

### 4.1 Global constraints

- Gates before EVERY commit: `cargo fmt --check` ·
  `cargo clippy --locked --all-targets --workspace -- -D warnings` ·
  `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace` ·
  `scripts/check-ux-traceability.sh` · `scripts/check-architecture.sh`.
- Display tests headless via `scripts/check-display-tests.sh`
  (the ignore marker `requires a display; run via xvfb-run` counts as
  coverage).
- Status change `[planned]→[active]` ONLY in the task commit that says so —
  and exclusively in phase 2 (before that the section does not exist).
- New user-facing strings (probably none) via the `N_!` catalogs + `de.po`
  in the same commit.
- Commits in English, no attribution footer, no push.
- **Blocklist phase 1 (verified overlap, as of 2026-07-17 — do NOT
  touch):**
  - feat/missing-import-errors (OPEN, worktree `../reprise-issues`) owns:
    `crates/reprise-gnome/src/ui/scan/**` (complete),
    `crates/reprise-gnome/src/ui/sidebar/**` (largely, incl.
    `sidebar_presentation.rs`), `ui/style/mod.rs`,
    `ui/window/library_shell.rs`, `ui/window/window.rs`,
    `ui/window/window_runtime_wiring.rs`.
  - feature/context-menu-unification (OPEN) owns:
    `ui/window/window_action_wiring.rs` and `docs/ux-rules.md` (section N).
  - `window_navigation.rs` and `information_column.rs` lie in the T4 rework
    area and likewise stay untouched in phase 1.
  - Merged and therefore FREE: feat/tag-editor-rework,
    feat/queue-playlist-improvements, feat/context-menu-improvements,
    feat/global-search-rework (`ui/browse/**` is free).
- **`docs/ux-rules.md` is touched in this branch exclusively in phase 2**
  (G1) — one integrates, never two in parallel.

### 4.2 Phase 1 — immediate, conflict-free

Parallelization map:

```text
T0 → T2 → { T7 ∥ T8 } → T3 → STOP (phase gate 4.3)
```

File ownership wave phase 1 (disjoint, T7/T8 in parallel through separate
agents; `ui/motion.rs` is **read-only** for both after T2):

| Task | Exclusive files |
|---|---|
| T2 | `ui/motion.rs` (new), `ui/mod.rs` (module registration) |
| T7 | `ui/player_bar/**`, `ui/compact/**`, `ui/style/cover_accent.rs` |
| T8 | `ui/style/tokens.rs`, `ui/eq_bars.rs`, `ui/browse/browse_chooser.rs`, `ui/preferences/preference_rhythmbox.rs`, `ui/info_panel/info_panel.rs`, `ui/lyrics/lyrics_view.rs` |
| T3 | `scripts/check-motion-tokens.sh` (new), CI/gate wiring |

#### Task 0: commit the plan

Commit this file (`docs/plans/ux-rules-motion.md`).

```bash
git commit -m "docs: add motion rules plan (grilled 2026-07-17)"
```

#### Task 2: create `ui/motion.rs` (+ mandatory verification T-V)

- Token constants: `MICRO_MS = 150`, `STANDARD_MS = 250`,
  `AMBIENT_MS = 400`; crossfade-half helper (`half(token)`); easings as
  constants for `adw::Easing` (Micro: EaseOutQuad/"ease-out", Standard and
  Ambient: EaseOutCubic) and for CSS strings. **No spatial code** (G4).
- `timed(widget, from, to, token, target) -> adw::TimedAnimation` — sets
  `set_follow_enable_animations_setting(true)`.
- Gate helper `animations_enabled() -> bool` (one place instead of six
  formulations) for ticks/timers.
- Slot helper `replace_animation(slot, new)` with `skip()` on the
  predecessor (for MOT-6, T7).
- Unit tests for the token values; a `[gtk]` test that `timed(…)` sets the
  follow property (which at the same time verifies that the Adw default is
  false — finding 2.4.3).
- **T-V (mandatory):** a headless/xvfb spike on whether CSS transitions and
  `@keyframes` stand still under `gtk-enable-animations=false`. Document the
  result as a comment in `ui/motion.rs`. **If T-V comes out negative (CSS
  ignores the setting): record the finding, finish the task normally, but
  report it in the code phase's closing report as a GRILLING RETURN — no
  silent decision about a CSS degradation path.**

```bash
git commit -m "feat(motion): add motion tokens and central animation helpers"
```

#### Task 7: player/compact/accent — token migration + MOT-6 skip semantics

- Track crossfade (`player_bar.rs`) → Standard halves (2×125, unchanged
  tempo); icon crossfade → **Micro halves 2×75** (G2i — deliberately slower
  than the previous 2×60; correct the doc comment);
  `compact_player.rs::CROSSFADE_HALF_MS` → Standard half; the hover overlay
  revealer (`compact_player_layouts.rs`) → Micro; the accent crossfade
  (`cover_accent.rs`) → **Ambient** (G2ii); the waveform peaks build-up
  (`waveform_seek.rs`) → Ambient (atmospheric, non-interactive; the per-bar
  stagger stays an implementation detail).
- All Adw animations in these files via `motion::timed()` or the follow
  property instead of hand-gating; gate the waveform position smoothing
  (`animations_enabled()` false → set the position hard) (G8, player side).
- MOT-6 skip semantics: switch the single-slot systems (track crossfade,
  icon crossfade, accent fade, compact_player) to
  `motion::replace_animation()` with `skip()`.
- TDD: `[gtk]` test `mot_6_…` (a second `set_track` during a running
  animation → the end state of the first one is immediately visible, no
  intermediate state; the model state changes before the animation ends);
  `[gtk]` test `set_gtk_enable_animations(false)` → immediate end state
  including position smoothing (model `sidebar_device_card.rs:579ff`).
- NO flip (the section does not exist yet; flips in phase 2, T9).

```bash
git commit -m "feat(motion): MOT-6 skip semantics and player-side token migration"
```

#### Task 8: token migration of the conflict-free remaining call sites

- `tokens.rs`: `TRANSITION` consumes `motion::MICRO_MS` (hover/focus/press:
  the duration stays 150 ms; the easing follows the Micro token (ease-out),
  accepted in the review of 2026-07-18).
- `info_panel.rs:148`, `lyrics_view.rs:82`, `browse_chooser.rs:28`,
  `preference_rhythmbox.rs:323`: explicit `set_transition_duration` with the
  Standard token (previously the default ~200 → deliberately 250).
- `eq_bars.rs`: a comment pointing at the MOT-5 EQ exception (G7) — the loop
  durations (1100/650 ms) are not transition tokens and stay.
- `[gtk]` test `mot_1_…` (sampled widgets carry the token duration).
- NO flip.

```bash
git commit -m "feat(motion): migrate conflict-free call sites to motion tokens"
```

#### Task 3: lint `scripts/check-motion-tokens.sh`

In the style of `check-ux-traceability.sh`. Forbids, outside
`ui/motion.rs`/`ui/style/tokens.rs` (G6):
1. `TimedAnimation::new(…)` with an integer literal as the duration,
2. `set_transition_duration(`/`.transition_duration(` with an integer
   literal.

Ticks/CSS remain a review matter. **Phase 2 leftover list:** the files not
yet migrated (`ui/sidebar/sidebar_device_card.rs`,
`ui/scan/scan_progress.rs`, `ui/window/window.rs`) sit in an allowlist
documented in the script with the comment `# Phase 2 — migrated in T4/T5 and
removed from the allowlist`. Add it to the local gate battery of the
follow-up tasks; wire it in at the same place where
`check-ux-traceability.sh` hangs.

```bash
git commit -m "ci: add motion-token lint (MOT-1 gate)"
```

**→ END OF PHASE 1. STOP. No further task without gate check 4.3.**

### 4.3 Phase gate (hard criterion)

Phase 2 may only begin once ALL three conditions are met:

1. **Merge condition (machine-checked):** the following command prints
   `GATE OPEN`:

   ```bash
   git fetch origin --prune
   gate_open=1
   for b in feat/missing-import-errors feature/context-menu-unification; do
     if git rev-parse --verify --quiet "origin/$b" >/dev/null; then
       git merge-base --is-ancestor "origin/$b" origin/main \
         || { echo "GATE CLOSED: $b not merged into origin/main"; gate_open=0; }
     fi
   done
   # Zusatzprobe: Sektion N muss real auf main sein (deckt Branch-Löschung
   # ohne Merge ab)
   git grep -q "## N. Track-Kontextmenü" origin/main -- docs/ux-rules.md \
     || { echo "GATE CLOSED: section N not on origin/main"; gate_open=0; }
   [ "$gate_open" = "1" ] && echo "GATE OPEN"
   ```

2. **Integration:** `origin/main` is merged into `feat/ux-rules-motion`,
   conflicts resolved, the full gate battery green.
3. **Ownership scan repeated:** the blocklist check from 4.1 is repeated
   against the state current AT THAT POINT (list the open branches/worktrees,
   check the overlap with the T4/T5/T6 files). New overlaps become handoffs,
   not edits — when in doubt, back to the user.

### 4.4 Phase 2 — after the gate

Parallelization map:

```text
GATE → T1 → { T4 ∥ (T5 → T6) } → T9 → T10
```

File ownership wave phase 2 (disjoint, T4 and T5/T6 in parallel through
separate agents; `ui/motion.rs` read-only):

| Task | Exclusive files |
|---|---|
| T1 | `docs/ux-rules.md` |
| T4 | `ui/window/library_shell.rs`, `ui/window/window_navigation.rs`, `ui/window/window.rs`, `ui/sidebar/sidebar_presentation.rs`, `ui/info_panel/information_column.rs` |
| T5+T6 | `ui/scan/scan_progress.rs`, `ui/sidebar/sidebar_device_card.rs` (+ the associated tests) |
| T9 | `RELEASING.md`, `docs/ux-rules.md` (flips), `scripts/check-motion-tokens.sh` (leftover-list check) |
| T10 | `.superpowers/sdd/progress.md` |

#### Task 1: section O in `docs/ux-rules.md`

The section text from section 3 verbatim, after the last section (expected:
N), before the closing paragraph. Verify the letter situation against the
real state of main (should N not be the last section: adjust the comment;
the letter stays O only if O is free — otherwise the next free letter plus
an adjustment of ALL MOT references; that is a mechanical rename, not a new
decision). `scripts/check-ux-traceability.sh` green (all MOT `[planned]`,
the prefix is detected dynamically — no script update needed).

```bash
git commit -m "docs: add ux-rules section O (MOT motion rules, planned)"
```

#### Task 4: MOT-3 — sidebar symmetry (the trigger bug) → flip MOT-3

TDD: the `[gtk]` display test `mot_3_…` first (both side surfaces use the
same widget pattern; toggle round trip with a wide AND a narrow window).
Rework per G5: the left sidebar onto `adw::OverlaySplitView` (position
start); port `apply_sidebar_visibility`, the `manually_hidden` logic and the
breakpoint(<800 px) interplay out of `window_navigation.rs`; extend the
existing tests with breakpoint cases (check for focus stealing when hiding).
The inner Tracks/Albums/Artists stack + the StatusPage⇄list stacks receive
the Standard crossfade (MOT-3 sentence 2); the outer stack
(`window.rs:368`) migrates 150→Standard and drops out of the lint allowlist.
Flip MOT-3 in this commit.

```bash
git commit -m "feat(ui): MOT-3 — left sidebar slides like the right panel"
```

#### Task 5: MOT-7 — finish centralizing the gating → flip MOT-7

The remaining manual `is_gtk_enable_animations` places
(`sidebar_device_card.rs`) onto the motion.rs helper; gate the progress
interpolation (tick) through `animations_enabled()` (false → set the value
hard); gate the scan pulse timer (false → the timer does not start, the bar
stays statically determinate); the transitions of these files 150→Standard
token, remove the files from the lint allowlist. TDD modelled on
`sidebar_device_card.rs:579ff` (`set_gtk_enable_animations(false)` → the end
state immediately, the pulse timer does not start). Flip MOT-7 in this commit
(the player side has been covered since T7/phase 1, the T-V finding is
available from T2).

```bash
git commit -m "feat(motion): MOT-7 — centralize enable-animations gating"
```

#### Task 6: MOT-2 — harden background surfaces → flip MOT-2

Scan card/device card/lyrics loading state: pin down crossfade without
displacement (already a crossfade today — the test fixes that), no slide
transitions on background triggers. `[gtk]` test `mot_2_…`: the transition
types of the background widgets are Crossfade/None; the scan-card reveal
displaces no neighbors (allocation comparison). Same agent as T5 (the file
ownership overlaps), its own commit. Flip MOT-2 in this commit.

```bash
git commit -m "feat(ui): MOT-2 — background surfaces fade in place, never slide"
```

#### Task 9: RELEASING.md + the remaining flips (MOT-1, MOT-4, MOT-6)

- MOT-4 `[manual]`: a bullet in "## Manual GNOME QA" (English, IDs
  verbatim, pattern RELEASING.md:174-184) → flip MOT-4.
- Flip MOT-1: the lint allowlist is empty (T4/T5 done), all call sites from
  the inventory consume tokens — the commit body points at the phase 1
  commits (T7/T8).
- Flip MOT-6: the implementation lies in T7 (phase 1) — the commit body
  points at it.
- MOT-5 stays `[planned]` (G9, follow-up branch) — the flip-criterion
  comment is already in the section text.

```bash
git commit -m "docs: MOT-1/4/6 — manual QA entry and flip completed motion rules to active"
```

#### Task 10: closing

Full gate battery incl. display tests; ledger entry in
`.superpowers/sdd/progress.md` (naming the follow-up branch scope: the new
MOT-5 behavior + the queue drop animation); handoff notes for overlaps newly
discovered in phase 2; the merge title documents the final section-letter
situation.

---

## 5. Test strategy per rule

| Rule | Level | Mechanically checkable (headless, xvfb) | Honestly manual |
|---|---|---|---|
| MOT-1 | [gtk] | Token constants (unit); samples: widget `transition_duration()` == token; lint T3 flanks it (does not count as coverage — the rule-named test does) | Felt coherence of the durations (the deliberate 150→250 change!) |
| MOT-2 | [gtk] | Transition type of the background widgets (Crossfade/None, never Slide); the scan-card reveal displaces no neighbors (allocation comparison) | "Nothing animates under the cursor" for real |
| MOT-3 | [gtk] | Both side surfaces: same widget type (`OverlaySplitView`), same configuration; toggle round trip with a wide/narrow window; breakpoint cases | Visual synchrony of the slides (Adw-internal) |
| MOT-4 | [manual] | — (a negative rule about code that does not exist) | Reload/scroll/DnD of a 10k list: no row movement |
| MOT-5 | [gtk] | **This branch:** the icon name after a skip is correct; the existing crossfades use Micro/Standard halves. **Follow-up branch:** pulse/waveform-crossfade/desaturation tests (`set_gtk_enable_animations(false)` → immediate end state) | The effect of pulse/desaturation (follow-up branch) |
| MOT-6 | [gtk] | A second `set_track`/`set_state` during a running animation → end state frame-accurate; the model state (`playback_state`) changes before the animation ends | The feel of responsiveness under fast clicking |
| MOT-7 | [gtk] | `set_gtk_enable_animations(false)`: the follow property is set, tick callbacks set hard, the pulse timer does not start (model `sidebar_device_card.rs:579ff`) | CSS behavior on real desktops (after T-V) |

---

## 6. Acceptance checklist

**Phase 1:**

- [ ] `ui/motion.rs` exists (Micro 150 / Standard 250 / Ambient 400, no
      spatial code); `timed()` sets the follow property (test).
- [ ] The T-V finding is documented as a comment in `motion.rs`; on a
      negative finding it is reported as a grilling return.
- [ ] Player: icon crossfade 2×75, track crossfade Standard halves, accent
      fade Ambient; skip semantics active (MOT-6 test green).
- [ ] Conflict-free call sites tokenized; `tokens::TRANSITION` consumes
      `MICRO_MS`.
- [ ] `check-motion-tokens.sh` green with a documented phase 2 leftover
      list.
- [ ] NO file from the blocklist touched; `docs/ux-rules.md` untouched;
      no flips.

**Phase 2:**

- [ ] Gate check 4.3 ran and returned `GATE OPEN`; main integrated;
      ownership scan repeated.
- [ ] Both sidebars slide identically (`OverlaySplitView` on both sides);
      breakpoint/`manually_hidden` cases tested.
- [ ] The inner library stack and StatusPage⇄list crossfade like the outer
      stack (MOT-3 sentence 2 — hard cuts until now).
- [ ] No background event (scan/sync/mount/lyrics loading) animates
      anything under the cursor or shifts layout.
- [ ] `gtk-enable-animations=false` → completely instant, incl. position
      smoothing and pulse.
- [ ] No animation ever delays an action; a second action skips.
- [ ] The lint allowlist is empty; no integer durations at animation APIs
      outside `motion.rs`/`tokens.rs`.
- [ ] `check-ux-traceability.sh` green; MOT-1/2/3/4/6/7 `[active]`, MOT-5
      `[planned]` with its flip criterion; the MOT-4 bullet verbatim in
      RELEASING.md.
- [ ] Ledger entry; the merge title names the final section-letter
      situation.
- [ ] **Deliberate tempo change signed off:** the app feels measurably
      different because of 150→250 on surfaces — accept that explicitly, do
      not discover it as a side effect (G2).

**Follow-up branch (not here):** the Play/Pause scale pulse, the waveform
crossfade on track change, the pause desaturation of the waveform fill
(→ flip MOT-5), the queue drop/remove animation (use the MOT-4 exception).

---

## 7. Operational notes

- The worktree `.worktrees/transitions` exists (branch `feat/transitions`,
  clean) and lags behind origin/main. Before starting: `git pull --ff-only`,
  then `git branch -m feat/transitions feat/ux-rules-motion`. **No build
  during worktree setup.**
- Pipeline: this plan → code phase headless in the worktree (ONLY phase 1) →
  gate check → a separate phase 2 run.
- `docs/ux-rules.md` stays untouched until the gate (G1); phase 1
  (T2, T7, T8, T3) is fully independent of it.

---

## 8. Open risks

1. **CSS transitions vs. enable-animations unverified** (T-V in T2). If the
   test comes out negative, MOT-7 needs a CSS degradation path (load an
   animation-free token variant) — that is a grilling return, not a silent
   decision; the effort is not estimated today.
2. **The OverlaySplitView animation cannot be tokenized** — by the G3
   wording the app's most prominent transition stays outside the token
   system; symmetry (MOT-3) depends on BOTH sides using the same
   Adw-internal animation (hence G5: exactly the same widget).
3. **The sidebar rework (G5) touches focus/breakpoint special cases**
   (`manually_hidden`, focus stealing, <800 px) — a regression surface; the
   existing tests are deliberately extended with breakpoint cases in T4.
4. **The default of `follow-enable-animations-setting`** is false per the
   Adw docs — T2 verifies that in a test instead of relying on the
   documentation.
5. **Perceptibility of the token migration** (G2): 150→250 ms on all
   surfaces is a deliberate, app-wide tempo change — acceptance signs off on
   it explicitly (phase 2 checklist).
6. **Phase 2 waiting time:** until feat/missing-import-errors and
   feature/context-menu-unification are on main, the territory may shift
   again (further branches, renamed files, new sections in ux-rules.md).
   That is why gate check 4.3 mandatorily repeats the ownership scan; new
   overlaps become handoffs, not edits. The letter situation O is likewise
   verified against the real state of main in T1.
