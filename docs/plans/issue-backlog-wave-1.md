---
slug: issue-backlog-wave-1
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-22
strands: 1,2,3
merge_order: 1,2,3
---
# Closing the open issue backlog — wave 1

`docs/plans/open-issue-sweep-2026-08.md` triaged the tracker on 2026-08-22 and
turned what remains into numbered tasks. This is the first implementation wave
against that triage. It carries no new analysis of its own: every task below
points back at the sweep plan's task number and at the issue it discharges.

**Base for all three strands:** `origin/dev` = `1515487599`.

Two of the sweep's tasks are deliberately **not** in this wave:

- **#622** (Last.fm bundled credentials) is owned by another session; its work is
  already on `dev` as `1515487599`. Nothing here touches it.
- **#475** (`ScrollAdoptionGeometry` → `ListLayout`) is sequenced *after* strand 3
  by the sweep plan's task 13, because both write the same scroll path and
  running them together makes either one's measurement unreadable.

## The cut

| Strand | Issues | Sweep tasks | Why it is its own strand |
|---|---|---|---|
| 1 | #254, #250, #405 | 15, 14, 6 | Three small, mutually independent defects in three disjoint file groups. None of them changes a scroll or sort path. |
| 2 | #404 | 5 | A new control plus the rule that governs it. Touches the browse bar, the sort state and `docs/ux-rules.md` — nothing any other strand writes. |
| 3 | #620 | 1, 3, 4 | The centring preseed. Behaviour-carrying on the one write path this repository has already fixed four times; it must be measured alone. |

### File ownership

A strand writes **only** the paths it owns. Reading another strand's files is
allowed; writing them is not, and neither is a verification step that compares
against a value another strand produces (those live in the post-merge list
below).

**Strand 1**
```
crates/reprise-gnome/src/ui/stats/**
crates/reprise-gnome/src/ui/playback/**
crates/reprise-gnome/src/ui/session_restore.rs
crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs
crates/reprise-gnome/src/ui/track_list/rating.rs
scripts/cua-e2e/README.md
docs/plans/issue-backlog-wave-1-1.md
```

**Strand 2**
```
crates/reprise-gnome/src/ui/browse/**
crates/reprise-gnome/src/ui/track_list/track_list_sort.rs
crates/reprise-gnome/src/ui/strings.rs
docs/ux-rules.md
docs/plans/issue-backlog-wave-1-2.md
```

**Strand 3**
```
crates/reprise-gnome/src/ui/track_list/**   (except rating.rs and track_list_sort.rs)
crates/reprise-gnome/src/ui/scroll_center.rs
crates/reprise-gnome/src/ui/list_geometry_layout.rs
docs/plans/one-centering-path-preseed-variant.md
docs/plans/issue-backlog-wave-1-3.md
```

The three overlaps that were designed away rather than hoped away:

- `crates/reprise-gnome/src/ui/strings.rs` — strand 2 owns it. Strand 1 needs no
  new string: `strings::rate_n_stars(n)` already exists and is already per-star.
- `crates/reprise-gnome/src/ui/track_list/track_list.rs` (the `Shared` struct) —
  **nobody** writes it. Strand 2 was explicitly given a design that reads
  `shared.sort.borrow()` when its popover opens instead of adding an observer,
  precisely so that this file stays untouched.
- `crates/reprise-gnome/src/ui/track_list/` — split by file, not by directory:
  `rating.rs` to strand 1, `track_list_sort.rs` to strand 2, everything else to
  strand 3.

## Merge order

`1, 2, 3`. Strands 1 and 2 are independent of each other and of strand 3; the
order is by size, so the cheap ones land first and strand 3 rebases onto a `dev`
that already carries them. Strand 3 last also keeps its control-arm measurement
the newest thing on the branch.

## Post-merge cross-checks

These read files no single strand owns, so none of them may run inside a strand:

1. **The a11y semantics gate over the whole tree.**
   `scripts/check-accessibility-semantics.sh` reads all of
   `crates/reprise-gnome/src/ui`. Strand 1 adds star labels, strand 2 adds a sort
   control; only after both have landed does a green run mean anything about the
   pair. Run it once on the `dev` that contains both.
2. **The displayless GNOME suite on the merged tree.**
   `cargo test --locked -p reprise-gnome` with fresh XDG roots and
   `REPRISE_AUDIO_SINK=fakesink`. Each strand runs it on its own branch; the
   merged run is the one that proves they do not interact.
3. **#404 against #405.** Both issues came from the same CUA sweep. Whether the
   sweep's findings are discharged is a question about the merged app, not about
   either branch — re-run the sweep's a11y missions once, after both have landed,
   and record the numbers on the issues.
4. **The `Fixes #444` gate stays closed.** No strand in this wave may claim
   `Fixes #444`; §4C of `docs/plans/queue-anchor-grill-followups.md` is satisfied
   only when the third mutation has been run and recorded. That measurement runs
   outside this plan.

## Out of scope

`main`, the promotion gate, the release channel, #622, #475, #411, #406, #444 and
#597. Each of those is either owned elsewhere, sequenced after this wave, or a
measurement rather than a change.
