---
slug: library-doctor-redesign
worktree: /home/marvin/Projects/reprise-library-doctor-redesign
branch: feature/library-doctor-redesign
phase: refactored
codex_session:
created: 2026-08-05
---
# Library Doctor — redesign implementation plan

Base: `origin/dev` = `d0054ab5b5`, worktree `feature/library-doctor-redesign`.
Sources this plan implements, in precedence order:

1. `docs/plans/library-doctor-redesign-brief.md` — the commissioned change. Canonical.
2. `docs/plans/library-doctor-design-frames.md` — what the mockup shows, frame by frame.
3. `docs/plans/library-doctor-inventory.md` — the state of the code before this plan.
   Its line numbers were spot-checked; the corrections are recorded in §F.

Binding repository contracts that outrank this plan: `AGENTS.md`, `docs/ux-rules.md`,
`TESTING.md`. Everything below is written so that a reader with no other context can
execute it.

**§F is not a list of options.** Every question this plan ever had is decided there, in the
indicative. Six of those decisions were made by the product owner and are marked **U-1** …
**U-6**; the rest are engineering decisions with their reasoning attached. Nothing in §F is
open, and nothing in it may be re-litigated during implementation. Where a decision
deliberately departs from the mockup, §F says so and gives the reason — the implementer
does not see the mockup and must not "restore" the frame.

---

## A. What changes and why

Today the Library Doctor is reachable from two doors and speaks in the vocabulary of its
own implementation. The ⋮ menu carries a `Library Doctor` item, and Preferences → Plugins
carries a second, fuller one: an `adw::ExpanderRow` with a scope dropdown, the remote
switch, a static "local fixes always included" line, `Run Scan Now` and
`Revert Last Cleanup` (`crates/reprise-gnome/src/ui/preferences/preference_library_doctor.rs`,
`plugin_row()` at line 255, registered from `preference_plugins.rs:423` under
`LOCAL_PLUGIN_IDS` at line 22). After a scan, the result page is a flat
`adw::PreferencesGroup` of nine `adw::ActionRow`s — four fixed rows plus a hidden cleanup
row plus five problem classes (`summary_page.rs:184–205`) — in which "Safe · local,
preselected", "Suggestions · review" and "Tracks Checked · skipped" sit at the same visual
rank, although one is a finished fact, one is a decision and one is a statistic. The
review page then renders every finding as a flat `GtkListView` row that carries its own
four column captions, because each row wraps its cells in a per-row
`adw::BreakpointBin` (`review_row.rs:113–186`, captions built in `value_widgets()` at
line 188). At 1,105 findings that is 4,420 caption labels, one shared meaning repeated
per row. Nothing is applied without asking, so the unambiguous cases — a stray space, a
casing fix, a missing MusicBrainz recording ID that changes nothing a human can see —
consume the same attention as a genuine judgement call.

The target state shrinks the feature to what a player can do that a dedicated tagger
cannot: make the library internally consistent, and decide the unambiguous cases itself.
There is exactly one entry point, the ⋮ menu; the sidebar carries the noun — a
`Library Doctor` row under `ISSUES` that exists only while a completed scan has
unreviewed findings. The scan produces two tiers instead of three: everything that is
local, preselected and not stale, plus every `DoctorField::RecordingMbid` proposal
regardless of tier, is enqueued as a tag-write job the moment the scan completes and
reported as done; everything else is shown for review, preselected, grouped by album,
under one column header for the whole page. Unresolved spelling groups keep their model
but move to the bottom of the review page, marked optional and skippable. The Doctor gets
its own start page (scope as a segmented control, the remote toggle, `Run Scan Now`, and
the only remaining home for `Revert Last Cleanup`), its own post-apply page and its own
"nothing to fix" state; progress stays in the sidebar card it already uses. Undo becomes a
bracket: the quietly applied job and the reviewed job of the same `scan_id` revert
together. The same feature becomes operable from `crates/reprise-mcp` behind a new
`tags:write` capability, writing through the same job queue, so a scan an agent ran
produces the same sidebar entry and a GUI Undo reverts an MCP apply.

The boundary is explicit. **The scan engine stays untouched**: `scan.rs`,
`local_rules.rs`, `scope.rs` and the tier-neutral proposal shape in `types.rs` keep their
behavior; the only edit to `scan.rs` is the addition of the post-scan auto-apply hook
described in P4, which calls existing write code and changes no detection. **The
MusicBrainz/AcoustID clients and the remote cache stay untouched**: everything under
`crates/reprise-core/src/library/library_doctor/remote/` (~2,388 lines, orchestrator,
providers, cache, arbitration) is out of scope, as are DOC-1b and DOC-1c, the request
allowlist, the rate limits and the 30-day/7-day cache TTLs. **The tag-write job machinery
stays untouched in its mechanism**: the journal, the guarded write primitive, the
conflict/unavailable/failed classification and crash recovery keep their contracts; this
plan adds a cross-process lock in front of job creation and a paired revert on top of the
existing single-job revert, and changes nothing about how a single file is written. This
is a surface and a policy change.

### One counting rule, everywhere

Read this before implementing any number. **Every count the user sees is a count of tag
changes that will be written, or were written.** A tag change is one `(track_id, field)`
pair. That unit is the same in the summary headline, in the per-kind lines, in the album
pill, in the review toolbar, in the footer, on the post-apply page, in the sidebar badge
and in every MCP response. A display row that collapses an album-level change into
`All 11 tracks` is worth **eleven**, not one. The consequence is deliberate and is
described in §F U-1: the numbers this implementation produces are larger than the numbers
printed in the mockup frames. That is correct behavior, not a regression, and nobody
should "fix" it.

---

## Stages — three Codex runs, not one

The implementation is cut into three runs. Between the runs the suite and a review pass
happen; a run must not start before the previous stage's acceptance is green.

Each stage below states three things: which packages it contains, one acceptance block
that can literally be pasted into a shell, and what state the worktree is in when the
stage ends — in particular whether the workspace compiles and the suite is green. The
answer is **yes in all three cases**; the work has been ordered to make that true, and
where an ordering constraint exists to keep it true it is called out.

### Stage 1 — Core and policy (P1, P2, P3, P4, then the Stage-1 sweep)

The core API changes shape here: the tier filter, the summary field names, the selection
preset, the cleanup return types. Every one of those names has a caller inside
`reprise-gnome`, so **Stage 1 cannot end at P4** — it would leave a workspace that does
not compile. It ends with a mechanical sweep instead.

**Order inside the stage.** P1 first. P2 and P3 in parallel after P1. P4 after P1 and P3.
Then the sweep, sequential, single writer.

**The Stage-1 sweep (`S1`) — mechanical only, no design.** It touches GTK files that
Stage 2 packages own; that is safe because the stage boundary is sequential and no Stage-2
package has started. The full list of call sites, verified against `d0054ab5b5`:

| Site | Change |
| --- | --- |
| `library_doctor/mod.rs:163` | `DoctorReviewFilter::AllChanges` → `NeedsReview` |
| `library_doctor/mod.rs:172` | `DoctorReviewFilter::LocalSafeOnly` → `AutoApply` |
| `library_doctor/review_page.rs:485` (test) | `AllChanges` → `NeedsReview` |
| `library_doctor/review_page.rs:223, 304–321, 362, 402` | `session.all_safe()` → `session.all()`; the button keeps its `DOCTOR_ALL_SAFE` label until P8 renames it |
| `library_doctor/summary_page.rs:33, 45, 66, 95, 395, 415, 418, 420, 421, 502, 514, 523` | `safe_changes` → `auto_applied_changes` |
| `library_doctor/summary_page.rs:411, 507` | `counts.safe` → `counts.auto_applied` |
| `library_doctor/mod.rs:512` | `cleanup.track_count` — unchanged field, no edit needed; verify it still compiles against the new `DoctorCleanup` |
| `preferences/preference_library_doctor.rs:300` | `last_cleanup().ok().flatten().is_some()` — unchanged, verify only |
| `library_doctor/jobs.rs:59–75` | `run_revert` returns `Option<DoctorCleanupReport>` instead of `Option<DoctorWriteReport>`; its caller in `mod.rs` reads the summed counters (`reverted_tracks`, `failed_tracks`, `conflict_tracks`, `unavailable_tracks`, `cancelled`) that `DoctorCleanupReport` carries for exactly this reason |

The sweep may also adjust assertions in `summary_page.rs`'s own tests that pin the old
three-tier numbers. **Adjust the assertion, never the core predicate** — if a GTK test now
disagrees with `is_auto_applied()`, the test is the thing that is out of date.

**Acceptance (Stage 1)**
```sh
cargo fmt --check
cargo clippy -p reprise-core --all-targets -- -D warnings
cargo test -p reprise-core library_doctor:: queries::doctor db::
cargo test --workspace
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'   # must print nothing
scripts/check-ux-traceability.sh
scripts/check-architecture.sh
```

**State at the end of Stage 1.** The workspace compiles, `cargo test --workspace` is
green, and the app runs. The GUI is still the old surface: the Preferences row still
exists, the summary is still nine `ActionRow`s, the review page still repeats its captions
per row. What already changed underneath: `RecordingMbid` proposals never reach the review
list, every reviewable row arrives preselected, the undo bracket covers a whole scan, a
second tag-write job is refused in the database, and the schema is at v57 with the stored
scan pointer cleared. `apply_auto_tier()` exists but no GUI action calls it yet — that is
P7. So after Stage 1 the Doctor writes nothing on its own; it is the old flow on the new
core. `docs/ux-rules.md` still describes the old *surface* for the GTK-level rules
(DOC-2b, DOC-2c, DOC-3b, DOC-7b); that is intentional and the traceability gate tolerates
it, because those rules only flip when the surface they describe changes, in Stage 2.

### Stage 2 — Surface (P5 first, then P6, P7, P8 in parallel)

**P5 must land before P6–P8 start.** It is the only writer of
`strings_library_doctor.rs` in this stage, and the three surface packages consume its
constants. This is a change against the draft ordering, where P5 ran alongside the core:
with a three-run cut there is no parallel window across stages, so P5 becomes the opening
step of Stage 2 instead of a companion of Stage 1.

**The one thing Stage 2 would otherwise break.** P5 adds constants that nothing consumes
yet, and P6–P8 orphan constants that P10 only removes in Stage 3. `reprise-gnome` is a
binary-only crate, so unused `pub` items are `dead_code`, and the workspace clippy gate
runs with `-D warnings`. Verified: `strings_library_doctor.rs` has **no** unused symbol
today and therefore carries no blanket allow, unlike `strings_news.rs`,
`strings_releases.rs`, `strings_concerts.rs`, `strings_radio.rs` and
`strings_podcasts.rs`, which all start with `#![allow(dead_code)]`. The fix is stated as a
decision in §F-20: P5 adds `#![allow(dead_code)]` to the top of the file, and P10 removes
it again after the removal list has been applied. Without that line, Stage 2's acceptance
cannot be green; with it, both the not-yet-consumed and the not-yet-removed states are
lint-safe.

**Shared contracts fixed by this plan so the three packages need not wait on each other:**
the moved remote toggle lives at `crate::ui::library_doctor::remote_toggle` (P6 moves it,
P7 and P8 import from there), and `open_review()` loses its filter parameter (P7 changes
the signature, P8 codes against the new one).

**Acceptance (Stage 2)**
```sh
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
scripts/check-display-tests.sh --filter doc_
scripts/check-accessibility-semantics.sh
scripts/check-ux-traceability.sh
scripts/check-architecture.sh
xvfb-run -a cargo run -p reprise-gnome --example doctor_review_sections_repro
```

**State at the end of Stage 2.** The workspace compiles, the suite is green, and the whole
GUI redesign is live and usable end to end: one entry point, the start page, the sidebar
ISSUES row, the three-block summary, the grouped review page, post-apply, nothing-to-fix,
the playlist quick-add. `preference_library_doctor.rs` and `job_page.rs` are gone.
`crates/reprise-mcp` is untouched and still has no Doctor tools. `strings_library_doctor.rs`
still contains the dead constants from the removal list and the temporary
`#![allow(dead_code)]`. `scripts/check-frontend-thinness.sh` will now fail as a **floor**
violation, because two files were deleted and the budgets still name the old counts — that
is expected and is P10's job; it is therefore deliberately **not** part of Stage 2's
acceptance block.

### Stage 3 — Agent adapter and gates (P9, then P10)

P9 adds the three MCP tools and the `tags:write` capability against a core API that Stage 1
froze and Stage 2 exercised. P10 removes the dead strings and the temporary allow,
regenerates the catalogs, re-measures the thinness budgets and appends the ledger line.

**Acceptance (Stage 3) — the full gate battery, and it is the release gate for this branch**
```sh
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo test -p reprise-mcp
scripts/check-ux-traceability.sh
scripts/check-frontend-thinness.sh
scripts/check-accessibility-semantics.sh
scripts/check-architecture.sh
scripts/check-display-tests.sh
cargo audit
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'   # must print nothing
```

**State at the end of Stage 3.** Everything above is green, the feature is complete on both
surfaces, `strings_library_doctor.rs` contains no dead constant and no blanket allow, and
`docs/ux-rules.md` §Y describes exactly what the code does.

---

## B. Work packages

Ten packages across three stages. Core and policy first, then the surface, then the MCP,
then the gate cleanup. Every package lists the files it owns exclusively, its dependencies,
its rule-named tests, and one acceptance command that can be run.

### Shared declaration files — the one documented exception

Four files are append-only registries, not implementation. Several packages add one line
each; **no package may remove or rewrite another package's line there**:

- `crates/reprise-core/src/library/library_doctor/mod.rs` (`mod …;` / `pub use …;`)
- `crates/reprise-gnome/src/ui/library_doctor/mod.rs` — only its `mod …;` block; the
  coordinator body below it is owned by P7
- `crates/reprise-mcp/src/main.rs` (`mod …;`)
- `po/POTFILES.in`

`docs/ux-rules.md` follows the repository's own workflow: each package edits exactly the
rule blocks it flips or rewrites, and never another package's block (AGENTS.md, "UX rules
are binding"). One sequential exception, allowed because the packages are in different
stages: P9 appends its two MCP test names to the test list of **DOC-10b**, which P3 wrote
in Stage 1.

`crates/reprise-gnome/src/ui/strings_library_doctor.rs` is owned by **P5 for additions**
and by **P10 for removals**. P5 opens Stage 2, P10 closes Stage 3; they are never in flight
at the same time. No other package edits that file.

### Ordering and parallelism

