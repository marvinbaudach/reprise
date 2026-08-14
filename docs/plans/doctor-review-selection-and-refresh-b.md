---
slug: doctor-review-selection-and-refresh-b
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-14
---
# Strand B — one toggle updates the rows it toggled

Mother plan: `docs/plans/doctor-review-selection-and-refresh.md`. Read it for
the full diagnosis (§A), the complete decision list (§B), the scope fence (§G),
the merge order (§I) and the post-merge cross-checks (§J). This file is the
strand-local body: what B implements, in which order, what B owns, and how B is
proved.

**B starts only after strand A (`docs/plans/doctor-review-selection-and-refresh-a.md`)
has merged into `dev`.** Branch from `origin/dev` **after a fetch**, not from
A's branch tip. A and B both rewrite `bind_album_header`
(`review_header.rs:182-281`) and both add tests to `review_page_tests.rs`; there
is no cut that makes them concurrent (mother §H).

Line references were read in `051fb088df`. Strand A changes
`review_header.rs`, `review_row.rs`, `review_model.rs`,
`strings_library_doctor.rs`, `review_page_tests.rs`,
`review_row_contract_tests.rs` and the body of `toggle_position`
(`review_page.rs:149-159`); re-read those before relying on a cited range.
Everything B rewrites in `review_page.rs`, `review_conflicts.rs` and
`review_snapshot.rs` is untouched by A.

---

## Why this strand exists

`ReviewState::refresh()` (`review_page.rs:57-100`) is the **only** update path.
Selection changes reach it through `set_selected` (`:138-147`), the album header
(`review_header.rs:215`), the row checkbox (`review_row.rs:52-58`) and the
select-all handler (`review_page.rs:534-548`). One toggled album costs:

| Work | Where | Cost |
| --- | --- | --- |
| re-group the whole scan | `grouped_rows_for`, `review_model.rs:124-233` | one `ReviewRowModel` per row with deep-cloned `album_key`/`title`/`artist`/`track`/`field`/`current`/`proposed` strings |
| ↳ inside it, `group_review_rows` | `reprise-core/.../grouping.rs:47-101` | per-track linear search over album seeds (`:83`) → O(tracks·albums) |
| ↳ and `album_from_seed` | `grouping.rs:103-114` | full `session.rows()` scan **per album** (`:108-113`) → O(albums·rows) |
| replace the entire store | `store.splice(0, n_items, &objects)`, `review_page.rs:80` | `FilterListModel` + `CustomSorter` + section sorter re-run over everything |
| force another filter pass | `filter.changed(FilterChange::Different)`, `:82` | even though the category filter did not change |
| rebuild the conflicts widget tree | `refresh_conflicts`, `:206-240` | full `ReviewConflicts::new` (`review_conflicts.rs:20`, a 140-line widget tree) + `store.append` (`:239`) on **every** refresh |
| deep-clone every visible row **twice** | `visible_rows()` at `:109` and `:114` | `row_at` clones each `ReviewRowModel` (`:284-291`) |
| rebuild every visible album header | `bind_album_header`, `review_header.rs:203-280` | a fresh `CheckButton` plus ~8 widgets per header, and `:191-193` clones every row of the section one at a time |

The `DOC-9b … start/end = 4294967295` warnings (`u32::MAX` =
`gtk4::INVALID_LIST_POSITION`) are the same cause from the other side: the
`notify::start` / `notify::end` handlers installed in `album_header_factory`
(`review_header.rs:148-180`) fire in the middle of the store swap,
`row_at(model, header.start())` returns `None`, and the header keeps whatever
child it had. Worse than the warning is what the early return hides: when the
bounds are merely **partial** rather than invalid, `bind_album_header` computes
the album's check state from an incomplete section (`review_header.rs:191-202`)
and displays a wrong state with no warning at all.

A third, permanent source of the same warning: the conflicts panel is appended
into the same `gio::ListStore` as a bare widget (`review_page.rs:239`), and
`compare_rows` sorts non-boxed objects last (`:346-355`), so the panel forms its
own section, `row_at` cannot produce a `ReviewRowModel` for it, and that
section's header keeps a recycled album header.

