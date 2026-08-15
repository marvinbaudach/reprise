---
slug: doctor-review-selection-and-refresh
strands: a,b
merge_order: a,b
worktree:
branch:
phase: shipped
codex_session:
created: 2026-08-14
---
# Library Doctor review page — selection truth and a refresh that does not rebuild the world

Base: `origin/dev` = `051fb088df`. Every line reference below was read in that
tree; where the prompt's or the draft's reference was off by one, the corrected
range is used. `origin/dev` has since moved to `5721ade95e`, and
`git diff 051fb088df origin/dev -- crates/reprise-gnome/src/ui/library_doctor/
crates/reprise-gnome/src/ui/strings_library_doctor.rs` is **empty**, so the
references hold on the current head too. Branch from `origin/dev` after a fetch,
not from the recorded hash.

Four reported/derived defects, all on the **review** page
(`crates/reprise-gnome/src/ui/library_doctor/review_*.rs`). Three are small and
land together as **strand A**; the fourth is a rework of the page's update path
and lands on top of them as **strand B**. This plan is written so that **A can
ship without B**.

Binding repository contracts that outrank this plan: `AGENTS.md`,
`docs/ux-rules.md`, `TESTING.md`.

Nothing in the scan engine, the MusicBrainz/AcoustID clients, the tag-write
machinery or `reprise-core` changes. `DoctorReviewSession::set_selected`'s
refusal of a non-`Ready` row
(`crates/reprise-core/src/library/library_doctor/review.rs:468-470`, inside
`set_selected` at `:458-476`) is **correct and stays** — writing a proposal
derived from a file that changed after the scan would be wrong. What changes is
only what the UI says about it, and what it costs.

Strand bodies: `docs/plans/doctor-review-selection-and-refresh-a.md`,
`docs/plans/doctor-review-selection-and-refresh-b.md`. This file is the shared
context and is **frozen** once written: the strands amend their own files, never
this one.

---

## A. The defects, as read in the code

### A-1 — the album header checkbox silently undoes itself

`master_check_state` (`review_header.rs:19-25`) returns three facts: `active`,
`inconsistent` **and** `sensitive`. Two of the three call sites use all three;
`bind_album_header` uses two:

| Call site | `active` | `inconsistent` | `sensitive` |
| --- | --- | --- | --- |
| top select-all, `review_page.rs:113-136` (`refresh_master_check`) | yes (`:128`) | yes (`:129`) | yes (`:130-131`) |
| album header, `review_header.rs:203-215` | yes (`:207`) | yes (`:208`) | **never** |

For an album whose rows are all `DoctorReviewRowState::Stale`, the header's
`row_ids` is empty: it is built from `selectable_row_ids`
(`review_header.rs:194-197`), which `grouped_rows_for` filters down to `Ready`
rows only (`review_model.rs:192-200`). So `total == 0`, `master_check_state`
reports `sensitive: false`, and nothing applies it. The box stays clickable.

The click path, end to end:

```
checkbox.connect_toggled            review_header.rs:215
  → on_select(&[], true)            OnSelect  = review_header.rs:10
  → ReviewState::set_selected       review_page.rs:138-147   ← loops over zero ids
  → self.refresh()                  review_page.rs:57-100    ← rebuilds from the model
  → bind_album_header               review_header.rs:207     ← set_active(false) again
```

Nothing was attempted, nothing failed, and the check mark disappears. The
journal of the running build agrees: **no** `RowNotReady` warning was logged
(so `session.set_selected` was never reached — `review_page.rs:141-143` would
have logged it), while the `DOC-9b` warning from `review_header.rs:184-188`
fired 12 times with `start=4294967295 end=4294967295`.

Second, smaller lie on the same header: with `total == 0`,
`album_change_count(0, 0)` (`review_header.rs:283-289`) falls into the `else`
branch and the pill reads **"0 changes"** (`strings_library_doctor.rs:591-599`)
— for an album that has changes, all of them out of date.

### A-2 — a refused row does not say why

A `Stale` row is excluded from `selectable_row_ids` (`review_model.rs:192-200`),
`row_selectable` is false (`review_model.rs:355-361`) and the row's checkbox is
correctly insensitive (`review_row.rs:239`). The core would refuse the write
anyway (`review.rs:468-470`).

The only explanation anywhere is the page-level banner:
`review_stale_notice` (`review_page.rs:311-319`) →
`strings::doctor_stale_notice` (`strings_library_doctor.rs:377-385`) — *"N fixes
are out of date — these files changed after the scan."* — next to the
`Scan again` button (`review_page.rs:450-453`). At the row itself there is
nothing: the Source column shows only `confidence.label`
(`review_row.rs:253-269`) until a *write* has happened, because the status
suffix is only appended when `model.outcome` is `Some`. Before the first Apply,
`outcome` is always `None`. So the user clicks a dead checkbox, reads no reason,
and concludes the page is broken.

The remedy stays the rescan. What is missing is the reason at the point of the
click.

### A-3 — every toggle rebuilds everything

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
| force another filter pass | `filter.changed(FilterChange::Different)`, `review_page.rs:82` | even though the category filter did not change |
| rebuild the conflicts widget tree | `refresh_conflicts`, `review_page.rs:206-240` | full `ReviewConflicts::new` (`review_conflicts.rs:20`, a 140-line widget tree) + `store.append` on **every** refresh |
| deep-clone every visible row **twice** | `visible_rows()` at `review_page.rs:109` and `:114` | `row_at` clones each `ReviewRowModel` (`review_page.rs:284-291`) |
| rebuild every visible album header | `bind_album_header`, `review_header.rs:203-280` | a fresh `CheckButton` plus ~8 widgets per header, and `review_header.rs:191-193` clones every row of the section one at a time (`row_at`, `:291-298`) |

The `DOC-9b … start/end = 4294967295` warnings (`u32::MAX` =
`gtk4::INVALID_LIST_POSITION`) are the same cause seen from the other side: the
`notify::start` / `notify::end` handlers installed in
`album_header_factory` (`review_header.rs:148-180`) fire in the middle of the
store swap, `row_at(model, header.start())` returns `None`, and the header keeps
whatever child it had. Note the failure the early return hides: when the bounds
are merely **partial** rather than invalid, `bind_album_header` computes the
album's check state from an incomplete section (`review_header.rs:191-202`) and
displays a wrong state without any warning at all.

There is a third, permanent source of that same warning: the conflicts panel is
appended into the same `gio::ListStore` as a bare widget
(`review_page.rs:239`). `compare_rows` sorts every non-boxed object last
(`review_page.rs:346-355`), so the panel forms its own section, `row_at` cannot
produce a `ReviewRowModel` for it, and the section's header keeps a recycled
album header. With conflicts on screen this warns on every single refresh.

Reported effect on a real library (254 MB database, many albums): unchecking and
re-checking one album's checkbox "kostet viel Zeit".

### A-4 — a dead double-click costs a full rebuild