```
Stage 1 — core                         Stage 2 — surface              Stage 3 — agent + gates
P1 ──┬── P2 ──┐                        P5 ──┬── P6 ──┐
     └── P3 ──┴── P4 ── S1 sweep ──▶        ├── P7 ──┼──▶            P9 ── P10
                                            └── P8 ──┘
     (stage acceptance + review)       (stage acceptance + review)
```

- **P1** must land first; P2 and P3 are parallel with each other after P1.
- **The S1 sweep** closes Stage 1 and is the only sequential step there.
- **P5** opens Stage 2 and must be complete before P6–P8 begin.
- **P6, P7, P8** are parallel with each other, with one caveat: P7 and P8 both append to
  the `mod` block of `crates/reprise-gnome/src/ui/library_doctor/mod.rs`.
- **P9 comes after the surface**, as the brief orders it: the GTK packages are the ones
  that discover whether the core API from P1–P4 is shaped right, and the MCP consumes a
  settled API rather than a guessed one.
- **P10** is last and depends on everything.

---

### P1 — Core: two tiers instead of three

**Stage** 1.

**Owns**
- `crates/reprise-core/src/library/library_doctor/review.rs`
- `crates/reprise-core/src/library/library_doctor/review_tests.rs`
- `crates/reprise-core/src/library/library_doctor/presentation.rs`
- `docs/ux-rules.md` — the **DOC-3a** and **DOC-8b** blocks only

**Depends on** — nothing.

**Delivers (tests)**
- `doc_8b_auto_applied_tier_is_local_preselected_plus_every_recording_mbid` (core)
- `doc_8b_stale_rows_are_never_auto_applied` (core)
- `doc_8b_review_tier_preselects_every_ready_row` (core)
- `doc_8b_recording_mbid_never_reaches_the_review_tier` (core)
- `doc_8b_all_preset_selects_every_ready_row_and_none_clears_them` (core)
- `doc_2b_summary_reports_auto_applied_review_and_conflicts_separately` (core, rewrite of
  the existing `doc_2b_summary_separates_safe_review_classes_and_unresolved_groups`)

**Red first** — `doc_8b_auto_applied_tier_is_local_preselected_plus_every_recording_mbid`
must fail to compile (`DoctorReviewFilter::AutoApply` does not exist) before any
implementation.

**Acceptance**
```sh
cargo test -p reprise-core library_doctor:: -- --nocapture
```
plus `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` empty.

---

### P2 — Core: album grouping projection

**Stage** 1.

**Owns**
- `crates/reprise-core/src/library/library_doctor/grouping.rs` (new)
- `crates/reprise-core/src/library/library_doctor/grouping_tests.rs` (new)
- one `mod`/`pub use` line in the core doctor `mod.rs`

**Depends on** P1.

**Delivers (tests)**
- `doc_9b_rows_group_by_album_in_scope_order` (core)
- `doc_9b_album_level_change_collapses_into_one_row_over_all_tracks` (core)
- `doc_9b_tracks_without_an_album_form_one_trailing_group` (core)
- `doc_9b_group_counts_report_written_changes_not_display_rows` (core)

**Red first** — `doc_9b_rows_group_by_album_in_scope_order` fails to compile
(`grouping::group_review_rows` does not exist).

**Acceptance**
```sh
cargo test -p reprise-core library_doctor::grouping
```

---

### P3 — Core: paired undo and the cross-process write lock

**Stage** 1.

**Owns**
- `crates/reprise-core/src/library/library_doctor/cleanup.rs` (new)
- `crates/reprise-core/src/library/library_doctor/cleanup_tests.rs` (new)
- `crates/reprise-core/src/library/library_doctor/write.rs`
- `crates/reprise-core/src/library/library_doctor/write_tests.rs`
- `crates/reprise-core/src/library/tag_write_lock.rs` (new)
- `crates/reprise-core/src/library/tag_write_job/store.rs`
- `crates/reprise-core/src/library/mod.rs` — one `mod tag_write_lock;` line
- one `mod`/`pub use` line in the core doctor `mod.rs`
- `docs/ux-rules.md` — the **DOC-10a** and **DOC-10b** blocks only

**Depends on** P1.

**Delivers (tests)**
- `doc_10a_undo_reverts_the_quiet_and_the_reviewed_job_of_one_scan` (core)
- `doc_10a_undo_works_when_only_the_quiet_job_exists` (core)
- `doc_10a_partial_revert_leaves_the_cleanup_available_for_a_second_attempt` (core)
- `doc_10a_cancel_between_jobs_does_not_start_the_remaining_job` (core)
- `doc_10a_a_fully_reverted_scan_is_no_longer_offered` (core)
- `doc_10b_a_second_tag_write_job_is_refused_while_one_is_prepared_or_running` (core)
- `doc_10b_a_finalized_interrupted_job_does_not_hold_the_lock` (core)
- `doc_10b_tag_editor_and_doctor_share_one_lock` (core)
- `doc_10b_gui_sees_the_same_refusal_while_an_mcp_job_runs` (core — the "foreign" job is
  simulated by inserting a `prepared` job row directly, which is exactly what a second
  process would leave behind)

**Red first** — `doc_10a_undo_reverts_the_quiet_and_the_reviewed_job_of_one_scan`:
build two `doctor_apply` jobs against one `scan_id`, call `revert_last_cleanup()`, assert
both are reverted. Against today's code it reverts only the newer one and must fail.

**Acceptance**
```sh
cargo test -p reprise-core library_doctor::cleanup library_doctor::write tag_write
```

---

### P4 — Core: auto-apply at scan completion, scan state, pending count, schema v57

**Stage** 1.

**Owns**
- `crates/reprise-core/src/library/library_doctor/scan.rs`
- `crates/reprise-core/src/library/library_doctor/store.rs`
- `crates/reprise-core/src/library/library_doctor/tests.rs`
- `crates/reprise-core/src/db_library_doctor.rs`
- `crates/reprise-core/src/db.rs` — `SUPPORTED_SCHEMA_VERSION` and one migration line
- `crates/reprise-core/src/queries/doctor.rs` (new)
- `crates/reprise-core/src/queries/mod.rs` — one `mod`/`pub use` line
- `docs/ux-rules.md` — the **DOC-10c** block only

**Depends on** P1, P3.

**Delivers (tests)**
- `doc_8b_scan_completion_enqueues_the_auto_applied_job_before_the_summary` (core)
- `doc_8b_a_scan_with_no_auto_rows_creates_no_job` (core)
- `doc_8a_pending_review_count_excludes_everything_already_written_for_that_scan` (core)
- `doc_8a_pending_review_count_is_zero_once_the_scan_is_marked_reviewed` (core)
- `doc_8a_conflicts_alone_do_not_produce_a_pending_count` (core)
- `doc_10c_upgrade_clears_the_stored_scan_pointer_and_keeps_the_cleanup_revertible` (core)

**Red first** — `doc_10c_upgrade_clears_the_stored_scan_pointer_and_keeps_the_cleanup_revertible`
opens a v56 fixture database with a `last_complete_scan_id` set, migrates, and asserts the
pointer is `NULL` while `last_cleanup()` still returns the job. Fails today because
migration 57 does not exist.

**Acceptance**
```sh
cargo test -p reprise-core library_doctor:: queries::doctor db::
```

---

### P5 — Strings and copy (additions)

**Stage** 2, and it opens the stage: P6, P7 and P8 must not start before it is complete.

**Owns**
- `crates/reprise-gnome/src/ui/strings_library_doctor.rs` (additions only; removals are P10)

**Depends on** — nothing in code terms; scheduled first inside Stage 2.

**Delivers (tests)** — none of its own. Strings are proven by the packages that consume
them; this package exists so that P6–P8 can run in parallel without fighting over one
file. It also adds `#![allow(dead_code)]` as the file's first line (§F-20) — without it
the not-yet-consumed constants fail `-D warnings`, because `reprise-gnome` is a
binary-only crate.

**Acceptance**
```sh
cargo clippy -p reprise-gnome --all-targets -- -D warnings
```

---

### P6 — GTK: entry points, sidebar, and the playlists section

**Stage** 2.

**Owns**
- `crates/reprise-gnome/src/ui/primary_menu.rs`
- `crates/reprise-gnome/src/ui/sidebar/sidebar.rs` (the `Shared` fields only)
- `crates/reprise-gnome/src/ui/sidebar/sidebar_rebuild.rs`
- `crates/reprise-gnome/src/ui/sidebar/sidebar_presentation.rs`
- `crates/reprise-gnome/src/ui/sidebar/sidebar_row_wiring.rs`
- `crates/reprise-gnome/src/ui/sidebar/sidebar_issues_section.rs`
- `crates/reprise-gnome/src/ui/sidebar/sidebar_playlist_creation.rs`
- `crates/reprise-gnome/src/ui/sidebar/sidebar_playlist_quick_add.rs` (new)
- `crates/reprise-gnome/src/ui/sidebar/sidebar_tests.rs`
- `crates/reprise-gnome/src/ui/preferences/preference_library_doctor.rs` (deleted)
- `crates/reprise-gnome/src/ui/library_doctor/remote_toggle.rs` (new — the surviving part)
- `crates/reprise-gnome/src/ui/preferences/preference_plugins.rs`
- `crates/reprise-gnome/src/ui/preferences/preference_plugins_tests.rs`
- `crates/reprise-gnome/src/ui/preferences/preferences.rs`
- `crates/reprise-gnome/src/ui/preferences/mod.rs`
- `crates/reprise-gnome/src/ui/toasts.rs`
- `docs/ux-rules.md` — the **DOC-8a** and **NAV-14** blocks only

**Depends on** P4 (pending count), P5.

**Delivers (tests)**
- `doc_8a_the_menu_carries_exactly_one_library_doctor_item_and_no_sync_device` (gtk)
- `doc_8a_the_issues_entry_appears_only_with_unreviewed_findings` (gtk)
- `doc_8a_quiet_fixes_produce_one_undo_toast_and_review_findings_produce_none` (gtk)
- `doc_7b_library_doctor_has_no_preferences_surface` (gtk — retarget of the deleted
  `doc_7b_library_doctor_is_available_without_an_activation_state`)
- `doc_6b_library_doctor_controls_explain_job_locking` (gtk — retarget, see §E)
- `doc_7a_first_remote_enable_requires_confirmation_and_cancel_stays_off` (gtk — moves
  with `remote_toggle.rs`, unchanged assertions)
- `nav_14_the_playlists_header_creates_a_playlist_in_place_without_a_dialog` (gtk)
- `nav_14_escape_discards_the_new_playlist_row_and_the_playlist` (gtk)
- `nav_14_an_empty_name_keeps_the_untitled_playlist` (gtk)
- `nav_14_import_playlist_lives_in_the_overflow_menu` (gtk)

**Red first** — `doc_8a_the_menu_carries_exactly_one_library_doctor_item_and_no_sync_device`
asserts the library section of the primary menu has exactly two items; today it has three.

**Acceptance**
```sh
cargo test -p reprise-gnome sidebar:: primary_menu:: preferences::
scripts/check-accessibility-semantics.sh
```

---

### P7 — GTK: start page, summary, post-apply, empty state, progress

**Stage** 2.

**Owns**
- `crates/reprise-gnome/src/ui/library_doctor/mod.rs` (coordinator body; the `mod` block is
  shared, see above)
- `crates/reprise-gnome/src/ui/library_doctor/start_page.rs` (new)
- `crates/reprise-gnome/src/ui/library_doctor/summary_page.rs` (rewritten)
- `crates/reprise-gnome/src/ui/library_doctor/result_pages.rs` (new)
- `crates/reprise-gnome/src/ui/library_doctor/job_page.rs` (deleted)
- `crates/reprise-gnome/src/ui/library_doctor/jobs.rs`
- `crates/reprise-gnome/src/ui/library_doctor/progress_card.rs`
- `crates/reprise-gnome/src/ui/library_doctor/tests.rs`
- `crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs`
- `crates/reprise-gnome/src/ui/window/window_navigation.rs`
- `docs/ux-rules.md` — the **DOC-2b**, **DOC-2c**, **DOC-7b**, **DOC-8c**, **DOC-9a** and
  **DOC-9c** blocks only

**Depends on** P1, P3, P4, P5.

**Delivers (tests)**
- `doc_8c_start_page_carries_scope_remote_run_and_the_only_revert` (gtk)
- `doc_8c_last_scan_block_is_hidden_without_a_revertible_cleanup` (gtk)
- `doc_9a_summary_renders_three_blocks_and_never_a_zero_row` (gtk)
- `doc_9a_summary_omits_the_conflicts_block_without_conflicts` (gtk)
- `doc_9a_every_visible_count_is_a_written_change_count` (gtk — the album-level case:
  a scan whose only review finding is one album-artist change over eleven tracks reports
  eleven, not one; see §F U-1)
- `doc_9c_post_apply_names_the_quiet_fixes_and_the_unresolved_conflicts` (gtk)
- `doc_9c_post_apply_reports_the_write_report_not_the_plan` (gtk)
- `doc_9c_nothing_to_fix_is_distinct_from_the_pre_scan_state` (gtk)
- `doc_8a_done_marks_the_scan_reviewed_and_clears_the_sidebar_entry` (gtk)
- `doc_2c_running_scan_shows_the_same_two_blocks_counting_up_with_actions_locked` (gtk —
  rewrite of the existing `doc_2c_running_scan_prefers_partial_results_without_enabling_review`)
- `doc_2c_block_one_counts_in_the_future_until_the_quiet_write_finishes` (gtk — see §F U-3)
- `doc_5c_progress_uses_tracks_as_the_primary_currency` (gtk — unchanged, must stay green)
- `doc_7a_acoustid_unavailable_is_visible_only_for_remote_mode` (gtk — unchanged)
- `doc_2a_scope_choice_freezes_the_requested_input_shape` (gtk — unchanged)
- `doc_7b_entry_scope_defaults_to_library_and_suggests_filtered_view` (gtk — unchanged)

**Red first** — `doc_9a_summary_renders_three_blocks_and_never_a_zero_row`, against a scan
whose conflict count is zero, asserts the summary contains two blocks. Today it renders
nine rows.

**Acceptance**
```sh
cargo test -p reprise-gnome library_doctor::
scripts/check-display-tests.sh --filter doc_
```

---

### P8 — GTK: the review page

**Stage** 2.

