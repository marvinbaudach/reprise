---
slug: motion-player
worktree: ~/Projects/reprise/.worktrees/transitions
branch: feat/motion-player
phase: shipped
codex_session:
created: 2026-07-18
---
# MOT-5 player motion (follow-up branch) — implementation plan

This shipped plan is retained because tracked files still reference it as durable documentation.

**Status:** grilled 2026-07-18, ready for /code (Phase A)
**Branch:** feat/motion-player (on origin/main `3d878d4`)

Goal: flip MOT-5 `[planned]→[active]` (docs/ux-rules.md, Section O) per the
flip criterion: scale pulse, waveform crossfade and pause desaturation
implemented and covered by a `[gtk]` test.

## 1. Grilled decisions (2026-07-18, final)

| No. | Question | Decision |
|---|---|---|
| G1 | Scale-pulse mechanics | **CSS `@keyframes` one-shot** via class toggle on the play⇄pause change. Duration/easing from `motion::MICRO_MS`/`motion::MICRO_CSS_EASING` via `format!` into the CSS string (no raw literal in the source). The class is set on the state change and removed again via `glib::timeout_add_local_once(Duration::from_millis(motion::MICRO_MS as u64))`; another change during the pulse: remove the class → set it again in the next frame (clean retrigger). No additional gating needed: GTK CSS demonstrably honors `gtk-enable-animations=false` hard (T-V sentinel `mot_7_css_honours_enable_animations_setting`). |
| G2 | Waveform crossfade | **Alpha crossfade** old→new: two-pass draw (old display bars with falling, new ones with rising alpha) over **Ambient (400 ms)**. First track (no old peaks) keeps the existing stagger build-up. |
| G3 | Pause desaturation | **OKLCH chroma reduction ×0.45** at draw time; the color math from `cover_accent.rs` is opened up to `pub(in crate::ui)` instead of duplicated. The transition is **animated over Standard (250 ms)** via `motion::timed` (desaturation_progress 0↔1); `gtk-enable-animations=false` → hard flip. The `cover_accent` pipeline (global `@reprise_player_accent` provider) stays untouched — desaturation is purely local in the waveform `draw()`. |
| G4 | Queue animation | **Omitted.** The MOT-4 exception is permissive, not mandatory, and blocks no flip. Implement it only once the TAG-1 reload path is touched in a queue branch of its own. |
| G5 | Sequencing | **Two-phase.** Phase A immediately (T0, T2 waveform crossfade, T3 desaturation — territory is free). Phase B only after the three active player-bar branches (`feat/artist-cover-playerbar-fixes`, `feat/keyboard-nav`, `feat/minor-improvements`) have been merged: T1 scale pulse + T5 flip. |

## 2. Core audit findings (verified)

- `ui/motion.rs` provides MICRO/STANDARD/AMBIENT, `half()`, `timed()`
  (follow-enable-animations), `animations_enabled()`, `replace_animation()`
  (skip). The icon crossfade (2×75) and the track crossfade (2×125) already
  exist tokenized — on their own they do not flip MOT-5.
- `set_peaks()` hard-overwrites `raw_peaks`/`display_peaks`
  (`waveform_seek.rs:317-319`); build-up: `build_progress`/`build_start_us`
  (`:130-131`), tick `ensure_tick_callback()` (`:407-449`), stagger in the
  `draw()` (`:486-504`). No previous state, no opacity mechanism in the draw.
- `draw()` reads the fill color per frame via `area.color()` (`:476`) —
  `.waveform-seek { color: @reprise_player_accent; }`. `WaveformSeek` does NOT
  know the playback state (no setter); `PlayerBar::set_state()`
  (`player_bar.rs:370-386`) does not forward to the waveform.
- The OKLCH conversion lives in `cover_accent.rs:44-140` (module-private);
  `lerp` helper `cover_accent.rs:309-313`. No shared state between the
  cover_accent provider and the waveform draw (independence confirmed).
- Button CSS: `.player-bar-play` with `transition: … transform` and
  `:active { transform: scale(0.94) }` (`player_bar_layout.rs:268-278`) — a
  CSS transform on the button is proven. Keyframes precedent:
  `eq_bars.rs:91-115`, `player_bar_layout.rs:304-312`.
- Test precedents: `mot_6_second_track_and_state_changes_finish_the_previous_visual_state`,
  `mot_7_player_bar_hard_switches_when_system_animations_are_disabled`
  (player_bar_tests.rs), `mot_7_waveform_completes_build_up_when_animations_disabled_mid_build`
  (waveform_seek_tests.rs). Mind the borrow-scope discipline before `window.close()`.
