# Library Doctor — redesign implementation brief

> Source of truth: Claude Design project `568f5576-9f1b-4214-bfa4-3e7c2e911712`,
> file `Library Doctor - Agent Prompt.md` (etag 1785922296435112, read 2026-08-05).
> Design reference: `Library Doctor.dc.html` — nine frames in flow order, plus two
> detail crops (1b, 3b). Read it before starting.

## Goal

Three problems: the feature is reachable from two places, the results screen mixes summary rows with category rows, and the review screen presents 1,105 flat rows with the column captions repeated in every single one. Shrink the feature to what a player can do that a dedicated tagger cannot — make the library internally consistent — and let it decide the unambiguous cases itself.

Do not change: the scan engine, the MusicBrainz/AcoustID clients, the remote cache, the tag-write job machinery. This is a surface and a policy change.

---

## 1. One entry point

- Delete `crates/reprise-gnome/src/ui/preferences/preference_library_doctor.rs` and its registration in the preferences page list. The whole block goes: Scope dropdown, remote toggle, the static `DOCTOR_LOCAL_ALWAYS_INCLUDED` row, `Run Scan Now`, `Revert Last Cleanup`.
- The global ⋮ menu keeps exactly one `Library Doctor` item. No count badge, no submenu.
- The menu holds the verb ("run a scan"). The sidebar holds the noun: when a completed scan has unreviewed findings, an entry appears under `ISSUES` next to `Missing files` — `Library Doctor` with the change count — and disappears once they are applied or dismissed. No permanent tab; an entry that is empty most of the time says nothing.
- Result reaches the user without interrupting: on the Doctor page it resolves in place, elsewhere it is that `ISSUES` entry. Never a modal, never a forced page switch. One `AdwToast` — `{n} tags fixed` with an Undo action — fires only for the quietly applied set, because that happened without asking.
- While you are in that menu: remove `Sync Device…` too. A phone connected over MTP already surfaces as its own entry in the left column, so the menu item is a second door to something that announces itself.
- Scope, the remote toggle, Run and Revert now live on the Doctor's own start page.
- The remote opt-in dialog stays where it is in the flow — first time the user enables the toggle on the start page, show the existing consent sheet (`LIBRARY_DOCTOR_REMOTE_HEADING` / `_BODY` / `_ENABLE`). `library_doctor.remote.enabled` and the consent version in `crates/reprise-core/src/library/library_doctor/preferences.rs` are unchanged.

## 2. Two tiers, not three

Today the scan produces Safe / Suggestions / Unresolved and shows all of them. New policy:

**Applied automatically, never shown as a decision**
- everything currently classified Safe (whitespace, unambiguous casing)
- every `DoctorField::RecordingMbid` proposal, regardless of tier

Enqueue these as a tag-write job the moment the scan completes, before the summary renders. Report them as done.

**Shown for review, preselected**
- all remaining proposals

**Unresolved spelling groups**
- unchanged in the model, but moved to the bottom of the review page and explicitly skippable.

Filter `DoctorField::RecordingMbid` out of `rows_for()` in `review_model.rs` so it can never reach the review list.

Undo has to cover both jobs as one unit. Check `crates/reprise-core/src/db_tag_write_jobs.rs` — the quiet job and the reviewed job from the same `scan_id` must revert together, and the summary/post-apply copy promises exactly that.

## 3. Start page

`Check your library` + one sentence of what happens. Then:

- Scope as a segmented control (`AdwToggleGroup` or `GtkStackSwitcher`-style), not a dropdown — three options, always visible.
- Remote toggle with the existing `LIBRARY_DOCTOR_REMOTE_DESCRIPTION` verbatim.
- `Run Scan Now` as the single primary action, with a track count and rough duration beside it.
- Below a separator: last scan timestamp, how many fixes it applied, and `Revert Last Cleanup`. This is the only remaining home for revert.

## 4. Progress lives in the sidebar, not on a page

The scan does not own a screen. `DoctorProgressCard` already renders the shared `.scan-card` and `mod.rs` already mounts it via `sidebar.append_doctor_card()` — that is the right home, the same bottom-left slot the library scan and the missing-files relink use (`missing_progress.rs`, FB-8). Keep the card exactly as it is: spinner, `DOCTOR_SCANNING`, percent, Cancel, progress bar, `doctor_track_progress()` detail line, and the click/Enter/Space activation that jumps to the Doctor page.