`toggle_position` (`review_page.rs:149-159`) is the row-activation path — the
`ListView` is built with `single_click_activate(false)` (`review_page.rs:441`)
and `rows.connect_activate(…)` at `:525` is its only caller, so this is
double-click and Enter. It passes `model.row_ids` (`:158`) — **all** ids of the
display row, including non-`Ready` ones — into `set_selected`. For a stale row
the core refuses every id (`review.rs:468-470`), one `RowNotReady` warning is
logged per id (`review_page.rs:141-143`), nothing visible changes, and
`set_selected` still ends in `self.refresh()` (`:146`): a full regroup, a full
splice, a full conflicts rebuild. The row checkbox already does the right thing
— it passes `selectable_row_ids` (`review_row.rs:56`). The activation path never
got the same treatment. No test covers `toggle_position` today
(`git grep toggle_position` in `library_doctor/` finds the definition and the
one call site, nothing else).

---

## B. Decisions. Nothing here is open.

- **R-1 — the album header checkbox honours `sensitive`.**
  `bind_album_header` applies all three fields of `MasterCheckState`. This is
  the whole of A-1's mechanical fix and it is one line.
- **R-2 — a checkbox that stands for an empty set states its reason in text,
  next to its count.** With `total == 0` the pill reads *count* **and** reason —
  `"3 changes · out of date"` — not the count alone and not the reason alone.
  The count is the album's inventory; the reason is a second fact about it, not
  a replacement for it. The header *root* carries the full sentence as its
  tooltip, and the root's accessible label (already set at
  `review_header.rs:276-279`) gains the same sentence.
  *Rejected: replacing the count with the reason* — the pill is the only place
  the album's inventory appears, and an album with 3 out-of-date changes is not
  an album with none. *Rejected: a tooltip on the checkbox itself* — GTK's
  default pick skips insensitive widgets, so the tooltip may never appear, and
  hover is not a keyboard channel (ACC rules). *Rejected: a per-album inline
  banner* — row height churn inside a virtualised list for a one-clause
  explanation.
- **R-3 — the row states its reason in the Source column**, through exactly the
  channel `outcome` already uses (`review_row.rs:253-269`), rendered by
  `set_full_text` (`review_row.rs:380-384`) so the label, the tooltip and the
  accessible description move together. *Rejected: a sixth column* — DOC-3b
  pins the columns and their size groups (`review_header.rs:112-136`).
  *Rejected: tooltip-only* — see R-2.
- **R-4 — every non-`Ready` state gets a label, not just `Stale`.**
  `DoctorReviewRowState` has three variants (`review.rs:86-90`); `Conflict`
  rows are refused by the same guard and are equally mute today. One extra
  match arm in one new function (`row_state_label`, §D-1) covers both.
  *Rejected: stale-only* — it would leave a second silent refusal behind and
  duplicate the decision later.
- **R-5 — the core refusal stays untouched.** `review.rs:468-470` keeps
  returning `RowNotReady`. After R-1 and R-17 the UI no longer calls
  `set_selected` with ids it knows will be refused, so the "silent no-op" path
  disappears at the source rather than being papered over.
- **R-6 — two update paths, not one.** `refresh()` stays the full rebuild and
  keeps its current semantics for every *structural* change: category filter
  (`review_page.rs:161-167`), remote visibility (`:169-172`), layout
  (`:174-180`), staleness marking (`:182-204`), the group-choice closure
  (`:220-229`, whose `state.refresh()` sits at `:228`), skip-all (`:242-250`),
  write report (`:252-275`). A new, second path handles *selection-only*
  changes. Reason: a selection toggle cannot change which rows exist —
  `set_selected` (`review.rs:458-476`) only flips `row.selected` and the tie map
  — so nothing about the grouping needs recomputing.
