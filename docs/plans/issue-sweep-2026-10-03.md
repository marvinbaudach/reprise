---
slug: issue-sweep-2026-10-03
worktree:
branch:
phase: planned
codex_session:
created: 2026-10-03
strands: a,b,c
merge_order: a,b,c
---
# Five open issues, fixed in three disjoint strands

## Why

A triage of the open issues on 2026-10-03 left five that can be fixed now: #1041, #1000,
#1055, #1018 and #1052. The user decided the open design questions:

- **#1055:** the track row grows with the font scale, so nothing is clipped. Rows do not drop
  information.
- **#1052:** the desktop gets the network-return retry, as its own rule (NET-7c), because the
  GTK mechanism differs from Android's NET-7b.
- **#1018:** Radio gets the module-off state that SRC-10a already describes. The code follows
  the active rule; the rule is not narrowed.

#1051 waits until `feature/android-cover-retry-and-repaint` has landed. #998 is fixed on that
branch.

## Parallelität

Three strands with disjoint file groups. Strand files carry the tasks.

| Strand | Issues | Owns |
|---|---|---|
| a — core | #1041 | `crates/reprise-core/src/artist_portrait/{cover_backfill,cover_backfill_tests}.rs`, and only if the fix needs it `crates/reprise-android-ffi/src/{artist_portrait,artist_portrait_tests,artist_portrait_cover_chain_tests}.rs` |
| b — Android | #1000, #1055 | `android/app/src/test/.../MainActivityMusicPathsTest.kt`; `android/app/src/main/.../{LibraryFramePolicy,LibraryTrackRows}.kt`; Android tests that hard-code the 72/64 dp row height, only where the fix changes what they measure |
| c — desktop | #1018, #1052 | `crates/reprise-gnome/src/ui/radio/**`, `crates/reprise-gnome/src/ui/window/source_connectivity.rs`, `crates/reprise-gnome/src/ui/cover/**`, the strings and `po/` entries they need, and `docs/ux-rules.md` (new NET-7c only) |

**Merge order: a, b, c**, and strand c lands **after** `feature/android-cover-retry-and-repaint`.
That branch adds NET-7a and NET-7b to `docs/ux-rules.md` next to where NET-7c goes; c rebases
onto it and places NET-7c directly after NET-7b. a and b have no seam with each other or with
that branch.

**Post-merge cross-checks.**

- After c rebases onto the cover branch: NET-7a, NET-7b and NET-7c read in order, and
  `scripts/check-ux-traceability.sh` passes.
- Strand b's font-scale fix needs a device check (rows at font scale 2.0 in the Titles list
  and a queue drag). The orchestrator runs it under `device-lock`; it is not a Codex task.
