---
slug: queue-section-anchor-landing
worktree: /home/marvin/Projects/reprise-queue-section-anchor
branch: feature/queue-section-anchor
phase: planned
codex_session:
created: 2026-08-14
---
# Landing plan — queue-section-anchor (#444)

Finish and land `feature/queue-section-anchor`, which fixes issue #444
("Sectioned Queue visits the top before restoring its anchor").

The anchor implementation is written and reviewed. What remained was
re-verification after a rebase onto `dev`. That re-verification turned up a
measurement defect that invalidates part of the branch's own evidence, so this
plan repairs the evidence before landing.

## State at the time of writing (2026-08-14 03:45, all measured)

- The rebase onto `origin/dev` completed. The branch is 6 commits ahead of
  `origin/dev`, 0 behind, merge-base `b99b71a932`. Codex resolved the 9-hunk
  conflict in `reload_anchor_scroll.rs` (dev's #463 probes vs. this branch's
  `&ListLayout` signatures) keeping both sides, and re-ran `cargo fmt --check`,
  `cargo clippy --all-targets --workspace -- -D warnings`,
  `cargo test --workspace` (5196 passed, 0 failed) and
  `scripts/check-architecture.sh` — all green. It ran no display tests.
- Post-rebase display pass, each test in its own process: 7 of 8 green,
  including the four unsectioned `navback_anchor_display_tests` controls, the
  #444 assertion, `que_1_queue_section_headers_share_one_height` and
  `browse_4_the_title_link_leaves_the_viewport_at_the_revealed_track`. Only
  `nav_back_to_a_large_sectioned_queue_never_visits_the_top` ("q-journey") was
  red.
- CI runs the display gate: `.github/workflows/ci.yml` → `scripts/ci-quality.sh`
  → `check-merge-readiness.sh` → `check-display-tests.sh`. Since #463 that
  covers the whole ignored suite, unfiltered. The dev run for #465 executed 671
  display tests with 0 failures.

## Four corrections to the handover

The handover `docs/plans/queue-section-anchor-handover.md` reached conclusions
that the measurements below contradict. They are corrected here rather than in
that document, which stays as the record of what was believed at the time.