Reported effect on a real library (254 MB database, many albums): unchecking and
re-checking one album's checkbox "kostet viel Zeit". **How much** is what B
measures before it fixes anything.

---

## Decisions this strand implements

Full text in the mother plan §B. B is bound by R-6 … R-14 and R-18 … R-21. The
three that shape the *order* of the work:

- **R-19 — B opens with a measurement gate, and the recorded profile decides the
  depth of the rest of B.** Stated in advance, below.
- **R-18 — the control arm is a switch in one build**
  (`REPRISE_DOCTOR_FULL_REFRESH=1`), not a second build.
- **R-21 — `MAX_TOGGLE_CHURN` is measured, never predicted.**

And the two that are easiest to get wrong:

- **R-10** — the `binding: Cell<bool>` guard wraps **every** programmatic state
  write, the bind path *and* the push path. It is a precondition of R-9's push,
  not a safety net: after `apply_selection` there is no full splice, so GTK does
  not necessarily re-bind the album header at all; the new state must be
  *pushed* into the realized widget through the registry, and a push calls
  `set_active` on an already-connected `CheckButton`, which emits `toggled`.
  Without the guard that emission is indistinguishable from a user click and
  re-enters `set_selected` — defect A-1 reproduced as a feedback loop instead of
  a lost check mark.
- **R-20** — the store's layout invariant (boxed rows at `0..n`, conflicts panel
  last) is pinned by a `debug_assert!` **and** a behavioural test, never assumed.

---

## Order of work — two `/code` runs with a measurement between them

### B-0 (commit 1). Instrumentation and probes only

Nothing below B-1 may be coded before this commit's measurement exists and is
written into this file.

1. **Per-stage timings in `refresh()` (R-14).** One `tracing::debug!` per stage,
   each with its own `elapsed_us`:
   - `grouped_rows_for` (`review_page.rs:71-75`),
   - `store.splice` (`:80`),
   - `refresh_conflicts` (`:81`),
   - the aggregate computation (`:98-99` — `refresh_filter_summary` at
     `:108-111` and `refresh_master_check` at `:113-136`, i.e. the two
     `visible_rows()` passes at `:102-106`),
   plus one whole-path line naming `path="full"`, the number of rows and the
   elapsed microseconds. `REPRISE_LOG=debug` — **never** `RUST_LOG`; this crate
   reads `REPRISE_LOG` (`main.rs:88-92`). Cost at the default level: nothing.
2. **The churn probe** of V-4(a), with `MAX_TOGGLE_CHURN` set to the number
   actually observed on the unmodified path and the measurement in a comment
   (R-21).
3. **The opt-in wall-clock probe** of V-4(b), timing `refresh()` only — at this
   commit there is no second path yet.
4. **Run V-4(c) on this commit** (real library, user at the GUI, session at the
   log). This is the pre-fix per-stage profile.

**The rule that reads the profile (R-19), fixed before the measurement:**

- if `grouped_rows_for` dominates — the regrouping, including the
  O(tracks·albums) seed search (`grouping.rs:83`) and the per-album
  `session.rows()` scan (`grouping.rs:108-113`) — then the incremental path
  **B-2/B-3/B-4 is mandatory**: only skipping the regroup removes that cost;
- if the aggregate passes and the conflicts panel dominate, then **B-1 alone is
  the fix** and the incremental path is dropped from this round. Record the
  profile here and in the PR body and say so plainly. A cheaper outcome is a
  result, not a failure;
- either way **B-1 lands**: R-8/R-11/R-12 are cheap and independently correct.

Record the measured profile in this file under a `### Measured profile` heading
before starting the second `/code` run, with the commit, the database, the album
and the five-cycle medians per stage.

### B-1 (commit 2+). The cheap, unconditional half

- **R-8 — one snapshot per full rebuild is the single source of the page's
  aggregate numbers.** `grouped_rows_for` is called once; its output plus the
  per-album aggregates and the page totals are cached. `refresh_filter_summary`
  (`:108-111`) and `refresh_master_check` (`:113-136`) read the cached totals
  instead of calling `visible_rows()` twice and deep-cloning every row each time
  (`row_at` at `:284-291` clones).
