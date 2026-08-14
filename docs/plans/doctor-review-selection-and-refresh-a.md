---
slug: doctor-review-selection-and-refresh-a
worktree: /home/marvin/Projects/reprise-doctor-review-selection-and-refresh-a
branch: feature/doctor-review-selection-and-refresh-a
phase: shipped
codex_session:
created: 2026-08-14
---
# Strand A — the review page stops lying about what it refuses

Mother plan: `docs/plans/doctor-review-selection-and-refresh.md`. Read it for
the full diagnosis (§A), the complete decision list (§B), the scope fence (§G),
the merge order (§I) and the post-merge cross-checks (§J). This file is the
strand-local body: what A implements, what A owns, and how A is proved.

Base: branch from `origin/dev` **after a fetch**. The references below were read
in `051fb088df`; `origin/dev` has since moved to `5721ade95e` with no change
under `crates/reprise-gnome/src/ui/library_doctor/` or in
`strings_library_doctor.rs`, so they hold on the current head. Verify with
`git diff 051fb088df origin/dev -- crates/reprise-gnome/src/ui/library_doctor/`
before starting; if that diff is no longer empty, re-read the cited ranges.

**A is landable on its own.** Strand B (the update-path rework,
`docs/plans/doctor-review-selection-and-refresh-b.md`) is coded only after A has
merged and must not be anticipated here.

---

## Why this strand exists — the three defects it fixes

**A-1 — the album header checkbox silently undoes itself.**
`master_check_state` (`review_header.rs:19-25`) returns `active`,
`inconsistent` **and** `sensitive`. `refresh_master_check`
(`review_page.rs:113-136`) applies all three (`:128`, `:129`, `:130-131`);
`bind_album_header` (`review_header.rs:203-215`) applies only the first two
(`:207`, `:208`) and **never** `sensitive`. For an album whose rows are all
`DoctorReviewRowState::Stale`, the header's `row_ids` is empty — it is built
from `selectable_row_ids` (`review_header.rs:194-197`), which `grouped_rows_for`
filters down to `Ready` rows only (`review_model.rs:192-200`). So `total == 0`,
`master_check_state` reports `sensitive: false`, nothing applies it, and the box
stays clickable. Clicking it runs
`connect_toggled` (`:215`) → `on_select(&[], true)` → `set_selected`
(`review_page.rs:138-147`, looping over zero ids) → `refresh()`
(`review_page.rs:57-100`) → `bind_album_header` → `set_active(false)`. Nothing
was attempted, nothing failed, the check mark disappears. Confirmed in the
journal of the running build: **no** `RowNotReady` warning (so
`session.set_selected` was never reached — `review_page.rs:141-143` would have
logged it), while the `DOC-9b` warning from `review_header.rs:184-188` fired 12
times with `start=4294967295 end=4294967295`.

Second lie on the same header: with `total == 0`, `album_change_count(0, 0)`
(`review_header.rs:283-289`) takes the `else` branch and the pill reads
**"0 changes"** (`strings_library_doctor.rs:591-599`) for an album that *has*
changes — all of them out of date.

**A-2 — a refused row does not say why.** A `Stale` row is excluded from
`selectable_row_ids` (`review_model.rs:192-200`), `row_selectable` is false
(`review_model.rs:355-361`), the checkbox is correctly insensitive
(`review_row.rs:239`), and the core would refuse the write anyway
(`reprise-core/src/library/library_doctor/review.rs:468-470`). The only
explanation anywhere is the page banner (`review_page.rs:311-319` →
`strings_library_doctor.rs:377-385`, next to `Scan again` at
`review_page.rs:450-453`). At the row itself: nothing. The Source column shows
only `confidence.label` (`review_row.rs:253-265`) until a *write* has happened,
because the status suffix is appended only when `model.outcome` is `Some`, and
before the first Apply `outcome` is always `None`.

**A-4 — a dead double-click costs a full rebuild.** `toggle_position`
(`review_page.rs:149-159`) is the activation path — the `ListView` is built with
`single_click_activate(false)` (`:441`) and `rows.connect_activate(…)` (`:525`)
is its only caller, so this is double-click and Enter. It passes
`model.row_ids` (`:158`), i.e. **all** ids including non-`Ready` ones. The core
refuses each one, one warning per id is logged (`:141-143`), nothing visible
changes, and `set_selected` still ends in `self.refresh()` (`:146`) — a full
regroup, a full splice, a full conflicts rebuild. The row checkbox already does
the right thing and passes `selectable_row_ids` (`review_row.rs:56`).