**C1 — the red q-journey is not inherited from `dev`.** The uniformity
precondition does not exist on `origin/dev`: `RenderedBandSamples`,
`rendered_band_samples` and `uniform_heights` have zero occurrences there. They
arrived with this branch's own last commit, `8c57332184 test(queue): settle
section band measurement`. On `dev` the test asserts only that both section
header titles render, and it is green in CI.

**C2 — the 20 px header is not a settling race.** Strengthening the settle
predicate to `has_both_bands() && uniform_heights().is_some()` makes the test
run the full `DISPLAY_TEST_TIMEOUT` (5.51 s instead of 0.49 s) and never
converge. The bands stay at `[20.0, 34.0]`.

**C3 — the fixture measures an app that does not exist.** Neither test in
`queue_section_geometry_display_tests.rs` installs the application stylesheet,
unlike `que_1_queue_section_headers_share_one_height`, which calls
`install_css_string_for_test(&app_css_for_test())`. Installing it changes the
geometry completely:

| | rows | section headers | q-journey |
|---|---|---|---|
| without the stylesheet | 34 px | 20 px and 34 px | red |
| with the stylesheet | 45 px | **36 px and 36 px** | green |

36 px is exactly the `section_header_height` the model assumes. The handover's
conclusion — "reality has two header heights, so the model is wrong" — does not
hold; the model is right and the fixture was wrong. The arithmetic
`upper = 77438 = 2276×34 + 54` was arithmetic about an unstyled list.

**C4 — with the real stylesheet the #444 assertion fails, and its oracle is at
fault.** Under real geometry the assertion `assert_eq!(topmost_title,
&anchor_title)` reports `left: "Track 1100"` (the topmost entry of
`rendered_rows()`) against `right: "Track 1101"` (the captured anchor). A probe
run in the same frame settles it:

```
value=49572  layout=ListLayout{ row_height:45.0, section_header_height:36.0, section_starts:[0,1] }
row_at(49572) -> position=1100  offset=0
row_top(1100) = 1100*45 + 2*36 = 49500 + 72 = 49572      exact, no remainder
column_view_title_bounds = [(0.0, 25.0), ...]            the non-scrolling title bar
rows_before = [("Track 1100", -19.0), ("Track 1101", 26.0), ...]
```

`rendered_rows()` measures with `compute_bounds(&column_view)`, a frame that
includes the fixed `GtkColumnViewTitle` bar, so the scroll viewport starts at
y ≈ 26. `Track 1101` renders at exactly 26 and is therefore flush with the
viewport top. `Track 1100` spans −19..26 and has **zero visible pixels** — a
realized virtualization slack row that `.first()` picks anyway because it sorts
by raw y including negatives.

Ruled out by the same probe: a stale row height (45.0 is right, remainder 0),
`headers_above` off by one (2 is correct for `section_starts: [0, 1]`), and a
capture/render frame skew (`adjustment.value()` is 49572 both before and inside
`capture()`). The position-to-id shift (position 1100 → track id 1101) is real
but intended: "Now Playing" occupies position 0.

With the naming convention neutralised and the stylesheet installed, the restore
half passes: the anchor row returns to within 1 px of its captured y
(`ANCHORPROBE anchor="Track 1101" anchor_y_before=26`, `test result: ok`).

## Decisions

1. **Scope stays #444.** The anchor becomes header-aware; the header-height
   model is not rebuilt. The uniformity precondition this branch introduced is
   its own debt and must go green.
2. **The production geometry is not touched.** `row_at` is provably correct
   here (`row_top(1100) = 49572 = scroll_value`, offset 0).
3. **The oracle's reference frame is repaired.** `rendered_rows()` must count
   only rows with a real visible share of the scroll region.
4. **Both queue-geometry tests install the application stylesheet.**
5. **Evidence before landing:** the focused eight plus a full local
   `scripts/check-display-tests.sh`, on top of fmt, clippy `-D warnings`, the
   workspace suite and the architecture gate.
6. **Two follow-up issues**, neither duplicating #460: display fixtures that
   measure without the stylesheet (and the reference-frame trap alongside it),
   and `validate`'s one-directional blindness.
7. **The three deferred review findings stay deferred**: duplicate entries in
   `section_starts` double-counted by `headers_above`, the unreachable `Option`
   on `content_height`/`max_scroll`, and `rendered_queue_headers` not filtering
   zero-height widgets. None of them bears on what was found here. The fourth,
   `uniform()`'s 0.5 px tolerance, dissolves with C3 — it was only ever suspect
   for rejecting an artefact.
8. **The handover and this plan are committed into the branch.** Both are
   untracked in the shared main checkout and vanish when another session cleans
   it.

## Work

### 1. Repair the measurement (production code untouched)

In `crates/reprise-gnome/src/ui/track_list/queue_section_geometry_display_tests.rs`:

- Install the application stylesheet in both `#[test]` functions, exactly as
  `queue_section_header_display_tests.rs:64` does:
  `crate::ui::style::install_css_string_for_test(&crate::ui::style::app_css_for_test());`
  immediately after `gtk4::init().unwrap()`.
- Repair `rendered_rows()` so it returns only rows with a visible share of the
  scroll region: subtract the `GtkColumnViewTitle` height from the reference
  frame, or require a strictly positive overlap with the viewport. A row whose
  bottom edge lands exactly at the viewport top has no visible pixels and must
  not be returned. Document in a comment why the raw `compute_bounds` frame is
  not the viewport frame, so the trap is not re-set later.
- Keep the band-uniformity precondition. With the stylesheet it measures
  `[36.0, 36.0]` and passes.
- Do not change `list_geometry_layout.rs`, `reload_anchor_scroll.rs`,
  `reload_restore.rs` or `track_list_reload.rs`.

Pass criterion: both tests in that file green, each in its own process, and the
`BANDPROBE` line reports `header_samples=[36.0, 36.0]`.

### 2. Re-verify

Every display test in its own process, own XDG roots, `dbus-run-session`,
`xvfb-run -a`, with `GDK_BACKEND=x11 WAYLAND_DISPLAY= GSK_RENDERER=cairo
REPRISE_AUDIO_SINK=fakesink`. Judge on `^test result:` **and its count** — a
name filter that matches nothing prints `ok. 0 passed`, which is not a pass.

