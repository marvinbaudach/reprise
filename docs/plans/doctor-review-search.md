---
slug: doctor-review-search
worktree: /home/marvin/Projects/reprise-doctor-review-search
branch: feature/doctor-review-search
phase: planned
codex_session:
created: 2026-08-15
---

# Search on the Library Doctor review page

> Every `file:line` here was read at `origin/dev` = `9fecc6d8f5` in the worktree
> `/home/marvin/Projects/reprise-plan-doctor-search`. Paths are relative to the
> repository root. Numbers not backed by a file in this tree are marked
> **unmeasured**, with the way to measure them.

---

## 1. Diagnosis

The review page is the last large list in the app with no text search. A real
scan produces 433 fixes across 122 albums; the only way to narrow that is three
category toggles, and there is no way to jump to a name.

- **No search widget in the Doctor.** `grep -rE
  'SearchEntry|SearchBar|search_scope|matches_any|matches_query'
  crates/reprise-gnome/src/ui/library_doctor/` returns **zero** matches.
- **The shared lens is deliberately hidden.** `library_chrome.rs:102` —
  `self.search_toggle.set_visible(!doctor_visible);` in `DoctorChrome::sync()`
  (`:98-111`), keyed off `content_stack.visible_child_name() ==
  Some("library-doctor")` (`:99-100`).
- **A rule says so, not only the code.** `docs/ux-rules.md:4390` **DOC-7c**
  `[active] [gtk]`, two relevant sentences: the header's "Library-only source
  title, search action, and scan action are hidden" (`:4396`) and "Review places
  only its 'All' and 'None' actions there" (`:4397-4398`). Both must be amended
  together — task 11.
- **A display test pins it.** `library_chrome_tests.rs:104` —
  `assert!(!chrome.search_toggle.is_visible());` inside
  `doc_7c_the_doctor_uses_the_shared_window_chrome` (`:52-115`). It asserts while
  the Doctor **root** shows; Review is pushed only afterwards. That is the seam.
- **Ctrl+F is a no-op.** `SectionSearch::visible_section()`
  (`section_search.rs:210-228`) resolves the stack page name; `"library-doctor"`
  is neither `LIBRARY_PAGE` (`:37`) nor a key in `page_source()` (`:42-52`), so
  it falls to `SearchScope::Unsupported` (`:223-225`) and `supports_search()`
  (`:162-164`) returns false.

### Corrections to the draft and the brief

Verified in the worktree; the right-hand column is what this plan uses.

| Claimed | Actual |
| --- | --- |
| `scope_for()` at `search_scope.rs:34-55` | `:34-54`. |
| `search_fields()` at `browse.rs:77-86` / `:78-87` | `crates/reprise-view/src/strings/browse.rs:77-88`. |
| Two **scope tables** in `filter_bar_strings.rs:116-122` / `:139-146` | They are **test case arrays**: `:115-123` and `:138-147`. Neither is an exhaustive `match`, so adding a scope does **not** break the build there — only `browse.rs:79-87` does. Extend both by hand or the chip text ships unproven. |
| `navigation.rs` is 146 lines | **145**. |
| `review_model.rs` is 524 lines | **523**. |
| `check-architecture.sh` forces `strings_*.rs` into `po/POTFILES.in` | It does **not**. `scripts/check-architecture.sh:459-468` checks four hard-coded files (`strings_app_shell`, `strings_artist`, `strings_issues`, `strings_news`). `strings_library_doctor.rs` sits in `po/POTFILES.in:6` by hand, unguarded. |
| A neutral search icon constant exists in `crate::ui::icons` | It does **not**. `icons.rs` declares only `DONE` (`:15`), `UNEXPLAINED_SEARCH_MATCH` (`:19`), `LYRICS` (`:24`), `VISUAL_BARS` (`:28`). Use the literal `"system-search-symbolic"` — task 4. |
| Add DOC-12a "after DOC-11a (`:4676`)" | DOC-11a is the highest *id*, but the **last** DOC rule is DOC-6c at `:4693-4699`, and `## Z.` begins at `:4700`. Insert after `:4699`. |
| `review_row_contract_tests.rs:336/:352/:371` guard the review CSS | They do not. The only class assertions there are `:470` and `:490`, and they check that a *widget carries a class*, not that a rule exists. **Nothing asserts the review CSS rules today** — task 0 brings its own net. |
| `#[ignore]` "counts as coverage" | Precisely: `check-ux-traceability.sh:99-107` accepts an `[active]` rule's test only when the attribute is **exactly** `#[ignore = "requires a display; run via xvfb-run"]`. |

Everything else the draft cited in `review_page.rs`, `review_snapshot.rs`,
`review.rs` and `grouping.rs` checked out; those lines are re-cited at the task
that uses them rather than listed here.

Two facts the tasks depend on: `DoctorReviewRowId` is
`pub struct DoctorReviewRowId(u64)` deriving `Copy, Eq, Hash`
(`review.rs:9-20`), so it is a free `HashSet` key; and `ReviewRowModel`
(`review_model.rs:85-104`) has **plain `pub(super)` fields**, no getters —
`row` `:86`, `row_ids` `:87`, `selectable_row_ids` `:88`, `album_key` `:92`,
`album_title` `:93`, `album_artist` `:94`, `track` `:98`, `field` `:99`,
`current` `:100`, `proposed` `:101`.

### What the category filter means today

It is not a display filter — it is a **scope on the session** that limits what
gets written to disk:

- `freeze_plan()` filters by `category_filter_matches` at `review.rs:663`, so
  the immutable Apply plan holds only in-scope rows.
- `all()` (`:478-495`) and `none()` (`:497-513`) touch only rows and tie
  templates inside the filter — `:482-484`/`:491` and `:501-503`/`:509`.
- `summary()` (`:644-656`) reports `tag_change_count` from the scoped plan **and**
  `total_tag_change_count` from *all* selected Ready rows (`:650-654`).
- The footer renders that pair via `review_footer_summary`
  (`review_summary.rs:41-56`) → `strings::doctor_filter_scope`
  (`strings_library_doctor.rs:413-422`), `"{shown} of {total} · filtered by
  {filter}"`.

So "what happens to a selected row a filter hides" is already settled: **it stays
selected, it is excluded from the plan, and the footer discloses the gap.**
Nothing clears `row.selected`. **DOC-9d** (`docs/ux-rules.md:4610-4627`) states
exactly this. Search behaves identically.

### The measured baseline

From #505/#506 on a real library, 330 visible rows; **not re-measured here**.

| Path | Median | Shape |
| --- | --- | --- |
| Full `refresh()` | **248 ms** | grew 13 → 667 ms over twelve triggers; up to 4,600 ms in longer sessions |
| Incremental `apply_selection()` | **13.6 ms** | flat |

`store.splice` was ~96 % of the full path, `grouped_rows_for` 3.6 %. The
monotonic growth is *unexplained* — routed around, not diagnosed. Treat it as a
standing property of the full-refresh path.

---

## 2. The decision

**Option (c), split by layer.**

> The query filters the **view** without ever splicing the store, and scopes the
> **session** so Apply, `All` and `None` obey it. It never reaches
> `group_review_rows`, so grouping and row identity are untouched.

1. `ReviewSnapshot` gains a per-row visibility mask and is the **only** place
   that decides whether a row matches. It derives `albums` and `totals` from
   visible rows only, so the header inventory, album counts and the master
   checkbox follow the query (DOC-9d, DOC-3c).
2. The `CustomFilter` (`review_page.rs:481-489`) **computes nothing**; it looks
   the answer up in the snapshot through the `index` map it already keeps.
3. The matching `DoctorReviewRowId`s go into the session as a new `query_scope`
   that `freeze_plan()`, `all()` and `none()` intersect with, exactly as they
   already do with `category_filter`.
4. `store.splice` is never called on a query change. Row positions never move,
   `ReviewSnapshot::index` stays valid, and the incremental selection path from
   #505/#506 is untouched **by construction**, not by care.

### One matcher, written in place, and the borrow rule