The core refusal (`review.rs:468-470`, inside `set_selected` at `:458-476`) is
**correct and stays**: writing a proposal derived from a file that changed after
the scan would be wrong. A changes only what the UI says about it and what the
refusal costs.

---

## Decisions this strand implements

Full text in the mother plan §B. The ones A is bound by:

- **R-1** — `bind_album_header` applies all three fields of `MasterCheckState`.
- **R-2** — a checkbox standing for an empty set states its reason in text
  **next to its count**: `"3 changes · out of date"`, never the count alone and
  never the reason alone. The count is the album's inventory; the reason is a
  second fact about it.
- **R-3** — the row states its reason in the Source column, through the channel
  `outcome` already uses, rendered by `set_full_text` (`review_row.rs:380-384`)
  so label, tooltip and accessible description move together.
- **R-4** — every non-`Ready` state gets a label, not just `Stale`;
  `DoctorReviewRowState` has three variants (`review.rs:86-90`).
- **R-5** — the core refusal stays untouched.
- **R-15** — no new UX rule ids: DOC-3c (`docs/ux-rules.md:4078-4085`) and
  DOC-9b (`:4435-4470`) are amended in place. Retarget, never delete.
- **R-17** — row activation offers only `selectable_row_ids` and returns early
  when that is empty.

---

## Tasks

### A-T1. `album_header_state` — extract the decision, then apply all of it

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

- `check = master_check_state(selected, selectable)`.
- `selectable > 0` → `pill = album_change_count(selectable, selected)` — today's
  wording (`review_header.rs:283-289`) — and `reason = None`. **This path must
  not move one character**: `doc_9b_a_fully_deselected_album_says_none_selected`
  (`review_header.rs:339-343`) pins `album_change_count(2, 0) ==
  "2 changes · none selected"` and `album_change_count(2, 1) == "1 change"`.
- `selectable == 0` → the pill carries both facts:
  `strings::doctor_change_count_out_of_date(changes)` for `Some(Stale)` or
  `None`, `strings::doctor_change_count_unresolved(changes)` for
  `Some(Conflict)`; `reason = Some(strings::text(row_state_reason(state)))`
  using the same state, so A-T1 and A-T3 share one vocabulary.

`changes` is the album's **inventory**: the sum of `row.row_ids.len()` over the
section's rows — written changes, not display rows, the unit
`doc_9b_the_album_pill_counts_written_changes_not_display_rows`
(`review_page_tests.rs:428-441`) already measures. `blocked_by` is `Some(Stale)`
if any row of the section is `Stale`, else `Some(Conflict)` if any is
`Conflict`, else `None`; `Stale` wins a mix because the rescan is the remedy the
banner already names.

In `bind_album_header` (`review_header.rs:203-215`):

```rust
let state = album_header_state(selected, total, changes, blocked_by);
checkbox.set_active(state.check.active);
checkbox.set_inconsistent(state.check.inconsistent);
checkbox.set_sensitive(state.check.sensitive);          // R-1
```

The pill (`review_header.rs:250-255`) takes `state.pill`. When `state.reason` is
`Some`, set it as the tooltip of the header **root** (`:261-280`; the root stays
sensitive, so the tooltip actually appears — GTK's default pick skips
insensitive widgets) and append it to the root's accessible label (`:276-279`).
When `None`, **clear** the tooltip: a recycled header must not keep a stale
reason. Do this now even though today's header is rebuilt per bind — strand B
starts reusing these widgets, and the behaviour has to be already correct.

Keep the `// a11y-semantics:` marker adjacent to `set_focusable(true)`
(`review_header.rs:212-213`): `scripts/check-accessibility-semantics.sh:12-24`
reads the line directly above every `set_focusable(true)` under `src/ui`.

### A-T2. Strings

Two plain consts next to `DOCTOR_STATUS_STALE` (`strings_library_doctor.rs:84`):

```rust
pub const DOCTOR_ROW_STALE_REASON: &str =
    N_!("This file changed after the scan — scan again to include this fix.");
pub const DOCTOR_ROW_CONFLICT_REASON: &str =
    N_!("The spelling for this album is still unresolved — pick one below.");
```

The pill carries a **number**, so it is not a const. Follow the exact shape of
`doctor_change_count_none_selected` (`strings_library_doctor.rs:420-429`) — a
`pub fn` over `plural()` with the two literals passed **bare**:

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
falls back to English however well the catalog is translated. A bare `const` for
a string with a number in it is the same bug in a different disguise.

`strings_library_doctor.rs` is already listed in `po/POTFILES.in:5`, so
extraction needs no gate change; the hardcoded POTFILES check at
`scripts/check-architecture.sh:457-465` covers four other string files and is
not touched.