**Owns**
- `crates/reprise-gnome/src/ui/library_doctor/review_page.rs`
- `crates/reprise-gnome/src/ui/library_doctor/review_row.rs`
- `crates/reprise-gnome/src/ui/library_doctor/review_model.rs`
- `crates/reprise-gnome/src/ui/library_doctor/review_header.rs` (new)
- `crates/reprise-gnome/src/ui/library_doctor/review_filter_bar.rs` (new)
- `crates/reprise-gnome/src/ui/library_doctor/review_conflicts.rs` (new)
- `crates/reprise-gnome/src/ui/library_doctor/review_row_contract_tests.rs`
- `crates/reprise-gnome/examples/doctor_review_sections_repro.rs` (new)
- `docs/ux-rules.md` — the **DOC-3b** and **DOC-9b** blocks only

**Depends on** P1, P2, P5.

**Delivers (tests)**
- `doc_9b_one_column_header_serves_the_whole_page` (gtk)
- `doc_9b_rows_carry_no_caption_labels` (gtk, source-contract test)
- `doc_9b_review_groups_render_one_header_per_album` (gtk, display)
- `doc_9b_every_reviewable_row_starts_selected` (gtk)
- `doc_9b_the_filter_bar_offers_only_categories_present_in_the_scan` (gtk)
- `doc_9b_conflicts_sit_at_the_end_and_skip_all_clears_them` (gtk)
- `doc_9b_footer_counts_the_changes_that_will_be_written` (gtk)
- `doc_9b_the_album_pill_counts_written_changes_not_display_rows` (gtk — an album with a
  collapsed `All 11 tracks` row and three single-track rows reads "14 changes"; §F U-1)
- `doc_8a_skip_all_marks_the_scan_reviewed` (gtk — §F U-4)
- `doc_3b_breakpoint_changes_layout_without_changing_row_identity` (gtk — retargeted at
  the page, see §E)
- `doc_3b_review_page_virtualizes_rows_without_horizontal_scroll` (gtk, display —
  unchanged assertions)
- `doc_4b_confidence_uses_redundant_source_text_tone_and_warning` (gtk — unchanged)
- `doc_4b_manual_candidate_exposes_available_remote_evidence` (gtk — unchanged)
- `doc_5d_write_outcomes_preserve_honest_review_state` (gtk — unchanged)

**Red first** — `doc_9b_rows_carry_no_caption_labels` asserts
`review_row.rs` contains no `value_widgets(` call. Fails today.

**Acceptance**
```sh
cargo test -p reprise-gnome library_doctor::review
scripts/check-display-tests.sh --filter doc_9b
xvfb-run -a cargo run -p reprise-gnome --example doctor_review_sections_repro
```

---

### P9 — MCP: `music_scan_tags`, `music_review_tags`, `music_apply_tags`

**Stage** 3.

**Owns**
- `crates/reprise-mcp/src/doctor_tools.rs` (new)
- `crates/reprise-mcp/src/doctor_actions.rs` (new)
- `crates/reprise-mcp/src/doctor_dto.rs` (new)
- `crates/reprise-mcp/src/capability.rs`
- `crates/reprise-mcp/src/startup.rs`
- `crates/reprise-mcp/src/server.rs`
- `crates/reprise-mcp/src/main.rs` — one `mod doctor_tools;` line (shared registry)
- `crates/reprise-mcp/tests/doctor_tools.rs` (new)
- `crates/reprise-mcp/tests/fixtures/doctor_scan_request.json`,
  `doctor_scan_response.json` (new)
- `crates/reprise-mcp/tests/common/mod.rs` — only if a helper is genuinely missing
- `docs/ux-rules.md` — the **DOC-11a** block, plus appending its two test names to the
  **DOC-10b** test list (the sequential exception noted above)

**Depends on** P1, P2, P3, P4 — and, by the stage cut, on all of Stage 2.

**Delivers (tests)**
- `doc_11a_scan_tags_does_not_write_without_apply_safe` (mcp integration)
- `doc_11a_apply_safe_requires_the_tags_write_capability` (mcp integration)
- `doc_11a_apply_tags_requires_the_tags_write_capability` (mcp integration)
- `doc_11a_review_tags_groups_by_album_and_filters_by_category` (mcp integration)
- `doc_11a_review_tags_counts_written_changes_per_album` (mcp integration — §F U-1)
- `doc_11a_doctor_responses_carry_no_file_paths` (mcp integration, D19 leak matrix)
- `doc_10b_mcp_refuses_while_a_gui_job_holds_the_lock` (mcp integration)

**Red first** — `doc_11a_scan_tags_does_not_write_without_apply_safe` calls the tool and
fails because the tool does not exist.

**Acceptance**
```sh
cargo test -p reprise-mcp
```

---

### P10 — Gates and bookkeeping

**Stage** 3.

**Owns**
- `crates/reprise-gnome/src/ui/strings_library_doctor.rs` (removals, and removing the
  temporary `#![allow(dead_code)]` that P5 added)
- `po/POTFILES.in`, `po/reprise.pot`, `po/*.po`
- `scripts/check-frontend-thinness.sh` (budget numbers only)
- `docs/ux-rules.md` — only the final consistency pass over §Y (no rule text another
  package wrote)
- `.superpowers/sdd/progress.md`

**Depends on** P1–P9.

**Delivers (tests)** — no new tests; it makes the existing gate battery green.

**Acceptance** — the full Stage 3 battery listed under "Stages" above.

---

## C. Per-package instructions

### P1 — Core: two tiers instead of three

**`review.rs` — replace the filter enum.**

Today:
```rust
pub enum DoctorReviewFilter {
    AllChanges,
    LocalSafeOnly,
}
```
New:
```rust
pub enum DoctorReviewFilter {
    /// Written without asking: local + preselected + not stale, plus every
    /// `RecordingMbid` proposal regardless of source or confidence.
    AutoApply,
    /// Shown for review, every ready row preselected. Carries the unresolved groups.
    NeedsReview,
}
```
`AllChanges` and `LocalSafeOnly` are deleted outright — there are no installations to keep
compatible (AGENTS.md, "Not released yet"). `AllChanges` gets **no** replacement: there is
no surface anywhere that lists the auto-applied rows (§F U-2).

Add one free function, the single place that decides the tier:
```rust
pub fn is_auto_applied(proposal: &DoctorProposal, stale: bool) -> bool {
    if stale {
        return false;
    }
    proposal.field == DoctorField::RecordingMbid
        || (proposal.source == ProposalSource::Local && proposal.preselected)
}
```
This function is the *only* tier predicate in the codebase. `presentation.rs` calls it,
`DoctorReviewSession::build` calls it, the MCP calls it. Do not restate the condition
anywhere else — a duplicated predicate that drifts is a known bug class in this repo.

In `DoctorReviewSession::build` (`review.rs:165`), replace the two `LocalSafeOnly`
branches:
- the skip condition at `review.rs:195–197` becomes
  `if is_auto_applied(&proposal, is_stale) != (filter == DoctorReviewFilter::AutoApply) { continue; }`
- the preselection at `review.rs:215` becomes `selected: state == DoctorReviewRowState::Ready`
  for both filters. Under `AutoApply` that is the same set as before; under `NeedsReview`
  it is the change the brief asks for ("Preselect everything").
- the unresolved-group block at `review.rs:233` runs for `NeedsReview` only (was
  `AllChanges`).

Rename the `all_safe()` preset to `all()` and make it select every ready row (drop the
`local_safe` lookup); `none()` is unchanged. Delete the now-unused `local_safe` map from
the session struct — its only reader was `all_safe()`.

Keep `RowSortKey`, `proposal_category()` and `field_position()` exactly as they are.
Row order inside a group must stay stable and the field sequence
Title → Artist → Album → AlbumArtist → Year → Genre → RecordingMbid is a DOC-3a promise.

`DoctorApplySummary::tag_change_count` and `file_count` keep their meaning exactly:
`tag_change_count` is the number of selected `(track, field)` changes, `file_count` the
number of distinct files. They are the canonical implementation of the counting rule from
§A; every other count in this plan is derived from the same unit.

**`presentation.rs` — rename the tier in the summary.**

```rust
pub struct DoctorScanSummary {
    pub auto_applied_changes: usize,   // was: safe_changes
    pub review_changes: usize,
    pub unresolved_groups: usize,
    pub checked_tracks: usize,
    pub skipped_tracks: usize,
    problem_counts: [DoctorProblemCount; 5],
}
```
`DoctorProblemCount.safe` becomes `auto_applied`. All of these are counts of `(track,
field)` changes — one proposal, one count — which is what makes the summary's numbers and
the review page's numbers the same unit. In `summary_for_parts()`
(`presentation.rs:114–139`) replace the inline predicate
```rust
let safe = proposal.source == ProposalSource::Local
    && proposal.preselected
    && !stale_tracks.contains(&proposal.track_id);
```
with `let auto = review::is_auto_applied(proposal, stale_tracks.contains(&proposal.track_id));`.
This is the verified duplication the plan removes: the old line is the second copy of the
tier decision.

`project_scan()` (`presentation.rs:41`) is unchanged — remote hiding and local-fallback
restoration keep working exactly as today.

**Rule text.** Rewrite **DOC-3a** (everything reviewable starts selected) and land
**DOC-8b** `[active]`; both texts are in §E.

### P2 — Core: album grouping projection

New file `grouping.rs`, under 200 lines, pure, no SQL.

```rust
pub struct DoctorReviewAlbum {
    pub key: String,             // normalized identity, never displayed
    pub title: String,           // raw album tag of the first member, or "" when absent
    pub artist: String,          // raw album artist, falling back to artist
    pub track_count: usize,      // distinct tracks of this album inside the scan scope
    pub change_count: usize,     // tag changes that will be written for this album
    pub rows: Vec<DoctorReviewDisplayRow>,
}

pub enum DoctorReviewDisplayRow {
    /// One concrete track/field change.
    Track { row_id: DoctorReviewRowId, track_id: i64 },
    /// The same field/current/proposed triple on every track of the album.
    AllTracks { row_ids: Vec<DoctorReviewRowId>, track_count: usize },
}
```

```rust
pub fn group_review_rows(
    scan: &DoctorScan,
    session: &DoctorReviewSession,
) -> Vec<DoctorReviewAlbum>;
```

Rules:
- The album key is
  `format!("{}\u{1}{}", normalize_group_key(album_artist_or_artist), normalize_group_key(album))`
  using `crate::library::group_key::normalize_group_key` — the same normalizer DOC-1a
  already mandates for the scan (`local_rules.rs:75`). Do not write a second normalizer.
- Album order follows the scope order of each album's first member
  (`scan.track_ids` position), so the review list follows the same order the scan froze.
- A row whose track has no album tag goes into exactly one trailing group with an empty
  key; it is rendered under the new `DOCTOR_NO_ALBUM` heading.
- `AllTracks` collapse triggers when, for one `(field, current, proposed)` triple inside
  one album, the set of affected row ids covers **every** track of that album that is in
  the scan. Anything less stays a `Track` row — a partial album is not "all tracks".
- **`change_count` counts the underlying row ids, never the display rows.** An album with
  one collapsed `AllTracks` row over eleven tracks plus three single-track rows has
  `change_count == 14` and `rows.len() == 4`. This is the counting rule from §A and it is
  decided in §F U-1; the mockup's own album pill says "4 changes" for exactly this album,
  and this implementation says 14 on purpose.

### P3 — Core: paired undo and the cross-process write lock

**Move, then extend.** `write.rs` is 764 lines and the 800-line rule applies to any file
substantially edited. Move `last_cleanup()` (`write.rs:717`), `revert_last_cleanup()`
(`write.rs:742`) and `revert_inputs()` (`write.rs:667`) into the new `cleanup.rs`, leaving
`prepare_job`, `run_job`, `claim_file` and `apply_review_plan` in `write.rs`. `write.rs`
drops to roughly 660 lines and `cleanup.rs` starts around 200.

**New shape.** `DoctorCleanup` exists today with a single `job_id`; it becomes a set.
```rust
pub struct DoctorCleanup {
    pub scan_id: i64,
    pub job_ids: Vec<i64>,     // ascending; the whole bracket of one scan
    pub created_at: i64,       // of the newest job in the set
    pub track_count: usize,    // distinct tracks over the whole set
}

pub struct DoctorCleanupReport {
    pub reports: Vec<DoctorWriteReport>,  // one per source job, newest first
    pub reverted_tracks: usize,
    pub failed_tracks: usize,
    pub conflict_tracks: usize,
    pub unavailable_tracks: usize,
    pub cancelled: bool,
}

impl LibraryDoctor<'_> {
    pub fn last_cleanup(&self) -> Result<Option<DoctorCleanup>, DoctorError>;
    pub fn revert_last_cleanup(
        &mut self,
        progress: impl FnMut(DoctorWriteProgress) -> DoctorWriteControl,
    ) -> Result<Option<DoctorCleanupReport>, DoctorError>;
}
```
The four summed counters on `DoctorCleanupReport` exist so the GTK side keeps rendering one
outcome for one user-visible operation; the S1 sweep maps today's single-report reads onto
them without new copy.

`last_cleanup()` keeps today's eligibility predicate per job — `kind='doctor_apply'`,
`state IN ('completed','cancelled','interrupted')`, at least one journal row with
`outcome='applied'`, and no journal row still `pending`/`prepared` — then selects the
**largest `scan_id`** among eligible jobs and returns *all* eligible jobs of that scan.
Because a fully reverted job has no `applied` rows left (the revert flips the source
journal to `reverted`, see `write_recovery.rs:96–102`), a consumed job drops out of the
set automatically and no extra bookkeeping is needed.

`revert_last_cleanup()` iterates the set **newest job first** and, per job, does exactly
what today's single-job path does: `revert_inputs(conn, job_id)` → `prepare_job(conn,
"doctor_revert", Some(job_id), Some(scan_id), &inputs)` → `run_job(...)`. The schema
requires exactly one `source_job_id` per revert job (`db_tag_write_jobs.rs:14–16`), so
one revert job per source job — never one merged job. Progress is reported as one
continuous count across the whole set: sum the file counts up front and offset each job's
`completed_tracks`.

The behavior at the edges, decided in §D-1: a failing job does not stop the next one; a
Cancel does; a partially reverted set stays offered.

**The lock.** New `crates/reprise-core/src/library/tag_write_lock.rs`:
```rust
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("another tag-writing job is already running")]
pub struct TagWriteBusy;

