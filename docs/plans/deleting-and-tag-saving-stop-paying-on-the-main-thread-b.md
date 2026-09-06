---
slug: deleting-and-tag-saving-stop-paying-on-the-main-thread-b
worktree: /home/marvin/Projects/reprise-deleting-and-tag-saving-stop-paying-on-the-main-thread-b
branch: feature/deleting-and-tag-saving-stop-paying-on-the-main-thread-b
phase: shipped
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
- Since pass 3 also owns `ui/track_list/track_list_reload.rs`,
  `ui/track_list/reload_anchor_scroll.rs`, `ui/track_list/track_list_model.rs`,
  `ui/track_list/track_list_columns.rs`, `ui/track_list/restore_intent.rs` and
  `ui/track_list/adjustment_hold.rs` (see §M).
- Reads but never edits: `ui/track_list/track_list_geometry.rs`, `ui/scroll_glide.rs`.
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

### Pass 1 (2026-09-06, worktree binary at `ea54973291`, runs B1–B3, harness G4 + new G5)

Machine under load during the runs (load avg ≈ 7: strand A's Codex and a
release build in parallel), so the `*_ms` columns are inflated against the
mother §0 fix arm (`write_ms` 119 → 308, `reload_ms` 266 → 443). The
diagnosis fields are load-independent; medians over three runs.

| Field | G4 Genre, 8 rows | G5 Artist, 8 rows |
|---|---|---|
| `view_len` / `snapshot_len` (at editor open) | 1929 / **500** | 1929 / **500** |
| `has_pre_save_view` | true | true |
| `before_len` / `after_len` | **500** / 1929 | **500** / 1929 |
| `first_mismatch` | **500** | **500** |
| `delta` | false (3/3) | false (3/3) |
| scroll writes ≤ 500 ms after save | 1 | 1 |
| largest write | 46 260 px (`SCROLL JUMP-TO-TOP`, then 0 → 46 260) | 46 260 px (same pattern) |
| `build_ms` | 38 (37–48) | 13 (11–22) |

**Diagnosis (B1).** The named candidate is confirmed exactly: `BrowseSnapshot::ids()`
is capped at 500 rows, so `OpenedReloadState.view_ids` holds the first 500 of
the 1929 view ids. `first_mismatch = 500 = before_len` means the prefix matches
and only the length differs — no reorder, no reload between open and save. The
delta is refused on `before != after` alone. Fix per B1: `view_ids` from
`shared.current_view_ids()` at open.

**Viewport after the full reload.** Both saves log `SCROLL JUMP-TO-TOP`
followed by one write 0 → 46 260 px: the model is replaced, the adjustment goes
to 0 and the anchor restores the previous position. For G4 the screenshot
after the save shows the same viewport as before (loaded track at y≈542) — the
46 260 px write is the restore, not a drift; with `delta=true` neither write
happens.

**G5 (B2).** After the Artist save ("Zz Measured Artist" on the 8 rows directly
above the loaded track) the viewport shows the loaded track at the top and the
rows below it; the 8 edited rows moved to the end of the artist order and are
**not** inside the viewport (screenshot `runs/B1/9-after-G5.png`). The reload
anchored on the previous scroll position (46 260 px, the loaded track), not on
the first edited row. G5 is therefore NOT met on the current code; B2 needs
the code change and a display test. Note that the first edited row was the
*top* row of the viewport before the save.

### Pass 2 acceptance (2026-09-06, worktree binary at `30aed5fe52`, runs B4–B6; control = B1–B3)

| Field | G4 Genre, 8 rows | G5 Artist, 8 rows |
|---|---|---|
| `delta` | **true (3/3)** — was false | false (3/3), expected: sort field |
| `first_mismatch` | -1 | 1028 (the edited rows moved) |
| `write_ms` | 121 (105–129) — was 308 under load | 98 (85–132) |
| `reload_ms` | **206 (205–209)** — was 443 (full reload under load; mother §0 full reload idle: 266) | 88 (86–266) |
| scroll writes ≤ 500 ms | 1 | 1 |
| largest write | **46 260 px** (`SCROLL JUMP-TO-TOP`, then 0 → 46 260) — unchanged | 46 260 px — unchanged |
| first edited row in viewport after save | n/a | **no** — viewport identical to B1 (loaded track at top, edited rows at the end of the list, `runs/B4/9-after-G5.png`) |