- **R-11 — `filter.changed()` fires only when the category filter changed**
  (`set_category`, `:161-167`), not on every refresh (`:82`).
- **R-12 — the conflicts panel is rebuilt only when its group fingerprint
  changed** — `Vec<(DoctorReviewGroupId, Option<DoctorValue>)>` over the
  category-matching groups, cached in a `RefCell` — and its section stops being
  reported as a lost album header: the header factory recognises a non-boxed
  section item and clears the header child instead of warning and keeping a
  recycled album header.

### B-2. `review_snapshot.rs` — the cached truth (R-13)

`review_page.rs` is 641 lines; `scripts/check-architecture.sh:20` fails at 800
and the repo's target band is 200–400. The snapshot type, the per-album
aggregates, the diff and the store index go into a new file; the two update
paths stay in `review_page.rs`.

```rust
pub(super) struct ReviewSnapshot {
    pub(super) rows: Vec<ReviewRowModel>,
    /// `album_key` → (selected changes, selectable changes, inventory changes)
    pub(super) albums: HashMap<String, AlbumCounts>,
    /// row id → position in the `gio::ListStore`
    index: HashMap<DoctorReviewRowId, u32>,
    pub(super) totals: ReviewTotals,   // selected, selectable, changes, albums
}
```

Built once from `grouped_rows_for` (`review_model.rs:124-233`) in one pass that
also accumulates the per-album counts and the totals — the same numbers
`review_header_counts` (`review_page.rs:301-309`) and `refresh_master_check`
(`:113-136`) compute today, produced without a second traversal and without
deep-cloning a single row. `AlbumCounts` carries the inventory count too, so
strand A's `album_header_state(selected, selectable, changes, blocked_by)` gets
its `changes` argument from the snapshot once the header is pushed (R-9)
instead of summing it per bind.

Two operations:

- `ReviewSnapshot::selection_diff(&self, session) -> Vec<(u32, ReviewRowModel)>`
  — recomputes only the selection facts of the cached rows
  (`selected_change_count` and `row.selected`, the logic at
  `review_model.rs:201-208`) against a
  `HashMap<DoctorReviewRowId, &DoctorReviewRow>` built from `session.rows()`,
  and returns the store index plus a fresh model for **each row whose facts
  actually changed**. No regrouping, no string allocation for unchanged rows.
- `ReviewSnapshot::with_selection(self, changed) -> ReviewSnapshot` — the new
  cached snapshot (new value, no in-place mutation), with `albums` and `totals`
  adjusted from the same diff.

*Rejected (R-7): turning `ReviewRowModel` into a `GObject` with a `selected`
property* — the better GTK idiom, the wrong size for this round: the model is a
plain struct inside a `glib::BoxedAnyObject` read by the filter (`:419-427`),
both sorters (`:346-368`), the row factory (`review_row.rs:90-98`), the header
factory (`review_header.rs:291-298`) and roughly a dozen tests. *Rejected: a
custom `ListModel` subclass* — identical positional bookkeeping, no extra
precision. *Rejected: mutating the boxed model in place* — violates the
immutability rule and lies to the sorter.

### B-3. Two paths in `ReviewState` (R-6)

```rust
fn refresh(self: &Rc<Self>)                                  // structural, as today
fn apply_selection(self: &Rc<Self>, session_changed: bool)   // selection only
```

`apply_selection`:

0. `if self.full_refresh_only { return self.refresh(); }` — R-18's control arm.
   The field is a plain `bool`, read **once** from
   `REPRISE_DOCTOR_FULL_REFRESH` where the `ReviewState` literal is built
   (`review_page.rs:486-…`, inside `LibraryDoctorReviewPage::new` at
   `:404-411`), defaulting to off.
1. `let changed = snapshot.selection_diff(&session)` — bail out early when it is
   empty: re-checking an already-checked album must cost nothing.
2. for each changed row, `store.splice(index, 1, &[new_boxed_object])`; adjacent
   indices are coalesced into one `splice` per run. Every index carries R-20's
   `debug_assert!`; index `n` is never written.