1. The focused eight: `navback_anchor_display_tests` ×4,
   `queue_anchor_names_the_row_at_the_viewport_top`,
   `nav_back_to_a_large_sectioned_queue_never_visits_the_top`,
   `que_1_queue_section_headers_share_one_height`,
   `browse_4_the_title_link_leaves_the_viewport_at_the_revealed_track`.
2. The full gate: `scripts/check-display-tests.sh`, unfiltered.
3. `cargo fmt --check`, `cargo clippy --all-targets --workspace -- -D warnings`,
   `cargo test --workspace`, `scripts/check-architecture.sh`.

Afterwards `xvfb-orphan-gc --apply`.

Pass criterion: no `^test result: FAILED` anywhere, and the display gate's test
count is not lower than the 671 the dev run for #465 executed.

### 3. Land

1. Commit `docs/plans/queue-section-anchor-handover.md` and this plan into the
   branch.
2. Open the PR against `dev`, referencing #444, with the C1–C4 corrections in
   the body — they are the reason the diff touches the tests at all.
3. Rebase, push, merge in one go via `~/.claude/skills/pipeline/scripts/land.sh`.
   Do not wait for CI first: the run takes ~45 minutes, `dev` moves faster than
   that, and GitHub then refuses the merge out of a stale mergeability cache.
   The evidence that carries the risk is the local gate above, which ran before
   landing.
4. Read what `dev` picked up since (`git log --oneline HEAD..origin/dev`) and
   judge whether it can touch `reload_anchor_scroll.rs` at all.
5. Watch the next dev run that *completes* and still contains the commit — the
   immediate one will most likely be cancelled by the next merge in the
   concurrency group. Fix forward if it goes red.
6. Remove the worktree and the branch (a squash merge leaves both), set
   `phase: shipped`, release the wake lock.

### 4. File the follow-ups

**Issue A — display fixtures measure an application that does not exist.**
A GTK display fixture that never installs the app stylesheet measures default
GTK geometry: 34 px rows instead of 45, section headers of 20 px and 34 px
instead of 36 px and 36 px. `queue_section_geometry_display_tests.rs` did this
and produced a conclusion about the product ("header heights are not uniform")
that was an artefact of the fixture. Second part of the same family: measuring
row positions with `compute_bounds(&column_view)` includes the non-scrolling
`GtkColumnViewTitle` bar, so "y = 0" is not the viewport top and realized slack
rows above the viewport look like visible rows. Ask for a sweep over the display
fixtures for both traps.

**Issue B — `validate` can only reject a guess that is too short.**
It treats `upper` below the prediction as "the range is still growing" and
returns `NoOpinion`, so a header-height guess that is too *long* is never
rejected. Note the asymmetry and what it would take to make the check
two-sided. Production code, therefore separate from Issue A.

Neither duplicates #460 (`scroll_center::centered_scroll_value_with_height` and
`track_list_reload::pending_reveal_anchor` keeping the rows-only model for
centring, plan Decision 10).

## Invariants the diff must not lose

Each was proven on a real display run before the rebase; losing one silently
reintroduces a bug.

1. `apply` builds the `ListLayout` **once** and passes that same layout to
   `scroll_to_anchor` and `prepaint_guard_position`. Two layouts mean the guard
   row and the written scroll value describe different rows.
2. A failed geometry validation must never drop the anchor. Unsectioned lists
   skip validation, "no opinion" keeps the section geometry, and only a proven
   rejection falls back to rows-only.
3. `shared.queue_sections` is a `RefCell`; section data is copied out in its own
   statement before any GTK call.
4. #463's probe instrumentation (`apply_probe`, `hold_probe`, `scroll_probe`,
   `set_hold_target`, `matches` and its `mod tests`) stays intact — it is the
   gate's own instrumentation.

## Operational notes

- Codex runs go through `heavy-run medium`, not `heavy`, which needs 4 of 6
  slots and starves behind other sessions.
- `heavy-run` swallows `codex-run.sh`'s stderr, so its launcher log stays 0
  bytes. That is not a hung run; compare worktree file mtimes instead.
- `.pipeline-codex.md` is tracked and carries a stale copy between runs — its
  existence is not a finish signal.
- `find` on this host is `bfs` and fails silently on relative `-newermt`
  (exit 0, no output). Compare `%T@` epochs instead.
