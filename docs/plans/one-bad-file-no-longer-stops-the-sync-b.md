---
slug: one-bad-file-no-longer-stops-the-sync-b
worktree: /home/marvin/Projects/reprise-one-bad-file-no-longer-stops-the-sync-b
branch: feature/one-bad-file-no-longer-stops-the-sync-b
phase: refactored
codex_session:
created: 2026-08-30
---
# Strand B — a lost track no longer strands the run

Part of `docs/plans/one-bad-file-no-longer-stops-the-sync.md`. Read that mother
plan first: it carries the evidence, the mechanism and the decisions this strand
implements. Do not re-derive them here.

B1 and B2 are **one change and must land together** — B1 alone leaves the delta
frozen, and B2 alone would stamp a playlist that was never republished. The
mother plan's decision 2 says why they are safe together.

## Owned files

- `crates/reprise-core/src/device_sync/machine.rs`
- `crates/reprise-core/src/device_sync/machine_tests.rs`
- `crates/reprise-core/src/device_sync/ledger.rs`
- `crates/reprise-core/src/device_sync/sync_log.rs`
- `crates/reprise-core/src/device_sync/sync_log_tests.rs`
- `crates/reprise-gnome/src/ui/device_sync/device_sync_effects.rs`
- `crates/reprise-gnome/src/ui/device_sync/device_sync_planned.rs`
- `crates/reprise-gnome/src/ui/device_sync/device_sync_run_log_tests.rs`
- `crates/reprise-gnome/src/ui/sidebar/sidebar_device_card_mirror_tests.rs`
- `crates/reprise-runtime/src/devices.rs`
- `crates/reprise-runtime/src/devices_tests.rs`
- `crates/reprise-runtime/src/runtime_tests.rs`
- `docs/ux-rules.md`
- `crates/reprise-gnome/src/ui/device_sync/device_sync_compact_tests.rs`
  (granted during the code phase 2026-08-30: it asserts the pre-amendment
  MTP-19 contract `writes.get() == 0`, which decision 1 of the mother plan
  replaces. Unowned by strand A, so the cut stays disjoint.)

Touch nothing under `crates/reprise-platform-linux/`.

The list is longer than the two tasks suggest because both changes widen an
enum the whole workspace matches on. Every site was enumerated before the cut:
`grep -rn "WritePlaylist {" --include='*.rs' crates/` gives nine, all above;
`grep -rn "SyncOutcome::Failed" --include='*.rs' crates/` adds four more, of
which `reprise-runtime/src/devices.rs:218` is production code that *constructs*
the variant and `device_sync/sync_log.rs:138` matches it without a `..` rest
pattern. Both break the moment `verified_sources` is added, and neither is
obvious from the task text.

## Task B1 — a held-back playlist is published without the entries that failed

`enter_playlist_writes` (`machine.rs:588-606`) currently skips any planned
playlist that names a failed device path. It publishes it instead, minus those
entries, so the playlist enters `successful_playlist_sources` and the removal
gate in `begin_removals` opens.

The effect carries the omitted set:

```rust
WritePlaylist { index: usize, omit_relative_paths: Vec<String> },
```

empty on every path that loses nothing.

The runtime cannot just filter and write. `PlaylistWrite` carries a
**pre-rendered** `contents: String` beside its `entries`, and the handler writes
`playlist.contents` verbatim (`device_sync_effects.rs:260`). So with a non-empty
`omit_relative_paths` the handler re-renders:

```rust
m3u::render_named_playlist(
    &playlist.entries.iter()
        .filter(|entry| !omit.contains(&entry.relative_path))
        .cloned().collect::<Vec<_>>(),
)
```

`render_named_playlist` is already public (`device_sync/m3u.rs:12`). With an
empty omit set the existing `contents` is used unchanged, so the common path
stays byte-identical to today's.

**The known wrinkle, kept visible.** For a `replace` whose copy failed before
`publish` ran, the *previous* file at the same path may still be on the device;
omitting its entry then drops a track from the playlist that does exist. It is
cosmetic and the next run restores it, because planning never consults
`last_synced_at`. It is a trade, not a free win.

What does **not** change: a playlist whose *write* failed, and an obsolete
playlist that could not be *deleted*, still close the removal gate. Their
contents are genuinely unknown. Only the "held back because a track was lost"
case is affected.

## Task B2 — a run that lost only tracks still advances `last_synced_at`

`finish()` returns `Failed` whenever `self.failures` is non-empty, and
`finish_sync` only calls `refresh_contents_after_sync` on `Completed`
(`device_sync_planned.rs:272-276`). So even with B1 the removals run but the
delta never settles.

Give `SyncOutcome::Failed` a `verified_sources` field too, and drive the
post-run refresh from what the run actually completed rather than from the
outcome variant. The error message, `sync_error` and the resume state are
untouched — the run is still reported as failed, it simply stops discarding the
work it did.

**Mind the asymmetry, it is deliberate.** On `Completed`, `finish()` fills
`verified_sources` from *every* `plan.playlist_writes` entry
(`machine.rs:711-719`). On `Failed` it must fill them from
`successful_playlist_sources` instead — otherwise a playlist whose write
genuinely failed would be stamped as synced. Same field, two sources.

## Task B3 — MTP-19's rule text

Replace the rule at `docs/ux-rules.md:660` with:

```
- **MTP-19** [active] [core] — A failed track holds back only what depends
  on it. A playlist that would point at a track that never arrived is
  published without that entry, so a published playlist names only files the
  device really has; the lost track returns on the next run that delivers it.
  Removals wait until every planned playlist has been rewritten, because an
  older playlist left on the device may still reference a file that is about
  to be deleted.
```

Keep the surrounding list formatting and the `[active] [core]` tags. Nothing
else in the repository restates MTP-19's wording — `docs/research/p5-surface-scopes.md:86`
is a scope table row and stays as it is.

## Verification

Every check reads only this strand's own files.

1. `mtp_19_a_playlist_held_back_by_a_failed_track_keeps_its_previous_file`
   (`machine_tests.rs:279`) is **rewritten**, not deleted, and renamed to match
   the new contract: the playlist is written without the lost track, and the
   removals run afterwards.
2. `mtp_19_a_playlist_that_could_not_be_rewritten_holds_every_removal_back` and
   `mtp_19_a_playlist_that_could_not_be_deleted_holds_every_removal_back` stay
   green unchanged.
3. New: a plan with one failing copy, one playlist covering that track and one
   removal reaches `Effect::RemoveTrack` and still finishes with the track in
   `failed_tracks`.
4. New: `Effect::WritePlaylist` carries the failed path in
   `omit_relative_paths`, and the runtime writes an m3u without that line while
   keeping the others.
5. New: a `Failed` outcome that republished a playlist reports that source in
   `verified_sources`, and one whose playlist write failed does not.
6. `cargo test -p reprise-core -p reprise-gnome -p reprise-runtime` green;
   clippy clean for the same three.
