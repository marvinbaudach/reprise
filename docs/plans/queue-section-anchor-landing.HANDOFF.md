# Handover — queue-section-anchor landing (#444), 14.08.2026 09:50

> **Updated 09:50.** The full display gate has since finished: **671 of 672
> green, one red — and that one is this branch's own regression.** See
> "The blocker" below. The branch is NOT landable as it stands. Everything else
> in this document still holds.

**Worktree:** `/home/marvin/Projects/reprise-queue-section-anchor`
**Branch:** `feature/queue-section-anchor` — 11 commits ahead of `origin/dev`, 1 behind
**Plan:** `docs/plans/queue-section-anchor-landing.md` (`phase: planned`, committed into the branch)
**Previous handover:** `docs/plans/queue-section-anchor-handover.md` — **four of its conclusions are wrong**, see below

---

## What is running right now

A full local display gate, started 08:33:

```
heavy-run medium -- bash /tmp/claude-1000/-home-marvin-Projects-reprise/\
9b27f4ba-f36d-4e4a-857a-03f4edfaa941/scratchpad/verify-final.sh
```

- Log: `<scratchpad>/verify-final.log`, per-stage logs in `<scratchpad>/verify/`
- Finish marker: the line `=== VERIFY DONE ===`
- **Do not judge progress by grepping `display-gate.log` for `== display test:`.**
  `scripts/check-display-tests.sh` collects each test into a temporary
  `results_dir` and only prints the per-test lines at the very end. A zero count
  mid-run is normal, not a stall. Watch `ps` for `check-display-tests` and for
  fresh `Xvfb` servers instead.
- It runs with `DISPLAY_TEST_JOBS=1` on purpose — the suite is herd-flaky when
  run in parallel. That is why it takes far longer than CI's ~45 minutes.

Everything before that stage already passed on the final code (see below), so
this run is the last outstanding piece of evidence.

---

## The pipeline ran plan → code → check → refactor. State: refactor done, verification in flight

### Verified on the final commit `ab02785783`