Three coupled decisions. A later reader will be tempted to unpick each; don't.

**One matcher.** The obvious shape is a free `review_row_matches(model, query)`
called independently by the filter *and* by the snapshot. That is two
evaluations of one predicate over two code paths — the same mistake option (b)
makes one layer up, where view and session disagree about scope. Two matchers
that agree today drift the day someone adds a field to one; the symptom is a row
drawn but excluded from the plan, or counted but invisible, and nothing in the
type system objects. So the predicate lives once, in `review_snapshot.rs`:

```rust
self.visible[i] = matches_any(
    [row.track.as_str(), row.album_title.as_str(), row.album_artist.as_str()],
    query,
);
```

and the filter is a lookup:

```rust
let filter = gtk4::CustomFilter::new(move |object| {
    let Some(boxed) = object.downcast_ref::<glib::BoxedAnyObject>() else {
        return object.is::<gtk4::Widget>();   // the conflicts panel, unchanged
    };
    let model = boxed.borrow::<ReviewRowModel>();
    filter_session.borrow().category_filter_matches(model.row.problem_class)
        && filter_snapshot.borrow().is_visible(model.row_ids.first())
});
```

Two details that are easy to get wrong. Look up **`row_ids.first()`, not
`row.id`**: `index` is built from `row.row_ids` (`review_snapshot.rs:42-53`) and
`selection_diff` already keys off `cached.row_ids.first()` (`:103-108`); a
collapsed "All N tracks" row maps many ids to one position, so the first is
enough. And an id the index does not know — including `None` — is **visible**,
so the default-constructed snapshot at `review_page.rs:575` does not hide the
list before the first `refresh()`. The conflicts panel never reaches
`is_visible` at all: it is a `Widget`, not a `BoxedAnyObject`, and returns at
the `else` at `:483`.

**Written in place.** **(contract)** `pub(super) fn apply_query(&mut self, query:
&str)` — not `with_query(self, …) -> Self`, and no clone. Both alternatives were
tried:

- `std::mem::take` + rebuild, the shape `apply_selection` uses at
  `review_page.rs:243-245`, parks a `ReviewSnapshot::default()` in the `RefCell`
  for the duration. Harmless today because nothing reads the snapshot in that
  window; not harmless once the *filter* reads it, since a default snapshot has
  an empty `index` and, under the lenient unknown-id rule, every row reads as
  visible — a real hole, for one turn, in the operation meant to hide rows.
- Closing that hole by **cloning** was measured on strand B: a deep copy on the
  hot path moved the selection medians from **435 µs to 623 µs**, and it was
  reverted. `rows` carries four `String`s per row; copying 330 of them to flip a
  `Vec<bool>` is not a trade worth making.

**The borrow rule.** *No `borrow()`/`borrow_mut()` on `self.snapshot` or
`self.session` may be held across `filter.changed(…)`, `store.splice(…)`, or any
other list-model mutation. Unpack first, then signal.* `gtk4::Filter::changed`
runs the closure **synchronously** for every item, and so does a splice; that
closure now borrows both cells, so an outstanding borrow is an immediate
`BorrowMutError` — the class AGENTS.md forbids outright. The tree already obeys
this for the session, which is the proof it is real: `refresh()` calls
`drop(session)` at `review_page.rs:107` before the splice at `:114`, and
`set_category` writes through a **temporary** borrow (`:288-290`) that ends at
the semicolon before `filter.changed(…)` at `:291`.

`set_query` follows the same shape, and its **order is the reverse of the
draft's** — mask first, signal second, or GTK re-filters against the previous
keystroke's mask:

```rust
fn set_query(self: &Rc<Self>, query: &str) {
    let query = query.trim();
    if self.query.borrow().as_str() == query { return; }
    *self.query.borrow_mut() = query.to_owned();          // borrow ends here
    self.snapshot.borrow_mut().apply_query(query);        // borrow ends here
    self.filter.changed(gtk4::FilterChange::Different);   // reads the fresh mask
    self.push_query_scope();                              // task 7
    // header, footer, master check, content child
}
```

### Why not (a) — the query on the session

Semantically cleanest: `grouped_rows_for` would return only matching rows and
everything downstream follows for free. Rejected on two independent grounds.

**Cost.** Every query change runs the full `refresh()` path: median 248 ms,
growing past 667 ms within a session. Category switches are rare; keystrokes are
not. The asymmetry of search worsens rather than rescues this: the first
keystroke shrinks 330 rows to a handful, cheap because the splice is
proportional to the result — but **clearing** restores all 330, the expensive
splice, at exactly the moment SEARCH-9 (`docs/ux-rules.md:3000`) forbids any
wait (*"Emptying the query waits not at all."*). Under (a) the one transition
that must be instant costs the most; under (c) both directions cost a filter
invalidation and an O(rows) mask pass.

**Correctness of the grouping.** Independently of cost, the query must not enter
`album_from_seed`. Its "All N tracks" collapse (`grouping.rs:134-145`) compares
a change-group's track set against the album's whole track set (`:141-142`). The
category filter removes *whole problem classes*, so a group survives or vanishes
intact and the collapse is stable. A text query removes *individual tracks*, so
a collapsed 12-track row fragments mid-typing — changing `row_ids`,
`selectable_row_ids`, the snapshot `index`, every album count and every DOC-3c
contract while the user types. This rules out (a) on its own.

### Why not (b) — the query only in the `CustomFilter`

Cheap and wrong. `freeze_plan()` (`review.rs:658`, filter at `:663`), `all()`
(`:478`) and `none()` (`:497`) read the **session**, which knows nothing about a
`FilterListModel`. Search "Beatles", see three albums, tick the master checkbox,
press Apply — and all 433 fixes are written. That breaks DOC-9d outright and
DOC-3c (`docs/ux-rules.md:4199`) with it, on the one page in the app that writes
tags to the user's files.

### What the query reads

**`track`, `album_title`, `album_artist`.** Case-insensitive substring,
mid-word, via the existing `reprise_view::search_scope::matches_any`
(`search_scope.rs:77-82`); an empty or whitespace-only query matches everything
(`:78-80`). Nothing new is built.

- **`album_key`** — never. A normalized key with a `\u{1}` separator
  (`grouping.rs:73-81`), not user-visible text; matches nobody could explain.
- **`field`** — rejected. A localized column caption (`review_model.rs:215`).
  The three category toggles do this job exhaustively and better; searching it
  would make "genre" match every genre row *and* any album named *Genre*,
  blurring the tabs rather than adding reach.
- **`current` / `proposed`** — deferred, not refused. "Find everything proposing
  *Remastered*" is powerful, but it is a *value* search, not the *name*
  navigation this page lacks, and a hit on `proposed` matches text that does not
  yet exist in the library. It also collides with the strikethrough presentation
  of a replaced `current` (DOC-3b). See §8.