/// Fails when any tag-write job of any kind is `prepared` or `running`.
/// Call inside the same transaction that inserts the new job row.
pub(crate) fn claim_tag_write_slot(conn: &Connection) -> Result<(), TagWriteBusy>;
```
Wire it as the first statement inside the `unchecked_transaction()` of both
`library_doctor::write::prepare_job` (`write.rs:287`) and
`tag_write_job::store::prepare_tag_write_job` (`store.rs:53`), so the check and the insert
commit atomically. Export `TagWriteBusy` from `library/mod.rs` and map it into
`DoctorError` as a distinct variant, so a caller can tell "busy" from "broken".

### P4 — Core: auto-apply at scan completion, scan state, pending count, schema v57

**Migration v57** in `db_library_doctor.rs`, registered in `db.rs` next to the existing
`migrate_v*` calls; bump `SUPPORTED_SCHEMA_VERSION` from 56 to 57.
```sql
ALTER TABLE library_doctor_state ADD COLUMN reviewed_scan_id INTEGER
  REFERENCES library_doctor_scans(id) ON DELETE SET NULL;
UPDATE library_doctor_state SET last_complete_scan_id = NULL, reviewed_scan_id = NULL;
```
Idempotent like every other migration in this file. It does **not** delete
`library_doctor_scans` rows — `tag_write_jobs.scan_id` references them with
`ON DELETE RESTRICT`, so a delete would either fail or destroy the undo journal.
Rationale in §D-2.

**Auto-apply hook.** In `scan.rs`, at the point where the scan result has been persisted
and `DoctorScanOutcome::Completed(scan)` is about to be returned (`scan.rs:195–198`), add:
```rust
pub struct DoctorScanCompletion {
    pub scan: DoctorScan,
    pub auto_applied: Option<DoctorWriteReport>,
}
```
and a new method
```rust
pub fn apply_auto_tier(
    &mut self,
    scan: &DoctorScan,
    progress: impl FnMut(DoctorWriteProgress) -> DoctorWriteControl,
) -> Result<Option<DoctorWriteReport>, DoctorError>;
```
which builds `DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::AutoApply)`,
takes `freeze_plan()`, returns `Ok(None)` when the plan is empty, and otherwise calls
`apply_review_plan()`. It is a separate method, not folded into `scan()`, for three
reasons: the MCP must be able to run a scan without it (`apply_safe: false`), the GUI must
be able to show the summary while the write runs, and a scan must never fail because a
file was read-only. It is also why nothing is written *during* the scan (§F U-3): the
write starts only after the scan has completed and the plan is frozen.

`store.rs` gains `set_reviewed_scan(conn, scan_id)` and `reviewed_scan_id(conn)`. Both
are called from the GTK surface in the two situations §F U-4 names — `Done` on the
post-apply page, and `Skip all` in the conflicts section.

**Pending count.** New `crates/reprise-core/src/queries/doctor.rs`:
```rust
pub fn count_pending_doctor_findings(db: &Db) -> Result<u32, rusqlite::Error>;
```
Zero when `last_complete_scan_id IS NULL` or when it equals `reviewed_scan_id`; otherwise
```sql
SELECT (SELECT COUNT(*) FROM library_doctor_proposals WHERE scan_id = :scan)
     - (SELECT COUNT(*) FROM tag_write_journal v
        JOIN tag_write_job_files f ON v.file_id = f.id
        JOIN tag_write_jobs j ON j.id = f.job_id
        WHERE j.scan_id = :scan AND j.kind = 'doctor_apply' AND v.outcome = 'applied')
```
clamped at zero. This is exact rather than approximate: the journal has one row per field
actually written, so the difference is exactly what is still open, and it needs no
staleness computation and no scan load. Both terms are per `(track, field)`, so the badge
carries the same unit as every other number in the feature (§A, §F U-1) — and, like them,
it will read larger than the mockup's "88". Unresolved groups are deliberately not counted:
they are optional, and a badge that demands attention for something the user may skip is
the wrong promise.

### P5 — Strings and copy

First line of the file, for the reason given in §F-20 and removed again by P10:
```rust
#![allow(dead_code)]
```

Add to `strings_library_doctor.rs`, all wrapped in `N_!`, placeholder style matching the
existing helpers (`{name}` inside the literal, `&[("name", value)]` beside it):

| Symbol | English source text |
| --- | --- |
| `DOCTOR_START_HEADING` | `Check your library` |
| `DOCTOR_START_BODY` | `Reprise fixes what is unambiguous — stray spaces, casing, missing MusicBrainz IDs — and asks you about the rest. Everything it does can be undone in one step.` |
| `doctor_scan_estimate(tracks, minutes)` | `{tracks} tracks · about {minutes} minutes` |
| `doctor_last_scan(when)` | `Last scan · {when}` |
| `doctor_last_scan_fixes(count)` | `{count} fixes applied · still reversible` |
| `doctor_fixes_applied(count)` | `{count} fixes already applied` |
| `doctor_fixes_to_apply(count)` | `{count} fixes to apply` |
| `doctor_spacing_casing_line(count)` | `{count} stray spaces and casing corrections` |
| `doctor_mbid_line(count)` | `{count} MusicBrainz IDs filled in — no visible change to your tags` |
| `doctor_mbid_line_pending(count)` | `{count} MusicBrainz IDs to fill in — no visible change to your tags` |
| `doctor_changes_need_your_eye(count)` | `{count} changes need your eye` |
| `doctor_across_albums(count)` | `across {count} albums` |
| `doctor_conflicts_headline(count)` | `{count} spelling conflicts, no clear winner` |
| `DOCTOR_CONFLICTS_BODY` | `Waiting at the end of the review list. Skippable — nothing breaks if you leave them.` |
| `DOCTOR_CONFLICTS_SECTION` | `Spelling conflicts` |
| `DOCTOR_CONFLICTS_OPTIONAL` | `Optional · nothing happens if you skip these` |
| `DOCTOR_SKIP_ALL` | `Skip all` |
| `doctor_apply_changes(count)` | `Apply {count} changes` (plural pair) |
| `DOCTOR_SCAN_AGAIN` | `Scan again` |
| `DOCTOR_RESULTS_KEPT` | `Results are kept until the next scan.` |
| `DOCTOR_NOTHING_TO_FIX` | `Nothing to fix` |
| `doctor_nothing_to_fix_body(checked, skipped)` | `{checked} tracks checked, {skipped} skipped. Your tags are already consistent with each other.` |
| `doctor_tracks_checked_heading(count)` | `{count} tracks checked` |
| `DOCTOR_UNDO_EVERYTHING` | `Undo everything from this scan` |
| `DOCTOR_DONE` | `Done` |
| `doctor_includes_quiet_fixes(count)` | `Includes the {count} quiet fixes. Available until the next scan.` |
| `doctor_tags_fixed(count)` | `{count} tags fixed` (the toast) |
| `DOCTOR_UNDO` | `Undo` |
| `DOCTOR_ALL` | `All` (replaces `DOCTOR_ALL_SAFE`) |
| `DOCTOR_TRACK` | `Track` (column header) |
| `DOCTOR_FIELD` | `Field` (column header) |
| `DOCTOR_NO_ALBUM` | `No album` |
| `doctor_all_tracks(count)` | `All {count} tracks` (plural pair) |
| `doctor_preselected_hint()` | `Everything here is preselected. Uncheck what you disagree with.` |
| `doctor_changes_and_albums(changes, albums)` | `{changes} changes · {albums} albums` |
| `doctor_conflict_scope(field, tracks)` | `{field} · {tracks} tracks` |
| `DOCTOR_FILTER_CASING` | `Casing` |
| `DOCTOR_FILTER_YEAR` | `Year` |
| `DOCTOR_FILTER_GENRE` | `Genre` |
| `NEW_PLAYLIST_UNTITLED` | `Untitled playlist` |

Two notes on the table:

- `doctor_fixes_to_apply()` and `doctor_mbid_line_pending()` are the future forms block 1
  uses while the scan runs and while the quiet write runs (§F U-3). The draft's
  `doctor_fixes_applied_so_far()` ("{count} fixes applied so far", the mockup's Frame 3
  wording) is **not** added: during the scan nothing has been applied yet, and the string
  would be untrue.
- The album pill reuses the existing `doctor_change_count()` (`{count} changes`) — do not
  add a second one.

Keep verbatim, do not touch: `LIBRARY_DOCTOR_REMOTE_DESCRIPTION`, `DOCTOR_PICK_ONE`,
`doctor_apply_summary()`, `doctor_tags_updated()`, `doctor_checked_counts()`,
`doctor_unresolved_spellings()`, `doctor_candidate()`, `doctor_track_progress()`,
`DOCTOR_SCANNING`, `DOCTOR_RESULTS_SO_FAR`, `DOCTOR_CONTROLS_LOCKED`,
`doctor_remote_confidence()`, `doctor_low_confidence()`, `doctor_review_row_description()`,
all `DOCTOR_STATUS_*`.

### P6 — GTK: entry points, sidebar, and the playlists section

**Primary menu.** In `update_library_section()` (`primary_menu.rs:50–66`) drop the
`Sync Device…` item and its `ACTION_SYNC_DEVICE` action registration (§F U-6 — the loss is
known and accepted). Keep `win.library-doctor` as one flat row with no badge and no
submenu. Add `Import playlist…` to the library section, pointing at a new
`win.import-playlist` action that reuses the import entry point
`sidebar_row_wiring.rs:145–147` used.

**Preferences.** Delete `preference_library_doctor.rs`, but **not wholesale** — it is the
only home of `remote_suggestions_row_for()`, which both `summary_page.rs:158` and
`review_page.rs:327` call, and of the versioned consent sheet the brief explicitly keeps.
Move `remote_suggestions_row()`, `remote_suggestions_row_for()`, `remote_toggle_action()`,
`present_remote_confirmation()`, `set_active_without_notify()` and the
`doc_7a_first_remote_enable_requires_confirmation_and_cancel_stays_off` test into the new
`crates/reprise-gnome/src/ui/library_doctor/remote_toggle.rs` unchanged. Delete
`plugin_row()`, `control_state()`, `DoctorPluginControlState` and
`DoctorPreferenceControls`; remove `doctor_controls` and `library_doctor_job_running` from
`preferences.rs:143–144, 226–227, 286`; remove `"library_doctor"` from `LOCAL_PLUGIN_IDS`
(`preference_plugins.rs:22`) and its arm at `:422`; update the two list assertions in
`preference_plugins_tests.rs:41, 69`. The core module descriptor
(`crates/reprise-core/src/modules.rs:93`) and its test stay untouched — the Doctor remains
a core module, it just no longer has a Preferences surface.

**Sidebar ISSUES row.** In `sidebar_rebuild.rs`, read
`queries::count_pending_doctor_findings(conn)` beside the existing `count_missing` /
`count_new_missing` reads (around `:59–71`), fold it into `has_issues` and append the row
after `Missing files`. Do **not** add a `ViewSource` variant: the Doctor is not a track
source and every exhaustive `match ViewSource` in the workspace would have to grow a dead
arm. Add instead a sibling of `add_issue_row()`:
```rust
fn add_issue_action_row(shared: &Rc<Shared>, title: &str, count: u32, action: &str);
```
which builds the same visual row via `sidebar_presentation::build_issue_nav_row()` but
activates a `GAction` name instead of navigating to a source. Reuse
`issue_row_presentation(count, NavIcon::…)` for the badge so the Doctor badge and the
Missing badge cannot drift, and so the badge picks up the existing `attention` treatment
rather than a new accent style (§F-11). Add one `NavIcon::LibraryDoctor` variant. The
row's a11y marker is mandatory
(`// a11y-semantics: role=… name=… state=… action=…` immediately before
`set_focusable(true)`).

**Toast.** `toasts.rs` gets one new helper beside `show()`:
```rust
pub(super) fn show_with_action(
    overlay: &adw::ToastOverlay,
    text: &str,
    button: &str,
    on_click: impl Fn() + 'static,
);
```
so the `{n} tags fixed` / `Undo` toast is not a ninth hand-rolled toast construction. It
fires **only** for the quietly applied set, and only when that set is non-empty. Findings
that need review never toast — they surface through the sidebar entry. Its number is the
write report's applied change count, in the same unit as everything else (§A).

**Playlists quick-add.** This is new behavior in the sidebar, not a rearrangement; there
is no inline-rename mechanism anywhere in this codebase today (verified: `EditableLabel`
does not appear in `crates/reprise-gnome/src` at all). Build it in full, as §F U-5 decides:

1. **The `+` button.** In `sidebar_presentation.rs`, extend `append_header()` with an
   optional trailing button (or add `append_header_with_action()`). Delete
   `append_playlist_action_rows()` together with `PlaylistActionRows`,
   `NavIcon::NewPlaylist` and `NavIcon::ImportPlaylist`, and the two tests at
   `sidebar_presentation.rs:377, 411` that build them. In `sidebar_rebuild.rs:226` the
   PLAYLISTS header gets the `+`; `:223–224` and `:285–287` go, as do the
   `new_playlist_row` / `import_playlist_row` fields on `Shared`
   (`sidebar.rs:118, 121, 238, 239`), their handlers in `sidebar_row_wiring.rs:140–147`
   and their initializers in `sidebar_tests.rs:18–19`.
2. **Creating.** New `sidebar_playlist_quick_add.rs`. On activation, call
   `reprise_core::library::playlists::create(conn, &text(NEW_PLAYLIST_UNTITLED))`, rebuild
   the sidebar, and put the fresh row straight into inline edit. `create_playlist_and_stay()`
   in `sidebar_playlist_creation.rs` survives as the shared creation path and becomes
   `pub(in crate::ui)`; `show_new_playlist_dialog()` is deleted.
3. **Editing.** The playlist row's label is replaced by a `gtk4::EditableLabel` bound to
   that row. Call `start_editing()` with the whole text selected, so the first keystroke
   overwrites `Untitled playlist`.
4. **Committing.** Enter, or focus-out, commits through
   `reprise_core::library::playlists::rename(db, id, name)` — a core function that exists
   and is currently called only from the CLI and the MCP. The DB row keeps its placeholder
   name until this moment; nothing is renamed while typing.
5. **Escape discards the row.** Escape leaves edit mode **and deletes the playlist** via
   `reprise_core::library::playlist_delete::delete(db, id, NEW_PLAYLIST_UNTITLED)`, then
   rebuilds the sidebar. The expected-name guard on `delete()` matches because step 4 has
   not run: the stored name is still the placeholder. Focus returns to the `+` button.
6. **An empty name keeps the placeholder.** Committing an empty or whitespace-only name
   does not delete and does not rename: the playlist stays `Untitled playlist`. Enter and
   focus-out mean "keep this", and a nameless row in the sidebar would be unaddressable.
   Only Escape destroys.
7. **Keyboard path, end to end.** The `+` button is focusable and activates on
   Enter/Space; activation moves focus into the `EditableLabel` in editing mode with the
   text selected; Enter commits and returns focus to the new row; Tab commits and moves
   on; Escape discards and returns focus to `+`. Both the `+` button and the
   `EditableLabel` carry the mandatory `// a11y-semantics:` marker.

