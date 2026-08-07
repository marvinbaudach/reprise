# The inventory is a memory, not a proof

Measured on the real phone on 7 August 2026, with the desktop's own library:

```
device (physically, under Music/Reprise)   214 audio files
desktop inventory (device_files)           278 rows
selected tracks (playlist:2 + smart:2)     291
planned copies                              20
```

271 of the 291 selected tracks have an inventory row. Only 214 of them are
actually on the phone. **57 selected tracks are missing from the device and the
sync will never notice**, because `plan_file_changes` asks only the inventory:

```rust
match inventory_by_id.get(&track_id) {
    None => plan.copy.push(file.clone()),
    Some(existing) if inventory_matches(existing, file) => {}   // <- "done"
    ...
}
```

and `inventory_matches` compares the *source* file, the device path and the
profile fingerprint — never whether that device path still exists. The same
`plan_mirror` call already receives `managed_files`, the fresh scan of the
device, and uses it for orphan removal. It just never uses it to answer "is
this track actually there".

A phone loses files for ordinary reasons: the user deletes an album in a file
manager, Android's storage cleaner reclaims space, a copy is interrupted after
the inventory row was written. Today every one of those is permanent.

## What to change

`plan_mirror` learns one new fact and one new rule.

**The fact.** `MirrorInput` gains a field that says whether `managed_files` is a
complete, successful scan of the device — something like
`managed_files_scanned: bool`. It must be an explicit input, not inferred from
`managed_files.is_empty()`: an empty list means "device is empty" and "we never
looked" equally well, and guessing wrong the second way would re-copy the whole
library. The GNOME runtime sets it from the state it already keeps
(`device.ever_inspected` together with `scan_error.is_none()`); every other
caller, including the tests that pass no scan at all, keeps today's behaviour by
leaving it `false`.

**The rule.** When the scan is authoritative and a desired track's recorded
device path is not among the scanned files, that track is copied again instead
of counting as done. It goes through the ordinary copy path, so it also counts
into `transfer_bytes` and shows up as an addition on the page.

Note the interaction with M15, and keep it: a track that is being copied in this
run is "arriving", so `arriving_audio_paths` already protects and re-writes its
analysis sidecar. A track that comes back this way therefore gets its analysis
back too, and its sidecar must not be swept as an orphan in the same run.

## What must not change

- A device that was never scanned, or whose scan failed, plans exactly what it
  plans today. This is the guard against re-copying a whole library because a
  cable was pulled.
- A track whose file *is* present and whose inventory row matches is still left
  alone. No re-copying of things that are fine.
- Nothing about removal, sidecars or playlists changes shape.

## Proof

Every claim gets a test that fails when the production line is reverted:

1. A desired track with a matching inventory row whose device path is absent
   from an authoritative scan is planned as a copy.
2. The same track with `managed_files_scanned: false` is *not* planned — today's
   behaviour, the guard.
3. The same track when its file *is* in the scan is still not planned.
4. The returning track's analysis sidecar is written and is not removed as an
   orphan in that same run.

State the mutation you used for each in the summary — which line you reverted
and which test went red.

## A UX rule

Add the rule to `docs/ux-rules.md` next to the other `MTP-*` rules, in the house
voice: what is missing from the phone is copied again; the inventory is what
Reprise remembers writing, not proof that it is still there.

## Ownership — read this before touching anything

Two other branches are being worked on **right now** in this same area, and this
package must not collide with them:

- `feature/device-sync-identity-serial` and
  `feature/device-sync-history-and-plan` own
  `crates/reprise-platform-linux/src/device_sync_identity.rs`,
  `crates/reprise-gnome/src/ui/device_sync/device_sync_planned.rs`,
  `device_sync_run_log.rs`, `device_sync_strings.rs`, `device_sync_history.rs`,
  `device_sync_content_panel.rs` and the `po/` catalogs.

Stay in `crates/reprise-core/src/device_sync/` for the logic. The one GNOME
change this needs — passing the new fact into `SyncPageInput`/`MirrorInput` —
belongs in `device_sync_compact.rs::recompute_delta_silent`, which those
branches barely touch (four added lines). If you find yourself editing any file
in the list above, stop and say so in the summary instead.