- Idle confirmed: no permanent loop without playback (mini EQ `.playing`,
  track list `.playback-paused`, skeleton static, the tick stops itself).

## 3. Task plan

> One commit per task, TDD, the commit title carries the rule ID. Gates before
> EVERY commit: `cargo fmt --check` · `cargo clippy --locked --all-targets
> --workspace -- -D warnings` · `env XDG_DATA_HOME=$(mktemp -d)
> XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace` ·
> `scripts/check-ux-traceability.sh` · `scripts/check-motion-tokens.sh` ·
> `scripts/check-architecture.sh`. Display tests headless INDIVIDUALLY via
> `dbus-run-session -- xvfb-run -a env … cargo test -p reprise-gnome --
> --ignored --exact <path>` (bundled in one process → known SIGSEGV).
> Status change `[planned]→[active]` ONLY in the named task commit (T5, Phase B).

### Phase A — immediately (the waveform territory is free)

#### Task 0: commit the plan

```bash
git commit -m "docs: add MOT-5 player-motion plan (grilled 2026-07-18)"
```

#### Task 2: waveform crossfade on track change (only `waveform_seek.rs` + tests)

- Extend `State`: `previous_bars: Vec<…>` (copy of the most recently drawn
  display bars including their heights), `crossfade_progress: f64` (1.0 = no
  fade), `crossfade_start_us: i64`.
- `set_peaks()`: if old display bars exist AND `animations_enabled()`, they are
  copied into `previous_bars` and `crossfade_progress = 0.0` is started
  (duration `motion::AMBIENT_MS`); in that case the stagger build-up is NOT
  started (`build_progress = 1.0`). Without old bars (first track) the existing
  build-up path stays unchanged. Disabled →
  `previous_bars.clear()`, `crossfade_progress = 1.0` (hard switch, the pattern
  of the existing disabled path).
- Tick (`ensure_tick_callback`): drives `crossfade_progress` analogously to
  `build_progress`; the settled condition is extended to cover the crossfade.
- `draw()`: while `crossfade_progress < 1.0` two passes — old bars with alpha
  `(1 - p)`, new ones with alpha `p` (each multiplied onto the existing alpha
  constants); after that exactly today's single-pass path.
- Width change during the fade (`ensure_resampled` invalidates display bars):
  skip the fade to its end state (`previous_bars.clear()`,
  `crossfade_progress = 1.0`) — the spirit of MOT-6, no resample of the old bars.
- Fast track change during a running fade: the old fade ends hard (the new bars
  become `previous_bars`), a new fade starts — no stacking.
- Tests (waveform_seek_tests.rs, display-ignored like the precedents):
  `mot_5_waveform_crossfades_to_the_new_track_instead_of_rebuilding`
  (old bars present → crossfade_progress starts, build_progress stays
  1.0; end state after it elapses) and
  `mot_7_waveform_crossfade_hard_switches_when_animations_are_disabled`
  (disabled → new bars immediately, previous empty, no tick).

```bash
git commit -m "feat(motion): MOT-5 — waveform crossfades to the new track"
```

#### Task 3: pause desaturation (`waveform_seek.rs`, `player_bar.rs` wiring, `style/cover_accent.rs` visibility)

- `cover_accent.rs`: open up the required conversion helpers (RGB↔OKLCH resp.
  chroma scaling; the minimum necessary surface, e.g. a new function
  `pub(in crate::ui) fn scale_chroma(r, g, b, factor) -> (f64, f64, f64)`
  alongside the existing private helpers) — NO change to
  provider/slot/extraction.
- `WaveformSeek::set_paused(paused: bool)`: drives `desaturation_progress`
  (0.0 = fully saturated, 1.0 = desaturated) via `motion::timed(&area, from,
  to, motion::STANDARD, CallbackAnimationTarget{ set progress + queue_draw })`,
  slot + `motion::replace_animation` (MOT-6: another change skips).
  `!animations_enabled()` → set the value hard, no tick (precedent:
  position smoothing).
- `draw()`/`draw_fallback()`: after `area.color()`, damp the fill color with
  `scale_chroma(…, 1.0 - 0.55 * desaturation_progress)` — at
  `desaturation_progress = 1.0` that is chroma ×0.45 (decision G3).
  Applies to the played fill and the playhead; unplayed/ghost alphas unchanged.