3. **no** `filter.changed()` (R-11), **no** `refresh_conflicts()` (R-12), **no**
   `store.splice(0, n, …)`.
4. replace the cached snapshot, then push the new state to the top select-all
   (the existing `refresh_master_check` body, now reading `totals`) and to the
   realized album headers of the affected albums (B-4), through the `binding`
   guard (R-10).
5. one `tracing::debug!` (R-14) naming `path="selection"`, `touched`,
   `elapsed_us`.

`set_selected` (`:138-147`) calls `apply_selection` instead of `refresh`. So does
the select-all handler (`:534-548`) — for `all()`/`none()` the diff legitimately
covers every row, so that case costs what it costs today; it is bounded by "the
rows that actually changed", which is the honest budget. Everything structural
keeps calling `refresh()` (R-6): category filter (`:161-167`), remote visibility
(`:169-172`), layout (`:174-180`), staleness marking (`:182-204`), the
group-choice closure (`:220-229`, whose `state.refresh()` sits at `:228`),
skip-all (`:242-250`), write report (`:252-275`).

`refresh()` itself loses the two `visible_rows()` passes (R-8), keeps the
selection-position restore (`:86-88`), and keeps B-0's per-stage and whole-path
lines with `path="full"`.

**Why a selection toggle may skip the regroup at all:** `set_selected`
(`reprise-core/.../review.rs:458-476`) only flips `row.selected` and the tie
map. It cannot change which rows exist, so nothing about the grouping needs
recomputing. The core's refusal of a non-`Ready` row (`:468-470`) stays
untouched (R-5).

### B-4. The header stops pulling from a half-updated model (R-9, R-10)

`album_header_factory` (`review_header.rs:148-180`) gains a registry, in the
shape the row factory already uses (`review_row.rs:37`, `:69-71`):

```rust
struct HeaderWidgets { root, checkbox, title, detail, pill, cover, caret,
                       album_key: RefCell<String>, binding: Cell<bool> }
type HeaderRegistry = Rc<RefCell<HashMap<usize /* ListHeader ptr */, Rc<HeaderWidgets>>>>;
```

- `connect_setup` builds the widget tree **once** and registers it;
  `connect_teardown` removes the entry (the row factory omits this today; do not
  copy that omission into the new code).
- `connect_bind` and the `notify::start`/`notify::end` handlers only *bind*:
  album title, artist, track count, and the check state **looked up by
  `album_key` in the snapshot's `albums` map** — never summed from
  `header.start()..header.end()`. A partial section silently computes a wrong
  state; a hash lookup cannot.
- `set_active`/`set_inconsistent`/`set_sensitive` are wrapped in the `binding`
  guard on **both** the bind and the push path (R-10), and the `toggled` handler
  reads the currently bound album's `row_ids` from the registry entry instead of
  capturing a fresh `Vec` per bind.
- a section whose first item is not a `glib::BoxedAnyObject` (the conflicts
  panel) clears the header child and does **not** warn (R-12).
- the invalid-bounds early return stays with its `DOC-9b` warning
  (`review_header.rs:184-188`) — still the right thing to log when GTK asks for
  a header outside the model.
- **strand A's behaviour must survive the move**: all three fields of
  `MasterCheckState` are applied (R-1), the pill carries count *and* reason when
  nothing is selectable (R-2), and the root's tooltip is **cleared** when the
  reason is `None` — with reused widgets that clearing is no longer academic.

`apply_selection` pushes to this registry: for each affected `album_key`, look up
the realized header, if any, and re-apply the check state. This removes the
"header rebinds against a half-updated model" class entirely — after B the
header never reads the model to decide its own state.

**R-9's premise, and why it is legal under DOC-3c** ("exactly the rows that
filter shows"): `group_review_rows` already applies the category filter before
grouping (`grouping.rs:111`, inside `album_from_seed`'s `session.rows()` scan
`:108-113`), which makes the `CustomFilter` at `review_page.rs:419-427` a
pass-through for boxed rows — so the snapshot *is* the visible set. That
equivalence is **pinned by a test** (V-3), not assumed. The filter object stays:
removing it would change the model chain and the tests that assert it, and it
still gates the non-boxed conflicts panel through `object.is::<gtk4::Widget>()`
(`:421`).