| Stage | Result |
|---|---|
| `navback_anchor_display_tests` ×4 (unsectioned controls) | **PASS** |
| `queue_anchor_names_the_row_at_the_viewport_top` (the #444 assertion) | **PASS** |
| `nav_back_to_a_large_sectioned_queue_never_visits_the_top` (q-journey) | **PASS** |
| `que_1_queue_section_headers_share_one_height` | **PASS** |
| `browse_4_the_title_link_leaves_the_viewport_at_the_revealed_track` | **PASS** |
| band probe | `row_samples=[45.0,45.0,45.0,45.0] header_samples=[36.0, 36.0]` |
| `cargo fmt --check` / `clippy --all-targets --workspace -- -D warnings` | green |
| `cargo test --workspace` | green, 0 failed groups |
| `scripts/check-architecture.sh` | green |
| full `scripts/check-display-tests.sh` | **671 of 672 passed, 1 failed** |

Each display test ran in its own process, own XDG roots, own `dbus-run-session`
and own `xvfb-run -a`, judged on `^test result: ok. 1 passed` — a name filter
that matches nothing prints `ok. 0 passed`, which the script does not count as a
pass.

---

## The blocker — a real regression, attributed by measurement

`ui::track_list::track_list_reload::search_viewport_display_tests::typed_search_reads_from_the_top_and_clearing_comes_back`
(`crates/reprise-gnome/src/ui/track_list/search_viewport_display_tests.rs:101`)

```
clearing returns within a row of where the search began: expected about 1200, got 1428
```

| | result |
|---|---|
| this branch, 3 runs | **FAILED, FAILED, FAILED** — byte-identical message every time |
| `origin/dev`, 3 runs in a fresh detached worktree | **ok, ok, ok** |

Deterministic on both sides, so it is neither herd flakiness nor a host
artefact: **this branch introduced it.** The comparison worktree is
`/home/marvin/Projects/reprise-devcheck` (detached at `origin/dev`, throwaway —
remove it with `git worktree remove --force` when done).

Note this test exercises the same capture/restore machinery the branch
re-signatured, but on the *search* path rather than the Queue path. The Queue
path is proven correct; the search path is not.

**Why this needs a decision rather than another Codex round:** grill decision 6
was "the production code stays untouched", and it rested on the then-measured
fact that the production arithmetic was right. That premise now fails for a
different code path. Whether the fix may reach into production code — or whether
the anchor work and the search path should be split — is a scoping call, not a
mechanical repair.

### Root cause — measured, not inferred

The test scrolls to **1200**, filters, clears the filter, and requires the view
back within 40 px. It gets 1428.

Both sides compute the anchor **identically and correctly**. With the branch's
own `REPRISE_SCROLL_PROBE=1` instrumentation (no source edits needed) the two
runs are line-for-line the same until one line:

```
SCROLLMODEL path=anchor.initial.apply anchor=Some((15, 10.0)) position=Some(35) row_height=34.0 sections=[] target=1200.0   # both
branch:  SCROLLTO writer=anchor.initial.scroll_to position=42  from=1200.0 upper=6800.0 page=239.0
dev:     SCROLLTO writer=anchor.initial.scroll_to position=35  from=1200.0 upper=6800.0 page=239.0
```

The **guard position handed to `scroll_to` is 42 on the branch and 35 on dev.**
GTK then scrolls to row 42, and 42 × 34 = **1428** — exactly the observed value.
42 − 35 = 7 rows = 238 px ≈ the page size of 239, so the guard names the row at
the *bottom* of the viewport instead of the anchor row.

An independent probe confirmed the layout here is plain `rows_only`
(`section_header_height: None, section_starts: []`, row height 34.0), and that
`reload_restore::scroll_target` returns exactly 1200.0. **The section logic is
not involved; the guard position is.**

Second difference in the same trace: dev runs a further corrective pass —
`anchor.idle.apply` → write 1200 → `scroll_to(35)` — which the branch never
reaches, so its wrong first shot stands.

That second point vindicates the *mechanism* of the review's HIGH finding, which
was refuted only for the Queue: because `NoOpinion` now keeps the layout, `apply`
returns `Some` and the refinement pass no longer runs. On the Queue that was
harmless (the assumed header height is the enforced CSS floor, measured exact);
here it is what leaves the bad guard position uncorrected.

**So it is a product bug, not a test expectation.** The fix is in production code
— `prepaint_guard_position` in `reload_anchor_scroll.rs` and/or restoring the
corrective pass — which is why it needs the user's scoping call against grill
decision 6.

Evidence: `<scratchpad>/scroll-cmp/{branch,dev}.log` (probe traces),
`<scratchpad>/search-probe/probe.log` (capture/restore arithmetic),
`<scratchpad>/attribute/` (3× branch red, 3× dev green).

---

## The four corrections that drove this session

The previous handover's diagnosis was built on a fixture that measured an
application that does not exist. All four are reproduced measurements, not
readings of the code.

**C1 — the red q-journey was never inherited from `dev`.** `RenderedBandSamples`,
`rendered_band_samples` and `uniform_heights` have **zero** occurrences on
`origin/dev`. They arrived with this branch's own commit `settle section band
measurement`. On `dev` the test only asserts that both header titles render, and
CI ran it green (671 display tests, 0 failures, in the dev run for #465).

**C2 — the 20 px header was not a settling race.** Strengthening the settle
predicate to `has_both_bands() && uniform_heights().is_some()` makes the test run
the full timeout (5.51 s instead of 0.49 s) without ever converging.

**C3 — the fixture never installed the application stylesheet.**

| | rows | section headers | q-journey |
|---|---|---|---|
| without the stylesheet | 34 px | 20 px and 34 px | red |
| with the stylesheet | 45 px | **36 px and 36 px** | green |

36 px is exactly the `section_header_height` the model assumes, and
`style/tokens.rs:66` (`SECTION_HEADER_MIN_HEIGHT: i32 = 36`) is compiled into
`.queue-section-header-row { min-height: 36px; }` in `queue_sections.rs:82-85`.
The assumed value *is* the enforced CSS floor. **The handover's conclusion
"reality has two header heights, so the model is wrong" is false — the model is
right and the fixture was wrong.** `upper = 77438 = 2276×34 + 54` was arithmetic
about an unstyled list.

**C4 — the reference frame, not the anchor, was off.** With the real stylesheet
the #444 assertion first failed with `left: "Track 1100"` (topmost entry of
`rendered_rows()`) against `right: "Track 1101"` (the captured anchor). Probed in
one frame:

```
value=49572  layout=ListLayout{ row_height:45.0, section_header_height:36.0, section_starts:[0,1] }
row_at(49572) -> position=1100  offset=0
row_top(1100) = 1100*45 + 2*36 = 49572          exact, zero remainder
cv_height=274  page_size=248  title_bottom=25
scrollables=["GtkColumnListView@26.0..274.0"]
rows_before=[("Track 1100", -19.0), ("Track 1101", 26.0), ...]
```

The scroll viewport starts at y = 26. `Track 1101` sits exactly there;
`Track 1100` spans −19..26 and has **zero visible pixels** — a realized
virtualization slack row that `.first()` picked because it sorted by raw y
including negatives. Ruled out by the same probe: stale row height, `headers_above`
off by one, and any capture/render frame skew.

---

## What the code phase changed (production code untouched throughout)

Four new commits on top of the pre-existing seven:

- `4f70f97a59 test(queue): measure only visible anchor rows` — installs the app
  stylesheet in both tests; first attempt at a viewport filter.
- `bba66ff899 test(queue): derive viewport from scroll adjustment` — repairs the
  CRITICAL below; got q-journey green.
- `ab02785783 test(queue): use the list viewport for anchor rows` — repairs the
  remaining one-pixel error; got the #444 assertion green.
- plus two `docs:` commits carrying the handover and the landing plan into the
  branch.

`list_geometry_layout.rs`, `reload_anchor_scroll.rs`, `reload_restore.rs`,
`track_list_reload.rs`, `track_list_geometry.rs` and `list_geometry.rs` are
byte-identical to what the previous session's reviewers already passed —
verified with `git diff --numstat`, not by reading a summary.

---

## Review findings and what happened to them

**CRITICAL (survived, fixed).** Codex's first repair looked for a descendant
whose type name equalled `"GtkListView"` and `expect()`ed it. That widget is
called **`GtkColumnListView`** — the exact-equality comparison never matched, so
both queue tests panicked in fixture setup before any assertion ran. Reproduced
twice in this session's own Xvfb runs. Fixed by matching
`type_name.contains("ListView")`, with `column_view.height() - page_size` as a
fallback. Both derivations were measured to agree exactly: 26.0 and 274.0.

Second round of the same finding: deriving the viewport top from the column-title
bar's bottom gives **25**, but the list starts at **26** — a one-pixel separator
sits between them, and the slack row survived `row_bottom > viewport_top` by
exactly that pixel.

**HIGH (refuted, dropped).** "The settle-gate removal leaves a first-commit
scroll value uncorrected." The refutation is C3's token evidence: the assumed
header height is the enforced CSS floor and reality measures exactly 36.0/36.0,
so the height error the whole chain depends on is zero. The mechanical trace
(NoOpinion → `apply` returns `Some` → the adoption branch gated on
`applied_layout.is_none()` does not fire) is accurate but has nothing to correct.

**LOW, deferred by decision** — do not re-raise these: duplicate entries in
`section_starts` double-counted by `headers_above`; the unreachable `Option` on
`content_height`/`max_scroll`; `rendered_queue_headers` not filtering zero-height
widgets. `uniform()`'s 0.5 px tolerance dissolved with C3.

---

## What is left to do

0. **Resolve the blocker above** — this comes first and needs the user's scoping
   call. The display gate has already run; it does not need repeating until the
   blocker is fixed, and then only the focused eight plus this one test, unless
   production code is touched (in which case run the full gate again).
1. ~~Wait for `=== VERIFY DONE ===`.~~ Done: 672 executed, 671 passed. The count
   is one higher than the 671 the dev run for #465 executed, so coverage did not
   shrink. `xvfb-orphan-gc --apply` has been run.
2. **Rebase onto `origin/dev`** — it moved again (`051fb088df docs: mark the
   dev-gate plan shipped (#472)`, docs only, no conflict expected). Rebase
   *before* the final evidence is claimed, or re-run the focused eight after it.
3. **Open the PR** against `dev` referencing #444, with C1–C4 in the body — they
   are the reason the diff touches tests at all.
4. **Land** via `~/.claude/skills/pipeline/scripts/land.sh <pr>` — rebase, push,
   merge in one go, without waiting for CI. Then watch the next dev run that
   *completes* and still contains the commit.
5. **File the two follow-up issues** (agreed in the grill, neither duplicating
   #460):
   - **A** — display fixtures that measure without the app stylesheet, plus the
     reference-frame trap (`compute_bounds(&column_view)` includes the
     non-scrolling title bar, so y = 0 is not the viewport top). Ask for a sweep.
   - **B** — `validate` treats `upper` below the prediction as "still growing"
     and returns `NoOpinion`, so it can only ever reject a guess that is too
     short, never one that is too long.
6. **Clean up:** remove worktree and branch after the squash merge, set the plan
   to `phase: shipped`, release the wake lock `ship-queue-section-anchor`.

---

## Decisions taken in the grill — do not re-open without the user

1. Scope stays #444; the header-height model is not rebuilt.
2. Production geometry is not touched — `row_at` was proven correct.
3. The oracle's reference frame gets repaired, not the production code.
4. Both queue-geometry tests install the application stylesheet.
5. Evidence before landing = the focused eight **plus** a full local display gate.
6. Two follow-up issues, as above.
7. The three remaining low findings stay deferred.
8. The handover and the landing plan are committed into the branch.

---

## Operational notes

- Wake lock `ship-queue-section-anchor` is held; release it when the strand ends.
- The scratchpad `<...>/9b27f4ba-.../scratchpad` was cleared once mid-session by
  the tmpfs GC, which silently swallowed a Codex launch (the redirect target
  vanished, so the command never ran). Re-create it before launching anything.
- Codex runs go through `heavy-run medium`; `heavy` starves behind other
  sessions. `heavy-run` swallows `codex-run.sh`'s stderr, so a 0-byte launcher
  log is not a hung run.
- The load-governor hook matches on **command text**: a plain `ps | grep codex`
  gets blocked as a "heavy entry point". Prefix such probes with
  `HEAVY_RUN_DISABLE=1`.
- `.pipeline-codex.md` is tracked and is rewritten by every Codex run; restore it
  before starting one so the diff stays clean.
- All throwaway experiments in this session patched the worktree and reverted via
  a `trap`; the scripts are in the scratchpad (`band-race-experiment.sh`,
  `css-experiment.sh`, `css-both-experiment.sh`, `restore-half-experiment.sh`,
  `viewport-probe.sh`) if any measurement needs repeating.
