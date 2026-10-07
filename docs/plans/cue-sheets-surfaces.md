---
slug: cue-sheets-surfaces
worktree:
branch:
phase: planned
codex_session:
created: 2026-10-06
strands: a,b,c
merge_order: a,b,c
---
# CUE sheets — surfaces (wave 3)

Mother plan. Frozen once the plan phase ends; each strand writes only its own file
(`cue-sheets-surfaces-{a,b,c}.md`).

**Precondition:** `docs/plans/cue-sheets-schema.md` (migration v91) has landed on
`dev`. All three strands branch from a `dev` that contains it. No strand registers a migration.

Spec: `docs/superpowers/specs/2026-10-04-cue-sheets-design.md`; decisions 4–7 of
`docs/plans/loudness-and-cue-sheets.md` bind this plan, except where a decision below replaces
them; `docs/plans/cue-sheets-core.md` ("As built", "Review fixes", "Left for wave 3") is the
code this wave builds on. The spec's sync rule becomes "Reprise never writes a `.cue` into the
music collection" (it does write a derived `.cue` on the device, decision 4).

**Why now:** the dev→main promotion is held until this wave is on dev (user decision
2026-10-06, `HANDOFF-2026-10-06-promotion-held-for-cue-wave3.md`). Since #1146 a CUE track
plays its whole file from 0:00, and trashing one CUE track trashes the album file.

## Working rules for every strand

- Test-first per task: failing test → run, see it fail → minimal code → green → gates →
  focused commit. AGENTS.md gate battery before every commit;
  `cargo +1.99.0 clippy --all-targets --workspace -- -D warnings` is CI's toolchain.
- Code files stay < 800 lines. Files near the cap are flagged in each strand file; extract a
  cohesive sibling instead of trimming docs.
- New user-visible behaviour gets a `[planned]` rule (`<!-- REVIEW: rule proposal -->`) in
  the strand's own `docs/ux-rules.md` section, flipped to `[active]` in the commit whose
  rule-named test proves it. Rule ids are pre-assigned below; never take another id.
- **No strand edits a crate root** (`reprise-core/src/lib.rs`, `reprise-platform-linux/src/lib.rs`,
  `reprise-gnome/src/main.rs`/`ui/mod.rs`). A new module hangs off a parent file the strand
  owns (`#[path]` or a `mod` line in that parent). Sole exception: `reprise-android-ffi/src/lib.rs`
  belongs to strand c (the analysis-failure set lives there); strand b does not touch it.
- **Names:** tasks are `a1…`, `b1…`, `c1…`. The wave-2 review findings keep their own ids
  and are always written "finding A10", "finding C5", … — never bare.
- **Implementers:** Opus `worker` agents, one per strand worktree, briefed with that strand
  file (user decision 2026-10-06, overrides the 2026-10-05 Sonnet-worker memory for this
  wave). `worker` is pinned to Sonnet in its frontmatter, so every spawn carries
  `model: "opus"` explicitly. Codex returns 2026-10-09 23:21; the refactor phase may use it.
- Headless runs follow the AGENTS.md isolation recipe. Device runs hold `device-lock` for the
  whole measurement.

## Decisions (grilled 2026-10-06)

1. **Gaps.** Gapless only between contiguous segments of one file (the next item is the
   segment that starts where this one ends) and at file ends as today. Every other transition
   into or out of a CUE track may have a short gap (~0.1–0.3 s). Desktop mechanism: a
   buffer-PTS probe on the gain element's sink pad.
2. **Crossfade.** Never at a transition involving a CUE track. Contiguous segments play
   through; any other transition with a CUE track on either side is a hard change.
3. **Open end of the last track.** "The last track of a file plays and is analysed to EOF",
   implemented locally, data model unchanged: desktop player ignores an `end_ms` within 1 s
   of the pipeline's file duration (a); analysis lets the file's last segment absorb
   everything to the decoded EOF and counts it complete (c, finding C5); the FFI gives the
   last segment of a file `segment_end_ms = None` and Kotlin clips it with
   `C.TIME_END_OF_SOURCE` (c).