Add only this: while a scan runs, the Doctor page itself shows `DOCTOR_RESULTS_SO_FAR` with the same two summary blocks as the finished state, counting up live, actions disabled, and `DOCTOR_CONTROLS_LOCKED` as the reason. Delete `job_page.rs` if it exists only to host a full-page progress view — `DOCTOR_JOB_PAGE_DESCRIPTION` becomes redundant once the card is the single progress surface.

## 5. Summary page — `summary_page.rs`

Replace the nine-row list with three blocks. Never render a row whose count is zero.

1. **`{n} fixes already applied`** — one line per kind underneath (spacing/casing, MusicBrainz IDs with "no visible change to your tags"), `Undo` on the right.
2. **`{n} changes need your eye`** — one line per remaining category, plus the album count; `Review {n} changes` primary on the right.
3. **`{n} spelling conflicts, no clear winner`** — only if any; states that they sit at the end of the review list and are skippable.

Under it: the scan facts as one muted line (`{checked} checked · {skipped} skipped`, scope, remote on/off) and a `Scan again` ghost button.

Drop `DOCTOR_TRACKS_CHECKED` as a list row — it is a statistic, not a finding.

## 6. Review page — `review_page.rs`, `review_row.rs`

This is the main work.

**Group by album.** Use `GtkListView` with a section model: sort the rows by album, call `gtk_sort_list_model_set_section_sorter()` with an album sorter, and set a `header_factory` on the list view. The header is the album row: group checkbox, cover, album title, `{artist} · {n} tracks`, change count. Album-level proposals (album artist, year, genre applied to every track) render their track cell as `All {n} tracks`.

**One column header for the whole page.** Build a single header `GtkBox` pinned above the scrolled window, and bind each of its cells into a horizontal `GtkSizeGroup` shared with the matching cell in every row. Then delete `value_widgets()`'s per-row caption label entirely — `DOCTOR_TRACK_AND_FIELD`, `DOCTOR_CURRENT`, `DOCTOR_PROPOSED`, `DOCTOR_SOURCE` are used once, at the top.

**Row shape:** `[check] [track] [field] [current, strikethrough] [→] [proposed] [source] [edit]`. Keep the strikethrough on current and the tone/warning logic from `confidence_presentation()` untouched.

**Move the breakpoint up.** Remove the per-row `adw::BreakpointBin` — that is what forced every row to carry its own captions. Put one `AdwBreakpoint` on the page at the existing 640px threshold; under it, rows stack and the shared header hides. `layout_for_width()` and the DOC-3b test stay, they just drive the page instead of the row.

**Preselect everything.** `DoctorReviewRow.selected` starts `true` for every reviewable row. `All` / `None` stay in the header — rename `DOCTOR_ALL_SAFE` ("All Safe") to plain "All", since safe fixes are no longer in this list.

**Filter bar** above the list: All / Casing / Year / Genre, built from the categories actually present in the scan.

**Unresolved groups move to the bottom**, inside a dashed container titled `Spelling conflicts`, marked optional, with a `Skip all` action. Each conflict is one row: what the conflict is about and how many tracks, then the candidate spellings as radio pills with their occurrence counts. Keep `DOCTOR_PICK_ONE` as the explanatory line.

**Footer** keeps `doctor_apply_summary()` verbatim (`{changes} tag changes · {files} files · undo available after`) and the primary button becomes change-based, not track-based — `Apply {n} changes`. `doctor_apply_tracks()` can go.

## 7. Post-apply

Own page: `doctor_tags_updated()` headline, then the counts, then `Undo everything from this scan` and `Done`. State that the undo includes the quiet fixes and lasts until the next scan. Unresolved conflicts left open are named here and come back with the next scan.

## 8. Empty state

Post-scan-with-no-findings is not the same as pre-scan. Add a state: `Nothing to fix`, `{checked} tracks checked, {skipped} skipped. Your tags are already consistent with each other.`, plus `Scan again`. Keep `DOCTOR_NO_RESULTS` for the pre-scan case.

---

## 9. Expose it on the MCP too