**G4 half met, G5 not met.** The delta path is taken, but the view still
runs the full `query matched 1929 tracks` query before the completion line, the
adjustment still drops to 0 during allocation (`SCROLL JUMP-TO-TOP` is logged
when the value falls by > 80 px, `track_list_builder.rs:171–184`) and is then
restored to exactly the pre-save value. For G5 the restored value is the
*same* 46 260 px although 8 rows above the anchor left that region — a raw
value restore, not a track anchor, so the display test that passes in pass 2
models something the app does not do. Code pointers from the trace (not yet
verified as the cause): on the delta path `tag_mutation_refresh.rs:110–116`
requests `PreserveAnchor`; `track_list_reload.rs:530–549` creates the
`AdjustmentHold` only when `viewport != Top` and `captured.anchor.is_some()`,
then `run_query` (`:545`, logs `query matched`) and `items_changed(position,
removed, added)` on `track_list_model.rs:566`; the restore is scheduled from
`reload_anchor_scroll.rs:139–154`. Also odd: `reload_ms` on the delta path
(206) is above the full reload of G5 (88).

**Pass 3 (session decision):** a third Codex run against these numbers. The
ownership for this pass is extended by `ui/track_list/track_list_reload.rs`
and `ui/track_list/reload_anchor_scroll.rs` (strand A never edits
`track_list/**`, so the intersection stays empty).

**Pass 3, first run (no commit):** Codex reproduced B3 red in a display test
(1929 rows, row 1028: adjustment 34 808 → 0) and stopped at the ownership
boundary: skipping the anchor `scroll_to` does not help, extending the
`AdjustmentHold` only restores after GTK has already written 0, and a
synchronous correction re-enters allocation. The cause is in
`track_list_model.rs`: its delta clears the cached windows and represents a
metadata refresh as remove+add in `items_changed`, so GTK reallocates the
realized rows and resets the adjustment. The fix is a non-structural
metadata-refresh API on the model (like `set_cached_rating`) plus rebinding
the realized edited cells without `items_changed` (`track_list_columns.rs`).
B4's cause is confirmed: `track_list_geometry::layout(...)` returns `None`
during real dialog completion and `tag_edit_flow.rs` then bypasses
`post_save_reload_anchor`, so the old anchor is restored. The 206 ms "delta"
reload still runs the complete sorted query and cache swap, then pays GTK
rebinding synchronously for the realized rows. Ownership extended once more
(session decision) by `ui/track_list/track_list_model.rs` and
`ui/track_list/track_list_columns.rs` — strand A never edits `track_list/**`.

### Pass 3 acceptance (2026-09-06, worktree binary at `436e6e1bee`, runs B7–B9; control = B4–B6)

| Field | G4 Genre, 8 rows | G5 Artist, 8 rows |
|---|---|---|
| `delta` | true (3/3) | false (3/3), expected |
| `write_ms` | 96 (83–107) | 105 (98–112) |
| `reload_ms` | 201 (11–202) — bimodal, one run 11 ms | 270 (268–287) |
| scroll writes ≤ 500 ms | **0** — was 1 | 1 |
| largest write | **0 px** — was 46 260; no `SCROLL JUMP-TO-TOP` for this save any more | 46 305 px (0 → 46 305 after `JUMP-TO-TOP`) |
| first edited row in viewport after save | n/a | **no** — viewport identical to B1/B4 (`runs/B7/9-after-G5.png`), the value moved by exactly one row (46 260 → 46 305) |

**G4 met** (delta path, no adjustment write). **G5 still not met in the
app:** the restored value is the old region plus one row height, not the
first edited track (which sits at the end of the artist order, ≈ 86 000 px).
Codex's `layout = None` display test is green, so the app takes yet another
path. Diagnosed next by the session (see below).

