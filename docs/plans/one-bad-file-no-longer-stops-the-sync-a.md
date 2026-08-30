---
slug: one-bad-file-no-longer-stops-the-sync-a
worktree: /home/marvin/Projects/reprise-one-bad-file-no-longer-stops-the-sync-a
branch: feature/one-bad-file-no-longer-stops-the-sync-a
phase: refactored
codex_session:
created: 2026-08-30
---
# Strand A — cleanup survives, writes name their step

Part of `docs/plans/one-bad-file-no-longer-stops-the-sync.md`. Read that mother
plan first: it carries the evidence, the mechanisms and the decisions this
strand implements. Do not re-derive them here.

Everything in this strand lives in **`crates/reprise-platform-linux`**. Touch no
other crate and no file outside the ownership list.

## Owned files

- `crates/reprise-platform-linux/src/device_sync.rs`
- `crates/reprise-platform-linux/src/device_sync_tests.rs`

## Task A1 — partial cleanup becomes best-effort

`cleanup_partials_in` (`device_sync.rs:406`) currently aborts the whole
synchronization run when a single `.part` cannot be deleted. Make it survive
that.

- A failed `delete_future` on a `*.part` is logged with `tracing::warn!` —
  include the file's path and the error — and the walk continues to the next
  entry and the next directory.
- **Do not change the signature.** It stays `Result<u32, DeviceIoError>`
  returning the number of partials actually removed. Two callers live in another
  crate (`device_sync_backend.rs:105`, `device_sync_smoke.rs:134`) and are owned
  by strand B; widening the return type would force this strand to edit files it
  does not own. The count of what was left behind belongs in the warning only.
- **Do not probe the child after a failed delete.** The mother plan's decision 4
  explains why: an unmeasured `query_info` would decide nothing and be untested.
- Failure to *enumerate* a directory stays fatal — a folder the run cannot list
  is a device the run cannot use. The existing `NotFound` arm on enumeration
  keeps its `continue`.

Why this is safe: an undeletable `.part` cannot damage a run. A leftover at the
*same* path is overwritten — `replace_managed` writes its partial with
`FileCopyFlags::OVERWRITE` and `publish` deletes the target before moving onto
it. One under a *different* path is never reused; the observed phantoms sat
under a case-variant directory, so the next run addresses a different MTP object
and simply ignores them. Either way the only cost is wasted space on the phone,
which the warning now names. The cost of today's behaviour is that no byte ever
moves.

## Task A2 — a managed write names the step that failed

Add to `device_sync.rs`:

```rust
/// Which step of a managed write produced the failure underneath.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteStep {
    ResolveStorage,
    CreateDirectories,
    CopyPartial,
    VerifyPartial,
    Publish,
}
```

and one `DeviceIoError` variant:

```rust
DuringWrite { step: WriteStep, source: Box<DeviceIoError> },
```

Its `Display` prefixes the inner message with what the step was, e.g.
`"creating the destination directory failed: device I/O failed: libmtp error:
could not send object info"`. Wrap each of `replace_managed`'s five steps in it.

Give `replace_playlist` the same treatment for its two steps
(`CreateDirectories`, `Publish`) — its own doc comment already calls publish
"the path that meets the broken rename on every single run".

Nothing above this needs plumbing. The trait boundary already flattens
`DeviceIoError` to a `String`, and both the warning at
`device_sync_effects.rs:206` and the deviation note beneath it interpolate that
string, so both gain the step without an edit in the other crate.

**Explicitly rejected:** a structured `step = …` field on the `tracing::warn!`.
That needs the typed error at the effect boundary, i.e. widening the
`replace_track` trait error type across seven implementors including six test
fakes — all owned by strand B. Not worth it for a log field.

## Verification

Every check reads only this strand's own files.

1. `cleanup_partials_removes_only_orphaned_part_files_under_the_managed_root`
   (`device_sync_tests.rs`) stays green unchanged.
2. New: one `.part` whose delete fails does not stop the walk. Make the delete
   fail by `chmod 0500` on the partial's parent directory — `unlink` needs write
   on the parent, while `enumerate_children` only needs read, so the walk still
   finds the file and only the delete fails. That is the local stand-in for the
   phantom, and the property under test ("one failure does not stop the rest")
   is backend-independent.

   **The two partials must live in different directories**, only one of them
   chmod'd. Putting both under the same read-only parent makes both deletes fail
   and the test asserts nothing. Assert through the returned count (`Ok(1)`, the
   deletable one), not through captured log output.

   The fixture is ineffective under a `root` uid, which ignores the mode bits.
   If the suite could ever run as root, skip the case rather than let it pass
   for the wrong reason.
3. New: a directory that cannot be enumerated still returns `Err`.
4. New: the step appears in the message. At minimum `CreateDirectories`,
   `VerifyPartial` — reachable through the existing
   `replacement_verifies_the_partial_size_before_overwriting_the_final_file`
   fixture — and `Publish`, reachable through
   `mtp_21_a_rename_that_left_nothing_behind_is_reported_not_believed`.
5. `cargo test -p reprise-platform-linux` green; `cargo clippy -p
   reprise-platform-linux` clean.