4. **Device sync per file.** A CUE file is transcoded per the profile like any file, once per
   file. Reprise writes a derived `.cue` on the device whose `FILE` line names the device
   file — also for files whose sheet was embedded. The phone lists all tracks of a synced file.
5. **Tag editor.** Segments are left out; a segment-only selection shows a notice instead of
   the editor; a mixed selection edits the whole-file rows and says how many CUE tracks were
   left out.
6. **Broken sheets** stay under Issues → Import errors with their own copy; Retry rescans the
   sheet's directory; an issue for a deleted sheet clears on the next scan of its directory.
7. **Exclusion identity (finding B9)** uses v91: an exclusion stores the segment's start and
   title and matches in the `scanner_segment_match` order.
8. **Phone attribution in scope.** Phone listens and ratings of CUE tracks are attributed per
   segment (findings B4/B8): the phone reports device path + segment start, the desktop
   resolves it. The analysis findings C5, C6, C8 are in this wave.
9. **Trash.** File plus its `.cue` go to the trash only when every present segment of that
   file is selected (a multi-FILE sheet only with the last of its files). A partial selection
   hides the selected segments (exclusions) and the dialog says so.

## The cut

| Strand | Plan file | Branch | Purpose |
|---|---|---|---|
| a — desktop playback | `cue-sheets-surfaces-a.md` | `feature/cue-sheets-surfaces-a` | a CUE track plays its own stretch on the desktop |
| b — desktop sites, scanner, sync | `cue-sheets-surfaces-b.md` | `feature/cue-sheets-surfaces-b` | every path-equals-track site acts per file or per segment; sheets on the phone; the return channel |
| c — Android + analysis | `cue-sheets-surfaces-c.md` | `feature/cue-sheets-surfaces-c` | the phone plays a CUE track as its own stretch; analysis cuts by time on both platforms |

Ownership (globs; full lists with near-cap flags live in each strand file):

- **a:** `platform-linux/src/{player.rs,player/**,gapless.rs,crossfade.rs,player_effects.rs,
  player_pipeline.rs,cava_stage.rs,signals.rs}` + new siblings beside them;
  `reprise-gnome/src/ui/playback/**`, `ui/window/player_backends.rs`, `ui/mpris_mirror.rs`;
  `reprise-core/src/playback.rs` (doc and defaulted additions only); ux-rules **section C**.
- **b:** GNOME `ui/delete_tracks*.rs`, `ui/track_list/{track_menu.rs,
  track_list_context_menu.rs,track_list_context_action_states.rs}`, `ui/tag_edit/**`,
  `ui/import_errors_view.rs`, `ui/device_sync/**`, their `ui/strings_*.rs` modules; core
  `cue/**`, `library/{scanner_cue*.rs,scanner_segments.rs,scanner_entry.rs,
  scanner_segment_match.rs,scanner_mobile_sync.rs,trash_tracks.rs,exclusions.rs,
  library_doctor/scope.rs}`, `db_library_exclusions.rs`, `queries/{maintenance.rs,
  maintenance_delete.rs,import_errors.rs}`, `device_sync.rs`, `device_sync/**`,
  `db_device_sync.rs`, `db_mobile_sync.rs`; FFI `src/{listen_export_journal.rs,
  listen_export_recorder.rs,library_listen_report.rs,source.rs,source_tests.rs}`;
  platform-linux `trash.rs`, `device_sync*.rs`, `device_transfer.rs`; matching `po/` entries;
  ux-rules **section AK**.
- **c:** `reprise-android-ffi/**` except b's five files (including FFI `lib.rs`); `android/**`; core
  `render_data_segments.rs` (+ `_tests.rs`), `waveform_cache.rs`, `db_spectrogram.rs`,
  `db_render_data_failures.rs`, `spectrogram_backfill.rs`; platform-linux `waveform.rs`;
  ux-rules **section E**.