### P7 — GTK: start page, summary, post-apply, empty state, progress

`summary_page.rs` is 554 lines today and does four jobs. Split it:

- **`start_page.rs`** — the pre-scan surface. First-aid-kit icon, `DOCTOR_START_HEADING`,
  `DOCTOR_START_BODY`; `Scope` as an `adw::ToggleGroup` with three `adw::Toggle`s (verified
  available in libadwaita 0.9.2 with the `v1_9` feature already enabled), replacing the
  `adw::ComboRow` at `summary_page.rs:152`; the remote card built from
  `remote_toggle::remote_suggestions_row_for()` with
  `LIBRARY_DOCTOR_REMOTE_DESCRIPTION` verbatim; the `Run Scan Now` primary button with
  `doctor_scan_estimate()` beside it; then a separator and the last-scan block —
  `doctor_last_scan()`, `doctor_last_scan_fixes()` and `Revert Last Cleanup`, fed from
  `LibraryDoctor::last_cleanup()`. The whole block is hidden when `last_cleanup()` is
  `None`. Keep the AcoustID-unavailable row and `show_acoustid_unavailable()` — DOC-7a
  still requires it.
- **`summary_page.rs`** (rewritten) — the three blocks. Block 1 `doctor_fixes_applied()`
  with the two per-kind lines and `Undo`; block 2 `doctor_changes_need_your_eye()` with one
  line per remaining problem class plus `doctor_across_albums()`, and
  `doctor_review_changes()` as the primary; block 3 `doctor_conflicts_headline()` plus
  `DOCTOR_CONFLICTS_BODY` in a dashed container. Below: `doctor_checked_counts()`, scope
  and remote state as one muted line, plus a `Scan again` ghost button and
  `DOCTOR_RESULTS_KEPT`. **A block whose count is zero is not rendered.** Delete
  `summary_row()`, `PROBLEM_CLASSES`, `problem_title()`, `problem_class_visible()` and the
  nine `adw::ActionRow`s. `DOCTOR_TRACKS_CHECKED` stops being a row and becomes the page
  heading `doctor_tracks_checked_heading()`.

  **Counting.** Block 1's headline is the number of tag changes in the auto-applied set,
  and its two lines split that same number by kind — spacing/casing versus
  `RecordingMbid`. Block 2's headline is `summary.review_changes`, i.e. the number of
  reviewable `(track, field)` changes, *not* the number of grouped display rows the review
  page will draw; `doctor_across_albums()` takes the album count from
  `grouping::group_review_rows()`. Both are the unit defined in §A and decided in §F U-1;
  they will read higher than the mockup's "1,017" and "88" for the same library, and that
  is the intended behavior.

  **While a scan runs**, the same two blocks render with `DOCTOR_RESULTS_SO_FAR` as the
  heading, live counts from `set_partial_summary()`, both actions insensitive, and
  `DOCTOR_CONTROLS_LOCKED` as the footer reason. Block 1 counts **in the future**: it uses
  `doctor_fixes_to_apply()` and `doctor_mbid_line_pending()` and its `Undo` is disabled,
  because at that moment nothing has been written. It keeps the future form until the quiet
  write job reports completion, and only then swaps to `doctor_fixes_applied()` /
  `doctor_mbid_line()` with the **report's** applied counts and an enabled `Undo`. See
  §F U-3.
- **`result_pages.rs`** — two `adw::StatusPage`-shaped surfaces: post-apply
  (`doctor_tags_updated()`, the counts, `DOCTOR_UNDO_EVERYTHING`, `Done`,
  `doctor_includes_quiet_fixes()`, and the named unresolved conflicts) and nothing-to-fix
  (`DOCTOR_NOTHING_TO_FIX`, `doctor_nothing_to_fix_body()`, `Scan again`).

  Post-apply reports the **write report**, never the frozen plan: `doctor_tags_updated()`
  takes the report's track count, the change figure is the report's applied-change count,
  and `doctor_includes_quiet_fixes()` takes the quiet job's applied-change count. A page
  that printed the plan would lie the moment one file was read-only (§F U-1).

  `Done` calls `store::set_reviewed_scan(conn, scan_id)` and triggers a sidebar rebuild, so
  the ISSUES row disappears — this is the whole-scan receipt from §F U-4. It fires even
  when some rows were left unapplied.
- **`job_page.rs` is deleted** together with `open_job_page()` (`mod.rs:441`), the
  `library-doctor-job` navigation tag, `LibraryDoctorJobPage` and
  `DOCTOR_JOB_PAGE_DESCRIPTION`. `window_navigation.rs:268` and the two
  `doc_6b_sidebar_*` tests that reference the job page must be retargeted at the summary
  page, which is now the single place a running job returns to. `progress_card.rs` is
  unchanged — spinner, `DOCTOR_SCANNING`, percent, Cancel, bar,
  `doctor_track_progress()`, click/Enter/Space activation all stay.

In `mod.rs`, the `DoctorScanOutcome::Completed(scan)` arm (`mod.rs:337–340`) now:
1. stores the scan and renders the summary immediately, with block 1 in its future form,
2. calls `apply_auto_tier()` on the worker thread through a new `jobs::run_auto_apply()`,
3. on completion, swaps block 1 to the applied form in place and — only if the user is
   *not* on a Doctor page — fires the `doctor_tags_fixed()` toast with `Undo` and triggers
   a sidebar rebuild. Never a modal, never a forced page switch.

`open_review()` loses its filter parameter (there is only `NeedsReview` now) and
`connect_review_safe()` / `DOCTOR_REVIEW_SAFE` / `doctor_review_safe_fixes()` go away.
No surface anywhere lists the auto-applied rows; do not add a disclosure or an expander to
block 1 (§F U-2).

### P8 — GTK: the review page

**Grouping.** Replace the plain `gio::ListStore` → `SingleSelection` chain
(`review_page.rs:236–245`) with
`ListStore` → `gtk4::SortListModel` → `SingleSelection` → `ListView`. Set both sorters on
the `SortListModel`: `set_sorter()` with a `CustomSorter` over
`(album_position, row_index_within_album)` and `set_section_sorter()` with a `CustomSorter`
that compares only `album_position`. Set `rows.set_header_factory(...)` to build the album
header: group checkbox, 38×38 cover placeholder, album title, `{artist} · {n} tracks`, and
a `doctor_change_count()` pill fed from `DoctorReviewAlbum::change_count` — written
changes, not display rows (§F U-1). **Do not subclass a section model.** `GtkSortListModel`
already implements `GtkSectionModel` (verified in gtk4 0.11.4,
`src/auto/sort_list_model.rs:20`), which is what makes this testable at all — see §D-3.

**One column header.** Build the header once in the new `review_header.rs` as a
`gtk4::Box` pinned above the `ScrolledWindow`, with the eight cells
`[checkbox spacer] Track | Field | Current | [arrow spacer] | Proposed | Source | [edit spacer]`.
Put each labelled cell into a horizontal `gtk4::SizeGroup` shared with the matching cell of
every row, so the columns line up without a `ColumnView`. Then delete `value_widgets()`
from `review_row.rs` entirely and let `build_row()` create bare labels. `DOCTOR_TRACK` and
`DOCTOR_FIELD` become two separate header cells; `DOCTOR_TRACK_AND_FIELD` is deleted.

**Row shape** stays `[check] [track] [field] [current, strikethrough] [→] [proposed]
[source] [edit]`. Keep the strikethrough (`review_row.rs:235–237`) and every tone/warning
decision from `confidence_presentation()` (`review_model.rs:43–77`) byte for byte —
DOC-4b is not being renegotiated.

**Breakpoint moves up.** Delete the per-row `adw::BreakpointBin` (`review_row.rs:140–154`).
Put one `adw::Breakpoint` on the page at the existing 640 px threshold
(`review_model::WIDE_BREAKPOINT`); under it, rows stack vertically and the shared header
hides. `layout_for_width()` loses its `#[cfg(test)]` gate and becomes the page's real
decision function.

**Preselect and presets.** Every reviewable row arrives selected from P1. Rename the
`All Safe` button to `All` (`DOCTOR_ALL`) and wire it to the renamed `session.all()`.

**Filter bar.** New `review_filter_bar.rs`: an `adw::ToggleGroup` above the list with
`All` plus one toggle per `ProblemClass` actually present in the scan, mapped to
`DOCTOR_FILTER_CASING` / `DOCTOR_FILTER_YEAR` / `DOCTOR_FILTER_GENRE`. It drives a
`gtk4::FilterListModel` inserted between the store and the sort model — never a rebuild of
the store, so selection state survives a filter change. Beside it,
`doctor_changes_and_albums()` and `doctor_preselected_hint()`. Its change figure is the
written-change total over the ready rows currently in view, the same unit as the album
pills that sum into it.

**Scrolling.** One `ScrolledWindow` over the whole grouped list, sticky column header
above it and sticky footer below it, virtualized by `GtkListView` as today. There is no
pagination and **no collapsed "28 more albums · 79 changes" remainder row** — that is a
mockup artifact of a static frame; see §F-13.

**Conflicts at the bottom.** New `review_conflicts.rs` moves the `adw::ComboRow` block
(`review_page.rs:107–162`) below the list into a dashed container titled
`DOCTOR_CONFLICTS_SECTION` with `DOCTOR_CONFLICTS_OPTIONAL` and a `DOCTOR_SKIP_ALL` ghost
button. Each conflict renders as one row: `doctor_conflict_scope()` on the left, the
candidates as standalone radio pills built from `doctor_candidate()` on the right.
`DOCTOR_PICK_ONE` stays as the explanatory line. `Skip all` clears every group's `chosen`,
deselects the materialized rows, **and** calls `store::set_reviewed_scan(conn, scan_id)`,
which retires the sidebar ISSUES row for this scan (§F U-4).

**Footer.** Keep `doctor_apply_summary()` verbatim — `{changes} tag changes · {files}
files · undo available after`, fed from `summary.tag_change_count` and `summary.file_count`
exactly as today. The primary button becomes
`doctor_apply_changes(session.summary().tag_change_count)`; `doctor_apply_tracks()` is
deleted. The button is insensitive at zero selected changes (§F-14). Footer, filter bar and
album pills therefore all speak the same unit and can only differ by selection.

**The per-row pencil** opens the existing Tag Editor for that track; its Save marks
affected rows stale and deselects them, unchanged. For an `AllTracks` row it opens the Tag
Editor with **all** of that album's track ids, using the existing multi-select batch edit
(§F-12).

**Accessibility.** With the visible captions gone,
`doctor_review_row_description()` (`strings_library_doctor.rs:103`) is the *only* thing
that names a row's columns. It stays exactly as it is and keeps feeding both
`ReviewRowModel::accessible_description()` (`review_model.rs:97`) and the row tooltip
(`review_row.rs:251`). An `AllTracks` display row substitutes `doctor_all_tracks(n)` for
the track name. The album header row gets its own accessible name from title + artist +
change count.

### P9 — MCP: the three tools

New `doctor_tools.rs` with
`#[tool_router(router = doctor_tool_router, vis = "pub(crate)")] impl RepriseServer`,
composed in `server.rs:134–143` as `+ Self::doctor_tool_router()`. Params and DTOs live in
`doctor_dto.rs`, the blocking bodies in `doctor_actions.rs`, exactly as
`source_tools.rs`/`source_actions.rs`/`discovery_actions.rs` are split today.

```rust
// doctor_dto.rs
pub struct ScanTagsParams {
    pub scope: DoctorScopeArg,      // whole_library | current_view | selection
    pub track_ids: Option<Vec<i64>>,
    pub remote: Option<bool>,       // default: the persisted library_doctor.remote.enabled
    pub apply_safe: Option<bool>,   // default: FALSE
}
pub struct ReviewTagsParams {
    pub category: Option<DoctorCategoryArg>,   // casing | year | genre
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}
pub struct ApplyTagsParams {
    pub action: ApplyTagsAction,    // apply | resolve | revert
    pub row_ids: Option<Vec<u64>>,
    pub album_keys: Option<Vec<String>>,
    pub group_key: Option<String>,  // resolve
    pub candidate: Option<String>,  // resolve
}
```

- `music_scan_tags` runs `LibraryDoctor::scan()` under `spawn_blocking`, then
  `apply_auto_tier()` **only** when `apply_safe == Some(true)`. Returns the same summary
  the GUI shows: applied, needs-review, conflicts, checked, skipped.
- `music_review_tags` reads the last complete scan, builds a `NeedsReview` session, runs
  `grouping::group_review_rows()` and returns albums with their rows, honoring `category`.
  Expose it as a tool rather than a `reprise://` resource: it takes a filter argument and
  pagination, which the seven existing read-only resources do not (§F-8).
- `music_apply_tags` covers `apply` (by row id or album key), `resolve` (pick a spelling)
  and `revert` (`revert_last_cleanup()`, the whole bracket).

**Counting in the responses.** Every numeric field an agent can read is a count of tag
changes, matching §A and §F U-1: `applied`, `needs_review` and the per-album `change_count`
are `(track, field)` counts; a collapsed display row carries `applies_to_tracks: n` and is
worth `n` in its album's `change_count`. Never report display rows as "changes" — an agent
that adds album counts together must arrive at the summary's total.

**Capability.** Add `CAP_TAGS_WRITE = "agent.capability.tags:write"`, default **false**,
to `capability.rs:14–33`, `tags_write_granted()`, `tags_write_effective()`, and the
`StartupCaps` field in `startup.rs:28–40`. Gate `apply_safe: true` and all of
`music_apply_tags`, exactly the way `sources:manage` is gated: snapshot at startup, check
live, `effective(startup, live)` so a grant needs a restart and a revocation bites
immediately. Denial goes through `DataError::CapabilityDenied("tags:write")` and reuses
the existing message in `error.rs:49–65`; that message states the restart requirement, so
an agent is told why a fresh grant is not yet in force (§F-9).

**Leak rule D19.** `DoctorWriteRow` carries `path: PathBuf` and `DoctorTrackRef` carries a
path, an inode and a device id. **None of them may appear in an MCP DTO.** Rows are
identified by `row_id` and `track_id` only; failures report `error_kind` and a track title,
never a file name. `tests/common/mod.rs`'s `assert_no_leaks` already fails on `/home/`,
`/music/`, `.flac` and `.db`; add `doc_11a_doctor_responses_carry_no_file_paths` to run it
over all three tools' responses.

**Refusal.** `TagWriteBusy` from P3 maps to a caller-visible tool error, not an internal
one — an agent must be able to retry.

### P10 — Gates and bookkeeping

