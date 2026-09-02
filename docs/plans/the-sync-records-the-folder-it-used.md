---
slug: the-sync-records-the-folder-it-used
worktree: /home/marvin/Projects/reprise-mtp-ledger
branch: fix/the-sync-records-the-folder-it-used
phase: coded
codex_session:
created: 2026-09-02
---
# The sync records the folder it used

Follow-up to `mtp-directory-error-2026-09-02.findings.md`, whose "Implemented
(2026-09-02)" section landed as the previous commit on this branch. Two items
were left open there deliberately; this plan closes them.

## The problem

`ensure_managed_directories` now adopts the folder spelling the device really
has (`Speaker Of The Dead` where the plan asked for `Speaker of the Dead`) and
`replace_managed` writes the file underneath that adopted spelling. The backend
returns only `CopyOutcome::Copied`, so the layer above never learns where the
file actually landed and `Effect::RecordFile` writes the **planned** path into
`device_files`.

Consequences, all of which predate the adoption change and are now merely
reached more often:

- `delete_managed` takes the ledger path, so removing such a track hits
  `NotFound`, returns `Ok(false)` and leaves an orphan on the device.
- `build_directory_spellings` chains ledger paths *and* the device scan, so the
  two spellings can tie and turn into `DirectorySpelling::Ambiguous`.
- A track whose folder drifted can be re-planned every run, because the desired
  path never matches what is on the device.

## The contract

**The ledger records the path the device actually has.** After a copy into an
adopted folder, `device_files.device_path` holds the adopted spelling, and the
next planning pass sees no work for that track.

## §1 — Carry the actual path out of the platform layer

`crates/reprise-platform-linux/src/device_sync_errors.rs:4`

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyOutcome {
    Copied,
}
```

Make the variant carry the relative path that was actually written:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CopyOutcome {
    Copied { relative_path: String },
}
```

`Copy` must go (a `String` field), `Clone` stays. Four construction sites and
four assertions follow:

- constructed: `device_sync.rs:439`, `device_sync_fake_backend.rs:448`,
  `device_sync_fake_backend.rs:530`, `device_sync_compact_tests.rs:60`
- asserted: `device_sync_tests.rs:248`, `:327`, `:446`, `:475`

In `replace_managed` (`crates/reprise-platform-linux/src/device_sync.rs`), the
adopted components are already in hand:

```rust
let directories = self
    .ensure_managed_directories(&storage, target_path, &components[..components.len() - 1])
    .await
    .map_err(|error| error.during(WriteStep::CreateDirectories))?;
let directory = child_of(&storage, &directories);
let target_name = components.last().expect("validated nonempty path");
```

`directories` is `<target_path components…> + <relative directories…>`, both
with the spellings the device reported. The ledger wants only the **relative**
tail plus the file name.

Do **not** recompute that tail at the call site by slicing off
`safe_target_components(target_path)?.len()`. That is count arithmetic resting
on the assumption that `safe_target_components` emits exactly one component per
input segment; the day it collapses an empty segment or a trailing slash, the
slice silently cuts into the album folder and writes a wrong path into the
ledger — which is the very bug this plan exists to fix, and a happy-path test
would not catch it.

Instead, `ensure_managed_directories` already knows where the boundary is,
because it loops over the two lists in order. Have it return both:

```rust
struct ResolvedDirectories {
    /// Every adopted component from the storage root down, for `child_of`.
    components: Vec<String>,
    /// Only the adopted components below the sync target folder.
    relative: Vec<String>,
}
```

Then the relative path is `relative.join("/")` with `target_name` appended, and
no call site does arithmetic.

**`replace_playlist` calls the same function** (with `&[]` relative directories)
and uses `child_of(&storage, &directories)` for its own path — it has to move to
`.components`. It is the site the merge conflict on this branch already touched,
so it is the one most likely to be left half-edited. Check it.

The file name itself is **not** adopted — only directories are. That is
deliberate; the failure this whole change is about is a directory failure.

The fake backend returns the relative path it was asked for; only the real
backend can adopt.

## §2 — Thread it through the machine into the ledger

The current chain (line numbers as of this branch):

- `device_sync_effects.rs:138` calls `.replace_track(…)`, and at `:154`
  `match result { Ok(_) => Event::TrackCopied(Ok(bytes)), … }` — the outcome is
  matched and thrown away.
- `machine.rs:363` turns `Event::TrackCopied(Ok(device_size))` into
  `Effect::RecordFile { index, device_size }` (`machine.rs:98`).
- `device_sync_effects.rs:171` handles that effect and at `:182` writes
  `device_path: entry.device_path.clone()` — `entry` is
  `transfer(work, index).desired`, i.e. the plan
  (`DesiredManagedFile`, `mirror.rs:84`).

Carry the actual path along that chain. Both the event and the effect need it:

```rust
Effect::RecordFile {
    index: usize,
    device_size: u64,
    device_path: String,
}
```

`Event::TrackCopied` gains the path beside the byte count — a small struct
rather than a tuple, so the two `String`/`u64` fields cannot be swapped by
accident. `device_sync_effects.rs:182` then writes the path from the effect
instead of `entry.device_path`.