Wording note: the banner says *"changed after the scan"*
(`strings_library_doctor.rs:377-385`); the row reason repeats that clause
verbatim so the two surfaces are recognisably the same fact.

### A-T3. `row_state_label` / `row_state_reason`

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

### A-T4. Render the reason through the Source column

`review_row.rs:253-265` composes the Source text, `:266-269` renders it. Change
the shape from "confidence, plus the write outcome if any" to "confidence, plus
the row's state if it is not `Ready`, plus the write outcome if any", joined with
the page's `·` separator. When `model.outcome` is `None` and
`row_state_label(model.row.state)` is `Some(label)`, the Source cell reads
`"MusicBrainz · 90% · Stale"`. The existing `outcome` branch keeps precedence:
after a write the outcome is the newer fact.

Still inside `bind` (`review_row.rs:232-311`):

- the **row root** gets `row_state_reason(...)` as its tooltip when the state is
  not `Ready`, and `set_tooltip_text(None)` when it is;
- `ReviewRowModel::accessible_description` (`review_model.rs:107-122`) appends
  the same reason, next to where it already appends `outcome.error`.

Do **not** touch `row_selectable` (`review_model.rs:355-361`) or
`review_row.rs:239`. The checkbox stays insensitive; that is the correct
behaviour and the whole point.

### A-T5. Row activation stops paying for a refusal

`toggle_position` (`review_page.rs:149-159`) becomes:

```rust
let model = boxed.borrow::<ReviewRowModel>();
if model.selectable_row_ids.is_empty() {
    return;
}
self.set_selected(&model.selectable_row_ids, !model.row.selected);
```

That is the whole change and it stays inside that function body — **A's only
hunk in `review_page.rs`**. Two effects: the per-id `RowNotReady` warnings
disappear (the core is no longer asked), and the `refresh()` behind the refusal
disappears with them. `set_selected`'s own warning (`review_page.rs:141-143`)
stays — it is still right to log if a *selectable* id is ever refused.

### A-T6. Rules

- **DOC-3c** (`docs/ux-rules.md:4078-4085`): extend the sensitivity clause from
  the master checkbox to **every** checkbox standing for a set of rows — the
  master and each album header — and add that an insensitive one names its
  reason in text, not only on hover. Note the amendment and its date in the rule
  body, house style.
- **DOC-9b** (`:4435-4470`): the album header's change count states the album's
  inventory and, when nothing in it is selectable, the reason next to that
  count; a row the page refuses to select names the reason in its Source cell
  and in its accessible description, activating it changes nothing and costs
  nothing, and the page-level banner stays as the aggregate. Add the new test
  names to the `*Tests:*` lists. DOC-4c/DOC-8b keep owning *why* such rows
  exist — no change there.

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
  `origin/dev`**. Before blaming this branch, run the same test on `051fb088df`
  and say which failures pre-date the change.
- A's tests assert sensitivity, text and tooltips — **not geometry** — so they do
  not need the app CSS. Any test that measures a widget's allocation must first
  call `crate::ui::style::install_css_string_for_test(&super::super::css())`
  (`ui/style/mod.rs:177`, `library_doctor/mod.rs:97`), or the measurement is
  meaningless.
- Redirect long output to a file and answer the question with `grep`/`wc`; never
  read a whole log back.

### V-1. A-1 — a test that fails before the fix

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

`doc_3c_an_album_with_nothing_selectable_binds_an_insensitive_header_check`,
`#[ignore = "requires a display; run via xvfb-run"]`: build the page the way
`doc_9b_the_first_row_carries_its_album_header` (`review_page_tests.rs:578-611`)
does, pump the main context, then for each realized header root
(`doctor-album-header-first` / `-later`, via `descendants_with_css_class`,
`:519-533`) identify the album from its labels (`descendant_label_text`,
`:738-752`) and assert:

- album A's `CheckButton` **is** sensitive;
- album B's `CheckButton` is **not** sensitive — the assertion that fails on
  `051fb088df`;
- album B's pill contains **both** its inventory count and the reason clause,
  and **not** `"0 changes"`;
- album B's header root tooltip is the reason sentence.

Plus the cheap unit test `doc_3c_album_header_state_names_the_reason_at_zero`
over the pure `album_header_state` — fast feedback, not the proof.

### V-2. A-2 — the *rendered* row, not the model

`review_row_contract_tests.rs` is already a child module of `review_row`
(`:1-7`), so `bind` and `build_row` are reachable. Add a literal
`ReviewRowModel` fixture; every field is `pub(super)`
(`review_model.rs:85-103`) and `DoctorReviewRowId::from_raw` (`review.rs:17-19`)
is public, so no database is needed.

