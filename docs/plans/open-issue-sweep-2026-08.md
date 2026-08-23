---
slug: open-issue-sweep-2026-08
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-22
---
# Closing out the open issue backlog

On 2026-08-22 the tracker held 29 open issues. Twelve of them describe
behaviour the repository has since shipped and were never closed; one more is
resolved but not in the shape it was filed; two describe harness artefacts
rather than product defects. This plan records that triage
with its evidence, and turns what actually remains into tasks.

**Every code reference is against `origin/dev` = `b9d5308011`.** The shared main
checkout lags behind; read with `git show origin/dev:<path>`.

The triage below is a **code and contract reading**, not a re-run of the
original reproductions. Where an issue's claim can only be settled by driving
the app, that is said so and the re-run is a task, not an assumption.

## Part A — issues to close, with their evidence

Each row names the artefact that discharges the issue. All twelve were closed
on 2026-08-22 with that artefact quoted in the closing comment, so each close is
a finding rather than an opinion.

| Issue | Discharged by |
|---|---|
| #8 Declare an MSRV | `Cargo.toml:23` — `rust-version = "1.92"` |
| #69 Android device in the sidebar | `ui/sidebar/sidebar_device_card.rs` — "Connected-device cards shown below the scrolling navigation rows" |
| #78 Saving metadata jumps the table to the top | `TAG-1` [active] — "Save is navigation-neutral: saving changes neither scroll nor the library's view" |
| #80 Podcast content visible over Music during the transition | `ui/window/content_stack.rs` — `transition_for_switch` returns `FadeThrough` for `podcasts\|youtube ↔ library`, with the rule-named test `mot_8_dense_source_switch_retains_standard_motion` |
| #96 Separate Podcasts, YouTube and Radio | four active rules name the issue directly (`MTP-54`, the Preferences block, `NET-1a`) |
| #98 Unify the empty states | `ui/source_empty_state.rs` — `SRC-10`, one shared shape used by `podcasts_view`, `radio_view` and the YouTube path |
| #105 Buffered range and buffering state | `PLAY-13` [active] — paler contiguous-buffer segment plus "Streaming · X% loaded" |
| #107 One offline presentation contract | `NET-3` [active] — seven states app-wide, and six concrete behaviours filed under "6e, issue #107" |
| #194 Episode rows absent from the a11y tree | `ui/podcasts/podcasts_groups.rs:358-372` — `Focusable`, `AccessibleRole::Button`, `Property::Label`, `State::Selected` |
| #284 Progress bars shift the layout | `FB-9`, with `fb_9_counterprobe_legacy_toolbar_status_moves_the_content` proving the jump exists **only** on the retired in-flow path |
| #403 Sidebar rows expose no action | `ui/sidebar/sidebar_presentation.rs:340-372` — a real `GtkButton` inside the row carries role and label, asserted by the presentation test |
| #460 Section-header-aware centring | `ui/list_geometry_layout.rs` owns `headers_above`; `reload_restore.rs` consumes `ListLayout`, and `flat_list_centered_track_scroll_target` survives only as a `#[cfg(test)]` rows-only counter-oracle |

Two further issues are closable as **harness artefacts**, which is what their own
filings already suspected:

- **#407** — closed 2026-08-22. "Search all fields" is the search entry's placeholder
  (`ui/strings.rs:335`), not a button; clicking it focuses an entry and changes
  no pixels the sweep's before/after oracle can see. "Add filter" is a
  `MenuButton` (`ui/browse/browse_bar.rs:103`) whose popover is a separate
  surface the screenshot comparison does not capture. Neither is a product
  defect. Close with that reading, and note the oracle gap on the sweep's own
  issue instead.
- **#108** — `ui/window/responsive_side_panels.rs` now carries an explicit
  `ConstraintState` with `changed_by_user`, `snapshot` and `unapply`, so a
  panel the user closed is not reopened by a later resize.
  `docs/plans/podcasts-youtube-radio-turn6.md:573` already recorded that the
  evidence pointed at a harness race. **Task 9 re-runs the scenario before this
  one is closed** — this is the one artefact claim that is cheap to falsify.