**`machine.rs:376-387` must compare against the recorded path, not the desired
one.** It decides whether the previous file has to be removed:

```rust
if previous.device_path != operation.desired.device_path {
    self.deferred_replacements.push((previous.device_path.clone(), …));
```

With an adopted spelling the desired path is exactly the one that is *not* on
the device, so this comparison has to use the path that was just recorded.
Otherwise a track whose folder drifted schedules a deletion of the file it just
wrote. That is the sharpest hazard in this change — cover it with a test.

Check whether the analysis and lyrics sidecars write ledger rows of their own
(`copy_analysis_sidecar` at `device_sync_effects.rs:460`, `copy_lyrics_sidecar`
at `:546`, and `write_track_metadata_list` at `:521` all discard the outcome
today). If they do, they need the same treatment; if they do not, say so in the
commit message and leave them.

## §3 — The MCP start logs nothing

`device_sync_page_actions.rs:43` logs `device sync started from page` on
success; the MCP path
(`device_sync_agent.rs`, `AgentDeviceSyncCommand::Start`, calling
`self.sync_now(&device_id)`) logs nothing at all. Two `sync_runs` rows
(106, 107) could not be attributed to a starter because of that asymmetry.

Add the matching `tracing::info!(device_id, "device sync started from agent")`
on the success arm — and the same for `Cancel`, which is equally silent.

## §4 — The ancestor gap: verify, then close it in prose or in code

`build_directory_spellings` (`device_case.rs:95`) folds only the **full parent
directory** of each known path; intermediate ancestors never enter the map. The
findings doc recorded that as an open gap.

The claim to check is that §1+§2 close it *without* new planning code. The map
at `device_case.rs:99-103` is built from the ledger `inventory` **chained with**
`managed_files`, the device scan. Today the ledger holds the planned spelling
and the scan holds the adopted one, so the two tie and the majority vote falls
to `DirectorySpelling::Ambiguous` — which is exactly the second consequence the
findings doc listed. Once the ledger records what the device has, both sources
agree and the vote resolves to `Resident` with the adopted spelling.

So the acceptance is a **tie-break**, not a happy path: a track with one ledger
row and one scan entry, both carrying the adopted spelling, must resolve to
`DirectorySpelling::Resident`, not `Ambiguous`. If that holds, record it in
`mtp-directory-error-2026-09-02.findings.md` and change no planning code. If it
does not hold, record intermediate ancestors as well.

Do not guess. Write the test that decides it.

## Tests

The file list below is a starting point, not a fence. Add tests where they
belong.

1. **The ledger gets the adopted spelling.** `device_sync_tests.rs` already
   drives `replace_managed` against a real temporary directory and already has
   the two adoption tests from the previous commit. Extend one: assert the
   returned `CopyOutcome::Copied { relative_path }` is the adopted spelling, not
   the requested one.
2. **The second run plans nothing.** The regression that matters, and the test
   that decides §4. With a ledger row carrying the adopted spelling and a device
   scan reporting the same folder, the spelling must resolve to `Resident` (not
   `Ambiguous`) and `compute_delta` must plan **zero** transfers for that track.
   Without §1+§2 the two sources disagree, the vote ties, and the track is
   re-planned every run.
3. **No self-deletion after adoption.** A transfer whose previous ledger row and
   whose newly recorded path are the *same* adopted spelling must push nothing
   into `deferred_replacements` (`machine.rs:383`). Build this as a machine-level
   test in `machine_tests.rs`.
4. **Control arm.** Each of the three must fail with the change reverted. State
   in the commit message which assertion falls and how — a test that passes both
   ways proves nothing.

## Verification scope

This change touches Rust only: `crates/reprise-core`, `crates/reprise-gnome`,
`crates/reprise-platform-linux`. Run:

- `cargo fmt`
- `cargo clippy --all-targets -p reprise-core -p reprise-gnome -p reprise-platform-linux`
- `cargo test -p reprise-core -p reprise-gnome -p reprise-platform-linux`

Do NOT run: `./gradlew` / `gradlew` in any form, the Android suite, `uniffi-bindgen`,
a release build of `reprise-android-ffi`, `cargo audit`, or a repo-wide gate
script. No Kotlin, no Android, no packaging is touched by this change. If
AGENTS.md or a gate document tells you to run the full gate before committing,
that instruction does not apply to this run — this exception is deliberate and
stated here.

## Out of scope

- The device itself. There is no phone attached to this machine right now, so
  the on-device arm cannot run. Do not fake it, do not claim it. The
  measurement belongs to a later session.
- `finish_sync` marking the whole run `failed` for a single lost track — that is
  the separate `one-bad-file-no-longer-stops-the-sync` plans.

## A trap left in the main checkout

`/home/marvin/Projects/reprise` (branch `song-visuals-ask-the-stored-category`)
still carries the **pre-#816** variant of the four files this branch's first
commit supersedes — `device_sync.rs` and `device_case.rs` in `reprise-core`,
`device_sync.rs` and `device_sync_tests.rs` in `reprise-platform-linux`. They
were written before the `ManagedWalk` rework and were carried across here with a
three-way apply; the conflict in `replace_managed` was resolved in favour of
dev's copy-to-final-name. Discard them once this branch lands. Do not merge them
forward.