The chosen three mirror `SearchScope::Tracks` ("Searches track, artist and
album", `filter_bar_strings.rs:139`), so one mental model holds across the app.
FIL-1d (`docs/ux-rules.md:1525-1541`) requires the chip to name exactly what it
reads; task 12 asserts that mechanically.

### Debounce: none, with a design target

SEARCH-9 mandates "exactly one wait […] the application's own debounce of
150 ms" and immediate clearing. That 150 ms exists because `SearchScope::Tracks`
reloads from SQLite per query (`view_session.rs:25`, applied at `:143`).
Concerts and Podcasts do **not** debounce — no I/O — and that argument applies
here more strongly still: the rows are already resident in
`ReviewSnapshot::rows`, and a query change is one `filter.changed()` plus an
O(rows) string pass. A 150 ms timer would make the page respond *slower* than
doing the work.

**Ship without a debounce.** Task 13 lands a probe with no budget. Decision rule
for the acceptance run in §7: if the measured median of one query change on the
real library exceeds **16.7 ms** (one frame at 60 Hz, where a keystroke becomes
a visible stutter), add the debounce by reusing `SEARCH_DEBOUNCE_MS` from
`view_session.rs:25` and keep the empty-query path synchronous per SEARCH-9. Do
not invent a different constant. **16.7 ms is a design target, not a
measurement — nothing in this tree has measured this path.**

### Selection under a query, and no special protection for Apply

Consistent with the category filter, because the code above already fixes it:

- A selected row hidden by the query **stays selected**, is **excluded from the
  plan**, and the footer discloses the gap.
- The **master checkbox** reads `master_check_state(totals.selected,
  totals.selectable)` (`review_page.rs:170`) from a snapshot whose totals now
  cover only visible rows, so it is checked when every *visible* selectable row
  is. Ticking it calls `session.all()` (`review_page.rs:617`), which intersects
  with `query_scope` and marks only visible rows.
- The **album header checkbox** binds `AlbumCounts` from
  `snapshot.borrow().albums`, also over visible rows. DOC-3c holds.

No confirmation dialog, no special Apply label, no "hidden selections" warning:

1. **The category filter has behaved this way since DOC-9d shipped.** A second
   mechanism firing only for search would give one user action two meanings
   depending on which control produced the filter. This plan buys consistency;
   a search-only safeguard is exactly where it would spend that back.
2. **The footer already discloses it.** If that disclosure is judged
   insufficient, the fix belongs to DOC-9d and applies to both filters.

### A "no matches" state of its own

The existing empty state is a **success** state: `adw::StatusPage` with
`crate::ui::icons::DONE` (`review_page.rs:521`, `"object-select-symbolic"`),
`DOCTOR_NO_CHANGES` = *"No Changes to Review"* (`strings_library_doctor.rs:67`)
and `DOCTOR_NO_CHANGES_DESCRIPTION` = *"Return to the results and choose another
review filter."* (`:68-69`). Showing that for a query with no hits claims, with
a green checkmark, that the library is clean while 433 fixes sit behind the
filter — a lie in the one place the user is least able to check it.

So the stack gets a **third child**, `"no-match"`, beside `"rows"`
(`review_page.rs:532`) and `"empty"` (`:533`): a neutral magnifier (**not**
`icons::DONE`), a title naming the query, a description naming how many fixes
the query is hiding, and a button that clears the search. The switch becomes
three-valued: rows present → `"rows"`; no rows and no query → `"empty"`; no rows
and an active query → `"no-match"`.

---

## 3. Prerequisite PR — task 0, landed on its own

`crates/reprise-gnome/src/ui/library_doctor/mod.rs` is **781** lines.
`scripts/check-architecture.sh:20` is `if (( lines >= 800 ))` — a file fails
**at** 800, so the budget is ≤ 799 and `mod.rs` has **18** lines of headroom.
Tasks 8 and 10 both need room there.

**This lands as its own PR, before the feature branch exists**, because the diff
is tiny and mechanical, because `check-architecture.sh` is then proven green
before a line of feature code exists, and because a later gate failure on the
feature PR is then unambiguously the feature's. Precedent: #506 (`9fecc6d8f5`)
ran exactly this kind of extraction as its own change. **Do not start the feature
branch until this is merged into `dev`;** branch off the merge commit.

- Create `library_doctor/review_css.rs` with `pub(super) fn css() -> String`,
  mirroring `start_page_css.rs` (13 lines) exactly: an array of `&'static str`
  rules, `.join(" ")`.
- Move these entries out of `mod.rs`'s `css()` (`:99-141`) **verbatim and in
  order**: `:109-130` (the `.doctor-album-*`, `.doctor-review-*`,
  `.doctor-current-empty`, `.doctor-card-*` rules, including the comments at
  `:125-126` and `:128-129`) and `:132-138` (`.doctor-review-meta*`,
  `.doctor-review-stale`, `.doctor-review-footer*`, `.doctor-review-apply`).
  29 rules.
- **Leave behind:** `:101-108` (`.doctor-conflicts-*`, `.doctor-conflict-*`) and
  `:131` (`&start_page_css::css(),`).
- Add `mod review_css;` beside `mod start_page_css;` (`:19`) and splice
  `&review_css::css(),` where `:109` was.

The joined string changes **order** — `start_page_css`'s rules now follow the
review rules instead of sitting between them — but not content. Safe here and
only here: every moved selector (`.doctor-album-*`, `.doctor-review-*`,
`.doctor-card-*`, `.doctor-current-empty`) is disjoint from every
`start_page_css` selector (`.doctor-start-*`), so no equal-specificity rule can
win differently.

*Proof:* `scripts/check-architecture.sh` green with `mod.rs` at **≈754** (781 −
29 + 2); `cargo test --locked --workspace --exclude reprise-platform-linux`
unchanged from the same command on `origin/dev`. And **bring your own net** —
nothing in the tree asserts these rules exist today. Add one test in
`review_css.rs` asserting `crate::ui::library_doctor::css()` still contains
`.doctor-album-check`, `.doctor-review-row`, `.doctor-card-accent`,
`.doctor-review-footer`, `.doctor-review-apply` (moved), plus
`.doctor-start-run` and `.doctor-conflicts-dashed` (must **not** have moved).

---

## 4. Tasks, in order

One strand, one PR, one sequence. **(contract)** signatures are fixed by this
plan and may be written against before the defining task lands. Every task
compiles on its own unless it says otherwise.

### Task 1 — Core: a query scope on `DoctorReviewSession`

Files: `crates/reprise-core/src/library/library_doctor/review.rs`, that
directory's `mod.rs`, **new** `…/library_doctor/review_query_tests.rs`.

> **`review_tests.rs` is at 799 lines — one below the failing threshold. Do not
> add a line to it.** New core tests go in the new file, declared beside
> `mod review_tests;` (`mod.rs:37`).

- Add `query_scope: Option<HashSet<DoctorReviewRowId>>` beside `category_filter`
  (`review.rs:209`), initialised `None` in `build` beside `category_filter: None`
  (`:366`).
- **(contract)** `pub fn set_query_scope(&mut self, scope:
  Option<HashSet<DoctorReviewRowId>>)` — mirrors `set_category_filter`
  (`:388-390`).
- **(contract)** `pub fn query_scope_matches(&self, id: DoctorReviewRowId) ->
  bool` — `true` when `query_scope` is `None`, else set membership. Mirror
  `category_filter_matches` (`:392-396`) and keep it a two-liner; `review.rs` has
  room for the field, setter, matcher and three call sites, not much more.
- Honour it in exactly three places, ANDed with the existing category check:
  `freeze_plan()` (a second `.filter(…)` beside `:663`); `all()` in **both**
  branches (tie templates `:482-484`, rows `:491`); `none()` in both
  (`:501-503`, `:509`).
- Preserve it across a rebuild in `set_remote_visible()`, beside
  `rebuilt.category_filter = self.category_filter.clone();` (`:432`).

*Proof* (`review_query_tests.rs`):
`doc_12a_the_query_scope_limits_the_frozen_plan`,
`doc_12a_all_and_none_operate_only_on_the_query_scope`,
`doc_12a_a_row_outside_the_query_scope_keeps_its_selection`,
`doc_12a_the_query_scope_survives_a_remote_visibility_rebuild`,
`doc_12a_the_query_scope_intersects_the_category_filter`.

### Task 2 — View: the `DoctorReview` scope and its strings

Files: `crates/reprise-view/src/search_scope.rs`,
`crates/reprise-view/src/strings/browse.rs`,
`crates/reprise-gnome/src/ui/filter_bar_strings.rs`.

The addition is additive, and precisely why matters: a new `SearchScope` variant
breaks the build in exactly **one** place, the exhaustive `match` in
`search_fields()` (`browse.rs:77-88`). `scope_for()` (`search_scope.rs:34-54`)
matches on `ViewSource`, not `SearchScope`. `section_search_wiring.rs` uses
scopes only as values. The two arrays in `filter_bar_strings.rs` are **test
data** — the build stays green if you forget them and the chip text ships
unproven. Extend all three by hand.

- Add `DoctorReview` to `SearchScope` (`search_scope.rs:14-28`) with a doc
  comment saying it is reached by navigation, not by a `ViewSource`. **Do not**
  add an arm to `scope_for()` — there is no Doctor variant in `ViewSource`
  (`crates/reprise-core/src/view_source.rs:19-85`) and inventing one would be a
  lie the shell cannot honour. `supports_search()` (`:60-62`) covers it
  automatically.
- Extend the loop in `search_8a_sections_without_a_list_do_not_support_search`
  (`:118-128`).
- `browse.rs:77-88`: add the arm and the constant `SEARCH_FIELDS_DOCTOR_REVIEW =
  "track, album and artist"` beside the others (`:27-34`).
- `filter_bar_strings.rs`: add `(SearchScope::DoctorReview, "⌕ “wer” in track,
  album and artist")` to `:115-123` and `(SearchScope::DoctorReview, "Searches
  track, album and artist")` to `:138-147`.

*Proof:* the two extended arrays. FIL-1d is only satisfied if the chip names the
same three fields task 3 reads — task 12 closes that mechanically.

### Task 3 — Snapshot: the mask is the only matcher

File: `crates/reprise-gnome/src/ui/library_doctor/review_snapshot.rs`.

- **(contract)** `ReviewSnapshot::from_rows(rows: Vec<ReviewRowModel>, query:
  &str) -> Self`. `refresh()` (`review_page.rs:91-95`) passes
  `self.query.borrow().as_str()` as a temporary that ends before the call
  returns, per the borrow rule.
- Add `visible: Vec<bool>` parallel to `rows`. Build `albums` and `totals`
  (`:36-73`) from visible rows **only**; keep `index` (`:31`) over **all** rows,
  so positions stay absolute and `splice_selection_rows` (`:143-175`) keeps
  working against the unfiltered store.
- Add `pub(super) unfiltered_changes: usize` — `totals.changes` over all rows,
  computed once in `from_rows` and never recomputed. The no-match state needs
  "how many fixes is the query hiding", which cannot come from a query-aware
  total.
- **(contract)** `pub(super) fn apply_query(&mut self, query: &str)` —
  recomputes `visible`, `albums`, `totals` **in place**. No `mem::take`, no
  clone; §2 says why both were rejected.
- **(contract)** `pub(super) fn is_visible(&self, id: Option<&DoctorReviewRowId>)
  -> bool` — `index` lookup then `visible[pos]`; `None` and unknown ids are
  **visible**.
- **(contract)** `pub(super) fn visible_selectable_row_ids(&self) ->
  HashSet<DoctorReviewRowId>` — every `selectable_row_ids` entry of every visible
  row; the set task 7 pushes into the session.
- Guard `with_selection` (`:114-140`): a position whose row is **not** visible
  must update `self.rows[position]` but must **not** adjust `album.selected` or
  `totals.selected`, since an invisible row contributes nothing to those
  aggregates. Without the guard the `checked_sub` at `:126-130` and `:131-136`
  underflows and panics the first time `session.all()` touches a hidden selected
  row.

*Proof* (in `review_search_tests.rs`):
`doc_9d_a_searched_header_counts_only_the_matching_rows`,
`doc_3c_the_master_check_covers_only_the_searched_rows`,
`review_snapshot_apply_query_preserves_absolute_row_positions`,
`review_snapshot_toggling_a_hidden_row_does_not_move_the_totals`.

### Task 4 — Page: query state, the lookup filter, the no-match child

Files: `review_page.rs`, **new** `library_doctor/review_search.rs`.

`review_search.rs` is declared **inside `review_page.rs`**, not in the
directory's `mod.rs`:

```rust
#[path = "review_search.rs"]
mod review_search;
```

beside the existing `#[cfg(test)] #[path = …]` declarations (`:716-726`) but
**without** `#[cfg(test)]`. As a child module it sees `ReviewState`'s private
fields and can write `impl super::ReviewState { … }`. This is what keeps
`review_page.rs` under budget; do not inline this logic there.

- `ReviewState` (`:40-70`) gains `query: Rc<RefCell<String>>`, built before the
  filter and cloned into the closure.
- Rewrite the `CustomFilter` closure (`:481-489`) to the two-clause lookup in
  §2. Keep the widget escape hatch at `:483` byte-for-byte.
- Pass the query into `ReviewSnapshot::from_rows` at `:91-95`.
- Add the third stack child. Build it in `review_search.rs` as **(contract)**
  `pub(super) fn no_match_page(on_clear: Rc<dyn Fn()>) -> adw::StatusPage`,
  called from `review_page.rs` next to `empty` (`:520-524`), with
  `content.add_named(&no_match, Some("no-match"));` after `:533`. Building it in
  the child module is not decoration: `review_page.rs` has ~38 lines of headroom
  after this task and the StatusPage plus its button wiring is ~15 of them.
  - Icon: the literal `"system-search-symbolic"`. **There is no constant for
    this** (`icons.rs:15-28` has only `DONE`, `UNEXPLAINED_SEARCH_MATCH`,
    `LYRICS`, `VISUAL_BARS`). That literal is already used unguarded at
    `library_chrome.rs:43` and as `DOCTOR_GLYPH_FALLBACK`
    (`library_doctor/mod.rs:81`), and the guard test
    `every_icon_name_the_app_asks_for_can_be_drawn` (`icons.rs:153-179`) scans
    every `"…-symbolic"` literal under `src/ui` and asserts the installed theme
    can draw it. Green on `dev`, so this name is **proven drawable**. Invent a
    nicer one and that test will tell you.
  - The clear button goes back through the shared entry, not around it: it calls
    the page's `set_search_query("")` sink (task 10), so entry, chip and list
    clear in one step.
- **(contract)** `pub(super) fn set_content_child(&self)` in `review_search.rs`
  — the three-valued switch replacing the two-valued one at `:132-133`, called
  from `refresh()` in its place and from `set_query`:

  ```rust
  let name = if self.sorted.n_items() > 0 { "rows" }
      else if self.query.borrow().is_empty() { "empty" }
      else { "no-match" };
  ```

  `refresh()` reads `n_items()` at `:131` **after** the splice, correct for all
  three branches.

**This task compiles and changes no behaviour**: `query` is written nowhere yet,
so the filter always sees an empty string (`matches_any` returns `true`,
`search_scope.rs:78-80`) and `"no-match"` is unreachable.

*Proof* (task 12): `doc_12a_the_review_search_matches_track_album_and_artist`,
`doc_12a_the_review_search_ignores_the_normalized_album_key`,
`doc_12a_an_empty_query_hides_nothing`,
`doc_12a_the_conflicts_panel_survives_an_active_query`,
`doc_12a_a_query_with_no_matches_shows_its_own_state`.

### Task 5 — The no-splice query path

File: `review_search.rs`.

- **(contract)** `pub(super) fn set_query(self: &Rc<Self>, query: &str)`, in the
  exact order of §2: early return on an unchanged trimmed query; write
  `self.query`; `apply_query`; `filter.changed(Different)`; `push_query_scope()`
  (task 7); then `refresh_filter_summary()`, `refresh_master_check()`,
  `refresh_action_summary(self.ready_count.get())`,
  `album_headers.push_selection(&self.snapshot.borrow().albums)` — a scoped
  borrow that ends before the call returns — and `set_content_child()`.
- Emit one `tracing::debug!(path = "search", rows = …, elapsed_us = …,
  "DOCTOR_REVIEW_REFRESH path")` matching the shape at `review_page.rs:146-151`
  and `:263-268`, so the §7 harness needs no changes.

**`self.store.splice` must not appear anywhere in this path.** That is the
invariant task 13 measures at exactly 0.

*Proof:* `doc_12a_a_query_change_splices_no_store_items` (task 13),
`doc_12a_clearing_the_query_restores_every_row` (task 12).

### Task 6 — Header, footer and the search chip

Files: `review_filter_bar.rs`, `review_summary.rs`, **new**
`crates/reprise-gnome/src/ui/strings_library_doctor_search.rs`, `strings.rs`,
`po/POTFILES.in`.

> **New strings go in a new file.** `strings_library_doctor.rs` is at **777** of
> 799 — 22 lines — against roughly 37 needed (two footer scope functions, a
> no-match title, a plural description, a button label). It does not fit. Create
> `strings_library_doctor_search.rs`, wire it into `strings.rs` with that file's
> existing pattern —
> ```rust
> #[path = "strings_library_doctor_search.rs"]
> mod library_doctor_search;
> pub use library_doctor_search::*;
> ```
> — and **add it to `po/POTFILES.in` by hand**. `check-architecture.sh:459-468`
> checks four unrelated files, so nothing catches the omission and the strings
> would silently stop being translatable. `strings.rs` is at 759 and has room.
> Use the `N_!("…")` / `formatted(…)` / `plural(…)` house style of
> `strings_library_doctor.rs:67-71` and `:413-422`.

`ReviewFilterBar` is **not** built on the shared `layout.replace_scoped_search`
used by Concerts and Podcasts — it is a plain `gtk4::Box` with a copy column and
a toggle slot (`review_filter_bar.rs:34-44`). The chip goes into a new third
slot in `root`; do not retrofit the shared filter-bar layout here.

- **(contract)** `pub(super) fn set_committed_query(&self, query: &str)` on
  `ReviewFilterBar`. Non-empty renders exactly one chip via
  `filter_bar_strings::scoped_search_chip_label(SearchScope::DoctorReview,
  query)` (`:25-27`); empty renders none. SEARCH-11 needs no extra work — while
  the popover is open the committed string is already empty, decided centrally
  by `search_chip::committed_query` (`section_search.rs:362`).
- **(contract)** extend `review_footer_summary` (`review_summary.rs:41-56`) to
  `(summary, category, query: &str, ready_count)`. With a non-empty query — with
  or without a category — return `strings::doctor_filter_scope(shown, total,
  <scope text>)`, reusing `"{shown} of {total} · filtered by {filter}"`
  (`strings_library_doctor.rs:413`). Add one scope text for the query alone and
  one for query-and-category together.
- The header inventory needs **no** change: `refresh_filter_summary`
  (`review_page.rs:161-166`) already reads `snapshot.totals`, which task 3 made
  query-aware.

*Proof:* `doc_9d_the_footer_states_the_scope_of_the_search`,
`doc_9d_the_footer_names_both_search_and_category_when_both_are_active`,
`doc_12a_a_committed_query_renders_exactly_one_chip`. Model them on the existing
`doc_9d_the_footer_states_the_scope_of_the_filter` (`review_page_tests.rs:80`) —
read that file, do not edit it.

### Task 7 — Push the scope into the session

Files: `review_search.rs`, one line in `review_page.rs`.

- **(contract)** `pub(super) fn push_query_scope(&self)`:

  ```rust
  let scope = {
      let query = self.query.borrow();
      if query.is_empty() { None }
      else { Some(self.snapshot.borrow().visible_selectable_row_ids()) }
  };                                     // both borrows end here
  self.session.borrow_mut().set_query_scope(scope);
  ```

  The scoped block is not style. The session borrow must not overlap the
  snapshot borrow, and no `session.borrow_mut()` may be live when anything
  re-runs the filter closure, which also borrows the session
  (`review_page.rs:486-487`).
- Leave the master-checkbox handler (`review_page.rs:611-625`) calling
  `session.all()` / `session.none()` **unchanged**. Task 1 made those honour the
  scope, and routing through them preserves the tie-template handling at
  `review.rs:480-489` that a direct `set_selected` sweep would silently drop.
- `set_category` (`:286-293`) ends in `refresh()`, which rebuilds the snapshot
  with the current query — but the visible set changed, so the session's scope is
  stale. Call `push_query_scope()` at the end of `refresh()`, after the snapshot
  swap at `:112`, so both entry points stay consistent from one place.

*Proof:* `doc_12a_apply_writes_only_the_searched_set`,
`doc_12a_select_all_under_a_query_marks_only_the_matching_rows`,
`doc_9d_a_row_hidden_by_the_query_keeps_its_selection_and_stays_out_of_the_plan`,
`doc_12a_search_and_category_compose_as_an_intersection`.

**Mutation probe — part of this task, not a post-merge check.** Once the tests
are green, comment out the body of `push_query_scope` and re-run
`doc_12a_apply_writes_only_the_searched_set`. It **must** turn red. If it stays
green the test measures the view and not the session, and the DOC-9d guarantee
is unproven. Restore the body afterwards
(`mutation-probe-must-hit-production-code`).

### Task 8 — Route: the review page is a search section

Files: `section_search.rs`, `library_doctor/navigation.rs`,
`window_runtime_wiring.rs`.

**Read this before writing code — the obvious design is broken.**

The tempting shape is to hear the Doctor's `visible-page` notify and call
`search.activate(SearchScope::DoctorReview, …)`. That loses a race every time the
user enters Review from outside the Doctor. `DoctorNavigation::show_review`
(`navigation.rs:48-60`) first calls `show_content()` (`:74-91`), which sets the
stack page to `"library-doctor"` (`:81`); the stack's notify fires
**synchronously** and `observe`'s handler (`section_search.rs:183-187`) calls
`refresh_later()`, which does not resolve now but queues
`glib::idle_add_local_once` (`:194-201`). Only then does `show_review` push the
review page (`:56`/`:58`), where a direct `activate` would set `DoctorReview`.
The queued idle runs last: `refresh()` (`:203-208`) → `visible_section()`
(`:210-228`) → `"library-doctor"` is unknown → `Unsupported` → `activate`
(`:253-261`) sees a different scope → `switch_view` (`:263-273`) clears the query
and the lens vanishes.

So the lens would appear and then disappear on first entry. Navigating root →
Review *within* the Doctor happens not to fire the stack notify, which is exactly
why this would survive casual testing — and why the draft's "it works because
navigation inside the `NavigationView` raises no content-stack notify" is not a
safe foundation.

**The fix: make `visible_section()` the single authority**, so both signals
resolve identically whatever order they arrive in.

- `section_search.rs`:
  - `ShellState` (`:65-69`) gains `doctor_review_visible: Option<Rc<dyn Fn() ->
    bool>>`, defaulted `None` in `observe` (`:171-189`) so existing callers are
    unaffected.
  - **(contract)** `pub(in crate::ui) fn observe_doctor_review(self: &Rc<Self>,
    navigation: &adw::NavigationView)`: stores a closure over a **weak** handle
    reading `navigation.visible_page().and_then(|page| page.tag())
    .is_some_and(|tag| tag == "library-doctor-review")`, and connects
    `connect_visible_page_notify` to `self.refresh_later()`. Weak, like every
    handle in this module — `:71-76` explains why at length.
  - `visible_section()` gains **one** arm beside `DEVICE_SYNC_PAGE` (`:221`),
    with `const DOCTOR_PAGE: &str = "library-doctor";`:

    ```rust
    DOCTOR_PAGE => match &shell.doctor_review_visible {
        Some(visible) if visible() => SearchScope::DoctorReview,
        _ => SearchScope::Unsupported,
    },
    ```

    Today `"library-doctor"` falls through `other =>` to `Unsupported`
    (`:223-225`), so this only splits a case that already existed.
- `navigation.rs`: widen `:9` to **(contract)** `pub(super) const REVIEW_TAG:
  &str = "library-doctor-review";` so nobody retypes the tag, and add
  **(contract)** `pub(super) fn navigation_view(&self) -> &adw::NavigationView`
  returning `&self.doctor_navigation` (`:14`).
- `window_runtime_wiring.rs`: call
  `section_search.observe_doctor_review(library_doctor_navigation)` right after
  the existing `section_search.observe(…)` block (`:530-534`).
  `library_doctor_navigation` is already a field of the wiring context (`:63`,
  `:109`), and the view is created at `window.rs:262` and added to the stack as
  `"library-doctor"` at `:263`, long before this runs.

"Leaving Review drops the query" then needs no new state: the idle resolves to
`Unsupported`, `activate` calls `switch_view`, and `switch_view` clears the
outgoing scope's query (`:265`), blanks the entry (`:266`) and closes the
popover (`:267`). That is SEARCH-8a via the existing mechanism.

*Proof* — in **both** directions, because `ViewSource` has no Doctor variant
(`view_source.rs:19-85`), so `scope_for()` can never produce `DoctorReview` and
the scope is only ever set explicitly. An explicitly set scope is exactly the
kind that leaks, and a leak puts the lens on Start and Result in violation of
the DOC-7c this plan just amended:

- `doc_12a_entering_review_activates_the_doctor_search_scope` — build the stack,
  the doctor `NavigationView` and a root page; `observe` +
  `observe_doctor_review`; push a page tagged `"library-doctor-review"`; drain
  the idle (`while glib::MainContext::default().iteration(false) {}`); assert
  `search.supports_search()` and `search.is_active(SearchScope::DoctorReview)`.
- `doc_12a_leaving_review_drops_the_scope_and_the_query` — from that state, type
  a query, pop back to the root, drain the idle, then assert
  `search.is_active(SearchScope::Unsupported)`, `!search.supports_search()` and
  an empty entry (SEARCH-8a).
- `doc_12a_the_doctor_root_page_supports_no_search` — the same fixture without
  ever pushing Review.

All three carry `#[ignore = "requires a display; run via xvfb-run"]` and
`crate::ui::test_main_context::lock_main_context()`.

### Task 9 — Chrome: reveal the lens on Review only

Files: `library_chrome.rs`, `window.rs`, `library_chrome_tests.rs`.

`DoctorChrome` (`library_chrome.rs:21-29`) holds the content stack but not the
Doctor's `NavigationView`, and `sync()` (`:98-111`) only hears the stack
(`:89-93`). Both need the same second signal as task 8.

- `wire_content_stack` (`:70-95`) gains a `doctor_navigation:
  &adw::NavigationView` parameter, stored on `DoctorChrome`. At the only
  production call site (`window.rs:404-409`) `library_doctor_navigation` is
  already in scope, created at `window.rs:262`.
- In `sync()`, replace `:102` with visible when **not** in the Doctor **or**
  when in it with Review showing:

  ```rust
  let review_visible = doctor_visible
      && self.doctor_navigation.visible_page()
          .and_then(|page| page.tag())
          .is_some_and(|tag| tag == "library-doctor-review");
  self.search_toggle.set_visible(!doctor_visible || review_visible);
  ```

  `scan_button`, `source_title` and `doctor_title` (`:103-105`) keep keying off
  `doctor_visible` alone — the amended DOC-7c keeps them hidden throughout.
- Connect `doctor_navigation.connect_visible_page_notify` to the same weak
  `sync()` closure the stack uses (`:88-93`).
- `library_chrome_tests.rs:104` stays exactly as it is — it asserts while the
  Doctor **root** shows, which the amendment leaves true.

*Proof:* extend `doc_7c_the_doctor_uses_the_shared_window_chrome` (`:52-115`)
with `assert!(chrome.search_toggle.is_visible())` after the review page is
pushed and `assert!(!chrome.search_toggle.is_visible())` after it is popped.

### Task 10 — Wiring: register the Doctor's query sink

Files: `section_search_wiring.rs`, plus the page's public surface in
`review_page.rs`.

- Add a field to `SectionSearchViews` (`:17-24`) for the Doctor launcher and
  `install_library_doctor(search, launcher)` modelled line-for-line on
  `install_concerts` (`:194-224`): `set_on_search_query_changed` one way,
  `search.register(SearchScope::DoctorReview, apply, commit, clear_facets)` the
  other. Every handle weak, as every existing install does. Add the call to
  `install` (`:26-33`).
- **The review page does not exist at window construction, twice over.** The
  coordinator is deferred (`library_doctor/mod.rs:227-232`,
  `deferred_after_quiet`), and even then the review page is built lazily inside
  `open_review` (`:637-695`, `if existing.is_none()`) into
  `review: RefCell<Option<Rc<LibraryDoctorReviewPage>>>` (`:155`). All three
  closures must resolve through the launcher and do nothing when the page is
  absent. Registering at construction is still correct — `register`
  (`section_search.rs:138-153`) only stores closures. **Do not** call
  `Deferred::get()` from the registration path; that would undo the startup
  deferral. Reach the page only from inside the closures, once the user has
  opened the Doctor.
- **(contract)** on `LibraryDoctorReviewPage`: `pub(in crate::ui) fn
  set_search_query(&self, query: &str)`, `set_committed_search_query(&self, query:
  &str)`, `clear_all_filters(&self)`. The first forwards to
  `ReviewState::set_query`, the second to `ReviewFilterBar::set_committed_query`,
  the third clears the query **and** resets the category toggle to "All"
  (FIL-2a, `docs/ux-rules.md:1564-1580`).

*Proof:* `doc_12a_the_shared_entry_reaches_the_review_page`,
`doc_12a_clear_all_drops_both_the_query_and_the_category`.

### Task 11 — Rules

File: `docs/ux-rules.md`.

Insert DOC-12a after DOC-6c ends at **`:4699`**, immediately before `## Z.
Single-pane track browser` at `:4700` — DOC-11a (`:4676`) is the highest *id* but
not the last rule. Verified free: `DOC-12` and `DOC-12a` appear nowhere in the
repository, and the id matches the parser at `check-ux-traceability.sh:24-25`.

```
- **DOC-12a** [active] [gtk] — **The review list is searchable, and the search
  is a filter like any other.** Ctrl+F and the header lens open the shared
  search popover on the Doctor's Review page and nowhere else in the Doctor;
  the query matches track, album and artist, case-insensitively and mid-word,
  and never the normalized album key, the field caption, or the current and
  proposed values. Search and the category tabs compose: a row must satisfy
  both. The result is an active filter in the sense of DOC-9d — Apply writes
  only the matching set, `All` and `None` operate only on it, the header counts
  it, and the footer states the scope. A row the query hides keeps its
  selection and stays out of the plan until the query is removed, exactly as
  under a category filter; there is no extra confirmation and no search-only
  label. A query with no matches shows its own state, naming the query and the
  number of fixes it is hiding and offering to clear it — never the
  nothing-to-review success page. Leaving Review drops the query, entry and
  chip in one step, as a section switch does (SEARCH-8a).
  *Tests:* `doc_12a_the_review_search_matches_track_album_and_artist`,
  `doc_12a_search_and_category_compose_as_an_intersection`,
  `doc_12a_apply_writes_only_the_searched_set`,
  `doc_12a_a_query_with_no_matches_shows_its_own_state`,
  `doc_12a_leaving_review_drops_the_scope_and_the_query`.
```

Amend DOC-7c by appending to it in the style of DOC-3a (`:4179-4183`) and DOC-3c
(`:4211-4212`) — two-space indent, one italic run, no blank line before it. The
amendment must reconcile **both** affected sentences (`:4396`, `:4397-4398`) or
the rule contradicts itself:

```
  *Amended 2026-08-15: the search action is the single exception to both
  sentences above. It stays hidden on Start and Result and is revealed on
  Review, which is searchable per DOC-12a, so the clause "Review places only
  its 'All' and 'None' actions there" reads "only its selection actions and
  its search action". The Library-only source title and the scan action stay
  hidden on every Doctor page.*
```

*Proof:* `scripts/check-ux-traceability.sh`. DOC-12a is `[active]`, so it needs
≥ 1 `fn doc_12a_…` with `#[test]` within five lines above (`:43-49`). Note the
strictness at `:99-107`: on an `[active]` rule the only acceptable ignore
attribute is the literal `#[ignore = "requires a display; run via xvfb-run"]`.

### Task 12 — Tests

File: **new** `library_doctor/review_search_tests.rs`, declared in
`review_page.rs` beside the existing test modules (`:716-726`).

> **Do not add tests to `review_page_tests.rs`** — it is at **725** of 799.
> `review_refresh_tests.rs` (387) and `review_page_perf_tests.rs` (232) have
> room, but keep the new surface in one file so it reads as a unit.

Collect here every `doc_12a_*`, `doc_9d_*` and `doc_3c_*` name promised by tasks
3–10 that is not assigned to another file. Follow the fixture route
`review_page_perf_tests.rs` already uses: build with
`LibraryDoctorReviewPage::new` over
`super::super::review_row::contract_tests::{scan, conflict_scan}` (`:13`,
`page_for` at `:73-86`), read filtered rows back through
`page.state.visible_rows()` (`review_page.rs:154-159`, which reads `self.sorted`
and therefore reflects the filter), and mark every widget-tree test with
`#[ignore = "requires a display; run via xvfb-run"]` plus
`crate::ui::test_main_context::lock_main_context()`.

One test here guards a promise nothing else enforces:

- `fil_1d_the_review_chip_names_exactly_the_fields_the_search_reads` — assert
  that `scoped_search_chip_label(SearchScope::DoctorReview, "x")` names track,
  album and artist, **and** that a row matching only on `album_key`, `field`,
  `current` or `proposed` is not visible. FIL-1d forbids a chip from claiming a
  field the view does not search and from searching one it does not name; this
  closes both halves in one place so the two lists cannot drift apart.

### Task 13 — The performance seam

File: `library_doctor/review_page_perf_tests.rs` (232 lines, ample room).

- **Both existing probes run unchanged and must stay green.**
  `review_selection_toggle_touches_only_the_toggled_album` (`:112-167`, budget
  `MAX_TOGGLE_CHURN = 24` at `:23`) and `review_selection_toggle_wall_clock_probe`
  (`:169-193`) prove search did not disturb the incremental selection path. Do
  not relax either budget; if search makes them fail, the design was implemented
  wrong, not the budget.
- Add `doc_12a_a_query_change_splices_no_store_items`: reuse the
  `connect_items_changed` counter from `:124-128`, set a query hiding most rows,
  then clear it, and assert the counter is **exactly 0** across both. **This
  budget is not guessed** — zero store churn is the design invariant of option
  (c), so the number is derivable, and any non-zero result means the query
  reached a splice path.
- Add `review_search_wall_clock_probe`, gated on `REPRISE_DOCTOR_PERF_ALBUMS`
  like `:172-181`. Measure median and max of `set_query` across a fixed
  keystroke sequence (a growing prefix, then one clear) and print
  `PERFORMANCE doctor_review path=search albums=… rows=… median_us=… max_us=…`
  in the existing format (`:185-192`). **It lands with no budget.** No prior
  measurement of a `filter.changed()` pass on this model exists anywhere in this
  tree, and inventing a threshold would be a fabricated number. The first real
  run (§7) sets it; record it here and add the assertion in a follow-up.

#### The probe must fail loudly when it is not measuring

A hard requirement, from a measured failure. On the strand-B run,
`review_selection_toggle_wall_clock_probe` under `xvfb-run -a dbus-run-session
-- cargo test` finished **green in 0.17 s**, printed **no** `PERFORMANCE` line,
and left **zero** `DOCTOR_REVIEW_REFRESH` lines in the log.
`REPRISE_DOCTOR_PERF_ALBUMS` never reached the test process and **the cause is
still unknown**. The existing probe swallows this at `:172-174`:

```rust
let Ok(album_count) = std::env::var("REPRISE_DOCTOR_PERF_ALBUMS") else {
    return;                     // green, silent, measured nothing
};
```

A green run that measured nothing is worse than a red one, because it is
reported as evidence. The new probe must therefore:

1. **Variable unset:** print exactly one stderr line —
   `PERFORMANCE doctor_review path=search skipped=REPRISE_DOCTOR_PERF_ALBUMS-unset`
   — and return. A skip that names itself can be grepped for; a bare `return`
   cannot.
2. **Set but the measurement does not come about** — the fixture builds zero
   rows, the keystroke sequence yields no timings, or `set_query` is never
   reached — `panic!` naming which of those it was. Never return green.
3. **Malformed value:** panic, as `:175-181` already does.

**Check the variable actually arrives before trusting any number.** Run the probe
once with a deliberately invalid value and confirm the run turns **red**:

```
REPRISE_DOCTOR_PERF_ALBUMS=not-a-number \
  xvfb-run -a dbus-run-session -- \
  cargo test -p reprise-gnome --locked review_search_wall_clock_probe -- --ignored --nocapture
```

If that passes, the variable is not reaching the process and **every number from
this probe is meaningless** — fix the plumbing before reading a median. Then run
it with a valid count and confirm the `PERFORMANCE` line appears; `--nocapture`
is required, since `eprintln!` from a passing test is swallowed without it.

Leave `review_selection_toggle_wall_clock_probe` alone — its numbers are cited in
#505/#506, and hardening it is a separate change.

---

## 5. File budget

The gate is `scripts/check-architecture.sh:20` — `if (( lines >= 800 ))`, over
`find crates -name '*.rs'` (`:24`), **including** test files. A file fails **at**
800, so the budget is ≤ 799. "Now" is `wc -l` in the worktree; "after" is an
estimate.

| File | Now | After | Left |
| --- | ---: | ---: | ---: |
| **Prerequisite PR** | | | |
| `library_doctor/mod.rs` | 781 | **≈754** | 45 |
| `library_doctor/review_css.rs` | — | ≈50 | new |
| **Feature PR** | | | |
| `library_doctor/mod.rs` | ≈754 | ≈760 | 39 |
| `library_doctor/review_page.rs` | 726 | ≈761 | 38 |
| `library_doctor/review_search.rs` | — | ≈200 | new |
| `library_doctor/review_search_tests.rs` | — | ≈420 | new |
| `library_doctor/review_snapshot.rs` | 217 | ≈290 | 509 |
| `library_doctor/review_filter_bar.rs` | 164 | ≈215 | 584 |
| `library_doctor/review_summary.rs` | 56 | ≈95 | 704 |
| `library_doctor/navigation.rs` | 145 | ≈160 | 639 |
| `library_doctor/review_page_tests.rs` | 725 | **725** | frozen — do not touch |
| `library_doctor/review_page_perf_tests.rs` | 232 | ≈340 | 459 |
| `library_doctor/review_header.rs` | 561 | 561 | untouched |
| `library_doctor/review_model.rs` | 523 | 523 | untouched |
| `core/…/review.rs` | 733 | **≈768** | 31 |
| `core/…/review_tests.rs` | **799** | **799** | **0 — do not touch** |
| `core/…/review_query_tests.rs` | — | ≈230 | new |
| `core/…/library_doctor/mod.rs` | 41 | 43 | 756 |
| `reprise-view/search_scope.rs` | 151 | ≈160 | 639 |
| `reprise-view/strings/browse.rs` | 275 | ≈285 | 514 |
| `ui/filter_bar_strings.rs` | 183 | ≈187 | 612 |
| `ui/strings_library_doctor.rs` | 777 | **777** | frozen — new strings elsewhere |
| `ui/strings_library_doctor_search.rs` | — | ≈60 | new |
| `ui/strings.rs` | 759 | 762 | 37 |
| `window/section_search.rs` | 393 | ≈420 | 379 |
| `window/section_search_wiring.rs` | 297 | ≈345 | 454 |
| `window/library_chrome.rs` | 223 | ≈245 | 554 |
| `window/library_chrome_tests.rs` | 437 | ≈455 | 344 |
| `window/window.rs` | — | +1 | its own 600-line cap, `:26-31` |
| `window/window_runtime_wiring.rs` | — | +3 | — |

Four files carry real risk:

1. **`core/…/review_tests.rs` at 799 — zero headroom.** One line fails the gate;
   task 1 creates `review_query_tests.rs` for exactly that.
2. **`ui/strings_library_doctor.rs` at 777 — 22 lines** against ~37 needed. It
   does not fit; task 6 creates a new file rather than trimming strings.
3. **`library_doctor/mod.rs` at 781 before the prerequisite PR — 18 lines.** Task
   0 must land first; it is the only reason tasks 8 and 10 have room.
4. **`core/…/review.rs` at 733 — 66 lines.** Task 1's estimate is ~35; it fits
   with no room for a second helper.

`review_page.rs` looks tight at ≈761 and is safe *because* tasks 4 and 5 put the
logic in `review_search.rs` as a child module. Inlining `set_query` or the
no-match page there crosses 800 and fails the gate — the intended early warning.

---

## 6. Gates

From the worktree root. **Check each against `origin/dev` before treating a
failure as this branch's fault** — `check-frontend-thinness.sh` has known red
stages on `dev`, and the display suite is herd-flaky.

| Command | What it proves here |
| --- | --- |
| `scripts/check-architecture.sh` | Every `.rs` under `crates/` stays ≤ 799 (`:20`). The gate most likely to trip: `mod.rs`, `review_page.rs`, `review_tests.rs`, `strings_library_doctor.rs` and `review.rs` are all near the line. Also the 600-line caps on `window.rs` and the UI orchestrators (`:26-40`) — task 9 touches `window.rs`. |
| `scripts/check-ux-traceability.sh` | DOC-12a is `[active]`, so ≥ 1 `fn doc_12a_…` with `#[test]` within five lines above (`:43-49`); confirms the amended DOC-7c still has its test. Only the exact display-ignore string counts (`:99-107`). |
| `scripts/check-display-tests.sh` | Runs every `#[ignore]` test in `reprise-gnome` via `cargo test -p reprise-gnome -- --ignored`. New display tests need **no registration** — they are found by name. This is where the `doc_12a_*` widget tests, the route tests, the amended `doc_7c_…` and the zero-churn probe actually execute. **Not optional:** the plain `cargo test` below never runs them. |
| `scripts/check-frontend-thinness.sh` | The `reprise-view` production floor — task 2 adds to it, so this helps. The `rusqlite`/filesystem/thread/worker ceilings on `reprise-gnome`: this change adds none and must not start. |
| `cargo clippy --locked --all-targets --workspace -- -D warnings` | The new `SearchScope` variant makes `search_fields()` (`browse.rs:77-88`) non-exhaustive, so rustc catches any match this plan missed; and a borrow-ordering mistake in tasks 5 and 7 surfaces here as a compile error rather than a runtime panic. |
| `cargo test --locked --workspace --exclude reprise-platform-linux` | The non-display suite: `review_query_tests.rs`, the snapshot arithmetic, the footer strings, the two scope arrays, the `search_scope.rs` loop. |

Post-merge, two things remain and neither can run inside the branch: the
acceptance run in §7, and **the gate list against `origin/dev`** — run all six on
the merge commit and on `origin/dev` and compare. Red in both is not this
change's fault; red only here is.

---

## 7. Acceptance after landing

Numbered and owed, not a footnote. The synthetic fixture cannot answer the
debounce question: 16×12 generated albums are not a 254 MB library, and the
`tracing` instrumentation this plan adds does not exist at all in a test
process. These points need the real library, a person at the GUI and the session
reading the log — the shape of §F-4(c)/(d) of strand B. The harness exists and
needs no rebuilding:

- `~/.cache/reprise-doctor-b0-harness/doctor-b0-run.sh` — builds, copies the
  binary to a name containing no "reprise" (other sessions' cleanups run
  `pkill -f reprise`), launches it under `setsid` with an isolated profile, and
  writes `~/.cache/doctor-b0/*.log`.
- `…/doctor-b0-medians.sh <log>` — per-stage and whole-path medians.
- `…/ACCEPTANCE-strand-b.md` — the full procedure, including the pre-flight
  ("quit your own Reprise; a copy taken while the app holds the WAL is a
  different database").

**A-1 — Fast enough to ship without a debounce.** On the real library open
Review, type a five-character prefix one key at a time, clear with Escape,
repeat five times. Read the `path="search"` medians. **This point's result is the
debounce decision** against the 16.7 ms design target of §2: above it, add the
debounce as specified; below it, record the number here and add the assertion to
`review_search_wall_clock_probe`.

**A-2 — The selection path did not regress.** Same session: toggle an album
header checkbox five times with an active query and five times without. The
`path="selection"` medians must match the strand-B fix arm (≈13.6 ms) and the
`touched` counts must be unchanged. A changed number that still passes the
synthetic budget is still a regression signal.

**A-3 — The list does not move.** With a query active, scroll so an album header
sits mid-window, screenshot, toggle one row, screenshot. No displacement. The
scroll-anchor half cannot be checked from a test.

**A-4 — The no-match state appears and is escapable.** Type a query matching
nothing. Confirm the `"no-match"` child — magnifier, query in the title, hidden
count in the description — and **not** the green "No Changes to Review" page.
Click its clear button and confirm the full list and an empty header entry come
back in one step.

Two traps that already cost time on this harness:

- **The log fields are ANSI-coloured, in files too.** `grep -F 'stage="search"'`
  returns zero against a log that contains it. Strip escapes first —
  `sed 's/\x1b\[[0-9;]*m//g'` — and match the **message text**
  (`DOCTOR_REVIEW_REFRESH path`), which carries no escapes. Both harness scripts
  do this; a hand-rolled `grep` will not.
- **`tracing::debug!` output does not exist in test runs.** The subscriber is
  installed in the app's `main()`, so every stage timing here comes **only** from
  a real app run. A test that "checks the log" is checking an empty string.

---

## 8. Out of scope

- **The 66 remaining `DOC-9b` warnings** from `review_header.rs`. They fire on
  scroll, not on toggle, and are unrelated to search. Tracked elsewhere.
- **The unexplained monotonic growth of the full `refresh()` path** (13 → 667 ms
  and beyond). Routed around, as in #505/#506; diagnosing it is separate work.
- **Searching `current` and `proposed`.** Argued in §2; a follow-up needs its own
  chip text under FIL-1d and its own answer to "a match on text not in the
  library yet".
- **Search on Start and Result.** The amended DOC-7c keeps the lens hidden there,
  and neither page has a list.
- **Persisting the query.** Queries stay session-scoped; leaving Review drops
  the query (task 8).
- **Highlighting matched substrings.** No view in the app does this;
  `search_scope.rs` has no highlighting API (`:64-82`) and building one is a
  cross-app change.
- **Hardening `review_selection_toggle_wall_clock_probe`.** Its numbers are cited
  in #505/#506; only the new probe gets the stricter contract (task 13).

---

## Parallelität

**No cut. One strand, one PR** — after the prerequisite PR of §3, which is
sequenced before the feature branch rather than parallel to it.

This is not an oversight. The draft proposed two waves — a "Strand A" of the two
additive crate APIs (tasks 1 and 2), then everything else. That split is
withdrawn for four reasons; the first three are structural, the fourth is why
even the surviving split is not worth running.

1. **`review_page.rs` is the junction.** Tasks 4, 5, 7 and 10 all edit it — the
   query field, the lookup filter, the third stack child, the session push, the
   public API. Any strand not owning it cannot write its own entry point, and the
   file sets stop being disjoint.
2. **`mod.rs` had 18 lines.** Task 0 must land before tasks 8 and 10 both write
   there, so they serialise on it however the rest is arranged — which is exactly
   why it became its own PR instead of a wave.
3. **Nothing is provable until the whole chain lands.** Every `doc_12a_*` test
   exercises entry → wiring → route → page → snapshot → session → footer. Split
   strands would each merge unverifiable and the first green run would come after
   the last merge — the failure mode
   `handoff-evidence-lists-inherit-their-gaps` describes, where six green gates
   were reported and the suite that mattered had never run.
4. **The proposed wave 1 buys no wall-clock at all.** It blocks wave 2
   **completely**: wave 2 cannot compile without `set_query_scope`,
   `query_scope_matches` and `SearchScope::DoctorReview`. The two waves run
   strictly in sequence, so total elapsed time is unchanged — while the split
   costs a second `/code` launch, a second review pass and a second landing
   cycle. The purpose of a cut in this pipeline is wall-clock; a fully serial cut
   has none.

The genuinely parallel work here is the prerequisite PR of §3, and it is already
separated: it can be reviewed and landed while this plan is still being read.