### B-5. The conflicts panel (R-12)

`refresh_conflicts` (`:206-240`) computes the fingerprint from the
category-matching groups and returns early when it equals the cached one. Its
`store.append` (`:239`) becomes append-or-replace at a tracked index, so the
panel survives a full `refresh()` that did not change the groups. The early
return on `groups.is_empty()` (`:216-218`) must additionally **remove** a panel
that is no longer wanted — today it cannot leak because the whole store is
spliced away, and that crutch is gone.

**Do not delete the literal `self.store.append(&panel.root);`.** Two existing
unit tests read the source of `review_page.rs` with `include_str!` and assert it
contains `"store.append(&panel.root)"`:
`doc_9b_conflicts_sit_at_the_end_and_skip_all_clears_them`
(`review_page_tests.rs:443-456`) and
`doc_9b_the_conflicts_panel_is_the_last_row_of_the_scrolled_list` (`:459-464`).
Keep `append` as the "panel not present yet" branch and use
`splice(index, 1, &[panel.root])` only for the replace branch; both tests then
stay green and stay honest. If a future shape makes that impossible, retarget
them to behaviour **in the same commit**, with the reason in the commit message
— never silently weakened.

---

## Verification

### How to run anything at all in this crate

- `reprise-gnome` has **no `--lib` target**. Unit tests:
  `cargo test -p reprise-gnome --bin reprise library_doctor::review`.
- Display/GTK tests are the `#[ignore = "requires a display; run via xvfb-run"]`
  ones and must run **one exact test per process** — GTK initialises once per
  thread and `--test-threads=1` still migrates between harness threads
  (`TESTING.md:296-303`). Use `scripts/check-display-tests.sh`; while iterating
  on one test, run that exact filter in its own process.
- `cargo test --exact` with a wrong module path **runs nothing and exits 0**.
  Read the run's own accounting: `grep -c '^test result: FAILED' run.log` plus a
  positive `N passed`. A summary line alone proves nothing.
- Display tests are flaky as a herd and several are **already red on
  `origin/dev`**. Before blaming this branch, run the same test on `origin/dev`
  and say which failures pre-date the change.
- B's tests assert store contents, counts and signals — **not geometry** — so
  they do not need the app CSS. Any test that measures a widget's allocation
  must first call
  `crate::ui::style::install_css_string_for_test(&super::super::css())`
  (`ui/style/mod.rs:177`, `library_doctor/mod.rs:97`), or the measurement is
  meaningless.
- Redirect long output to a file and answer the question with `grep`/`wc`; never
  read a whole log back.