`doc_9b_a_stale_row_names_its_reason_where_the_click_happens`,
`#[ignore = "requires a display; run via xvfb-run"]`:

1. bind a `Stale` model → `widgets.source.text()` contains the stale label,
   `widgets.root.tooltip_text()` is the reason sentence,
   `widgets.selected.is_sensitive()` is false, and
   `model.accessible_description()` contains the reason;
2. bind a `Ready` model into the **same** widgets → the reason is gone from both
   the Source text and the tooltip. Recycling is where this class of bug hides.

### V-3. A-4 — the dead activation, measured as churn

`doc_9b_activating_an_unselectable_row_selects_nothing`,
`#[ignore = "requires a display; run via xvfb-run"]`, in `review_page_tests.rs`,
on the `stale_album_scan()` fixture:

- connect a counter to `page.state.store.connect_items_changed` summing
  `removed + added` — `state` is a private field of `LibraryDoctorReviewPage`
  (`review_page.rs:397-401`) and `review_page_tests` is a child module
  (`review_page.rs:641`), so it is reachable;
- call `page.state.toggle_position(p)` for a stale row's position;
- assert the counter is **0** and the session's selection is unchanged;
- second half, on a `Ready` row: the activation **does** flip the selection, so
  the test cannot pass by doing nothing at all.

This fails on `051fb088df`: today the refusal is followed by `refresh()`'s full
`store.splice(0, n, …)` (`review_page.rs:80`), so the counter is ~`2n`.

### V-4. Gates before landing

`cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test -p reprise-core`, `cargo test -p reprise-gnome`,
`scripts/check-display-tests.sh` (unfiltered — it enumerates **every**
`--ignored` test in the crate, `:18-21`, and has run the whole set since #463),
`scripts/check-ux-traceability.sh`,
`scripts/check-accessibility-semantics.sh`, `scripts/check-architecture.sh`,
`scripts/check-frontend-thinness.sh`, `scripts/check-input-parity.sh`.

Traceability trap (mother §B, R-16): `scripts/check-ux-traceability.sh:93-108`
rejects any `#[ignore]` on a rule-named test unless the reason is *exactly*
`"requires a display; run via xvfb-run"`. All four of A's new tests are
rule-named, so all four must carry that exact reason or none at all.

Existing tests that must keep passing untouched:
`doc_3c_the_master_check_mirrors_the_visible_selection` (`review_header.rs:302-337`),
`doc_9b_a_fully_deselected_album_says_none_selected` (`review_header.rs:339-343`),
`doc_9d_the_header_counts_the_inventory_while_the_footer_counts_the_selection`
(`review_page_tests.rs:339-356`),
`doc_9b_every_section_boundary_binds_a_non_empty_header` (`:613-696`),
`doc_9b_the_conflicts_panel_covers_no_row` (`:466-513`),
`doc_9b_the_album_pill_counts_written_changes_not_display_rows` (`:428-441`),
`doc_9b_review_groups_render_one_header_per_album` (`:556-577`).

---

## File ownership

A owns, for the whole life of the strand:

- `crates/reprise-gnome/src/ui/library_doctor/review_header.rs`
- `crates/reprise-gnome/src/ui/library_doctor/review_row.rs`
- `crates/reprise-gnome/src/ui/library_doctor/review_model.rs`
- `crates/reprise-gnome/src/ui/library_doctor/review_page_tests.rs`
- `crates/reprise-gnome/src/ui/library_doctor/review_row_contract_tests.rs`
- `crates/reprise-gnome/src/ui/strings_library_doctor.rs`
- `docs/ux-rules.md`
- **exactly one hunk** in
  `crates/reprise-gnome/src/ui/library_doctor/review_page.rs`: the body of
  `toggle_position` (`:149-159`, task A-T5). Nothing else in that file — not the
  `mod` declarations, not `refresh()`, not `set_selected`.

A does **not** touch: `review_page.rs` beyond that hunk, `review_conflicts.rs`,
`reprise-core`, `summary_*.rs`, `result_pages.rs`, `running_page.rs`,
`progress_card.rs`, `sidebar/*`, or anything else fenced off in the mother plan
§G. In particular A does not add `review_snapshot.rs`, does not introduce a
second update path and does not add performance probes — all of that is strand
B's, and duplicating it here would land the same change twice with two different
reasons in the history.

Strand B is coded only after A has merged into `dev` and inherits
`review_header.rs`, `review_page_tests.rs`, `strings_library_doctor.rs` and
`docs/ux-rules.md` from A. See the mother plan §H and §I.
