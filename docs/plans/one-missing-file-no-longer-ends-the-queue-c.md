---
slug: one-missing-file-no-longer-ends-the-queue-c
worktree: /home/marvin/Projects/reprise-one-missing-file-no-longer-ends-the-queue-c
branch: feature/one-missing-file-no-longer-ends-the-queue-c
phase: shipped
codex_session:
created: 2026-09-01
---
# Strand C — The sync stops deleting what it is about to re-copy

Mother plan: `docs/plans/one-missing-file-no-longer-ends-the-queue.md`. Read it
and `docs/plans/android-source-error-on-synced-track.findings.md` first.

This is the strand that closes the window the other two only survive.

## File ownership

Touch only `crates/reprise-core/src/device_sync/**`.

**Do not edit `crates/reprise-core/src/lib.rs`** and do not touch
`crates/reprise-core/src/playback/**` — strand A owns that tree.

Note `crates/reprise-core/src/queries/smart.rs` is **not** owned here. The smart
query itself is correct and stays untouched (mother plan, D6); this strand
changes what the *planner* does with its result.

## The mechanism

`crates/reprise-core/src/device_sync/mirror.rs:478` deletes every inventory
entry that is not in today's `desired` set — no grace period, no tombstone,
and the `pinned` flag on `DeviceFileRecord` is never consulted:

```rust
for existing in inventory {
    if desired.contains_key(&existing.track_id) || retained_ids.contains(&existing.track_id) {
        continue;
    }
    if safe_managed_path(&existing.device_path) { /* … push removal … */ }
}
```

`desired` comes from a live smart-playlist query
(`device_sync/snapshot.rs:34`), and `queries/smart.rs` applies both the list's
own `ORDER BY <sort_field>` and its `LIMIT limit_count`. With a volatile
`sort_field` — `rating`, `play_count`, `last_played_at` — a track sitting at the
cap boundary drops out on one run and returns on the next. Measured in the wild:
the same album lost six files per run in runs 82, 87 and 88, each time
re-copied afterwards.

## C1 — Prove the churn in a test before changing behaviour

Write the failing test first: two consecutive plans over the same inventory,
where a track leaves the capped selection between them and returns. Assert the
file is **not** scheduled for removal in between.

Keep it a planner-level test with a seeded selection — do not reach for a live
database, and do not depend on wall-clock time.

## C2 — A stability margin on the cap

Chosen in the grill over a grace period (`device_files` has no timestamp column,
so that needs a migration) and over honouring `pinned` (dead outside one test,
no UI, and no defence against automatic churn).

The rule: a track that is **already resident** is only removed once it falls
below `limit_count + margin` — not merely below `limit_count`. A track inside
the margin band is left alone; the cap still governs what gets *added*.

Decide and document in the code:

- the margin's size and whether it is fixed or proportional (a fixed count is
  simpler to reason about and to test; state the number and why);
- that device growth is bounded by exactly the margin, so the guarantee the cap
  offers is weakened by a known, small amount rather than removed;
- that a track failing the smart list's **rules** — as opposed to merely losing
  its rank — is still removed immediately. The margin protects against rank
  flapping, not against a track that genuinely no longer belongs.

Genuine removals must keep working: a track deleted from the library, a
playlist that no longer names it, or a device that is legitimately full. The
`deleted` column in `sync_runs` is the control arm and must not become
permanently zero.

## C3 — Character drift still creates duplicate device objects

Run 87 deleted `It's Not Just a Party…` under **both** apostrophe spellings —
typographic `’` and ASCII `'` — so the device carries two objects for one song.

`the-sync-keeps-the-name-the-phone-already-has` (#773) taught the planner to
adopt a resident spelling that differs only in **case**. Typographic versus
ASCII apostrophe is the same class of problem and is not covered. Extend the
existing spelling-adoption comparison to fold that difference too, **reusing the
folding helpers already there** rather than adding a second normaliser beside
them.

Be careful about the direction the existing code already settled: adopt the
spelling the phone already has, rather than renaming the device's file to match
the tag. Follow-up work in `the-unplanned-track-keeps-its-file` fixed a
data-loss path in exactly this area — read it before touching
`device_case.rs`, and do not reintroduce the case it closed.

## Verification

```sh
cargo test -p reprise-core device_sync
```

The two-run device check ("the second sync plans zero transfers for what the
first placed, and `deleted` is not permanently zero") needs real hardware and
belongs to the mother plan's post-merge list, not here.