Delete from `strings_library_doctor.rs`: `DOCTOR_LOCAL_ALWAYS_INCLUDED`,
`DOCTOR_SAFE_FIXES`, `DOCTOR_SUGGESTIONS`, `DOCTOR_TRACKS_CHECKED`, `DOCTOR_REVIEW_SAFE`,
`doctor_review_safe_fixes()`, `doctor_apply_tracks()`, `DOCTOR_ALL_SAFE`,
`DOCTOR_JOB_PAGE_DESCRIPTION`, `DOCTOR_TRACK_AND_FIELD`, `DOCTOR_SCAN_OPTIONS`,
`DOCTOR_RESULTS`, `LIBRARY_DOCTOR_DESCRIPTION` (the Plugins subtitle, now unreachable),
`doctor_problem_counts()`. Verify each with `rg` before deleting; the inventory's
"already unused" list is wrong in at least four places (§F-1).

Then remove the `#![allow(dead_code)]` that P5 added at the top of the file and re-run
clippy: with the removal list applied, the file must be clean without the attribute, which
restores today's stricter state and proves the list was complete (§F-20).

Then: regenerate `po/reprise.pot`, reconcile the eight catalogs, keep `po/POTFILES.in`
correct, lower the `check-frontend-thinness.sh` budgets to the measured values in the same
commit, and append the ledger line to `.superpowers/sdd/progress.md`.

---

## D. The four delicate spots

### D-1. Undo as a bracket

**Verified premise.** `revert_last_cleanup()` (`write.rs:742`) reverts exactly one job:
`last_cleanup()` (`write.rs:717`) selects a single row with
`ORDER BY j.id DESC LIMIT 1`. Two jobs of one `scan_id` cannot be undone together today.

**Decision.** `last_cleanup()` returns a *set*: the largest `scan_id` among eligible
`doctor_apply` jobs, and all eligible jobs of that scan. `revert_last_cleanup()` walks the
set newest-first and issues one `doctor_revert` job per source job, because
`db_tag_write_jobs.rs:14–16` requires `doctor_revert` to name exactly one
`source_job_id`. A single continuous progress count spans the set, so the user sees one
operation.

**Only one of the two exists.** The set has one member and the behavior is byte-identical
to today. This is the normal case for a scan whose review was never applied, and for a scan
with no auto-applicable rows.

**One of them partially failed.** Do not stop. Revert every job in the set, and let each
field fail on its own terms — the same rule the apply path already follows (DOC-5a: a
conflicting field is skipped without blocking its siblings). Rationale: stopping at the
first failure leaves the user with a *more* mixed state and no way to finish, and the
Undo promise ("everything from this scan") would be broken by a single read-only file.
Report per-job outcomes in `DoctorCleanupReport`; the GUI shows
`doctor_write_failures()` once, never per file.

**Consequence, and it is a feature.** A field that could not be reverted keeps
`outcome='applied'` in its source journal, so `last_cleanup()` keeps returning that job and
a second Undo retries exactly the remainder. A fully reverted scan disappears from
`last_cleanup()` on its own — no extra bookkeeping table, no "consumed" flag.

**Cancel.** `DoctorWriteControl::Cancel` stops after the current file and does **not**
start the next job in the set. The cleanup stays available. This differs from failure on
purpose: cancel is a user intent, failure is not.

**What the copy may promise.** The summary's `Undo`, the toast's `Undo` and the post-apply
`Undo everything from this scan` all call the same paired revert, so
`doctor_includes_quiet_fixes()` is literally true. The post-apply caption must not promise
undo beyond the next scan — a new scan does not invalidate the journal, but it does replace
the visible pointer, and the copy says "until the next scan" for that reason.

### D-2. Migration

**Verified premise.** The tier is not persisted. `library_doctor_proposals`
(`db_library_doctor.rs:37–52`) stores `field, current_value, proposed_value, source,
confidence, preselected, problem_class` and no tier column; the split is recomputed on
every projection in `summary_for_parts()` (`presentation.rs:129–137`) from
`source == Local && preselected && !stale`. `DoctorScanSummary` is a derived value.

**Consequence.** No data migration is required to reclassify. An old scan projects
correctly under the new predicate the moment `is_auto_applied()` replaces the inline
condition.

**But the brief's hard requirement still bites.** Under the new policy a scan's
auto-applied set is written *at scan completion*. A scan stored under the old rules never
went through that step, and it contains `RecordingMbid` proposals that the new review list
filters out. If the old scan simply reloaded, those proposals would be invisible **and**
never applied — silently dropped. That is exactly the outcome the brief forbids.

**Decision: invalidate the pointer, keep the journal.** Migration v57 sets
`library_doctor_state.last_complete_scan_id = NULL` and adds a `reviewed_scan_id` column.
After the upgrade the Doctor opens on its start page with no result, and one `Run Scan Now`
produces a result under the new rules. Nothing is applied without asking, because nothing
is applied at all until a new scan completes.

**What is deliberately not done.** The `library_doctor_scans` rows are *not* deleted.
`tag_write_jobs.scan_id` references them with `ON DELETE RESTRICT`, so deleting a scan that
has an apply job would either fail the migration or, worse, take the undo journal with it.
Keeping the rows costs nothing and keeps `last_cleanup()` — which reads `tag_write_jobs`,
not the scan pointer — working across the upgrade. A user who ran a cleanup yesterday can
still revert it today. That is the property worth protecting.

**Test.** `doc_10c_upgrade_clears_the_stored_scan_pointer_and_keeps_the_cleanup_revertible`
builds a v56-shaped database with a completed scan, a completed `doctor_apply` job and a
set pointer; migrates; asserts `last_complete_scan_id IS NULL` and
`last_cleanup().is_some()`.

### D-3. Album grouping without test coverage

**The premise in the brief is true but does not apply here.** `SectionModel` is indeed
compiled out under `cfg(test)` — but only for the hand-written subclass `TrackListModel`
(`track_list_model.rs:118–121`: `#[cfg(not(test))] type Interfaces = (gio::ListModel,
gtk4::SectionModel);`), because its `interface_init` asserts that the registering thread ran
`gtk4::init()`, which `cargo test`'s worker threads race for. The gate is a property of
*subclassing*, not of sections.

**Decision: do not subclass.** `GtkSortListModel` implements `GtkSectionModel` natively
(gtk4 0.11.4, `src/auto/sort_list_model.rs:20`) and exposes `set_section_sorter()`
(`:128`). Using it means no `interface_init` in this crate, no `cfg(test)` gate, and GTK
maintains the section ranges itself whenever the underlying `ListStore` changes. This is
also precisely what the brief's own instruction describes
("`gtk_sort_list_model_set_section_sorter()`"). A hand-rolled section model in the Doctor
is forbidden by this plan.

**Proof, in three layers.**

1. **The grouping decision is core and display-free.** `grouping::group_review_rows()`
   (P2) is a pure function over `DoctorScan` + `DoctorReviewSession`. Its four
   `doc_9b_*` tests run in the ordinary `cargo test --workspace` and prove group identity,
   order, the `AllTracks` collapse, the no-album bucket and the counting rule. This is
   where the risk actually lives, and it is fully covered.
2. **The wiring gets a display test.** `doc_9b_review_groups_render_one_header_per_album`
   carries `#[ignore = "requires a display; run via xvfb-run"]` — the one justification
   string `check-ux-traceability.sh` accepts for an `[active]` rule — builds the real page
   over a synthetic three-album scan and asserts, through `SectionModelExt::section()`,
   that each row reports the range of its album and that the header factory produced
   exactly three headers. It runs under `scripts/check-display-tests.sh`, one exact test
   per process.
3. **Partial deltas get an example.** `crates/reprise-gnome/examples/doctor_review_sections_repro.rs`,
   modeled on `examples/queue_section_shift_repro.rs`, builds the same stack, then mutates
   one row in place and re-reads every section range. The known failure mode in this
   repository is an `items_changed` that is not accompanied by a `sections_changed` for the
   touched range; the example asserts the ranges survive and exits non-zero when they do
   not, with a `--no-sections-changed` flag that reproduces the bug on demand. Run it as
   `xvfb-run -a cargo run -p reprise-gnome --example doctor_review_sections_repro`.

**The concrete rule the code must follow.** Any in-place row update goes through
`ListStore::splice(position, 1, &[new_item])` — never through mutating the boxed object
without an emission — so the `SortListModel` re-runs both sorters and re-declares its
sections. If a future change ever needs a partial `items_changed` on an owned model, it
must emit `sections_changed(position, n_items)` covering at least the touched range in the
same turn.

### D-4. GUI and MCP write into the same queue

**Verified premise.** The only write lock today is `TagWriteGate`
(`crates/reprise-gnome/src/ui/tag_write_gate.rs`), an `Arc<AtomicBool>` inside one process.
The MCP is a separate process over stdio that opens a fresh short-lived `Db` per request
(`data.rs:85`, `Db::open_ready`). A process-local atomic cannot see it. Two concurrent
tag-write jobs against one library are therefore possible today the moment the MCP can
write.

**Decision: the job row is the lock.** `tag_write_jobs.state` already carries the
invariant `state IN ('prepared','running') ⟺ finished_at IS NULL`
(`db_tag_write_jobs.rs:11`), and both writers must insert a job row before touching a file.
`claim_tag_write_slot(conn)` (P3) runs as the first statement inside the same transaction
that inserts the job:

```sql
SELECT 1 FROM tag_write_jobs WHERE state IN ('prepared', 'running') LIMIT 1
```

A hit aborts the transaction with `TagWriteBusy`. SQLite's WAL gives exactly one writer at
a time and the surfaces already carry a `busy_timeout` plus a facade retry
(`TESTING.md`, "Busy-retry under a held foreign write transaction"), so the check and the
insert are atomic against a competing process — the loser's `BEGIN IMMEDIATE` waits, then
sees the winner's row.

**What each side sees.**
- **GUI, MCP busy** — `start_apply()` / `start_revert()` already surface
  `TAG_WRITE_BUSY` ("Another tag-writing job is already running") through a toast
  (`mod.rs:468, 523`). That message now also covers a foreign process; its wording already
  does not claim the job is the user's own. The `TagWriteGate` stays as the cheap
  in-process guard — it is a fast path, not the truth — and `DOCTOR_CONTROLS_LOCKED`
  keeps explaining the disabled controls.
- **MCP, GUI busy** — `music_apply_tags` and `music_scan_tags { apply_safe: true }`
  return a caller-visible tool error carrying the `TagWriteBusy` message, never an internal
  error, so an agent can back off and retry. Read-only `music_review_tags` and
  `music_scan_tags { apply_safe: false }` are never refused: they take no slot.

**Stale locks.** A crashed writer leaves a `prepared`/`running` row that would hold the
lock forever. It does not, because the existing recovery path closes it:
`LibraryDoctor::finalize_incomplete_writes()` (`write_recovery.rs:153`) moves such a job to
`interrupted` (or `cancelled`) with a `finished_at`, and it already runs at startup. The
plan adds one thing: the MCP calls `recover_incomplete_tag_write_jobs()` before its first
`claim_tag_write_slot()` in a session, so an agent-only host is not blocked by a GUI crash.
`doc_10b_a_finalized_interrupted_job_does_not_hold_the_lock` pins this.

**Shared identity.** Both surfaces write with `kind='doctor_apply'` and the same
`scan_id`, so an agent-run scan produces the same sidebar entry
(`count_pending_doctor_findings` reads the journal, not the process) and a GUI `Undo`
reverts an MCP apply through the same bracket from D-1. That is the point of putting the
lock in the database rather than in either process.

---

## E. Gates and bookkeeping

### What must be green at the end

```sh
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo audit                      # only RUSTSEC-2024-0436 (paste) is accepted
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'   # must be empty
scripts/check-architecture.sh
scripts/check-ux-traceability.sh
scripts/check-frontend-thinness.sh
scripts/check-accessibility-semantics.sh
scripts/check-display-tests.sh
```

The per-stage subsets are listed under "Stages"; this block is the Stage-3 battery and the
release gate for the branch.

**`check-frontend-thinness.sh` is a ceiling *and* a floor.** Verified in the script: a
count above the budget and a count below it both fail
(`frontend thinness: $name is down to $actual (budget still says $allowed)`). This plan
deletes `preference_library_doctor.rs` (392 lines) and `job_page.rs` (60 lines) from
`crates/reprise-gnome/src`, so `rusqlite=112`, `filesystem=19`, `threads=15`, `workers=7`
must be re-measured and lowered **in the same commit** that removes the code — which is
P10. This is why the script is deliberately absent from Stage 2's acceptance block: between
the deletion in Stage 2 and the re-measurement in Stage 3 it fails as a floor violation,
and that is expected. The zero-tolerance bans (`gstreamer`, `zbus`, `.conn(`) stay at zero
in every stage — note that the Doctor coordinator's field is `self.conn: Rc<Db>`, a field
access, not the banned `.conn()` call. `view_floor=1352` for `crates/reprise-view/src` is
untouched by this plan; if it moves, something went into the wrong crate.

**`check-accessibility-semantics.sh`** requires
`// a11y-semantics: role=… name=… state=… action=…` on the line immediately before every
`set_focusable(true)`. New focusable widgets in this plan: the sidebar Doctor row, the
PLAYLISTS `+` button, the inline-rename `EditableLabel`, the scope `ToggleGroup`, the
filter `ToggleGroup`, the album header checkbox, the conflict radio pills.

**`check-display-tests.sh`** discovers every `#[ignore]`d test in `reprise-gnome` and runs
each one in its own process under a private D-Bus, Xvfb, and isolated XDG roots. New
display tests must carry exactly `#[ignore = "requires a display; run via xvfb-run"]` —
the only justification `check-ux-traceability.sh` accepts on an `[active]` rule's test.
Never run them as one filtered invocation (`TESTING.md`, "Isolated GTK and desktop tests").

**`check-ux-traceability.sh` across the stage boundaries.** The gate requires every
`[active]` rule to have **at least one** test carrying its ID, and forbids a test naming an
ID that is missing or `[replaced]`. That is what makes the three-stage cut legal: a rule
lands `[active]` in the package that writes its first test, and a later package may append
further test names to the same rule's list. Concretely, DOC-10b goes `[active]` in P3 with
its three core tests and gains its MCP test name in P9; DOC-11a lands `[active]` in P9 and
not before, because no `doc_11a_*` test exists until then.

### `docs/ux-rules.md` §Y — rewritten, retargeted, new

**Rewritten** (same ID, new text; the old text describes behavior that no longer exists):