**Probe naming (R-16).** `scripts/check-ux-traceability.sh:93-108` rejects any
`#[ignore]` on a rule-named test unless the reason is *exactly*
`"requires a display; run via xvfb-run"`, and `scripts/check-display-tests.sh:18-21`
enumerates **every** `--ignored` test in the crate and runs each one (unfiltered
since #463). Therefore: performance probes carry **no** rule-name prefix, and the
heavy one no-ops without its environment variable — otherwise it joins the merge
gate. Precedent for the env-var shape:
`crates/reprise-gnome/src/ui/track_list/track_list_model_scalability_tests.rs:13-25`
(`REPRISE_PERF_TRACKS`).

### V-3. R-9's and R-20's premises, pinned

- `doc_9b_the_snapshot_is_the_visible_row_set` (unit): with and without a
  category filter, `sorted.n_items()` equals `snapshot.rows.len()` plus the
  conflicts panel when one is present. If `group_review_rows`' filtering
  (`grouping.rs:111`) and the `CustomFilter` (`review_page.rs:419-427`) ever
  diverge, this test says so instead of the header quietly counting the wrong
  set.
- `doc_9b_the_conflicts_panel_stays_the_last_store_item` (display,
  **behavioural** — not an `include_str!` assertion): on a scan with conflicts,
  every store item at `0..n` downcasts to `glib::BoxedAnyObject`, item `n`
  downcasts to `gtk4::Widget`, and both statements still hold after one
  selection toggle. This is R-20's pin; the `debug_assert!`s are the second line
  of defence, not the first. The invariant holds today and was read: `refresh()`
  writes exactly the boxed rows with `store.splice(0, n_items, &objects)`
  (`:80`), `refresh_conflicts` appends the panel afterwards (`:239`), and
  `compare_rows` (`:346-368`) returns `Ordering::Larger` for a non-boxed left
  against a boxed right (`:349-355`), so the panel is last in the sorted view
  too.
- `doc_9b_a_rebound_album_header_does_not_emit_a_selection_change` (display,
  **required**, R-10): count calls into the `on_select` callback while (a)
  rebinding a realized album header and (b) **pushing** new state into it via
  `apply_selection` — both must be **zero**. The push half is the one that
  matters: it is the path that has no full splice behind it and therefore the
  path GTK does not re-bind on its own. A test that only rebinds proves half the
  guard.
- `doc_9b_the_conflicts_section_binds_no_album_header` (display, R-12): with
  conflicts on screen, the panel's section carries no album header child, and no
  `DOC-9b` warning is emitted for it.

### V-4. The control arm, four measurements

**(a) Churn, deterministic, in the merge gate.** New file
`review_page_perf_tests.rs`, declared as a child module of `review_page` next to
the existing `mod tests;` (`review_page.rs:641`),
`#[ignore = "requires a display; run via xvfb-run"]`, name without a rule prefix:
`review_selection_toggle_touches_only_the_toggled_album`.

- fixture: 16 albums × 12 rows = 192 rows, generated;
- connect to `state.store.connect_items_changed` and sum `removed + added`
  (`state` is a private field of `LibraryDoctorReviewPage`,
  `review_page.rs:397-401`, reachable from a child module);
- toggle one album's rows through `state.set_selected(...)`;
- assert the churn is at most `MAX_TOGGLE_CHURN`;
- assert correctness in the same test: the toggled album's header counts are
  right, every other album's are unchanged, and `sorted.n_items()` is unchanged.

**`MAX_TOGGLE_CHURN` is the control arm and it is measured (R-21).** Commit B-0
commits the number it *observed* on the unmodified path, with the measurement in
a comment:

```rust
/// Measured on <commit> with the 16 × 12 fixture: <observed> items changed for
/// one album toggle (one full splice of the whole store).
const MAX_TOGGLE_CHURN: u32 = /* measure — do not predict */;
```

The commit that makes the update incremental **lowers it in the same change**
and records both numbers. The draft's arithmetic (`2 × 193 = 386` before,
`2 × 12 + 2 = 26` after) is an expectation about the *shape* of the number and
is explicitly **not** a budget; committing it unmeasured is forbidden. A budget
nobody lowers is a budget nobody believes —
`scripts/check-frontend-thinness.sh:1-12` states that principle for the whole
repo.

**(b) Wall clock, synthetic, opt-in — both paths, one build.**
`review_selection_toggle_wall_clock_probe`, ignored, returns immediately unless
`REPRISE_DOCTOR_PERF_ALBUMS` is set. With it set: build `N` albums × 12 rows and
time nine single-album toggles with `Instant`, printing median and max. Once
`apply_selection` exists, the same probe times **both** paths in the same
process by flipping the R-18 field between runs, so the comparison carries no
build-to-build drift. Both numbers go into the PR body.

**(c) Wall clock, the real library — the measurement the report is about.**
The synthetic arm cannot reproduce a 254 MB database.

1. copy the real database including its `-wal` and `-shm` sidecars after a clean
   app shutdown — a copy without the sidecars is a *different* database;
2. run the app against an isolated XDG profile pointed at the copy, with
   `REPRISE_LOG=debug`, `REPRISE_AUDIO_SINK=fakesink`;
3. open Library Doctor → Review, uncheck and re-check the **same** album five
   times with `REPRISE_DOCTOR_FULL_REFRESH=1` (control), then five times without
   it (fix) — one build that has both paths, one session, same album, same
   window size, same order; quit;
4. from the log: per-stage medians (R-14), whole-path medians for both arms and
   the ratio; plus `grep -c 'DOC-9b'` per half (V-4(d));
