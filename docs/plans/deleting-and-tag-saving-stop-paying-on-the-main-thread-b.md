---
slug: deleting-and-tag-saving-stop-paying-on-the-main-thread-b
worktree: /home/marvin/Projects/reprise-deleting-and-tag-saving-stop-paying-on-the-main-thread-b
branch: feature/deleting-and-tag-saving-stop-paying-on-the-main-thread-b
phase: planned
codex_session:
created: 2026-09-06
---
# Strand B — tag save: the delta path is taken, and a resort keeps the edited row in view

Strand B of `docs/plans/deleting-and-tag-saving-stop-paying-on-the-main-thread.md`.
Read the mother plan's §0 (the delta refusal as the code shows it), §1 (G4, G5,
R2) and §2 (rules) first.

Runs in two Codex passes (mother §2 R-two-pass): pass 1 = B0 only, then the
session measures and writes §M; pass 2 = B1–B2 against the numbers.

## File ownership

- Owns: `crates/reprise-gnome/src/ui/tag_edit/**`,
  `crates/reprise-gnome/src/ui/track_list/tag_mutation_refresh*.rs`,
  `crates/reprise-gnome/src/ui/track_list/track_list_model_change.rs`.
- Reads but never edits: `ui/track_list/track_list_reload.rs`,
  `ui/track_list/track_list_geometry.rs`, `ui/scroll_glide.rs`.
- Does not touch `ui/playback/**` or `ui/delete_tracks*.rs` (strand A).

## Pass 1 — diagnosis

### B0 — the refusal says why

On the `tag-edit batch completed` line in `tag_edit_flow.rs` add
`has_pre_save_view` (bool), `before_len`, `after_len`, and `first_mismatch`
(the index of the first position where `before` and `after` differ, or `-1`;
computed only when `has_pre_save_view`). Also log, at editor open, one `info`
line `tag editor view snapshot` with `view_len` (`current_view_ids().len()`)
and `snapshot_len` (`BrowseSnapshot::ids().len()`, 0 when the snapshot is
`None`) so the two sizes can be compared without a debugger. No behaviour
change. Acceptance: fields present; `tag_edit_flow_tests.rs` green.

**Pass 1 ends here.** The session runs the harness (8 rows, Genre, artist
sort; then one save that edits the Artist field under artist sort for G5/R2)
and writes §M.

## Pass 2 — fix by what §M shows

### B1 — the delta path is taken

Named candidate (mother §0): `OpenedReloadState.view_ids` is
`BrowseSnapshot::ids()` — the browsed query's rows, not the view's — while the
save side compares against `shared.current_view_ids()`. If §M shows
`snapshot_len != view_len` or `first_mismatch >= 0` with equal lengths, take
`view_ids` from `shared.current_view_ids()` at open, independent of whether
the browse snapshot succeeded; the snapshot keeps its one job (prev/next
inside the editor). If §M shows `has_pre_save_view=false`, fix the reason the
snapshot is `None` instead. If §M shows a view reload between open and save,
find the caller and make the pre-save ids survive it (or re-capture them
before the write). Whatever the cause: write the test first in
`tag_edit_flow_tests.rs` (or `tag_mutation_refresh_display_tests.rs` if it
needs a model) so it is red on the current code and green after.
Acceptance: `delta=true` on the 8-row Genre save in the harness and no
adjustment write > 45 px after the save (G4).

### B2 — a sort-field save keeps the first edited row in the viewport

For a save that changes the sort field the full reload stays and the
existing rule applies (`tag_1_year_save_keeps_the_edited_album_inside_the_viewport_after_resort`).
`post_save_reload_anchor` (`tag_reload_anchor.rs`) re-anchors on the first
edited track whose patch touches the sort field; when `layout` is `None`
(`tag_edit_flow.rs` around line 558) the anchor keeps its position and only
`selected_ids` changes. §M's second measurement (Artist edit under artist
sort) says whether the first edited row is inside the viewport after the
reload. If it is not: find where the anchor is lost (the `None` layout
branch, or `reanchor_on_track` with the pre-save `old_view_ids`) and keep the
edited row visible. If it is: no code change; the §M numbers become R2.
Acceptance: G5 in the harness; the resort test green; if code changed, a
display test for the case that was wrong.

## §M — measurements (written by the session between the passes)

_pass 1: `has_pre_save_view`, `before_len`, `after_len`, `first_mismatch`,
`view_len`, `snapshot_len` for the Genre save; adjustment writes and their
sizes after the Genre save and after the Artist save._

_pass 2 / acceptance: `delta`, `reload_ms`, adjustment writes for both saves._

## Report

State the diagnosed cause of the refusal, what changed, the §M tables, and
R2 for the mother plan's §5.