**G5 diagnosis (session trace, 2026-09-06).** `post_save_reload_anchor`
does return `(first_edited_id, 0.0)` for `layout = None`
(`tag_reload_anchor.rs:66–73`) and it does reach the reload
(`tag_mutation_refresh.rs:55–62, :127` → `track_list_reload.rs:514–546` →
`reload_anchor_scroll::schedule`, `:139–154`, `apply()` at `:197`). What the
app then does and the test does not: playback is active, so
`scroll_glide.deliberate_destination()` is `Some` (the centred current
track), and `restore_intent::deliberate_destination_outranks`
(`restore_intent.rs:10–34`) makes `apply()` return `ApplyResult::StoodDown`
(`reload_anchor_scroll.rs:601–609`) — the anchor is never applied. The
`AdjustmentHold` built unconditionally at `track_list_reload.rs:544` was
seeded with the pre-save value (`adjustment_hold.rs:100–104`) and protects it
for 250 ms (`SCROLL_ADJUSTMENT_HOLD`), which is the 46 260 → 46 305 px restore
seen in the log. Codex's display test never sets `playing_track_id`
(`tag_mutation_refresh_display_tests.rs:451–473`), so the glide never
outranks anything there. Fix scope: a post-save sort-field anchor must
outrank the glide's destination (the user chose the edited row over the
current track in the grill, item 7), and the hold must not pin a value that
`apply()` stood down from. Ownership extended by
`ui/track_list/restore_intent.rs` and `ui/track_list/adjustment_hold.rs`.

### Pass 4 acceptance (2026-09-06, worktree binary at `7c2dd4c485`, runs B10–B12; control = B7–B9)

| Field | G4 Genre, 8 rows | G5 Artist, 8 rows |
|---|---|---|
| `delta` | true (3/3) | false (3/3), expected |
| `write_ms` | 118 (97–119) | 100 (80–103) |
| `reload_ms` | 203 (197–219) | 344 (328–344) — was 270 |
| scroll writes ≤ 500 ms after completion | 0 | 0 (the anchor write lands after the window) |
| adjustment after the save | unchanged (46 260 px) | **46 260 → 86 058 px**, one write, no `JUMP-TO-TOP` |
| first edited row in viewport | n/a | **yes** — all 8 edited rows visible, first one at row 10 of the viewport (`runs/B10/9-after-G5.png`) |

**G4 met, G5 met.** R2 (for the mother plan's §5): after a sort-field save
there is exactly one adjustment write, from the pre-save position to the
first edited row's new position (46 260 → 86 058 px in this gesture); the
Genre save writes nothing. The G5 reload is 344 ms against 270 before the fix
(reported, not gated; the mother plan keeps the loaded-track reload out of
scope).





## Report

State the diagnosed cause of the refusal, what changed, the §M tables, and
R2 for the mother plan's §5.

## Refactor

- B1 resolves each registered generic text cell through its live weak `ListItem` before re-rendering, with a narrowed-removal display regression.
- B2 gates full text rendering on a metadata generation while ordinary playback changes only toggle the marker class, preserving the metadata-only delta refresh.
- B3 routes `open_editor` through the tested full-view reload-ID selection instead of testing `OpenedReloadState::at_open` with a hand-picked vector.

### Acceptance after the refactor pass (runs B13–B15, 2026-09-07, release binary at `1c8bb07701`)

Same harness, same library (1929 rows). Left = coded-phase acceptance B10–B12, right = B13–B15.

| Field | G4 Genre | G5 Artist (sort field) |
|---|---|---|
| `delta` | true 3/3 → true 3/3 | false 3/3 → false 3/3 (expected) |
| `first_mismatch` | −1 → −1 | 1028/1029/1028 → same |
| scroll writes after save | 0 → 0 | 0 → 0 |
| largest write | 0 px → 0 px | 0 px → 0 px |
| `reload_ms` | 203 (197–219) → 178 (11–202) | 344 (328–344) → 345 (333–368) |
| first edited row in viewport | n/a | yes, all 8 edited rows visible (`9-after-G5.png`, B15) |

G4 ✓ (delta path, no viewport jump), G5 ✓ (first edited row in viewport). The G5 reload stays at ~345 ms — reported (R2), not gated. The metadata-generation gate (B2 fix) keeps the G4 cell refresh intact: the edited cells show the new value without `items_changed`.