- **DOC-2b** [active] [gtk] — **The result page is a summary of three meanings, never a
  write surface.** After a scan the Doctor shows at most three blocks: what was already
  applied without asking, what needs a decision, and what has no clear winner. A block
  whose count is zero is not rendered. The applied block names its kinds separately —
  spacing/casing and MusicBrainz IDs, the latter with "no visible change to your tags" —
  and carries Undo. The decision block names one line per remaining category plus the
  album count and carries "Review N changes". The conflict block states that the
  conflicts sit at the end of the review list and are skippable. Every number on the page
  counts tag changes that were or will be written, one per track and field. Checked and
  skipped tracks are one muted line of scan facts, not a finding. With the remote switch
  off, remote categories and counts disappear completely while the local result stays.

- **DOC-2c** [active] [gtk] — **A running scan shows the same two blocks, counting up in
  the tense that is true.** During the job the Doctor page shows "Results found so far"
  with the applied and the decision block already in their final shape, updating after
  every completed track, all actions insensitive, and "Locked while a Library Doctor job is
  running" as the reason. The applied block counts in the future — "N fixes to apply" with
  its Undo disabled — because nothing is written while the scan runs; it switches to
  "N fixes already applied" only when the quiet write job reports, and then reports what
  the write actually applied. The intermediate state is never persisted and never
  applicable. Cancel or an error discards it and restores the last completed result.

- **DOC-3a** [active] [core] — **Review decides per field, and everything reviewable
  starts selected.** Every concrete track/field change has its own selection and arrives
  preselected. "All" selects every ready row, "None" clears everything; neither touches a
  stale or conflicting row. A tie shows "N spellings, no clear winner — pick one" with only
  real candidates and their frequencies, with no default. Picking a candidate materializes
  the affected diffs; individual rows stay deselectable; changing the candidate recomputes
  them and preserves manual deselections while the same row remains affected. Review order
  stays stable during the session, in scope order and the fixed field sequence Title,
  Artist, Album, Album Artist, Year, Genre. Apply receives an immutable plan of exactly the
  current selection.

- **DOC-3b** [active] [gtk] — **One column header serves the whole page, wide and
  narrow.** The review page carries exactly one header row — Track, Field, Current,
  Proposed, Source — bound to every row through a shared size group; no row repeats a
  caption. Empty appears as "— empty —"; a replaced Current value is struck through. One
  page-level breakpoint at 640 px stacks Current → Proposed and hides the shared header;
  there is no per-row breakpoint and no horizontal page scroll in either state. Both
  presentations bind the same selection and preserve row focus and stable order.
  Ellipsized values keep a full-text tooltip and an accessible description that names
  track, field, current, proposed and source, because the visible captions are gone.
  "Edit track tags…" opens the existing Tag Editor; its Save marks affected rows stale and
  deselects them.

- **DOC-5c** [active] [gtk] — **Write jobs don't freeze the UI.** Apply and Revert run in
  the shared sidebar progress card with a visible Cancel and the same geometry as
  Scan/Sync. Progress counts tracks: "Updating tags… 42/128 tracks", "Tags updated · 128
  tracks", "42 tracks updated · 86 cancelled". The Apply button counts changes —
  "Apply N changes" — because that is the unit the review list decides in. Collected errors
  appear once as "N updated, M failed · Details", never per file. The remote toggle and the
  selection are locked during the write job.

- **DOC-7b** [active] [gtk] — **The Library Doctor has exactly one entry point.** The
  global ⋮ menu carries one flat "Library Doctor" item with no badge and no submenu; there
  is no Preferences surface for the feature. Its start page owns scope, the remote switch,
  "Run Scan Now" and the only "Revert Last Cleanup" in the app. The summary is a root page
  in `content_nav`, the review page is pushed onto it, and Back returns with the in-session
  selection unchanged. There is no Doctor dialog and no Apply confirmation dialog. Scope is
  not persistent: Whole Library by default, Current View suggested from a filtered view,
  Selection from a selection context.

  *(The "STATS-DEDUP hint" entry point the old text claimed is dropped, not implemented —
  §F-17.)*

**Retargeted** (ID kept alive, test re-pointed in the same commit that moves it):

- **DOC-3b** — its three tests move: `doc_3b_breakpoint_changes_layout_without_changing_row_identity`
  now exercises the page-level breakpoint instead of the per-row one;
  `doc_3b_source_column_keeps_its_caption_and_value_in_one_parented_section` is replaced by
  `doc_9b_rows_carry_no_caption_labels`, which asserts the opposite property — that no
  caption exists — so the ID keeps a test through
  `doc_3b_review_page_virtualizes_rows_without_horizontal_scroll` and the reworked
  breakpoint test.
- **DOC-4b** — the rule text is unchanged; only the home of
  `doc_4b_confidence_uses_redundant_source_text_tone_and_warning` and
  `doc_4b_manual_candidate_exposes_available_remote_evidence` moves with `review_model.rs`.
  Do not touch the assertions.
- **DOC-6b** — `doc_6b_library_doctor_controls_explain_job_locking` lives in
  `preference_plugins_tests.rs`, which this plan deletes. Retarget it at the start page's
  locked state. `doc_6b_tag_write_gate_has_one_owner_and_releases_on_every_exit` stays;
  the two `doc_6b_sidebar_*` tests in `window_navigation.rs` move from the deleted job page
  to the summary page.
- **DOC-7a** — `doc_7a_first_remote_enable_requires_confirmation_and_cancel_stays_off`
  moves with the consent sheet into `library_doctor/remote_toggle.rs`, assertions
  unchanged.
- **DOC-6a** and **DOC-1d** stay as `[replaced by …]` markers. Do not delete them; IDs are
  append-only.

**New rules** (each lands `[active]` in the package that writes its first test; a rule
without a test does not pass `check-ux-traceability.sh`):

- **DOC-8a** [active] [gtk] — **The menu holds the verb, the sidebar holds the noun.**
  The global ⋮ menu is the only way to start a scan. While a completed scan has unreviewed
  findings, and only then, a "Library Doctor" row appears under ISSUES next to "Missing
  files", carrying the count of tag changes still waiting; it disappears when the scan is
  acknowledged — "Done" on the post-apply page, or "Skip all" in the conflicts section —
  even if not every row was applied. A finished scan never interrupts: on the Doctor page it
  resolves in place, elsewhere it is that sidebar row. Exactly one toast fires, "N tags
  fixed" with Undo, and only for the set that was applied without asking. Findings that need
  review never toast.
  *Tests:* `doc_8a_the_menu_carries_exactly_one_library_doctor_item_and_no_sync_device`,
  `doc_8a_the_issues_entry_appears_only_with_unreviewed_findings`,
  `doc_8a_quiet_fixes_produce_one_undo_toast_and_review_findings_produce_none`,
  `doc_8a_pending_review_count_excludes_everything_already_written_for_that_scan`,
  `doc_8a_pending_review_count_is_zero_once_the_scan_is_marked_reviewed`,
  `doc_8a_conflicts_alone_do_not_produce_a_pending_count`,
  `doc_8a_done_marks_the_scan_reviewed_and_clears_the_sidebar_entry`,
  `doc_8a_skip_all_marks_the_scan_reviewed`.

- **DOC-8b** [active] [core] — **Two tiers, and exactly one predicate decides.** A
  proposal is applied without asking when it is a MusicBrainz recording ID, or when it is
  local and preselected; never when its track is stale. Everything else is shown for
  review, preselected. Recording IDs never appear in the review list. The applied set is
  enqueued as a tag-write job the moment the scan completes, before the summary is
  presented, and is reported as done; nothing is written while the scan is still running.
  There is no surface that lists the applied set — it is represented by two counted lines
  and an Undo. The tier is computed by one function used by the core, the GTK surface and
  the agent adapter alike; a second copy of the condition is a defect. *Tests:*
  `doc_8b_auto_applied_tier_is_local_preselected_plus_every_recording_mbid`,
  `doc_8b_stale_rows_are_never_auto_applied`,
  `doc_8b_review_tier_preselects_every_ready_row`,
  `doc_8b_recording_mbid_never_reaches_the_review_tier`,
  `doc_8b_all_preset_selects_every_ready_row_and_none_clears_them`,
  `doc_8b_scan_completion_enqueues_the_auto_applied_job_before_the_summary`,
  `doc_8b_a_scan_with_no_auto_rows_creates_no_job`.

- **DOC-8c** [active] [gtk] — **The start page owns the run.** Scope is a segmented
  control with three always-visible options, not a dropdown. The remote toggle carries its
  privacy sentence verbatim and, on first activation, the existing versioned consent sheet.
  "Run Scan Now" is the single primary action, with a track count and a rough duration
  beside it. Below a separator, and only when a revertible cleanup exists, the last-scan
  line and "Revert Last Cleanup" — the only revert in the app. *Tests:*
  `doc_8c_start_page_carries_scope_remote_run_and_the_only_revert`,
  `doc_8c_last_scan_block_is_hidden_without_a_revertible_cleanup`.

- **DOC-9a** [active] [gtk] — see DOC-2b; DOC-9a covers the zero-count rule, the block
  order and the counting unit. *Tests:*
  `doc_9a_summary_renders_three_blocks_and_never_a_zero_row`,
  `doc_9a_summary_omits_the_conflicts_block_without_conflicts`,
  `doc_9a_every_visible_count_is_a_written_change_count`.

- **DOC-9b** [active] [gtk] — **The review list is grouped by album.** Rows are grouped by
  album in scope order under one header per album carrying a group checkbox, cover, title,
  "{artist} · N tracks" and a change count. A change that applies identically to every
  track of an album collapses into one row reading "All N tracks"; a partial album does
  not collapse. Tracks without an album form one trailing group. A filter bar offers only
  the categories the scan actually produced. Spelling conflicts sit at the end in a dashed,
  explicitly optional container with "Skip all". The whole list scrolls under a sticky
  header and a sticky footer; there is no pagination and no collapsed remainder row. Every
  count the user sees — the album pill, the toolbar total and the footer — counts tag
  changes that will be written, not display rows, so a collapsed "All N tracks" row is
  worth N. *Tests:* `doc_9b_rows_group_by_album_in_scope_order`,
  `doc_9b_album_level_change_collapses_into_one_row_over_all_tracks`,
  `doc_9b_tracks_without_an_album_form_one_trailing_group`,
  `doc_9b_group_counts_report_written_changes_not_display_rows`,
  `doc_9b_one_column_header_serves_the_whole_page`, `doc_9b_rows_carry_no_caption_labels`,
  `doc_9b_review_groups_render_one_header_per_album`,
  `doc_9b_every_reviewable_row_starts_selected`,
  `doc_9b_the_filter_bar_offers_only_categories_present_in_the_scan`,
  `doc_9b_conflicts_sit_at_the_end_and_skip_all_clears_them`,
  `doc_9b_footer_counts_the_changes_that_will_be_written`,
  `doc_9b_the_album_pill_counts_written_changes_not_display_rows`.

- **DOC-9c** [active] [gtk] — **After the write, and after a clean scan, the Doctor says
  so on its own page.** Post-apply names the updated tracks, the changes and albums, and
  the conflicts left open, states that they return with the next scan, and offers
  "Undo everything from this scan" beside "Done", with a caption naming the quiet fixes it
  includes. Its numbers come from the write report, never from the frozen plan. "Done"
  acknowledges the whole scan. A scan that found nothing is its own state — "Nothing to
  fix", the checked and skipped counts, "Scan again" — and is never confused with the
  pre-scan start page. *Tests:*
  `doc_9c_post_apply_names_the_quiet_fixes_and_the_unresolved_conflicts`,
  `doc_9c_post_apply_reports_the_write_report_not_the_plan`,
  `doc_9c_nothing_to_fix_is_distinct_from_the_pre_scan_state`.

- **DOC-10a** [active] [core] — **Undo is one bracket per scan.** The job applied without
  asking and the reviewed job of the same scan revert together as one operation with one
  progress count. A failing field does not stop the remaining fields or the remaining job;
  a cancel does stop the next job. A partially reverted cleanup stays offered, so a second
  Undo retries exactly the remainder, and a fully reverted scan is no longer offered.
  *Tests:* `doc_10a_undo_reverts_the_quiet_and_the_reviewed_job_of_one_scan`,
  `doc_10a_undo_works_when_only_the_quiet_job_exists`,
  `doc_10a_partial_revert_leaves_the_cleanup_available_for_a_second_attempt`,
  `doc_10a_cancel_between_jobs_does_not_start_the_remaining_job`,
  `doc_10a_a_fully_reverted_scan_is_no_longer_offered`.

- **DOC-10b** [active] [core] — **One tag-write slot, enforced in the database.** A
  tag-write job of any kind may only be created while no other job is prepared or running;
  the check and the insert share one transaction. The refusal is caller-visible on both
  surfaces — a toast in the app, a retryable tool error for an agent — and never an
  internal error. A job left behind by a crashed process is finalized by the existing
  recovery path and holds no slot. *Tests:*
  `doc_10b_a_second_tag_write_job_is_refused_while_one_is_prepared_or_running`,
  `doc_10b_a_finalized_interrupted_job_does_not_hold_the_lock`,
  `doc_10b_tag_editor_and_doctor_share_one_lock`,
  `doc_10b_gui_sees_the_same_refusal_while_an_mcp_job_runs`,
  `doc_10b_mcp_refuses_while_a_gui_job_holds_the_lock` *(added by P9 in Stage 3)*.

- **DOC-10c** [active] [core] — **An upgrade never inherits a decision.** A scan stored
  under the previous rules is not reinterpreted and nothing from it is applied; the stored
  result pointer is cleared on upgrade and the Doctor opens on its start page. The undo
  journal is untouched, so a cleanup applied before the upgrade stays revertible.
  *Test:* `doc_10c_upgrade_clears_the_stored_scan_pointer_and_keeps_the_cleanup_revertible`.

- **DOC-11a** [active] [core] — **The agent adapter finds and reports; it writes only when
  asked.** `music_scan_tags` is read-only by default: the automatic application of the
  unambiguous set happens only with an explicit `apply_safe`. Every mutation —
  `apply_safe`, and all of `music_apply_tags` — sits behind the `tags:write` capability,
  off by default, granted at startup and revocable live. Responses carry no file paths,
  no library roots and no credentials, and every count they report is a tag-change count in
  the same unit the app shows. Both surfaces write through the same job queue and the same
  scan id, so an agent scan produces the app's sidebar entry and an app Undo reverts an
  agent apply. *Tests:*
  `doc_11a_scan_tags_does_not_write_without_apply_safe`,
  `doc_11a_apply_safe_requires_the_tags_write_capability`,
  `doc_11a_apply_tags_requires_the_tags_write_capability`,
  `doc_11a_review_tags_groups_by_album_and_filters_by_category`,
  `doc_11a_review_tags_counts_written_changes_per_album`,
  `doc_11a_doctor_responses_carry_no_file_paths`.