**#79** is resolved, but not the way it was filed. `FB-8` was amended on
2026-08-07: the Issues block no longer disappears while a scan runs, and the
progress cards sit at the bottom of the pinned region
(`ui/sidebar/sidebar_issues_section.rs:29-48`) rather than between the heading
and `Missing files`. The complaint — activity sitting visually outside the
section — is gone; the requested ordering is not what shipped. **Decision
needed** (task 10): close as resolved-by-design, or reopen the ordering as a
one-line change.

## Part B — what actually remains

### 1. The centring fight, and the jump from the player bar

**New, user-reported on 2026-08-22:** clicking the running song in the player
bar reveals its row at the **top** of the table instead of centred. That
violates `NAV-10b` ("explicit metadata/reveal navigation always selects,
focuses, and centers").

This is not a new defect. `docs/plans/one-centering-path-for-jump-and-clear.md`
is `phase: shipped` (#576), but its follow-up
`docs/plans/one-centering-path-preseed-variant.md` is `phase: todo` **and
untracked in the working tree**. Its measurement already names the mechanism:

- our own edge snap (`centered_scroll_restore.rs:55-59`) writes the edge value
  before the centring attempts; and
- GTK's allocation pass after the model swap writes the remembered offset back.

They are interdependent, not alternatives. The fix is the preseed the anchor
path already uses (`reload_anchor_scroll.rs:52-80`): put the target value into
the geometry **before** GTK writes, rather than correcting GTK afterwards.

**Task 1.** Commit `one-centering-path-preseed-variant.md` — it is the root-cause
record for a live user-visible bug and must not stay untracked.

**Task 2.** ~~File the player-bar jump as its own issue~~ — done, **#620**,
referencing `NAV-10b` and that plan, with the reproduction: play a track, scroll
away, click the title in the player bar.

**Task 3.** Establish whether the player-bar reveal takes the restore path or
the anchor path. `centered_scroll_restore::schedule` versus
`reload_anchor_scroll`. Then extend the preseed plan to cover it — the plan
today speaks only of the search-clearing restore.

**Task 4.** Implement the preseed edition per that plan's tasks 1-3, with the
control-arm measurement it specifies: the four-step fight
(`gtk → hold → hold → …`) must collapse to a single landing, and a run whose
control arm never moved is reported as UNPROVEN, not as passed.

### 2. #404 — sorting is unreachable for assistive technology

`docs/plans/a11y-atspi-roles-and-actions.md` "Aufgabe B" specified the fix and
it was never built: today the column-header click is the only sort control
(`ui/track_list/track_list_sort.rs`), and `GtkColumnViewTitle` reports role
`filler` with zero actions — GTK offers no API to change that.

**Task 5.** Add the sort `MenuButton` (field plus direction) to
`ui/browse/browse_bar.rs`, beside `add_filter`. It writes the **same**
`shared.sort` and triggers the same reload — no second truth — and a header
click mirrors back into the menu's mark. Labels come from `strings`, not
literals; the button matches `+ Add Filter` in height and style classes.
The GTK defect gets its own upstream report, separately.

### 3. #405 — rating stars share an accessible name

The star buttons carry distinct tooltips via
`ui/track_list/rating.rs:277`, but `ui/lazy_tooltip.rs` supplies text only from
`query-tooltip`; it writes **no** AT-SPI description. The accessible name is
still the `★`/`☆` glyph on every star.

**Task 6.** Give each star an explicit
`accessible::Property::Label` (and a `Description` where the tooltip adds
something), so the tree distinguishes them. The stars stay
`focusable(false)` — the row is the collection's sole tab stop and rating stays
keyboard-reachable through Edit Tags — so this is about naming, not about a new
tab stop. Cover it with a test that reads the property back, not the tooltip.

### 4. #100 — selection is louder than the playing row

No stylesheet rule tones the native `:selected` fill down relative to the
now-playing treatment; `ui/style/` carries no selected-row rule at all.

**Task 7.** Decide the contract first and write it into `docs/ux-rules.md`: the
playing row must stay the loudest signal in the table while selection stays
unambiguous for the batch operations that need it. Then implement it as a
scoped stylesheet rule with a measured contrast check, not by eye.

### 5. #411 — no busy indicator for multi-second search and sort

Reprise ships busy indicators for Library Doctor, Device Sync, Podcasts,
Concerts, Releases, Radio, Lyrics and updates; the track list has none
(`ui/browse/` and `track_list_reload.rs` carry no spinner or busy state).

**Task 8.** Add a busy state to the search/sort reload path above a measured
latency threshold, using the existing indicator vocabulary rather than a new
one. The threshold and the measurement that sets it belong in the plan that
implements this.

### 6. #104 — YouTube stalls without feedback or recovery

The feedback half is partly discharged by `PLAY-13`. The recovery half is not:
there is no visible re-acquisition of a fresh media URL when the temporary one
expires.

**Task 11.** Split the issue: the presentation half against `PLAY-13`/`NET-3`,
and the transport half — detect a stalled pipeline, re-resolve the URL, resume.
The transport half needs a deterministic reproduction before any fix; the
issue's own filing already says none exists.

### 7. #444 / #597 — the third §4C mutation

Two of the three mutations are red; the rows-only `scroll_target` mutation is
still not run, and §4C forbids any `Fixes #444` claim until all three are.

**Task 12.** Run the remaining mutation and record the result on #597, then
decide whether #444 closes or needs a fix. `docs/plans/fixes-444-mutations.HANDOFF.md`
is untracked and must be committed alongside — same reason as task 1.

### 8. #475 — `ScrollAdoptionGeometry` still projects parallel fields

`ui/track_list/reload_anchor_scroll.rs:64-71` keeps `section_count`,
`preceding_sections` and `row_height` side by side, on the fallback path taken
when `applied_layout` is `None`.

**Task 13.** Replace them with `ListLayout`. Behaviour-carrying, since the state
is consumed inside `connect_value_changed` — verify against the scroll-adoption
display tests, not by inspection. **Sequence this after task 4**: both touch the
same write path, and doing them in parallel would make either one's measurement
unreadable.

### 9. #250 — `REPRISE_SMOKE_REPEAT=all` is overwritten

`arm_smoke_repeat` runs in the `PlayerController` constructor
(`ui/playback/player_controller.rs:500`); the session restore lands afterwards.

**Task 14.** Move the arming after the restore, or make it a restore-level
override. Whichever it is, a headless E2E must be able to *observe* that repeat
survived — an assertion on the state, not a log line.

### 10. #122 and #254 — two deterministic-looking flakes

- #122 `fil_9_filter_changes_center_the_visible_playing_track` fails off by
  exactly one row under load, with identical numbers on every failing run.
- #254 `stats_19_period_switch_tweens_bars_without_restarting_static_content`
  fails when the tween finishes before the first sample.

**Task 15.** #254 is a test-oracle defect: sample before arming, or assert on
the tween's declared duration rather than on an observed intermediate frame.
**Task 16.** #122's stable off-by-one is a finding, not noise — one row is
exactly the header band the centring gained in #460. Re-measure it against the
current `ListLayout` path before treating it as flakiness.

### 11. Re-runs that decide three closures

**Task 9.** Re-run the CUA sweeps that produced #108, #406 and #407:

```
CUA_E2E_PROFILE=release CUA_E2E_ONLY=responsive-window scripts/cua-e2e/run.sh
```

plus `hover-affordance-sweep` and `pointer-layout-reachability`, both seeds.
#406 in particular looks discharged — `ui/style/interactions.rs` now ships a
shared `.reprise-hover` tint with a transition — but "looks discharged" is not
evidence for a finding that was measured. Each of the three closes on its own
re-run or stays open with fresh numbers.

**Task 10.** Put the #79 ordering question to the repository owner and act on
the answer.

## Sequencing

Part A's closures and task 2 are done (2026-08-22). Task 1 lands with this plan.
Task 9's re-runs gate three closures and need nothing else. Task 3 gates task 4,
and task 4 gates task 13. Tasks 5, 6, 7, 8, 14, 15 and 16 are independent of
each other and of the centring strand.

## Out of scope

Nothing here changes `main`, the promotion gate, or the release channel. The
`Fixes #444` claim stays forbidden until §4C is satisfied in full — this plan
does not relax that gate, it schedules the last measurement it needs.