The same feature should be operable from `crates/reprise-mcp` so an assistant can do this for the user instead of them clicking through it. Follow the existing pattern exactly: a new `doctor_tools.rs` with `#[tool_router(router = doctor_tool_router, vis = "pub(crate)")] impl RepriseServer`, `music_*` names, params structs in their own module, `spawn_blocking` around the core call, `crate::error::structured_ok` / `into_tool_outcome`.

- `music_scan_tags` — scope + remote flag, returns the same summary the GUI shows: applied, needs-review, conflicts, checked/skipped.
- `music_review_tags` — read the pending findings, grouped by album, with a filter argument. Or expose it as a `reprise://tag-issues` resource alongside the other read surfaces; pick whichever matches how the rest of the read side is built.
- `music_apply_tags` — actions `apply` (by row id or by album), `resolve` (pick a spelling for a conflict group), `revert`.

Two things that are easy to get wrong:

**A scan is no longer read-only.** Under section 2 the GUI applies the safe set automatically. Do not let the MCP inherit that silently — `music_scan_tags` takes `apply_safe`, default **false**, so an agent-initiated scan finds and reports without touching files unless it was asked to. Mutations (`apply_safe: true`, and all of `music_apply_tags`) sit behind a new capability, `'tags:write'`, off by default, checked the same way `sources:manage` is.

**One job queue, two front doors.** Both surfaces write through `db_tag_write_jobs` against the same `scan_id`, so a scan an agent ran produces the same `ISSUES` entry in the sidebar, and a GUI `Undo` reverts an MCP apply. Respect the existing lock — `TAG_WRITE_BUSY` and `DOCTOR_CONTROLS_LOCKED` mean the MCP has to refuse while a GUI job runs, and the reverse.

---

## Strings — `strings_library_doctor.rs`

Remove: `DOCTOR_LOCAL_ALWAYS_INCLUDED`, `DOCTOR_SAFE_FIXES`, `DOCTOR_SUGGESTIONS`, `DOCTOR_TRACKS_CHECKED`, `DOCTOR_REVIEW_SAFE`, `doctor_review_safe_fixes()`, `doctor_apply_tracks()`, `DOCTOR_ALL_SAFE`, `DOCTOR_JOB_PAGE_DESCRIPTION`, `DOCTOR_TRACK_AND_FIELD` (as a per-row caption — becomes two separate column headers, Track and Field).

Keep verbatim: `LIBRARY_DOCTOR_REMOTE_DESCRIPTION`, `DOCTOR_PICK_ONE`, `doctor_apply_summary()`, `doctor_tags_updated()`, `doctor_checked_counts()`, `doctor_unresolved_spellings()`, `doctor_candidate()`, `doctor_track_progress()`, `DOCTOR_SCANNING`, `DOCTOR_RESULTS_SO_FAR`, `DOCTOR_CONTROLS_LOCKED`, the confidence helpers, all status labels.

Add: the auto-applied headline and its per-kind lines, the "needs your eye" headline, the spelling-conflicts section title and `Skip all`, the nothing-to-fix pair, `Apply {n} changes`, and the last-scan line on the start page. Wrap all of them in `N_!` and keep the placeholder style consistent with what is there.

---

## Before you finish

- Update `docs/ux-rules.md` § Y. Library Doctor / Tag Cleanup to match. Keep the DOC-* traceability ids alive; retarget DOC-3b (breakpoint) and DOC-4b (confidence presentation) at their new homes rather than deleting them.
- `scripts/check-ux-traceability.sh`, `scripts/check-frontend-thinness.sh`, `scripts/check-accessibility-semantics.sh`, `scripts/ci-quality.sh`.
- Accessibility: `doctor_review_row_description()` still has to produce a full spoken description per row now that the visible captions are gone — this matters more, not less.
- Migration: existing scans in `library_doctor_proposals` were stored without the auto-apply split. Decide whether to reclassify on read or to invalidate the last scan; do not silently apply anything from a scan the user ran under the old rules.

---

## Unrelated small change, same sidebar

`New playlist` and `Import playlist…` each occupy a full sidebar row for an action, above a list of actual playlists. Replace both with a single `+` button on the `PLAYLISTS` section header that creates a playlist immediately — new row in place, name preselected for inline rename, no dialog. Move `Import playlist…` into the global ⋮ menu with the other library-wide verbs. Two rows back for content, and the common action drops from two clicks to one.