- **R-7 — the incremental path replaces the changed row objects via
  `gio::ListStore::splice(index, 1, …)`**, driven by a diff against a cached
  snapshot, with a `HashMap<DoctorReviewRowId, u32>` store index maintained
  next to the store (its validity is R-20's business). *Rejected: turning
  `ReviewRowModel` into a `GObject` with a `selected` property* — that is the
  better GTK idiom and the wrong size for this round: the model is a plain
  struct inside a `glib::BoxedAnyObject` read by the filter
  (`review_page.rs:419-427`), both sorters (`:346-368`), the row factory
  (`review_row.rs:90-98`), the header factory (`review_header.rs:291-298`) and
  roughly a dozen tests; converting it is a rewrite that could not land as a
  fix. *Rejected: a custom `ListModel` subclass* — identical positional
  bookkeeping, no extra precision, one more subclass to maintain.
  *Rejected: mutating the boxed model in place* — violates the immutability
  rule and lies to the sorter.
- **R-8 — one snapshot per full rebuild is the single source of the page's
  aggregate numbers.** `grouped_rows_for` is called once; its output plus the
  per-album aggregates and the page totals are cached. `refresh_filter_summary`
  (`review_page.rs:108-111`) and `refresh_master_check` (`:113-136`) read the
  cached totals instead of calling `visible_rows()` (`:102-106`) twice and
  deep-cloning every row each time.
- **R-9 — the album header's check state is pushed from the snapshot, keyed by
  `album_key`, not pulled from `header.start()..header.end()`.** A partial
  section silently computes a wrong state; a hash lookup cannot. This is legal
  under DOC-3c ("exactly the rows that filter shows") because
  `group_review_rows` already applies the category filter before grouping
  (`grouping.rs:111`, inside `album_from_seed`'s `session.rows()` scan
  `:108-113`), which makes the `CustomFilter` at `review_page.rs:419-427` a
  pass-through for boxed rows — so the snapshot *is* the visible set. That
  equivalence is **pinned by a test** (§F-3), not assumed, and the filter object
  itself stays (removing it would change the model chain and the tests that
  assert it; it also still gates the non-boxed conflicts panel through
  `object.is::<gtk4::Widget>()`, `review_page.rs:421`).
- **R-10 — album header widgets are built in `setup` and only bound in `bind`,
  and the `binding: Cell<bool>` guard wraps every programmatic state write —
  the bind path *and* the push path.** The guard is a *precondition* of R-9's
  push, not a safety net for it: after `apply_selection` there is no full splice,
  so GTK does not necessarily re-bind the album header at all. The new state has
  to be **pushed** into the realized header widget through the registry, and a
  push calls `set_active` on an already-connected `CheckButton`, which emits
  `toggled`. Without the guard that emission is indistinguishable from a user
  click and re-enters `set_selected` — A-1 reproduced as a feedback loop instead
  of a lost check mark. The row factory already runs exactly this shape
  (`review_row.rs:37`, `:51-58`, `:233-240`). This is the single most dangerous
  step in strand B, and
  `doc_9b_a_rebound_album_header_does_not_emit_a_selection_change` is a
  **required** test that must cover the push path, not only a rebind.
- **R-11 — `filter.changed()` fires only when the category filter changed**
  (`set_category`, `review_page.rs:161-167`), not on every refresh.
- **R-12 — the conflicts panel is rebuilt only when its group fingerprint
  changed** — `Vec<(DoctorReviewGroupId, Option<DoctorValue>)>` over the
  category-matching groups, cached in a `RefCell` — and its section stops being
  reported as a lost album header (A-3, third paragraph): the header factory
  recognises a non-boxed section item and clears the header child instead of
  warning and keeping a recycled album header.
- **R-13 — strand B gets a new file.** `review_page.rs` is 641 lines and
  `scripts/check-architecture.sh:20` fails at 800; the repo's own target band is
  200–400. The snapshot type, the per-album aggregates, the diff and the store
  index move to a new `review_snapshot.rs`; the two update paths stay in
  `review_page.rs`.
- **R-14 — the timing instrumentation ships, per stage.** `refresh()` gets one
  `tracing::debug!` per stage — `grouped_rows_for` (`review_page.rs:71-75`),
  `store.splice` (`:80`), `refresh_conflicts` (`:81`), the aggregate computation
  (`:98-99`, i.e. the two `visible_rows()` passes) — each with its own
  `elapsed_us`, plus one whole-path line naming the path, the number of rows
  touched and the elapsed microseconds. `apply_selection` gets the whole-path
  line with `path="selection"`. It is how the arms of §F-4(c) are measured
  (`REPRISE_LOG=debug`, never `RUST_LOG` — this crate reads `REPRISE_LOG`,
  `main.rs:88-92`), and per repo precedent that beats an external profiler for
  GTK main-thread work. Cost at the default level: nothing.
- **R-15 — no new UX rule ids.** DOC-3c (`docs/ux-rules.md:4078-4085`) and
  DOC-9b (`:4435-4470`) are amended in place, so
  `scripts/check-ux-traceability.sh` keeps resolving through the `doc_3c_*` /
  `doc_9b_*` tests that already exist. Retarget, never delete.
- **R-16 — performance probes are named without a rule prefix, and the heavy
  one no-ops without its environment variable.**
  `scripts/check-ux-traceability.sh:93-108` rejects any `#[ignore]` on a
  rule-named test unless the reason is *exactly*
  `"requires a display; run via xvfb-run"`, and
  `scripts/check-display-tests.sh:18-21` enumerates **every** `--ignored` test in
  `reprise-gnome` and runs each one (unfiltered since #463). A wall-clock probe
  that ran unconditionally would therefore join the merge gate. Precedent for
  the env-var shape:
  `crates/reprise-gnome/src/ui/track_list/track_list_model_scalability_tests.rs:13-25`
  (`REPRISE_PERF_TRACKS`).
- **R-17 — row activation offers only what can be selected.**
  `toggle_position` (`review_page.rs:149-159`) passes `model.selectable_row_ids`
  instead of `model.row_ids`, and returns early when that is empty. A
  double-click on a stale row then costs **nothing**: no refused core call, no
  warning per id, no `refresh()`. *Rejected: leaving it and relying on the core's
  refusal* — the refusal is correct and still free, but the `refresh()` behind it
  is not, and A-3 makes that the expensive half. *Rejected: deferring it to
  strand B* — it is a two-line body change in the *interaction*, not in the
  update path, and it is what makes B's incremental path measurable without a
  second dead-cost source.
- **R-18 — the control arm is a switch in one build, not a second build.**
  `REPRISE_DOCTOR_FULL_REFRESH=1` routes selection changes back through
  `refresh()`, i.e. the pre-fix path. The variable is read **once** where the
  `ReviewState` literal is built (`review_page.rs:486-…`, inside
  `LibraryDoctorReviewPage::new` at `:404-411`) into a plain `bool` field and
  defaults to off; `apply_selection` starts with
  `if self.full_refresh_only { return self.refresh(); }`. Both arms of §F-4(c)
  then come from **one** build and **one** session against the same database
  copy. *Rejected: the draft's two-build scheme* (a patched build of
  `051fb088df` for the control, the branch for the fix) — two builds, two app
  starts and two window geometries introduce drift in exactly the dimension
  being measured, and the control build would have to be reconstructed by hand
  for every re-measurement. The switch costs one branch on a `bool` per
  selection change and nothing at the default log level.
- **R-19 — strand B opens with a measurement gate, and the recorded profile
  decides the depth of the rest of B.** B's first commit is instrumentation only
  (R-14 per-stage timings plus the probes of §F-4(a)/(b)); the real-library run
  of §F-4(c) is performed on **that** commit; then, by this rule, stated in
  advance:
  - if `grouped_rows_for` dominates — the regrouping, including the
    O(tracks·albums) seed search (`grouping.rs:83`) and the per-album
    `session.rows()` scan (`grouping.rs:108-113`) — then the incremental path
    R-7/R-9/R-10 is **mandatory**, because only skipping the regroup removes
    that cost;
  - if the aggregate passes (R-8) and the conflicts panel (R-12) dominate, then
    R-8/R-11/R-12 alone are the fix and the incremental path is **dropped from
    this round**: record the profile in the plan and in the PR body and say so.
    A cheaper outcome is a result, not a failure;
  - either way R-8/R-11/R-12 land, since they are cheap and independently
    correct.
  This is why B is coded in **two** `/code` runs with the measurement between
  them (see `## Parallelität`).
- **R-20 — the store's layout invariant is pinned, not assumed.** R-7's
  `HashMap<DoctorReviewRowId, u32>` is only valid while the boxed row objects
  occupy store indices `0..n` and the conflicts panel is the **last** item.
  Today that holds and was read: `refresh()` writes exactly the boxed rows with
  `store.splice(0, n_items, &objects)` (`review_page.rs:80`), `refresh_conflicts`
  appends the panel afterwards (`store.append(&panel.root)`, `:239`), and
  `compare_rows` (`:346-368`) returns `Ordering::Larger` for a non-boxed left
  against a boxed right (`:349-355`), so the panel is last in the sorted view
  too. The incremental path therefore (a) carries a `debug_assert!` that every
  splice index is `< snapshot.rows.len()` and that
  `store.n_items() == rows + panel_present`, (b) **never** touches index `n`, and
  (c) is pinned by a behavioural test (§F-3), not by a source-text assertion.
- **R-21 — `MAX_TOGGLE_CHURN` is a measured number, never a predicted one.** The
  probe of §F-4(a) ships with a placeholder that the implementing agent replaces
  with the value it measured, and with the measurement — commit, fixture size,
  observed total — in a comment next to the constant. The draft's arithmetic
  (`2 × 193 = 386` before, `2 × 12 + 2 = 26` after) is an expectation about the
  *shape* of the number and is explicitly **not** a budget; committing it
  unmeasured is forbidden. A plan that ships a guessed budget ships a budget
  nobody can trust, and `scripts/check-frontend-thinness.sh:1-12` already states
  that principle for the whole repo.

---

## C. Task 1 — the album header stops lying (A-1) · strand A

Files: `review_header.rs`, `docs/ux-rules.md`,
`crates/reprise-gnome/src/ui/strings_library_doctor.rs`,
`review_page_tests.rs`.

### C-1. Extract the header's decision, then apply all of it

Add next to `master_check_state` (`review_header.rs:19-25`):

```rust
pub(super) struct AlbumHeaderState {
    pub(super) check: MasterCheckState,
    pub(super) pill: String,
    /// `Some` exactly when the checkbox is insensitive: why it is.
    pub(super) reason: Option<String>,
}

pub(super) fn album_header_state(
    selected: usize,
    selectable: usize,
    changes: usize,
    blocked_by: Option<DoctorReviewRowState>,
) -> AlbumHeaderState
```

- `check = master_check_state(selected, selectable)`
- `selectable > 0` → `pill = album_change_count(selectable, selected)` (today's
  wording, `review_header.rs:283-289`), `reason = None`. This path must not move
  one character: `doc_9b_a_fully_deselected_album_says_none_selected`
  (`review_header.rs:339-343`) pins `album_change_count(2, 0)` and
  `album_change_count(2, 1)` exactly.
- `selectable == 0` → the pill carries **both facts** (R-2):
  `pill = strings::doctor_change_count_out_of_date(changes)` for
  `Some(Stale)`/`None`, `strings::doctor_change_count_unresolved(changes)` for
  `Some(Conflict)`; `reason = Some(strings::text(row_state_reason(state)))` with
  the same state (§D-1 owns `row_state_reason`, so Task 1 and Task 2 share one
  vocabulary).

`changes` is the album's inventory: the sum of `row.row_ids.len()` over the
section's rows, i.e. written changes, not display rows — the unit
`doc_9b_the_album_pill_counts_written_changes_not_display_rows`
(`review_page_tests.rs:428-441`) already measures. `blocked_by` is `Some(Stale)`
if any row of the section is `Stale`, else `Some(Conflict)` if any is
`Conflict`, else `None`; `Stale` wins a mix because the rescan is the remedy the
banner already names.

In `bind_album_header` (`review_header.rs:203-215`) then:

```rust
let state = album_header_state(selected, total, changes, blocked_by);
checkbox.set_active(state.check.active);
checkbox.set_inconsistent(state.check.inconsistent);
checkbox.set_sensitive(state.check.sensitive);          // R-1
```

and the pill (`review_header.rs:250-255`) takes `state.pill`. When
`state.reason` is `Some`, set it as the tooltip of the header **root**
(`review_header.rs:261-280`, root is sensitive) and append it to the root's
accessible label (`:276-279`); when `None`, clear the tooltip — a recycled
header must not keep a stale reason (this matters once strand B reuses header
widgets; do it now so the behaviour is already correct).

Keep the `// a11y-semantics:` marker adjacent to `set_focusable(true)`
(`review_header.rs:212-213`) — `scripts/check-accessibility-semantics.sh:12-24`
reads the line directly above every `set_focusable(true)` in `src/ui`.

### C-2. Strings

Two plain consts next to `DOCTOR_STATUS_STALE` (`strings_library_doctor.rs:84`):

```rust
pub const DOCTOR_ROW_STALE_REASON: &str =
    N_!("This file changed after the scan — scan again to include this fix.");
pub const DOCTOR_ROW_CONFLICT_REASON: &str =
    N_!("The spelling for this album is still unresolved — pick one below.");
```

The pill carries a **number**, so it is not a const. It follows the exact shape
of `doctor_change_count_none_selected` (`strings_library_doctor.rs:420-429`) —
a `pub fn` over `plural()` with the two literals passed **bare**:

```rust
pub fn doctor_change_count_out_of_date(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} change · out of date",
        "{count} changes · out of date",
        count,
        &[("count", &count_text)],
    )
}

pub fn doctor_change_count_unresolved(count: usize) -> String { /* same shape */ }
```

`plural()` arguments are deliberately **not** wrapped in `N_!` — the file's own
comment (`strings_library_doctor.rs:8-17`) records the measurement: with the
wrapper xgettext emits two dead singulars and no `msgid_plural`, so the string
falls back to English no matter how well the catalog is translated. A bare
`const` here would be the same bug in a different disguise.

*Rejected: one function with the clause as an argument* — gluing translated
fragments. *Rejected: the stale wording for both cases* — an album blocked only
by conflicts is not "out of date"; that is a new lie replacing the old one.

`strings_library_doctor.rs` is already listed in `po/POTFILES.in:5`, so
extraction needs no gate change (the hardcoded POTFILES check at
`scripts/check-architecture.sh:457-465` covers four other string files and does
not need touching). Wording note: the banner says *"changed after the scan"*
(`strings_library_doctor.rs:377-385`); the row reason repeats that clause
verbatim so the two surfaces are recognisably the same fact.

### C-3. Rules

- DOC-3c (`docs/ux-rules.md:4078-4085`): extend the sensitivity clause from the
  master checkbox to **every** checkbox that stands for a set of rows — the
  master and each album header — and add that an insensitive one names its
  reason in text, not only on hover. Note the amendment and its date in the
  rule body, house style.
- DOC-9b (`:4435-4470`): the album header's change count states the album's
  inventory and, when nothing in it is selectable, the reason next to that
  count. Add the new test names to its `*Tests:*` list.

---

## D. Task 2 — the refused row says why, and costs nothing (A-2, A-4) · strand A

Files: `review_model.rs`, `review_row.rs`, `strings_library_doctor.rs`,
`review_row_contract_tests.rs`, one hunk in `review_page.rs`,
`review_page_tests.rs`, `docs/ux-rules.md`.

### D-1. One label function per state, mirroring `outcome_label`

In `review_model.rs`, next to `outcome_label` (`:363-372`):

```rust
pub(super) const fn row_state_label(state: DoctorReviewRowState) -> Option<&'static str> {
    match state {
        DoctorReviewRowState::Ready => None,
        DoctorReviewRowState::Stale => Some(strings::DOCTOR_STATUS_STALE),
        DoctorReviewRowState::Conflict => Some(strings::DOCTOR_STATUS_CONFLICT),
    }
}

pub(super) const fn row_state_reason(state: DoctorReviewRowState) -> Option<&'static str>
```

`Stale` reuses `DOCTOR_STATUS_STALE` (`strings_library_doctor.rs:84`) — the same
word `outcome_label` already prints for `DoctorWriteRowState::Unavailable`
(`review_model.rs:369`), so the pre-write and post-write surfaces agree instead
of inventing a second vocabulary.

### D-2. Render it through the Source column

`review_row.rs:253-265` composes the Source text today and `:266-269` renders
it. Change the shape from "confidence, plus the write outcome if any" to
"confidence, plus the row's state if it is not `Ready`, plus the write outcome
if any", joined with the page's `·` separator. Concretely, when `model.outcome`
is `None` and `row_state_label(model.row.state)` is `Some(label)`, the Source
cell reads `"MusicBrainz · 90% · Stale"`. The existing `outcome` branch keeps
precedence: after a write the outcome is the newer fact.

Then, still inside `bind` (`review_row.rs:232-311`):

- the **row root** gets `row_state_reason(...)` as its tooltip when the state is
  not `Ready`, and `set_tooltip_text(None)` when it is — the root stays
  sensitive, the checkbox does not (R-2/R-3).
- `ReviewRowModel::accessible_description` (`review_model.rs:107-122`) appends
  the same reason, next to where it already appends `outcome.error`.

Do **not** touch `row_selectable` (`review_model.rs:355-361`) or
`review_row.rs:239`. The checkbox stays insensitive; that is the correct
behaviour and the whole point.

### D-3. Row activation stops paying for a refusal (R-17)

`toggle_position` (`review_page.rs:149-159`) becomes:

```rust
let model = boxed.borrow::<ReviewRowModel>();
if model.selectable_row_ids.is_empty() {
    return;
}
self.set_selected(&model.selectable_row_ids, !model.row.selected);
```

That is the whole change and it is confined to that function body — strand A's
only hunk in `review_page.rs`. Note the two effects: the `RowNotReady` warnings
per id disappear (the core is no longer asked), and the `refresh()` behind the
refusal disappears with them. `set_selected`'s own warning at
`review_page.rs:141-143` stays — it is still the right thing to log if a
selectable id is ever refused.

### D-4. Rules

DOC-9b gains one clause: a row the page refuses to select names the reason in
its Source cell and in its accessible description, activating it changes
nothing and costs nothing, and the page-level banner stays as the aggregate.
DOC-4c/DOC-8b keep owning *why* such rows exist — no change there.

---

## E. Task 3 — one toggle updates the rows it toggled (A-3) · strand B

Files: `review_page.rs`, new `review_snapshot.rs`, `review_header.rs`,
`review_conflicts.rs`, new `review_page_perf_tests.rs`, new
`review_refresh_tests.rs`, `review_page_tests.rs`.

### E-0. The measurement gate comes first (R-19)

Commit 1 of strand B adds **only** measurement: R-14's per-stage and whole-path
`tracing::debug!` lines in `refresh()`, the churn probe and the opt-in
wall-clock probe of §F-4(a)/(b) with their measured pre-fix constants (R-21).
Then §F-4(c) is run on that commit, on the real library. The recorded profile
selects the depth of the rest of B by R-19's rule. Nothing below E-0 may be
coded before that profile exists and is written into the strand file.

### E-1. `review_snapshot.rs` — the cached truth

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
§C-1's `changes` argument comes from the snapshot once the header is pushed
(R-9) instead of being summed per bind.

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

### E-2. Two paths in `ReviewState`

```rust
fn refresh(self: &Rc<Self>)                                  // structural, as today
fn apply_selection(self: &Rc<Self>, session_changed: bool)   // selection only
```

`apply_selection`:

0. `if self.full_refresh_only { return self.refresh(); }` — R-18's control arm.
1. `let changed = snapshot.selection_diff(&session)` — bail out early when it is
   empty (re-checking an already-checked album must cost nothing).
2. for each changed row, `store.splice(index, 1, &[new_boxed_object])`; adjacent
   indices are coalesced into one `splice` per run. Every index is
   `debug_assert!`-ed against R-20's invariant; index `n` is never written.
3. **no** `filter.changed()` (R-11), **no** `refresh_conflicts()` (R-12), **no**
   `store.splice(0, n, …)`.
4. replace the cached snapshot, then push the new state to the top select-all
   (the existing `refresh_master_check` body, now reading `totals`) and to the
   realized album headers of the affected albums (E-3), through the `binding`
   guard (R-10).
5. one `tracing::debug!` (R-14) naming `path="selection"`, `touched`,
   `elapsed_us`.

`set_selected` (`review_page.rs:138-147`) calls `apply_selection` instead of
`refresh`. So does the select-all handler (`review_page.rs:534-548`) — for
`all()`/`none()` the diff legitimately covers every row, so that case costs what
it costs today; it is bounded by "the rows that actually changed", which is the
honest budget. Everything listed in R-6 keeps calling `refresh()`.

`refresh()` itself loses the two `visible_rows()` passes (R-8), keeps the
selection-position restore (`review_page.rs:86-88`), and keeps the per-stage and
whole-path lines from E-0 with `path="full"`.

### E-3. The header stops pulling from a half-updated model

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
  `album_key` in the snapshot's `albums` map** (R-9) — never summed from
  `header.start()..header.end()`.
- `set_active`/`set_inconsistent`/`set_sensitive` are wrapped in the `binding`
  guard on **both** the bind and the push path (R-10), and the `toggled` handler
  reads the currently bound album's `row_ids` from the registry entry instead of
  capturing a fresh `Vec` per bind.
- a section whose first item is not a `glib::BoxedAnyObject` (the conflicts
  panel) clears the header child and does **not** warn (R-12).
- the invalid-bounds early return stays, and keeps its `DOC-9b` warning
  (`review_header.rs:184-188`) — it is still the right thing to log when GTK
  asks for a header outside the model.

`apply_selection` pushes to this registry: for each affected `album_key`, look up
the realized header, if any, and re-apply the check state. This is what removes
the "header rebinds against a half-updated model" class entirely — after strand
B the header never reads the model to decide its own state.

### E-4. The conflicts panel

`refresh_conflicts` (`review_page.rs:206-240`) computes the fingerprint from the
category-matching groups and returns early when it equals the cached one. Its
`store.append` (`:239`) becomes append-or-replace at a tracked index, so the
panel survives a full `refresh()` that did not change the groups. The early
return on `groups.is_empty()` (`:216-218`) must additionally **remove** a panel
that is no longer wanted — today it cannot leak because the whole store is
spliced away, and that crutch is gone.

**Do not delete the literal `self.store.append(&panel.root);`.** Two existing
unit tests read the source text of `review_page.rs` with `include_str!` and
assert it contains `"store.append(&panel.root)"`:
`doc_9b_conflicts_sit_at_the_end_and_skip_all_clears_them`
(`review_page_tests.rs:443-456`) and
`doc_9b_the_conflicts_panel_is_the_last_row_of_the_scrolled_list` (`:459-464`).
Keep `append` as the "panel not present yet" branch and use
`splice(index, 1, &[panel.root])` only for the replace branch; both tests then
stay green and stay honest. If a future shape makes that impossible, the tests
must be retargeted to behaviour in the same commit, with the reason in the
commit message — never silently weakened.

---

## F. Verification. A green suite is not the deliverable.

### F-0. How to run anything at all in this crate

- `reprise-gnome` has **no `--lib` target**. Unit tests:
  `cargo test -p reprise-gnome --bin reprise library_doctor::review`.
- Display/GTK tests are the `#[ignore = "requires a display; run via xvfb-run"]`
  ones and must run **one exact test per process** — GTK initialises once per
  thread and `--test-threads=1` still migrates between harness threads
  (`TESTING.md:296-303`). Use `scripts/check-display-tests.sh`. While iterating
  on a single test, run that one exact filter in its own process.
- `cargo test --exact` with a wrong module path **runs nothing and exits 0**.
  Read the run's own accounting: `grep -c '^test result: FAILED' run.log` and a
  positive `N passed`. A summary line alone proves nothing.
- Display tests are flaky as a herd and several are **already red on
  `origin/dev`**. Before blaming this branch, run the same test on
  `051fb088df` and say which failures pre-date the change.
- The tests this plan adds assert sensitivity, text and tooltips — **not
  geometry** — so they do not need the app CSS. Any test that measures a
  widget's allocation must first call
  `crate::ui::style::install_css_string_for_test(&super::super::css())`
  (`ui/style/mod.rs:177`, `library_doctor/mod.rs:97`), or the measurement is
  meaningless.
- Redirect long output to a file and answer the question with `grep`/`wc`; do
  not read whole logs back.

Gates to run before landing either strand:
`cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test -p reprise-core`, `cargo test -p reprise-gnome`,
`scripts/check-display-tests.sh`, `scripts/check-ux-traceability.sh`,
`scripts/check-accessibility-semantics.sh`, `scripts/check-architecture.sh`,
`scripts/check-frontend-thinness.sh`, `scripts/check-input-parity.sh`.

### F-1. A-1 — a test that fails before the fix

The existing unit test `doc_3c_the_master_check_mirrors_the_visible_selection`
(`review_header.rs:302-337`) already asserts `sensitive: false` at `(0, 0)` and
**passes today** — measured on `051fb088df`:
`cargo test -p reprise-gnome --bin reprise library_doctor::review` →
`test result: ok. 29 passed; 0 failed; 10 ignored`, that test among the 29. The
defect is in the *binding*, so the regression test has to bind.

New fixture in `review_page_tests.rs`, next to `ready_and_stale_scan`
(`:150-163`): `stale_album_scan()` — two albums, album A with one `Ready` row,
album B with every row `Stale`. Album A is the control arm: without a row that
stays selectable, a test that finds "no sensitive checkbox" would also pass on a
page that renders nothing.

New display test, `#[ignore = "requires a display; run via xvfb-run"]`,
`doc_3c_an_album_with_nothing_selectable_binds_an_insensitive_header_check`:
build the page as `doc_9b_the_first_row_carries_its_album_header`
(`review_page_tests.rs:578-611`) does, pump the main context, then for each
realized header root (`doctor-album-header-first` / `-later`, via
`descendants_with_css_class`, `:519-533`) identify the album from its labels
(`descendant_label_text`, `:738-752`) and assert:

- album A's `CheckButton` is sensitive;
- album B's `CheckButton` is **not** sensitive — this is the assertion that
  fails on `051fb088df`;
- album B's pill contains **both** its inventory count and the reason clause,
  and **not** `"0 changes"`;
- album B's header root tooltip is the reason sentence.

Plus a cheap unit test `doc_3c_album_header_state_names_the_reason_at_zero`
over the pure `album_header_state` — fast feedback, not the proof.

### F-2. A-2 — the *rendered* row, not the model

In `review_row_contract_tests.rs` (already a child module of `review_row`, so
`bind` and `build_row` are reachable — `:1-7`), add a literal `ReviewRowModel`
fixture; every field is `pub(super)` (`review_model.rs:85-103`) and
`DoctorReviewRowId::from_raw` (`review.rs:17-19`) is public, so no database is
needed.

`doc_9b_a_stale_row_names_its_reason_where_the_click_happens`,
`#[ignore = "requires a display; run via xvfb-run"]`:

1. bind a `Stale` model → `widgets.source.text()` contains the stale label,
   `widgets.root.tooltip_text()` is the reason sentence,
   `widgets.selected.is_sensitive()` is false, and
   `model.accessible_description()` contains the reason.
2. bind a `Ready` model into the **same** widgets → the reason is gone from both
   the Source text and the tooltip. Recycling is where this class of bug hides.

### F-2b. A-4 — the dead activation, measured as churn

`doc_9b_activating_an_unselectable_row_selects_nothing`,
`#[ignore = "requires a display; run via xvfb-run"]`, in
`review_page_tests.rs`, on the `stale_album_scan()` fixture:

- connect a counter to `page.state.store.connect_items_changed` summing
  `removed + added` (the field is private but `review_page_tests` is a child
  module of `review_page`, `review_page.rs:641`);
- call `page.state.toggle_position(p)` for a stale row's position;
- assert the counter is **0** and the session's selection is unchanged.

This fails on `051fb088df` — today the refusal is followed by
`refresh()`'s full `store.splice(0, n, …)` (`review_page.rs:80`), so the counter
is ~`2n`. Add a second half on a `Ready` row asserting the activation *does*
flip the selection, so the test cannot pass by doing nothing at all.

### F-3. R-9's and R-20's premises, pinned

- `doc_9b_the_snapshot_is_the_visible_row_set`: with and without a category
  filter, `sorted.n_items()` equals `snapshot.rows.len()` plus the conflicts
  panel when one is present. If `group_review_rows`' filtering
  (`grouping.rs:111`) and the `CustomFilter` (`review_page.rs:419-427`) ever
  diverge, this test says so instead of the header quietly counting the wrong
  set.
- `doc_9b_the_conflicts_panel_stays_the_last_store_item` (display, behavioural —
  **not** an `include_str!` assertion): on a scan with conflicts, every store
  item at `0..n` downcasts to `glib::BoxedAnyObject`, item `n` downcasts to
  `gtk4::Widget`, and both statements still hold after one selection toggle.
  This is R-20's pin; the `debug_assert!`s are the second line of defence, not
  the first.

### F-4. A-3 — the control arm, four measurements

**(a) Churn, deterministic, in the merge gate.** New file
`review_page_perf_tests.rs`, declared as a child module of `review_page` next to
the existing `mod tests;` (`review_page.rs:641`),
`#[ignore = "requires a display; run via xvfb-run"]`, name without a rule prefix
(R-16): `review_selection_toggle_touches_only_the_toggled_album`.

- fixture: 16 albums × 12 rows = 192 rows, generated;
- connect to `state.store.connect_items_changed` and sum `removed + added`;
- toggle one album's rows through `state.set_selected(...)`;
- assert the churn is at most `MAX_TOGGLE_CHURN`, a named constant;
- assert correctness in the same test: the toggled album's header counts are
  right, every other album's are unchanged, and `sorted.n_items()` is unchanged.

**`MAX_TOGGLE_CHURN` is the control arm, and it is measured (R-21).** Commit 1
of strand B commits the number it *observed* on the unmodified path, with the
measurement in a comment:

```rust
/// Measured on <commit> with the 16 × 12 fixture: <observed> items changed for
/// one album toggle (one full splice of the whole store).
const MAX_TOGGLE_CHURN: u32 = /* measure — do not predict */;
```

The commit that makes the update incremental **lowers it in the same change**
and records both numbers. A budget nobody lowers is a budget nobody believes.

**(b) Wall clock, synthetic, opt-in — both paths, one build.**
`review_selection_toggle_wall_clock_probe`, ignored, returns immediately unless
`REPRISE_DOCTOR_PERF_ALBUMS` is set (R-16 — otherwise it joins the merge gate,
which runs every ignored test). With it set: build `N` albums × 12 rows and time
nine single-album toggles with `Instant`, printing median and max. Once
`apply_selection` exists, the same probe times **both** paths in the same
process by flipping the R-18 field between runs, so the comparison carries no
build-to-build drift. Both numbers go into the PR body.

**(c) Wall clock, the real library — the measurement the report is about.**
The synthetic arm cannot reproduce a 254 MB database. Procedure, with R-14's log
lines and R-18's switch in place:

1. copy the real database including its `-wal` and `-shm` sidecars after a clean
   app shutdown — a copy without the sidecars is a *different* database;
2. run the app against an isolated XDG profile pointed at the copy, with
   `REPRISE_LOG=debug`, `REPRISE_AUDIO_SINK=fakesink`;
3. open Library Doctor → Review, uncheck and re-check the **same** album five
   times with `REPRISE_DOCTOR_FULL_REFRESH=1` (control), then five times without
   it (fix) — same app run where the build has both paths, same album, same
   window size, same order; quit;
4. from the log: per-stage medians (R-14), whole-path medians for both arms, and
   the ratio; plus `grep -c 'DOC-9b'` for both halves (§F-4(d));
5. the profiling run on B's commit 1 (E-0) has only one path and yields the
   pre-fix per-stage profile. Cross-check it against the finished branch's
   `REPRISE_DOCTOR_FULL_REFRESH=1` arm: if the two medians differ by more than
   session noise, the "control" arm is **not** the pre-fix path and the switch is
   wrong. Say so rather than reporting the ratio.

**Division of labour: the user drives the GUI run; the session evaluates the
log.** Codex does not run display tests and must not drive the real app; the
implementing agent produces the build, the exact command lines and the grep
recipe, and the user performs the five-cycle interaction. Report both medians and
the ratio. "Feels fast" is not a result. If the control arm does not reproduce
the reported slowness, say so — that is a finding about the diagnosis, not a
licence to skip the measurement.

**(d) The `DOC-9b` warnings.** In the same session, `grep -c 'DOC-9b' run.log`
per half. The control half is expected to show the `start=4294967295` entries;
the fix half must show none from a selection toggle. This is the one check that
cannot be done from a test.

### F-5. New tests owed, in one list

| Test | Level | Strand |
| --- | --- | --- |
| `doc_3c_album_header_state_names_the_reason_at_zero` | unit | A |
| `doc_3c_an_album_with_nothing_selectable_binds_an_insensitive_header_check` | display | A |
| `doc_9b_a_stale_row_names_its_reason_where_the_click_happens` | display | A |
| `doc_9b_activating_an_unselectable_row_selects_nothing` | display | A |
| `review_selection_toggle_touches_only_the_toggled_album` | display | B (commit 1, budget lowered later) |
| `review_selection_toggle_wall_clock_probe` | opt-in | B (commit 1, second path later) |
| `doc_9b_the_snapshot_is_the_visible_row_set` | unit | B |
| `doc_9b_the_conflicts_panel_stays_the_last_store_item` | display | B (R-20) |
| `doc_9b_a_rebound_album_header_does_not_emit_a_selection_change` | display | B (R-10, **required**, covers bind *and* push) |
| `doc_9b_the_conflicts_section_binds_no_album_header` | display | B (R-12) |

Existing tests that must keep passing untouched, because they encode the
contracts this plan works inside:
`doc_3c_the_master_check_mirrors_the_visible_selection`
(`review_header.rs:302-337`),
`doc_9b_a_fully_deselected_album_says_none_selected` (`review_header.rs:339-343`),
`doc_9d_the_header_counts_the_inventory_while_the_footer_counts_the_selection`
(`review_page_tests.rs:339-356`),
`doc_9b_every_section_boundary_binds_a_non_empty_header` (`:613-696`),
`doc_9b_the_conflicts_panel_covers_no_row` (`:466-513`),
`doc_9b_the_album_pill_counts_written_changes_not_display_rows` (`:428-441`),
`doc_9b_conflicts_sit_at_the_end_and_skip_all_clears_them` (`:443-456`),
`doc_9b_the_conflicts_panel_is_the_last_row_of_the_scrolled_list` (`:459-464`).
If strand B has to adjust `doc_9b_every_section_boundary_binds_a_non_empty_header`
because headers are pushed rather than pulled, the adjustment must keep its
assertion — *every* section boundary carries a header naming its album — and say
in the commit why the mechanism changed.

---

## G. Scope fence

This plan does **not** touch:

- the Doctor's summary/result surface, the running page, the start page, the
  post-apply page, the sidebar job card or the `ISSUES` block. That is
  `docs/plans/library-doctor-fix-round-2.md` (phase: planned) — nine defects on
  the **summary** page. There is no file overlap: that plan owns
  `summary_*.rs`, `result_pages.rs`, `running_page.rs`, `progress_card.rs`,
  `sidebar/*`, and it explicitly fences off "the review page" (its §J at
  `:555`, the fence sentence at `:558`). This plan owns only `review_*.rs` and
  stays out of `summary_*.rs`, `mod.rs` and `strings` entries owned there. The
  one shared file is `strings_library_doctor.rs`; this plan only **adds** two
  constants and two plural functions (§C-2) and renames nothing, so the two
  plans can land in either order.
- the Doctor page's window chrome / header bar. That is
  `docs/plans/doctor-progress-and-window-chrome.md` (phase: coded), whose edits
  to `review_page.rs` concern the page's navigation chrome, not its list.
- `reprise-core`: no change to `review.rs`, `grouping.rs`, `scan.rs`,
  `local_rules.rs`, `presentation.rs`. In particular the O(tracks·albums) seed
  search (`grouping.rs:83`) and the per-album `session.rows()` scan
  (`grouping.rs:108-113`) are **left alone** — strand B makes the page stop
  calling them on a selection change, which is the fix the report asks for.
  Making the grouping itself linear is a separate, core-side plan.
- the scan engine, the MusicBrainz/AcoustID clients, the remote cache, the
  tag-write jobs, `reprise-mcp`, the Android app.
- `DoctorReviewRowState`, `DoctorReviewError`, and the refusal at
  `review.rs:468-470` (R-5).
- turning `ReviewRowModel` into a `GObject` (R-7). If a future round wants
  property-bound rows, this plan's snapshot is the thing it replaces.

---

## H. The cut — two strands, and why they are not concurrent

An earlier draft cut this work three ways: a measurement strand P concurrent
with the fix strand A, both feeding B. That cut is **withdrawn**. Reasons, in
order of weight:

1. **A and B are not disjoint, and no cut makes them so.** Task 1 rewrites
   `bind_album_header` (`review_header.rs:182-281`); strand B rewrites that same
   function into a setup/bind pair with a registry. Both add tests to
   `review_page_tests.rs`. Concurrency here means a guaranteed conflict in the
   one file that carries the defect.
2. **P's only payoff was a number, and B can measure it itself.** The pre-fix
   churn and the pre-fix per-stage profile are measured in B's own first commit,
   at B's own branch point (E-0). Strand A does not touch the update path — its
   `review_page.rs` hunk is `toggle_position`'s body, which only decides *which
   ids* are passed to `set_selected`, not what `set_selected` costs — so the
   pre-fix numbers B measures after A has landed are the same numbers P would
   have measured before it.
3. **P's repair belonged to no strand.** The draft's own cross-check said "P
   cannot compile against A's tree". That reason was checked and does **not**
   hold: A adds `album_header_state`, `row_state_label` and `row_state_reason`
   and edits function bodies, but changes no signature a probe would call, and
   the fixtures build `DoctorScan` values and go through `grouped_rows_for`
   rather than constructing `ReviewRowModel` literals. The hazard survives as
   the **lesson**, not as the reason: *a cut whose repair belongs to no strand is
   not a cut*. That cross-check is dropped from §J.

So: **A → B, strictly sequential.**

### Strand A — A-1, A-2, A-4 · `docs/plans/doctor-review-selection-and-refresh-a.md`

**Purpose:** the checkbox stops undoing itself; the refused row says why; the
dead activation costs nothing.

**Owns:**
- `crates/reprise-gnome/src/ui/library_doctor/review_header.rs`
- `crates/reprise-gnome/src/ui/library_doctor/review_row.rs`
- `crates/reprise-gnome/src/ui/library_doctor/review_model.rs`
- `crates/reprise-gnome/src/ui/library_doctor/review_page_tests.rs`
- `crates/reprise-gnome/src/ui/library_doctor/review_row_contract_tests.rs`
- `crates/reprise-gnome/src/ui/strings_library_doctor.rs`
- `docs/ux-rules.md`
- **exactly one hunk** in
  `crates/reprise-gnome/src/ui/library_doctor/review_page.rs`: the body of
  `toggle_position` (`:149-159`, §D-3). Harmless, because B is coded only after
  A has landed — B inherits the fixed body and never sees a conflict.

**Tasks:** §C-1, §C-2, §C-3, §D-1, §D-2, §D-3, §D-4; tests §F-1, §F-2, §F-2b.

### Strand B — A-3 · `docs/plans/doctor-review-selection-and-refresh-b.md`

**Purpose:** a selection change costs the rows it changed.

**Owns (all of it, after A has landed):**
- `crates/reprise-gnome/src/ui/library_doctor/review_page.rs`
- `crates/reprise-gnome/src/ui/library_doctor/review_snapshot.rs` (new)
- `crates/reprise-gnome/src/ui/library_doctor/review_conflicts.rs`
- `crates/reprise-gnome/src/ui/library_doctor/review_page_perf_tests.rs` (new)
- `crates/reprise-gnome/src/ui/library_doctor/review_refresh_tests.rs` (new)
- **inherited after A merges:** `review_header.rs`, `review_page_tests.rs`,
  `strings_library_doctor.rs`, `docs/ux-rules.md`

**Tasks:** §E-0 first and alone, then §E-1 … §E-4 to the depth R-19's rule
selects; tests §F-3, §F-4(a)/(b) plus the budget lowering, the R-10/R-12/R-20
display tests, and §F-4(c)/(d) — both arms, one build, one session.

## I. Merge order

```
A ─→ B
```

- **A before B**, because both edit `review_header.rs` and
  `review_page_tests.rs`. If A is delayed, B waits — B must not "temporarily"
  fix the sensitivity itself; that would land A-1 twice with two different
  reasons in the history.
- **B branches from a `dev` that already contains A.** Fetch `origin/dev` again
  after A merges; do not branch B from A's branch tip.
- A is landable on its own. If B is cut short by R-19's cheaper outcome, that is
  a complete result, not a half-delivery.

## J. Post-merge cross-checks

Each of these needs a file or a channel the strand that caused it does not own.

1. **After A lands:** full `scripts/check-display-tests.sh` — unfiltered, which
   it now is (#463). A touches the row and header binding, which
   `doc_9b_the_conflicts_panel_covers_no_row` and
   `doc_9b_review_groups_render_one_header_per_album`
   (`review_page_tests.rs:556-577`) depend on.
2. **After A lands:** `scripts/check-ux-traceability.sh` — A amends
   `docs/ux-rules.md` and adds rule-named tests in two files; the gate resolves
   only across the whole tree.
3. **After B lands:** re-run A's
   `doc_3c_an_album_with_nothing_selectable_binds_an_insensitive_header_check`
   unchanged. B rewrites the exact binding A fixed; this test is the only thing
   that proves R-1 survived the move into the registry.
4. **After B lands:** re-run A's `doc_9b_a_stale_row_names_its_reason_…` — B's
   incremental path decides when a row is rebound at all, and a row that is
   never rebound never gets its reason.
5. **After B lands:** re-run A's
   `doc_9b_activating_an_unselectable_row_selects_nothing` — B replaces the
   update path that test measures churn on, and R-17's zero-churn claim must
   survive the replacement.
6. **After B lands:** the two `include_str!` tests of §E-4
   (`review_page_tests.rs:443-456`, `:459-464`). They live in A's file, they read
   B's file as text, and B's conflicts change is exactly what can break them.
7. **After B lands:** `scripts/check-architecture.sh` — B moves code between
   `review_page.rs` (641 lines today) and the new `review_snapshot.rs`; the
   800-line cap (`check-architecture.sh:20`) is measured over the whole tree.
8. **After B lands:** §F-4(c)'s paired run and §F-4(d)'s warning count. Neither
   is a test; both need the real library, the user at the GUI and the session at
   the log.
9. **Nobody but the integrator** can check that A's `review_header.rs` wording
   and B's `review_snapshot.rs` still agree on what "the visible row set" means —
   §F-3's test is the mechanised half; the DOC-3c wording is the other half and
   has to be read once with both diffs in hand.

### J. Result — all nine done, 15.08.2026

1–2 were run when A landed (#478). 3–9 were run against the merged `dev`
(`21a32c5fc3`, B as #505), in a worktree cut from that commit:

| # | Evidence |
|---|---|
| 3 | `…review_page::tests::doc_3c_an_album_with_nothing_selectable_binds_an_insensitive_header_check` — `running 1 test`, ok |
| 4 | `…review_row::contract_tests::doc_9b_a_stale_row_names_its_reason_where_the_click_happens` — `running 1 test`, ok |
| 5 | `…review_page::tests::doc_9b_activating_an_unselectable_row_selects_nothing` — `running 1 test`, ok |
| 6 | Both §E-4 `include_str!` tests are plain `#[test]` and ran inside the suite: **1874 passed, 0 failed, 716 ignored** |
| 7 | `scripts/check-architecture.sh` — passed; `review_page.rs` was at 795 of 800 |
| 8 | §F-4(c)/(d) were measured on the identical tree before the merge and are recorded in `-b.md`: median 248 → 13.6 ms, twelve toggles monotonic → flat, `DOC-9b` 356 → 66 |
| 9 | Read with both diffs in hand — see below |

**On 3–5, the balance sheet is not the evidence.** The first attempt passed the
bare function name to `cargo test … --exact`, which matches nothing: each run
reported `0 passed; 2590 filtered out`, `ok`, and exit 0. Only the full test
path executes anything. Every row above was confirmed by `running 1 test`.

**On 9:** they agree, and the check is not vacuous. `ReviewSnapshot::from_rows`
is fed exactly `grouped_rows_for(…)` (`review_page.rs:85`) — the visible,
category-filtered set that is also what gets spliced into the store, which is
what DOC-3c's "mirrors the visible selection" requires. The one thing that
looked wrong is not: `AlbumCounts::changes` counts `row_ids`, `ReviewTotals::changes`
counts `selectable_row_ids`. Two meanings under one field name, but each matches
its consumer's pre-B computation exactly — the album header summed `row_ids`
(dev `review_header.rs:237`) and the filter bar summed `selectable_row_ids`
(dev `review_page.rs:337`).

---

## Parallelität

**There is no concurrency in this plan, and that is the honest outcome of the
attempted cut.** Two strands, `merge_order: a,b`, strictly sequential.

- `/code` is run **twice**, not fanned out: **A first, landed, then B**. B is
  coded against a `dev` that already contains A. Coding B against a `dev`
  without A would conflict in `review_header.rs` with certainty (§H-1) — both
  strands rewrite `bind_album_header`.
- Within B, `/code` is run **twice again** (§E-0, R-19): commit 1 is
  instrumentation and probes only, then the real-library measurement of
  §F-4(c) happens, and only then is the rest of B coded — to the depth the
  recorded profile selects. The measurement sits *between* two coding runs
  because its result changes what gets coded.
- The one genuinely parallel piece the draft found — a measurement strand in its
  own new file — was withdrawn (§H-2, §H-3): its payoff was a number B can
  measure itself, and its integration cost belonged to no strand.
- Total serialisation cost is accepted deliberately. A cut that produces a
  guaranteed conflict in the file carrying the defect is not a cut, and a plan
  that pretends otherwise buys a merge conflict with a false schedule.