Disjointness: no path is in two lists. `docs/ux-rules.md` is shared by section only.
`po/` is b's alone — a and c add no translatable strings unless their strand file says so
(c's Kotlin strings live in Android resources). Read-only use of another strand's files is
allowed (calling `cue::parse` in a test, reading `track_source_fingerprint`).

Seams, fixed here so no strand needs another's code before merge:
- `playback_session.rs` (c) calls `listen_export_journal::prepare_report` (b): b keeps that
  signature unchanged and resolves segment identity inside its own files from the track id.
- `TrackRow` (c, `browse.rs`) and the RPT formats (b) do not touch each other.
- The derived `.cue` (b writes) and its cut on the phone over SAF (b's C1 fix) are both b's;
  only the live device proof is post-merge.
- Decision 3 has one consumer per strand; their agreement is a post-merge check.

Rule ids:
- a → section C: **PLAY-22** (own start, end, seek bar), **PLAY-23** (contiguous tracks of one
  file play through, each with its own gain), **PLAY-24** (no crossfade at a CUE transition).
- b → section AK: **CUE-11** trash per file, **CUE-12** tag editor leaves CUE tracks out,
  **CUE-13** Doctor skips CUE tracks, **CUE-14** sheet import errors, **CUE-15** sync copies a
  CUE album once with a derived sheet, **CUE-16** the phone lists every track of a synced CUE
  album, **CUE-17** phone listens and ratings count for the right track, **CUE-18** a hidden
  CUE track stays hidden across a sheet edit. CUE-19/20 reserved for b.
- c → section E: **MTP-66** a CUE track plays its own stretch on the phone with its own
  metadata, **MTP-67** the phone analyses CUE tracks, **MTP-68** the last track of a file plays
  to its end on the phone. MTP-69 reserved for c.

## Parallelität

Three strands, each in its own worktree from the post-schema `dev`; cap respected.

**Merge order:** `cue-sheets-schema` first (separate plan, already landed when `/code` runs).
Then a, b, c (`merge_order: a,b,c`) — there is no code dependency between them, so if one
finishes far ahead it may land first; the key states the default. Each rebases onto `dev`
before landing.

**Post-merge cross-checks** (none may run inside a strand):
1. Desktop, headless + one human listen: a CUE album queued plays gapless across its tracks
   with per-track gain; a CUE track followed by a whole-file track changes hard, no crossfade.
2. Sync that album to the phone (device-lock): one device audio file + one derived `.cue`;
   the phone lists all tracks and plays each from its own start to its own end (b × c).
   Measure the gap between two contiguous clips and whether each clip gets its own gain;
   record the numbers in strand c's file. No threshold blocks; a gap > 50 ms opens an issue.
3. Play a phone listen and a phone rating of track 3 of that album, sync back: the desktop
   counts them on track 3 (b's format v2 × c's playback session on the device).
4. Remove one CUE track from the library on the desktop, edit the sheet to insert a track,
   rescan: only that song stays hidden.
5. Decision 3 end to end: the last track of a CUE file whose metadata duration is short
   plays to EOF on desktop (a) and phone (c), and its analysis completes (c).
6. Release notes: renumber the 0.1.265 notes and rewrite the CUE paragraph in `CHANGELOG.md`
   and the metainfo (EN+DE) — a separate PR after all three strands, then lift the promotion
   hold (`HANDOFF-2026-10-06-promotion-held-for-cue-wave3.md`).
7. Before each `/code` (schema and surfaces): re-run the sibling-plan scan (`git worktree
   list` + their `docs/plans`), in particular any fix-forward from the #1149 post-merge
   emulator run touching `track_analysis/**`, and `feature/the-backfill-keeps-its-decode`
   (Android backfill files strand c reworks — land it first or have c rebase onto it).