- **Wiring NOT in Phase A:** `player_bar.rs` is locked by G5. Phase A
  delivers the `set_paused` API + tests at widget level; the call from
  `PlayerBar::set_state()` (`state != PlaybackState::Playing`, Stopped counts
  like Paused) and the identical wiring in the compact player follow in
  Phase B (T1 commit).
- Tests: `mot_5_pause_desaturates_the_waveform_fill_and_play_restores_it`
  (set_paused(true) → progress target 1.0, the slot carries STANDARD_MS +
  follows_enable_animations; set_paused(false) → back again) and
  `mot_7_waveform_desaturation_hard_switches_when_animations_are_disabled`.
  In addition, a unit assertion that `cover_accent`'s provider state stays
  untouched by `set_paused` (no call into the provider API — pure draw-time
  computation; e.g. via test comment/design, no forced mock).

```bash
git commit -m "feat(motion): MOT-5 — pause desaturates the waveform fill"
```

**→ END OF PHASE A. STOP. T1/T5 are locked (Phase B gate below).**

### Phase B gate (hard criterion)

Phase B starts only once ALL three branches
`feat/artist-cover-playerbar-fixes`, `feat/keyboard-nav`,
`feat/minor-improvements` are merged into origin/main (by content — with
squash merges the file state counts, not the branch pointer), main has been
integrated and a repeated ownership scan
(`git diff --name-only origin/main...<branch> | grep player_bar`) for
player_bar.rs/player_bar_layout.rs is empty.

### Phase B — after the gate

#### Task 1: scale pulse play⇄pause (`player_bar.rs`, `player_bar_layout.rs`)

- CSS in `player_bar_layout.rs::css()`: `@keyframes reprise-play-pulse`
  (`0% scale(1.0)` → `50% scale(0.92)` → `100% scale(1.0)`) + class
  `.player-bar-play.pulsing { animation: reprise-play-pulse {MICRO_MS}ms
  {MICRO_CSS_EASING} 1; }` — values via `format!` from the `motion` constants.
- `animate_play_icon_change()` (or `set_state()`): on a real state change
  remove the `pulsing` class and set it again (in idle/the next frame);
  removal after `MICRO_MS` via `timeout_add_local_once`.
  Check coexistence with the `:active` scale (press) and the icon crossfade.
- **Catch up the desaturation wiring (moved out of T3):**
  `PlayerBar::set_state()` calls `self.waveform.set_paused(state !=
  PlaybackState::Playing)`; wire the compact player identically.
  Display test: pause via `set_state` → the waveform desaturation starts.
- Tests: `mot_5_play_pause_pulses_on_state_change` (class set after
  set_state, removed after it elapses — display test with loop pump) +
  verification that with `gtk-enable-animations=false` the end state is
  identical (the keyframes do not run — covered by T-V, assertion:
  the class logic still runs through, button state correct).

```bash
git commit -m "feat(motion): MOT-5 — play/pause scale pulse"
```

#### Task 5: flip MOT-5 + wrap-up

- `docs/ux-rules.md`: MOT-5 `[planned]→[active]`; remove the flip-criterion
  comment. Verify beforehand against the state of main (ownership: one
  integrates, never two in parallel).
- Ledger `.superpowers/sdd/progress.md`: Phase A+B, decisions G1–G5,
  queue exception deliberately not implemented (G4).
- Full gate battery including all mot_ display tests individually.

```bash
git commit -m "docs: MOT-5 — flip player-bar motion rule to active"
```

## 4. Acceptance

- [ ] Track change: the waveform fades old→new over 400 ms (Ambient); no
      collapse to 0; the first track keeps its build-up. (Phase A)
- [ ] Pause visibly desaturates the waveform fill (chroma ×0.45), animated
      over 250 ms; play reverses it; `cover_accent` untouched. (Phase A)
- [ ] `gtk-enable-animations=false` → everything instant (hard-switch tests
      green, no hanging tick). (Phase A)
- [ ] Play⇄pause pulses 1.0→0.92→1.0 (Micro) in addition to the icon crossfade.
      (Phase B)
- [ ] MOT-5 `[active]`; traceability, motion lint, full battery green.
      (Phase B)
- [ ] No intervention in the three active player-bar branches before they merge.

## 5. Risks

1. Crossfade × `ensure_resampled()`: width change during the fade →
   the fade skips to its end state (decided, T2) — no resample of old bars.
2. Rapid play/pause hammering (Phase B): the class retrigger must force style
   invalidation (remove → idle → add).
3. The player-bar territory is actively moving — the Phase B gate with a
   repeated ownership scan is binding.