- **NAV-14** [active] [gtk] — **A section header carries its own create action.** The
  PLAYLISTS header carries a `+` button that creates a playlist immediately: a new row
  appears in place, named "Untitled playlist" with the name selected for inline rename, and
  no dialog opens. Enter or moving focus away commits the typed name; an empty name keeps
  "Untitled playlist"; Escape discards the row and the playlist with it. "Import playlist…"
  lives in the global ⋮ menu with the other library-wide verbs; neither action occupies a
  sidebar row. *Tests:*
  `nav_14_the_playlists_header_creates_a_playlist_in_place_without_a_dialog`,
  `nav_14_escape_discards_the_new_playlist_row_and_the_playlist`,
  `nav_14_an_empty_name_keeps_the_untitled_playlist`,
  `nav_14_import_playlist_lives_in_the_overflow_menu`.

**DOC-6c** [planned] [manual] stays planned; extend its text to name the new surfaces
(start page, grouped review with one header, post-apply, nothing-to-fix, the sidebar
entry) so the manual release pass covers them.

---

## F. Decisions taken

Nothing here is open. **U-1 … U-6** are product decisions made by the owner of this
feature; they override any contrary reading of the brief or the mockup. **F-1 … F-20** are
engineering decisions, each with the reason it was decided that way, so an implementer can
tell a deliberate choice from an oversight. Where a decision departs from the mockup, the
departure is named — the implementer never sees the frames and must not "restore" them.

### U-1. Every visible number counts written tag changes

The album pill, the summary headline, the review toolbar, the footer, the post-apply page,
the sidebar badge and every MCP field carry the same unit: one `(track_id, field)` pair
that was or will be written. A row that collapses an album-level change into
`All 11 tracks` is worth **eleven**.

Concretely, using the mockup's own first review group — "Count Your Blessings", four
display rows, one of which is `All 11 tracks · Album artist`: this implementation shows
**14 changes** in that album's pill, where the frame shows "4 changes". The same
multiplication happens to "88 changes need your eye", to "85 tag changes · 61 files" and to
the sidebar badge. **The numbers will be visibly larger than the mockup's. That is
correct.** Do not add a second, display-row-based count "to match the frame", and do not
file it as a bug.

*Why:* the footer string `doctor_apply_summary()` — "{changes} tag changes · {files} files
· undo available after" — is kept verbatim by decision, and it has always counted field
writes. A headline that counted display rows would disagree with the footer directly
beneath it, which is exactly the confusion this redesign exists to remove. One unit
everywhere also makes the numbers checkable: album pills must sum to the toolbar total, and
the toolbar total minus deselections must equal the footer.

This decision also settles the mockup's unreconciled 88 / 86 / 85: those figures are an
artifact of a static frame and are not reproduced. There is exactly one source per number —
the toolbar and the summary read the session's ready rows, the footer reads
`session.summary().tag_change_count`, and the post-apply page reads the **write report's**
applied counts, never the plan's. A post-apply page that reported the plan would lie the
moment one file was read-only.

*Affected sites, all of which this plan already states:* P2 `change_count`, P4
`count_pending_doctor_findings`, P7 summary blocks 1 and 2 and post-apply, P8 album pill,
filter-bar total and footer, P9 the DTO fields.

### U-2. The quiet fixes get two counted lines and an Undo — no list, ever

`DoctorReviewFilter::AllChanges` is deleted with no replacement. There is no surface that
enumerates the automatically applied changes: block 1 of the summary shows the total, two
lines splitting it by kind, and `Undo`. No expander, no disclosure, no third filter, no
"show me what you did" page.

*Why:* a list of 809 invisible MusicBrainz IDs is precisely the attention cost this
redesign removes, and Undo plus the tag editor already cover the "I want to see it" case.
Insight beyond the counter is deliberately not offered.

### U-3. During the scan, block 1 counts in the future

While the scan runs, block 1 reads `{n} fixes to apply` and its MusicBrainz line reads
`{n} MusicBrainz IDs to fill in — no visible change to your tags`, both counting up from
the partial summary, with `Undo` disabled. It keeps that form until the quiet write job
completes, then switches to `{n} fixes already applied` / `{n} MusicBrainz IDs filled
in — …` with the write report's actual counts and an enabled `Undo`.

**Nothing is written while the scan runs.** The write starts after the scan completes and
the plan is frozen.

*Deviation from the mockup:* Frame 3 shows "511 fixes applied so far" during the scan. That
sentence cannot be true under this policy, so the wording changes and
`doctor_fixes_applied_so_far()` is not added. *Why:* writing tags while the scan is still
discovering them would break DOC-5a's "read the file again immediately before writing" and
put two writers on the same file.

### U-4. One receipt for the whole scan retires the sidebar entry

`library_doctor_state.reviewed_scan_id` is set in exactly two situations: the user presses
`Done` on the post-apply page, or the user presses `Skip all` in the conflicts section of
the review page. The sidebar ISSUES row hides as soon as `reviewed_scan_id` equals
`last_complete_scan_id` — **even if not every row was applied**.

*Why:* per-row dismissal would need a new table and a new affordance for a state the user
has no reason to curate. Both gestures mean "I am finished with this scan"; treating them
as a whole-scan acknowledgement matches how the flow actually ends. The consequence is
intended: pressing `Skip all` while unapplied review rows remain does clear the entry.
Do not "correct" this to a per-row or partial rule.

### U-5. The playlist quick-add is built in full

`+` in the PLAYLISTS section header; the row appears in the list immediately; the name is
selected and typed over directly; Enter commits; **Escape discards the edit and the row**,
deleting the just-created playlist. Committing an empty name keeps `Untitled playlist`
rather than deleting — Enter means "keep this", and a nameless sidebar row would be
unaddressable. Implemented with `GtkEditableLabel`, which appears nowhere in this codebase
today, so this is new behavior in the sidebar rather than a rearrangement, and it is
planned as such in P6 including the full keyboard path.

*Deviation from the draft plan:* an earlier version had Escape fall back to
`Untitled playlist`. It now destroys the row. *Why:* the `+` is a one-click action with no
confirmation step, so Escape has to be the way out of an accidental click; leaving an
`Untitled playlist` behind after a cancel would make the sidebar accumulate junk.

### U-6. `Sync Device…` is removed from the ⋮ menu, and the loss is accepted

The item goes. The known consequence, stated here so it does not come back as a bug report:
the sidebar's device section is only visible while a device is connected
(`sidebar_device_card.rs:80` sets the section invisible when the device list is empty) and
there is no Sync page in Preferences, so after this change a **disconnected** device has no
entry point at all. That is accepted.

---

### F-1. Delete no string on the inventory's word

`doctor_evidence_value()`, `doctor_duration_ms()` and `doctor_duration_delta_ms()` are used
by `review_model::candidate_description()` (`review_model.rs:182–196`);
`doctor_group_count()` is used by `summary_page.rs:399`; `doctor_apply_summary()` is used
by `review_page.rs:52–55`. `preference_library_doctor.rs` is 392 lines, not 362. P10
re-greps every symbol before removing it. *Why:* the inventory was produced by a survey
agent and is a map, not a contract; a wrongly deleted string is a build break, and a
wrongly kept one is a translator's dead entry.

### F-4. The remote consent sheet moves, it does not die

The brief says "delete `preference_library_doctor.rs`", but that file also owns
`remote_suggestions_row_for()` — called by both the summary page and the review page — and
`present_remote_confirmation()`, the versioned opt-in dialog the brief explicitly keeps.
Those five functions and the DOC-7a test move to `library_doctor/remote_toggle.rs`; the
rest of the file is deleted. *Why:* deleting the file wholesale would delete a behavior the
brief preserves, and moving it puts the toggle next to the only feature that uses it.

### F-7. The pending-review badge ignores staleness, and that is fine

The badge is "proposals of the scan minus fields already written for that scan", which is
exact with respect to writes but counts a stale row as still pending until the user opens
the page and the exact revalidation runs. The over-count is accepted. *Why:* the
alternative is loading the whole scan and re-stat-ing every file on every sidebar rebuild,
and DOC-2a already establishes that staleness is checked cheaply on reopen and exactly
before writing — a badge is not a write.

### F-8. `music_review_tags` is a tool, not a `reprise://tag-issues` resource

*Why:* the seven existing resources are argument-free snapshots read by URI; this one needs
a category filter and pagination, which the resource protocol would have to encode into the
URI string.

### F-9. A `tags:write` grant needs a restart; a revocation does not

`effective(startup_snapshot, live_value)` (`capability.rs:96`) keeps its asymmetry, and the
denial message says so. *Why:* it is the established pattern for every write capability in
this crate, and weakening it for the one capability that writes to the user's music files
would be exactly backwards.

### F-10. The header bar and the sidebar follow the app's existing behavior

The frames disagree with themselves — Frame 1 has a back arrow and four icons, Frame 2 none
and three, Frames 3 and 4 a back arrow and two; some frames omit the PLAYLISTS section or
individual SMART rows. Implement the app's standard header and sidebar and replicate none
of the omissions. Only two deltas are real: the progress card pinned to the sidebar bottom
during a job, and the ISSUES entry. *Why:* the frames were assembled independently and the
variation carries no rule; copying it would produce a header that changes for no reason the
user can name.

### F-11. The accent highlight in the frames is annotation, not style

The menu item uses the app's existing selected/hover conventions; the sidebar row uses the
existing `attention` treatment from `issue_row_presentation()` when its count is non-zero.
*Why:* a permanently accent-tinted menu row would be the only one in the menu and would
read as "selected"; the sidebar already has a vocabulary for "there is something here".

### F-12. The per-row pencil opens the Tag Editor, and an `All N tracks` row opens all of them

Unchanged from DOC-3b: it opens the existing Tag Editor for that track and its Save marks
affected rows stale. For an `All N tracks` row it opens with all of that album's track ids
through the existing multi-select batch edit. *Why:* it is already the rule, it already
works, and an album-level row whose pencil edited one arbitrary track would be a trap.

### F-13. The review page scrolls; there is no "28 more albums" row

One `ScrolledWindow` over the whole grouped list, sticky header and sticky footer,
virtualized by `GtkListView` as today; no pagination and no collapsed remainder row.
*Deviation from the mockup:* Frame 5 truncates the list with "28 more albums · 79 changes";
that is how a static frame ends, not an affordance. *Why:*
`doc_3b_review_page_virtualizes_rows_without_horizontal_scroll` already proves
virtualization at 1,105 rows, and a "28 more albums" control would add a second navigation
model for no measured benefit.

### F-14. Apply is insensitive at zero selected changes

The post-apply page is never reached with an empty plan. *Why:* Frame 6 promises "Tags
updated"; reaching it after writing nothing would be a lie, and a third confirmation state
for "you selected nothing" is a page nobody should ever see.

### F-17. DOC-7b drops its "STATS-DEDUP hint" claim

No call site into the Doctor exists outside the primary menu, the (now deleted) preferences
row and the track context menu's `open_for_selection`. The claim is removed from the
rewritten rule rather than implemented. *Why:* an `[active]` rule that describes a
non-existent affordance is worse than no rule; if the hint is wanted later, it belongs in
its own `[planned]` rule with its own test.

### F-18. Schema migration v57 proceeds

AGENTS.md's "migrations are not a design criterion" section is scoped to *compatibility*,
not to the migration mechanism, which explicitly still exists. *Why:* the alternative —
reading a v56 database and reinterpreting it — is precisely the "leftover second source of
truth" that section forbids.

### F-19. Ten packages stay, and P5 stays separate

The core split (P1–P4) is kept because the tier predicate, the grouping projection, the
undo bracket and the migration each have their own failure mode and their own tests. P5 is
**not** merged into P10: with the three-stage cut it becomes the opening step of Stage 2,
and it is what lets P6, P7 and P8 run in parallel without three writers on one strings
file. *Why:* exclusive file ownership is what makes parallel execution safe.

### F-20. Unconsumed strings do break the build, so P5 adds an allow and P10 removes it

**New finding, verified while finalizing this plan; the draft's claim to the contrary was
wrong.** `reprise-gnome` is a binary-only crate (`[[bin]] name = "reprise"`, no `lib.rs`),
so unused `pub` items are `dead_code`, and the workspace gate runs
`clippy --all-targets -- -D warnings`. A symbol sweep over
`strings_library_doctor.rs` finds **zero** unused symbols today, which is why that file
carries no blanket allow — unlike `strings_news.rs`, `strings_releases.rs`,
`strings_concerts.rs`, `strings_radio.rs` and `strings_podcasts.rs`, which all begin with
`#![allow(dead_code)]`.

Decision: P5 adds `#![allow(dead_code)]` as the file's first line; P10 removes it again
after applying the removal list and proves the file is clean without it. *Why:* without the
attribute, P5's own acceptance fails on constants nothing consumes yet, and Stage 2 cannot
end green with constants whose last consumer disappeared but whose removal is P10's. F-1
already requires a re-grep before every deletion, so the lint was never the real guard here
— but the end state should still be the stricter one, which is why the attribute is
temporary.

---

### Where this plan deliberately differs from the mockup

The implementer does not see the frames. This table exists so that a later reader comparing
the running app to the mockup can tell intent from accident.

| Frame | What the mockup shows | What this plan builds | Decision |
| --- | --- | --- | --- |
| 4, 5, 6, 3b | "4 changes" per album, "88 changes", "85 tag changes", badge "88" | The same quantities counted as written tag changes, so they read visibly higher (that album reads "14 changes") | U-1 |
| 3 | "511 fixes applied so far" during the scan | "511 fixes to apply", switching to the applied wording only after the quiet write finishes | U-3 |
| 4 | Block 1 as a summary of what was applied | Same, and there is no way to expand it into a list | U-2 |
| 5 | Collapsed remainder row "28 more albums · 79 changes" | No remainder row; the list scrolls under a sticky header and footer | F-13 |
| 1b | Inline rename with no stated cancel behavior | Escape deletes the freshly created playlist; an empty commit keeps "Untitled playlist" | U-5 |
| 1, 3b | Accent-tinted menu item and sidebar row | The app's existing hover/selected conventions and the existing `attention` badge treatment | F-11 |
| 1–4 | Header icon set and sidebar contents vary per frame | The app's standard header and sidebar; only the progress card and the ISSUES row are real deltas | F-10 |
| 1 | `Sync Device…` present in an earlier menu | Removed, with no replacement entry point for a disconnected device | U-6 |
