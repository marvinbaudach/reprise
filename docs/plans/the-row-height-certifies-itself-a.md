---
slug: the-row-height-certifies-itself-a
worktree: /home/marvin/Projects/reprise-the-row-height-certifies-itself-a
branch: feature/the-row-height-certifies-itself-a
phase: coded
codex_session:
created: 2026-08-26
---
# Strand A — the loop, and everything that reads it

Mother plan: [`the-row-height-certifies-itself.md`](the-row-height-certifies-itself.md).
Read its sections 2–7 before starting; the diagnosis is not repeated here.

**File ownership.** This strand writes only to:

```
crates/reprise-gnome/src/ui/list_geometry*.rs
crates/reprise-gnome/src/ui/track_list/**
```

Nothing outside `crates/reprise-gnome`. The schema migration and the predecessor
plan's record belong to strand B and must not be touched here.

---

## Task 1 — the predicate: has GTK authored this `upper`?

`crates/reprise-gnome/src/ui/list_geometry.rs`

This is the foundation for tasks 2, 3 and 6. Build it first and alone.

`ListGeometryCache` gains a record of **its own last write**: the `(n_rows,
upper)` pair passed to `adjustment.configure` in `ListGeometry::configure`. Two
`Cell`s, or one `Cell<Option<(usize, f64)>>` — the shape is yours; the initial
state must mean "we have written nothing".

Add the predicate beside it:

```
gtk_authored(cache, adjustment.upper(), n_rows) -> bool
```

true when the recorded pair is absent, when its `n_rows` differs from the current
one, or when `adjustment.upper()` differs from the recorded `upper` by more than
`ROW_HEIGHT_AGREEMENT_EPSILON`. In every one of those cases the live `upper` is
not something we put there for this row count.

**Why bound to `n_rows` and not just the value:** right after a model swap GTK's
`upper` still describes the *old* row count. Seeding there is the legitimate case
the preseeding was built for, and a value-only comparison would wrongly call that
"GTK authored".

Keep it a free function over the cache, not a method on `ListGeometry`, so it
stays GTK-free and unit testable like the rest of the acceptance arithmetic in
this module.

**Test 1a** (unit): a cache that has written nothing reports GTK-authored for any
`upper`.
**Test 1b** (unit): after recording `(2006, 60180)`, an `upper` of `60180.0` with
`n_rows = 2006` is **not** GTK-authored; `90270.0` at the same count is.
**Test 1c** (unit): after recording `(2006, 60180)`, the same `60180.0` at
`n_rows = 500` **is** GTK-authored — a different row count means the record does
not describe the current model.

## Task 2 — `configure` seeds, it never overwrites

`crates/reprise-gnome/src/ui/list_geometry.rs`

`ListGeometry::configure` currently calls `preseed_upper` and writes whenever the
current range does not describe the wanted content. Add the predicate as a hard
precondition: **when `gtk_authored(...)` is true, `configure` does not write
`upper` at all.** GTK has spoken for this row count; its number is the truth and
we do not fight it.

`configure` still writes the *value* (the scroll position) as it does today —
this task restricts only the range.