5. the B-0 profiling run has only one path and yields the pre-fix per-stage
   profile. **Cross-check** it against the finished branch's
   `REPRISE_DOCTOR_FULL_REFRESH=1` arm: if the two medians differ by more than
   session noise, the "control" arm is **not** the pre-fix path and the switch is
   wrong. Say so rather than reporting the ratio.

**Division of labour: the user drives the GUI run; the session evaluates the
log.** Codex does not run display tests and must not drive the real app. The
implementing agent produces the build, the exact command lines and the grep
recipe; the user performs the five-cycle interaction; the session reads the log
and reports both medians and the ratio. "Feels fast" is not a result. If the
control arm does not reproduce the reported slowness, say so — that is a finding
about the diagnosis, not a licence to skip the measurement.

**(d) The `DOC-9b` warnings.** In the same session, `grep -c 'DOC-9b' run.log`
per half. The control half is expected to show the `start=4294967295` entries;
the fix half must show none from a selection toggle. This is the one check that
cannot be done from a test.

### V-5. Gates before landing

`cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test -p reprise-core`, `cargo test -p reprise-gnome`,
`scripts/check-display-tests.sh`, `scripts/check-ux-traceability.sh`,
`scripts/check-accessibility-semantics.sh`,
`scripts/check-architecture.sh` (the 800-line cap at `:20` is measured over the
whole tree, and B moves code between `review_page.rs` — 641 lines today — and
`review_snapshot.rs`), `scripts/check-frontend-thinness.sh`,
`scripts/check-input-parity.sh`.

Strand A's tests must pass **unchanged** after B lands — they are the proof that
B did not undo A (mother §J):
`doc_3c_an_album_with_nothing_selectable_binds_an_insensitive_header_check`,
`doc_9b_a_stale_row_names_its_reason_where_the_click_happens`,
`doc_9b_activating_an_unselectable_row_selects_nothing`. Also
`doc_9b_every_section_boundary_binds_a_non_empty_header`
(`review_page_tests.rs:613-696`): if the push-instead-of-pull header forces an
adjustment there, the adjustment must keep its assertion — *every* section
boundary carries a header naming its album — and the commit must say why the
mechanism changed.

---

## File ownership

B owns, after A has merged:

- `crates/reprise-gnome/src/ui/library_doctor/review_page.rs`
- `crates/reprise-gnome/src/ui/library_doctor/review_snapshot.rs` (new)
- `crates/reprise-gnome/src/ui/library_doctor/review_conflicts.rs`
- `crates/reprise-gnome/src/ui/library_doctor/review_page_perf_tests.rs` (new)
- `crates/reprise-gnome/src/ui/library_doctor/review_refresh_tests.rs` (new)
- **inherited from A:** `review_header.rs`, `review_page_tests.rs`,
  `strings_library_doctor.rs`, `docs/ux-rules.md`

B does **not** touch `reprise-core`. In particular the O(tracks·albums) seed
search (`grouping.rs:83`) and the per-album `session.rows()` scan
(`grouping.rs:108-113`) are **left alone**: B makes the page stop calling them on
a selection change, which is the fix the report asks for. Making the grouping
itself linear is a separate, core-side plan. B also stays out of everything
fenced off in the mother plan §G — `summary_*.rs`, `result_pages.rs`,
`running_page.rs`, `progress_card.rs`, `sidebar/*`, the scan engine, the
MusicBrainz/AcoustID clients, the tag-write jobs, `reprise-mcp`, the Android app
— and does not re-fix defects A-1, A-2 or A-4, which landed with strand A.

If R-19's rule selects the cheaper outcome, B ends after B-1 plus the tests that
cover it. That is a complete result; record the profile that decided it.

### Measured profile

- Profiling base: `57ff0bfc74` plus this B-0 instrumentation-and-probes commit.
- Generated fixture: 16 albums × 12 rows, plus the conflicts-panel store item.
- Observed churn: 386 items changed for one album toggle.
- Synthetic full-refresh probe at 16 albums × 12 rows over nine toggles:
  median 6,516 µs; maximum 6,690 µs.
- Real-library per-stage medians: pending the human GUI run on this commit.