Record the `(n_rows, upper)` pair on every write we do make (task 1's cell).

Expected effect on the recorded trail: the six
`SCROLLUPPER writer=anchor.configure want=60180.0 from=90270.0` lines disappear
entirely. That is this task's acceptance signal in the measurement arm below.

**Test 2a** (unit, over `preseed_upper` and the new precondition): with the
predicate false — cold start, nothing authored — a `Measured` height still seeds
its range exactly as today.
**Test 2b** (unit): with the predicate true, no range is written, whatever
`preseed_upper` would have wanted.
**Test 2c** (display): a cold start with an empty settings table still restores
its scroll anchor without a visible jump. This is the behaviour preseeding exists
for; it must survive the restriction.

## Task 3 — the layout takes GTK's quotient

`crates/reprise-gnome/src/ui/list_geometry.rs`,
`crates/reprise-gnome/src/ui/track_list/track_list_geometry.rs`

The row height handed to `ListLayout` — and therefore to
`ListLayout::centered_value`, the reveal's arithmetic — becomes:

- `upper / n_rows` when `gtk_authored(...)` is true;
- the remembered height (`ListGeometry::row_height`) only when it is not.

For sectioned lists the existing header subtraction stays: the row band is
`upper − n_sections × header_height`, divided by `n_rows`, as
`settled_content_row_height` already computes it.

This is what makes the reveal correct *by construction*: it computes its
destination from the same range it is about to write into. The discrepancy in
mother-plan section 3.5 becomes unreachable rather than guarded against.

**Test 3a** (unit): given a GTK-authored `upper` that disagrees with the
remembered height, the layout uses the quotient, not the remembered value.
**Test 3b** (unit): given a non-authored `upper`, the layout uses the remembered
height — the pre-allocation path is unchanged.
**Test 3c** (display): on a large flat list, reveal a row at position ≥ 1000 and
assert the adjustment's resting value equals
`ListLayout::rows_only(h).centered_value(...)` computed with `h = upper / n_rows`
read from the adjustment **after** it settles. *This is the test that would have
caught the reported bug outright.*

## Task 4 — delete `contradicting_row_height`

`crates/reprise-gnome/src/ui/list_geometry.rs`

Remove the function and its call in `remember_if_settled`. A disagreement between
the widget measurement and the adjustment quotient goes back to meaning *no
information*: nothing is persisted, and the remembered value stays as it was.

This is the function that installed the 30 (mother plan, section 3, "which branch
installed it"). It was added by the predecessor's strand A task 2 to solve a real
problem — a stale remembered height that could never be corrected, recorded in
`memory/reprise-row-height-cache-cannot-heal.md`. **Task 2 of this strand is what
makes its deletion safe:** once we stop overwriting a GTK-authored `upper`, GTK's
own number stands, the widgets settle against it, and `settled_row_height` agrees
and persists the truth. Healing now comes from not fighting GTK, not from a rule
that overrules it.

Drop `ROW_HEIGHT_AGREEMENT_EPSILON`'s use as a *disagreement* threshold along with
it; it stays as the agreement tolerance.

**Test 4a** (unit): a uniform widget measurement that disagrees with a
GTK-authored quotient persists nothing and leaves the cache untouched.
**Test 4b** (display, the healing arm): seed `ui.row_height` with a wrong value,
reload a large flat list, and assert that after settling the persisted value
equals `upper / n_rows` from the settled adjustment. Deleting the contradiction
rule must not cost the ability to recover from a wrong remembered height — this
test is the proof, and it is the one that guards against reintroducing
`reprise-row-height-cache-cannot-heal`.

## Task 5 — a row smaller than it wants is not evidence

`crates/reprise-gnome/src/ui/list_geometry.rs`

Two guards on the witness side, so that a mid-flight allocation can never certify
a height for the database.

**5.1 — natural height.** `ListGeometry::widget_heights` returns `Vec<i32>` of
allocated heights. Change it to carry both numbers per widget — allocated and
natural, the latter from `widget.measure(gtk4::Orientation::Vertical, -1)`,
exactly as `scroll_probe::probe_rows` already does (`scroll_probe.rs:153`: take
`nat`, fall back to `min` when `nat` is 0).

`RowMeasurement` gains one rule before any counting: **a row whose allocated
height is below its own natural height contributes nothing.** It is mid-flight,
whatever its absolute value. Zero-height (unrealized) rows keep being dropped as
today.

Effect on the recorded trail: all sixteen samples are discarded, because all are
`(30, 31)`. Note the counterexample in the same trail — `distinct_heights=[0, 30,
45]` — which confirms 45 is a real allocated height and passes the rule, since
natural 31 is a floor and not the pitch.

Keep the existing allocated-only `from_widget_heights` beside the new
constructor rather than breaking every call site;
`queue_section_header_display_tests.rs:79` uses it legitimately.

**5.2 — minimum sample.** `RowMeasurement::is_uniform` is `counts.len() == 1`
over the surviving set, so a handful of rows out of 206 realized widgets can
certify the whole list. Add `MIN_SETTLED_ROW_SAMPLE`, starting at 3: a
measurement built from fewer surviving rows is not uniform and has no modal,
however consistent it looks. Comment the reasoning — after 5.1 a genuinely
settled list offers dozens.

**Test 5a** (unit): a set where every allocated is below natural yields an empty
measurement — no modal, not uniform.
**Test 5b** (unit): the same set with allocated ≥ natural yields the modal.
**Test 5c** (unit): a mixed set keeps only the rows that reached their natural
height, and reports non-uniform when those disagree.
**Test 5d** (unit): two identical settled rows do not make a uniform measurement;
three do.
**Test 5e** (unit): the threshold counts *surviving* rows — 200 mid-flight rows
plus two settled ones is still not uniform.

## Task 6 — `capture_row_height` gets the same gate

`crates/reprise-gnome/src/ui/track_list/track_list_reload.rs`

Line 186 computes `adjustment.upper() / f64::from(old_total)` for flat lists.
**Keep the division** — it was never the wrong formula. Gate it on the predicate
from task 1, evaluated against `old_total`: when the current `upper` is not
GTK-authored for that count, return `None` and let the existing fallback in
`track_list_geometry::layout` (lines 30–31) do its job, exactly as the sectioned
branch already does.

This is the reader that produced the 30/45 alternation in the trail. Under the
gate it reads a moving value no longer.

**Test 6a** (display): with a poisoned `ui.row_height` seeded into the database,
a reload of a large flat list does not produce a `ListLayout` whose row height
equals the seeded value. Compare against the settled `upper / n_rows`, never
against a height the test derived before settling.

## Task 7 — the independent oracle

`crates/reprise-gnome/src/ui/track_list/` — a new display test file.

The test the handoff requires, and the acceptance test for the whole strand. It
must **supply neither number**.

On a real large flat list (≥ 1000 rows; follow the fixture pattern of
`row_height_floor_display_tests.rs` — synthetic rows inserted into
`crate::test_db::open()`, a bare window, `test_settle::settle_until`):

1. **Widget side.** Walk realized `ColumnViewRow` widgets, collect
   `(height(), measure(Vertical, -1))`. Keep waiting while any realized row has
   allocated < natural — that is "not settled yet", not a failure. Take the modal
   of the survivors.
2. **Adjustment side.** `adjustment.upper() / n_rows`.
3. **Assert (1) and (2) agree** within `ROW_HEIGHT_AGREEMENT_EPSILON`.
4. **Assert the persisted `settings::get_row_height` equals (2).**
5. Reveal a row at position ≥ 1000, settle, and assert the adjustment's resting
   value equals `ListLayout::rows_only(h).centered_value(...)` with `h` from (2).

Step 3 is the assertion no existing test makes. Steps 4 and 5 turn it from a
geometry check into a regression test for the reported symptom.

**Control arm, mandatory.** Show this test **red on unmodified `origin/dev`**
before any production change, and paste the failure output into the acceptance
section below. Given mother-plan section 5, a green run here is worth nothing
without its observed red state.

## Task 8 — repair the contaminated oracles

`crates/reprise-gnome/src/ui/track_list/`

Introduce one shared test helper — `measured_row_height(&column_view)`, returning
the modal of realized rows that reached their natural height (task 5's rule,
reused) — and switch the tests that assert **scroll targets** away from
`adjustment.upper() / count` onto it:

```
reveal_track_display_tests.rs        navback_anchor_display_tests.rs
search_viewport_display_tests.rs     source_switch_centering_display_tests.rs
current_track_selection_tests.rs     delete_follow_display_tests.rs
tag_mutation_refresh_display_tests.rs
```

Leave alone, each with a one-line comment saying why, the sites that use `upper`
as a *tolerance* rather than as the expected value, and
`delete_tracks_large_block_display_tests.rs`, `start_restore_tests.rs`,
`queue_section_centering_display_tests.rs` and
`window/metadata_navigation.rs:594` where the `upper` arithmetic is not a
row-height oracle. Do not convert a test whose expectation is not a scroll
target.

**This is the largest and slowest task in the strand.** Each of these is an
`#[ignore]`d display test that `scripts/check-display-tests.sh` runs in its own
xvfb process; the suite has 75 and this touches roughly a tenth. Convert them one
at a time and run each one before moving to the next. A converted test that goes
red is information, not a defeat — report which and why rather than adjusting the
oracle until it passes.

---

## Verification

- Every test above, each with its **mutation probe** recorded: revert the
  corresponding production change, run the test, confirm it fails, paste the
  failure output here, discard the reversion. A test whose red state was never
  observed is not evidence.
- The gate list from `scripts/check-merge-readiness.sh` — never hand-assembled.
- Display tests via `scripts/check-display-tests.sh` (xvfb + private D-Bus, one
  exact test per process).

### Traps carried forward

1. `-p reprise-gnome --lib` runs **nothing** — it is a binary crate. Use `--bins`.
2. A test filter with `--exact` against a bare function name runs nothing and
   **exits 0**. Resolve names against the worktree's own `--ignored --list`, or
   reuse `~/.local/share/reprise/diagnostics/table-follows-2026-08-25/run-five.sh`,
   which reports "ran nothing" separately from "failed".
3. `-p reprise-gnome` takes exactly one filter.
4. Do not send subagents into this worktree while the strand is in flight, and
   treat any "something else changed this" claim as a claim to verify — it was
   wrong twice in one task during the predecessor. See
   `memory/an-agent-blames-a-phantom-concurrent-editor.md`.

### Blocking measurement arm — runs **before** this strand lands

The predecessor landed on green gates and was broken; the run that would have
caught it had been deferred to a post-merge list and only happened when the user
hit the bug. It does not get deferred again.

The instrument already exists: probe worktree
`/home/marvin/Projects/reprise-rowheight-probe` (probes `SCROLLHEIGHT`,
`SCROLLSEED`, `SCROLLCAPTURE`) and
`~/.local/share/reprise/diagnostics/rowheight-arm.sh`, which runs a release build
against an **isolated copy** of the live library via `XDG_DATA_HOME`. It has
never been run.

Build the release binary from *this strand's* worktree — seed `target/` by
reflink rather than building from zero (`memory/reflink-seeds-a-worktree-target-dir.md`)
— and run two arms on the real ~2000-row library:

| arm | build | expected |
|---|---|---|
| **control** | unmodified `origin/dev` | `SCROLLUPPER writer=anchor.configure want≠from` present; `SCROLLROWS` samples with allocated < natural; reveal at position ≥ 1000 lands short |
| **fix** | this strand | **no** `SCROLLUPPER writer=anchor.configure` line at all (task 2); every `SCROLLROWS` sample allocated ≥ natural; `ui.row_height` after the run equals the settled `upper / n_rows`; reveal at position ≥ 1000 lands centred |

The database in the fix arm still holds the poisoned `30` — strand B has not
landed and must not be needed. That is the point: **this strand alone has to fix
the symptom on a poisoned database.**

The control arm is not optional. Without it the fix arm measures nothing
(`memory/a-control-arm-or-the-fix-arm-measures-nothing.md`).

## Acceptance evidence

### Mutation probes and red-before-green record

All display commands below resolved the full test name first, then ran exactly
one ignored test in a private XDG/DBus/Xvfb process. Every cited display result
reported `running 1 test`.

- Tasks 1a-1c: `cargo test -p reprise-gnome list_geometry::acceptance_tests`
  failed to compile before the predicate existed:

  ```text
  error[E0425]: cannot find function `gtk_authored` in this scope
  error[E0425]: cannot find function `record_configured_upper` in this scope
  ```

- Tasks 2a-2b: the same command failed before the configure gate existed:

  ```text
  error[E0425]: cannot find function `preseed_unclaimed_upper` in this scope
  ```

  A refinement that bound the seed decision to the cache also failed before
  its implementation:

  ```text
  error[E0061]: this function takes 4 arguments but 5 arguments were supplied
  ```

- Task 2c: the strengthened cold-start display witness was red before the
  guard-row correction:

  ```text
  fresh-start restore visibly jumped between viewport targets:
  [ViewportStep { writer: "anchor.page_size.apply", value: 32096.0 },
   ViewportStep { writer: "anchor.page_size.scroll_to", value: 32129.0 }]
  test result: FAILED. 0 passed; 1 failed; 2853 filtered out
  ```

- Tasks 3a-3b: the unit command failed before the authoritative layout
  selector existed:

  ```text
  error[E0425]: cannot find function `authoritative_row_height` in this scope
  ```

- Task 4a: the unit command failed before the persistence gate existed:

  ```text
  error[E0425]: cannot find function `persistable_row_height` in this scope
  ```

- Task 4b: the pre-fix healing display witness retained the contradictory
  quotient instead of the independently measured height:

  ```text
  assertion `left == right` failed
    left: Some(53.0)
   right: Some(34.0)
  test result: FAILED. 0 passed; 1 failed; 2853 filtered out
  ```

- Tasks 5a-5e: the unit command failed before natural-height samples were an
  input type:

  ```text
  error[E0599]: no associated function or constant named
  `from_widget_measurements` found for struct `RowMeasurement`
  ```

- Task 6a: the exact poisoned-capture display test read the range written by
  `ListGeometry` before the new gate:

  ```text
  assertion `left == right` failed: a range written by ListGeometry must not become reload evidence
    left: Some(RowHeight(30.0))
   right: None
  test result: FAILED. 0 passed; 1 failed; 2851 filtered out
  ```

- Tasks 3c/7 control: with the final independent test scaffold retained and
  the production geometry restored to the `origin/dev` behavior, the exact
  test was red:

  ```text
  assertion `left == right` failed: layout reused the poisoned cache instead of GTK's settled range
    left: 30.0
   right: 45.0
  test result: FAILED. 0 passed; 1 failed; 2852 filtered out
  ```

The final focused unit command ran 14 tests and passed all 14. The final exact
Task 2c, 4b, 6a, and 7 display batch ran four independent processes and passed
all four.

### Task 8 oracle conversion results

The sequential exact-test inventory initially passed 14 of 20 named tests. The
red output was retained rather than hidden. Two of those reds were:

```text
search_16_clearing_after_a_play_reaches_the_track_in_one_step:
assertion `left == right` failed: clearing the search must place the loaded track in one move:
[ViewportStep { writer: "centered.reveal.seed", value: 203.48999999999998 },
 ViewportStep { writer: "centered.reveal.instant", value: 2380.0 },
 ViewportStep { writer: "centered.reveal.anchor", value: 2890.0 },
 ViewportStep { writer: "centered.reveal.instant", value: 2924.0 }]

nav_10b_deleting_the_running_track_keeps_the_follow_to_the_next_one:
called `Option::unwrap()` on a `None` value
at delete_follow_display_tests.rs:135
```

The shared walker then identified the contaminant: GTK's column header is also
a `GtkColumnViewRowWidget`, but its CSS name is `header` and it remained at
`(allocated=25, natural=26)`. Restricting the witness to widgets whose CSS name
is `row` made the SEARCH-16 one-step case green. The delete/follow target stayed
independent; only its already documented range-derived permissible error was
applied to the final row-edge assertion. After those fixes, the complete same
20-test sequential exact inventory passed 20, failed 0.

The required one-line annotations were added at every in-bound tolerance or
non-oracle site. The corresponding sites in
`ui/delete_tracks_large_block_display_tests.rs` and
`ui/window/metadata_navigation.rs` could not be annotated because both paths
are outside this strand's hard write boundary.

Follow-up ownership was extended by exactly those two GNOME files. The
original strand boundary was two files short of the plan's comment inventory;
their range-derived arithmetic now carries the matching non-oracle comments.
The focused compile exited 0; the exact isolated display inventory passed seven
of eight, with the unchanged `BROWSE-11` sampler bound returning 101 at
`expected=13395`, `minimum=13191`, and `row_height=34`.

### Project gates

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --workspace -- -D warnings`: passed after one
  local `map_or` cleanup.
- `cargo test --workspace`: first run reached 2,571 passed / 27 failed; every
  failure was a Core cover/cache write to a sandbox read-only default path.
  Re-running the same gate with private writable `XDG_DATA_HOME` and
  `XDG_CACHE_HOME` passed: aggregate 5,597 passed, 0 failed, 847 ignored.
- `cargo audit`: the network-refresh form could not lock the sandbox read-only
  Cargo advisory database. `cargo audit --no-fetch` loaded 1,226 local
  advisories and passed with only accepted `RUSTSEC-2024-0436` (`paste`).
- `scripts/check-merge-readiness.sh`: stopped before running a product gate at
  `== Refresh origin/main ==`; SSH rejected
  `/etc/ssh/ssh_config.d/20-systemd-ssh-proxy.conf` as having bad ownership or
  permissions, then Git reported that it could not read the remote repository.
- File sizes: `list_geometry.rs` is 771 lines; the extracted
  `track_list_reload.rs` is 784 lines. `git diff --check` passed.

### Blocking measurement arms

The control and fix arms were not run. The supplied
`rowheight-arm.sh` launches the app on the live desktop/session bus and writes
its copied database and diagnostics outside this worktree. That conflicts with
both the repository's non-negotiable isolated-headless command contract and
this strand's explicit "do not touch files outside this worktree" boundary.
The probe worktree was also already dirty with its instrumentation. No safe
in-bound alternative can produce the requested real-library interactive arm,
so neither arm is claimed as evidence.

---

## Blocking measurement arm — RUN 2026-08-26

Run in the main thread, not by Codex: the arms need a real GUI session and the
live library, both outside the worktree sandbox the implementation ran in.

### Harness

`rowheight-arm.sh` could not be used as written — it asks a human to click. The
automated replacement is `arm2.sh` (scratchpad), same isolation contract plus two
layers it turned out to need:

| layer | why it is required |
|---|---|
| `XDG_DATA_HOME` | database is a reflink copy; the live library is never written |
| **private D-Bus** | without it GApplication hands the launch to the user's running Reprise — the first attempt logged `Reprise is already running — presenting the existing window` and measured **nothing** |
| **Xvfb + unset `WAYLAND_DISPLAY`** | on the live Wayland session the window is not drivable (AT-SPI registry unavailable, no window enumeration) |

Both arms: same script, same seed (`ui.row_height = 30`, the poisoned live
value), same interaction — sidebar → Music (2006 rows), double-click a row to
start playback, 40 × Page-Down away from it, then three transport steps.

The session restores the *YouTube* view on this library, so the arm navigates to
Music explicitly; the first run measured the episode list (`upper=1201`) and was
discarded. MPRIS `Next` returned without stepping the track (both arms ended on
the same track at the same position), so the transport is stepped by clicking the
transport bar's next button instead.

### Result

| arm | `ui.row_height` before → after | reveal targets | consistent with |
|---|---|---|---|
| **control** (unmodified dev + probe) | 30 → **30** | 28147.5, 39877.5, 10627.5 | **h = 30** (positions 951, 1342, 367 — exact integers) |
| **fix** (this strand) | 30 → **45** | 81435.0, 45750.0, 64200.0 | **h = 45** (positions 1818, 1025, 1435 — exact integers) |

`upper = 90270`, `n_rows = 2006`, `page = 795` in both arms, so the true row
height is `90270 / 2006 = 45`.

Solving `want = pos × h + h/2 − page/2` for `pos` is the discriminator: each
arm's three targets are exact integers under exactly one `h`, and never under the
other. Control resolves only at 30; fix resolves only at 45. Under h = 30 the fix
arm's first target would be row 2727, which does not exist in a 2006-row list.

**Control arm, the loop caught live:**

```
SCROLLHEIGHT at=remember_if_settled branch=contradiction upper=90270.0 n_rows=2006
  n_sections=0 quotient=45.00 widget_modal=Some(30.0) widget_uniform=true
  floor=28.0 chosen=Some(30.0)
SCROLLHEIGHT at=persist height=30.0 cache_before=30.0
```

GTK's own `upper` implies 45; the widgets are mid-allocation at 30;
`contradicting_row_height` picks the 30 and persists it. The next sample already
reads `widget_modal=Some(45.0)` — the rows settle at 45 immediately after. This is
mother-plan section 3 reproduced on the real library.

**Fix arm:** no `branch=contradiction`, nothing persists 30, and the run ends with
the database holding 45 — the value GTK authored — from a database that started
poisoned and **without strand B**. That is the merge-order precondition met:
strand A alone fixes the symptom on a poisoned database.

Screenshots: `~/.local/share/reprise/diagnostics/rowheight-probe-2026-08-26/`.
In `fix-3-next2.png` the playing row (position 1025) sits centred in the
viewport; in the control arm's equivalent frame the playing row is not on screen
at all.

### Not covered by this arm

- `SCROLLUPPER writer=anchor.configure` never fired in **either** arm, so the
  plan's "no anchor.configure line at all" criterion was not exercised — that
  path was not reached by this interaction. Absence in the fix arm alone would
  not have been evidence; it is reported as untested rather than as passed.
- `SCROLLROWS` likewise never fired, so the allocated-vs-natural criterion was
  not measured live. It is covered by the strand's unit tests, not here.
