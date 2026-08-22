# Handover — Android reads a deleted file as a provider failure, never as absent

State on 2026-08-22, ~13:00. **Diagnosis complete and measured on the real
device; nothing is implemented yet.** No branch, no worktree, no run in flight.
The next step is `/plan` → `/code` (implementation belongs to Codex).

## Symptom as reported

The user deleted tracks in the desktop app and ran a device sync. The Android
app keeps showing those tracks. Tapping one produces, as a red banner above the
list:

```
ERROR_CODE_IO_UNSPECIFIED: Source error
```

A manual rescan in the Android app does **not** clear them — this was the
user's own observation and it has since been reproduced and measured.

## What was measured (not inferred)

Device: Pixel 10 Pro XL, `59100DLCQ006SB`, app `org.reprise` 0.1.25
(versionCode 25, installed 2026-08-20). The app is a release build and **not
debuggable**, so its database cannot be pulled with `run-as`; every observation
below comes from the filesystem, the UI and logcat.

**1. The desktop sync did its job.** The files really are gone from the phone,
and the emptied album folders are still there:

```bash
adb shell 'ls -ld "/sdcard/Music/Reprise/Anchors & Hearts/Deathlist"'
# drwxrws--- 2 u0_a187 media_rw 3452 2026-08-22 12:23   <- empty, mtime = deletion
adb shell 'find /sdcard/Music/Reprise -type d -empty'   # 17 emptied album folders
adb shell 'find /sdcard/Music/Reprise -type f \( -iname "*.mp3" -o -iname "*.flac" \
  -o -iname "*.m4a" -o -iname "*.opus" -o -iname "*.ogg" -o -iname "*.wav" \) | wc -l'
# 728   (plus 46 files under /sdcard/Music/Reprise-YouTube)
```

The library header said **780 titles** at that moment.

**2. A full rescan runs, and changes nothing.** Triggered through the app's own
overflow menu (⋮ → Rescan). The progress screen was captured mid-run
("Scanning 198 of 780…"), the scan took roughly two minutes, and afterwards the
header still read **780 titles** with `999 • Anchors & Hearts • Deathlist`
still in the list — its folder being the empty one above.

**3. The scan marks nothing.** logcat was captured across the whole run:

```bash
adb logcat -c
adb logcat -s Reprise:V > scan.log      # this crate's tracing sink, see logging.rs
```

Not a single `scan: marked vanished track missing` line
(`scanner_vanish.rs:194` / `:207`) appeared.

**4. The decisive line — the same run, a different code path, same defect:**

```
W Reprise : reprise_core::device_sync::mobile_import: could not read analysis sidecar
  track_id=746
  sidecar="content://com.android.externalstorage.documents/tree/primary%3AMusic%2FReprise/document/primary%3AMusic%2FReprise%2FAnchors%20%26%20Hearts%2FDeathlist%2F05%20999.reprise-analysis"
  error=provider failure: Failed to determine if primary:Music/Reprise/Anchors & Hearts/Deathlist/05 999.reprise-analysis
        is child of primary:Music/Reprise: java.io.FileNotFoundException: Missing file for …
```

`provider failure: {detail}` is the `Display` text of `SafSourceError::Unknown`
(`crates/reprise-android-ffi/src/source.rs:47`), and Kotlin raises that variant
from exactly one place: the trailing `catch (error: RuntimeException)`. So on
this device a **deleted** SAF document arrives as a RuntimeException that merely
*mentions* `FileNotFoundException` in its message — it is not a
`FileNotFoundException`.

## Root cause

Core decides "the file is gone" on one value only —
`crates/reprise-core/src/library/scanner_vanish.rs:171`:

```rust
// This write needs confirmed absence. Present and Unknown both keep
// the row live; inability to reach a source is not a missing verdict.
if source.probe(path, LibraryLinkMode::Follow) != LibraryPathPresence::Absent {
    continue;
}
```

That contract is right, and the desktop satisfies it (`stat()` → `ENOENT` →
`Absent`). Android never does:

1. Every tree-URI operation passes through Android's
   `DocumentsProvider.enforceTree()`, which calls `isChildDocument()`. For a
   document that no longer exists, `ExternalStorageProvider` raises a
   **RuntimeException** — `"Failed to determine if <child> is child of
   <parent>: java.io.FileNotFoundException: …"` — not a bare
   `FileNotFoundException`.
2. `AndroidSafSource.probe`
   (`android/app/src/main/java/de/reprise/spike/AndroidSafSource.kt:22-39`)
   catches `FileNotFoundException` at line 28 and would return `null` — the
   correct absent answer — but that catch never fires. Line 36's
   `catch (error: RuntimeException)` takes it and throws
   `SafSourceException.Unknown`.
3. `BridgedSource::probe` (`crates/reprise-android-ffi/src/source.rs:181-193`)
   maps `Err(_)` to `LibraryPathPresence::Unknown`.
4. Back at `scanner_vanish.rs:171`, `Unknown != Absent`, so the row is skipped
   silently — no log, no counter, no trace.

The row therefore stays `PRESENT` forever. `queries/clauses.rs:26`
(`PRESENT = "missing_since IS NULL AND removed_at IS NULL"`) is doing its job
correctly; it is simply never told. The row keeps appearing in every list, and
playing it hands Media3 a URI whose document is gone —
`Media3PlaybackPort.kt:88` formats the resulting `PlaybackException` verbatim
into `ERROR_CODE_IO_UNSPECIFIED: Source error`.

**This is why a rescan cannot help: the rescan itself is blind.**

## Two more holes in the same seam

Both are the same missing concept — "the provider answered, and the answer is
*it does not exist*" — and both should be closed in one pass:

- `AndroidSafSource.kt:25-27` — an **empty cursor** (`moveToFirst()` false)
  falls through the `?:` into `throw SafSourceException.Unknown("The provider
  returned no metadata cursor")`. A provider that reports a missing document by
  returning zero rows instead of throwing hits exactly the same dead end. Only
  `resolver.query` returning a *null cursor* is a genuine unknown.
- `crates/reprise-android-ffi/src/source_error.rs:8-14` — `SafSourceError` has
  no not-found variant at all, so `BridgedSource::open_read` can never produce
  `io::ErrorKind::NotFound`. That is why `read_analysis_sidecar`
  (`crates/reprise-core/src/device_sync/mobile_import.rs:44-52`) logs a warning
  for every deleted sidecar although its `error.kind() != NotFound` guard exists
  precisely to stay quiet in that case. The log line quoted above is that guard
  misfiring.

## What the fix must do

Teach the SAF bridge to distinguish **confirmed absence** from **provider
failure**, and keep that distinction all the way into Core.

1. `AndroidSafSource.probe`: treat a `FileNotFoundException` found **anywhere in
   the cause chain** (including one wrapped in a RuntimeException by
   `enforceTree`) as absent → return `null`. An empty cursor → `null` as well.
   Keep a null cursor, `SecurityException` and genuine I/O failures as they are.
2. `AndroidSafSource.openReadFd`: same cause-chain detection, reported through a
   new `SafSourceError::NotFound` variant, mapped in `source_error.rs` to
   `io::ErrorKind::NotFound` (and to a matching `LibraryWalkErrorKind` in
   `walk_error`).
3. Do **not** widen this to "any RuntimeException means absent". The
   Present/Unknown/Absent split is what the scan's root guard
   (`scanner.rs`, `scanner_vanish::guard_evidence_under_root`) relies on to tell
   "your library folder is unreachable" from "your library is empty"; collapsing
   it would risk mass-marking a whole library missing when storage is merely
   unavailable.

### The fix must not be tailored to one provider or to one deleter

The Android app has to behave for a library that other apps touch: files
deleted by a file manager, by another music player, or by the system — not
only by Reprise's own desktop sync. Two consequences the plan has to honour:

- **Never key the detection on the message text.** The string
  `"Failed to determine if … is child of …"` belongs to
  `ExternalStorageProvider` on this Android version. A file manager's own
  `DocumentsProvider`, an SD card, USB-OTG or a cloud provider may report a
  vanished document as a bare `FileNotFoundException`, as an empty cursor, or
  as some other RuntimeException entirely. Matching on the message is how this
  bug returns on the next device.
- **Prefer a provider-independent confirmation over exception archaeology.**
  The robust rule is: *the parent directory is readable and does not list this
  child* → `Absent`. `SafSource::list_children` already exists and is exactly
  this question, it works on every provider regardless of how it signals
  errors, and it fails safe: if the parent itself cannot be listed, the answer
  is `Unknown`, never `Absent` — which is precisely the behaviour storage
  that is merely unavailable needs.

Recommended shape, to be settled in `/plan`: keep the cause-chain check
(`FileNotFoundException` anywhere in the chain) and the empty-cursor case as
the cheap fast path, and use the parent-listing check as the fallback whenever
the probe would otherwise return `Unknown`. That way one directory listing per
*unresolved* candidate is the worst case, not one per track.

Because deletions can come from anywhere, this also raises the value of the
rescan being reachable at all — see "ruled out" below: automatic reconciliation
is still not the cause of this bug, but with third-party deleters it is a real
follow-up worth its own plan.

### Tests the plan must require

- A fake `SafSource` that raises the **real** wrapped shape —
  `RuntimeException("Failed to determine if … is child of …: java.io.FileNotFoundException: Missing file for …")`
  with a `FileNotFoundException` cause — and asserts `probe` yields
  `LibraryPathPresence::Absent`. Without this the exact misclassification comes
  straight back. `crates/reprise-android-ffi/src/source.rs:292-510` already has
  fake-source tests to extend, including
  `probe_keeps_confirmed_absence_distinct_from_provider_failure` (line 399),
  which currently pins the *old* behaviour and needs revisiting.
- An empty-cursor case → `Absent`, and a null-cursor case → still `Unknown`.
- A fake source whose vanished document raises a provider-specific exception
  that is **neither** a `FileNotFoundException` nor an empty cursor: the
  parent-listing fallback must still resolve it to `Absent`, and an
  unlistable parent must still resolve to `Unknown`. This is the case that
  covers file managers and third-party providers.
- `open_read` on a not-found document → `io::ErrorKind::NotFound`, so
  `read_analysis_sidecar` stops warning.
- A Core-level test that `mark_vanished_with` does mark the row once the source
  reports `Absent` (guards the seam from the other side).

## What was ruled out — do not re-propose

- **"Android has no automatic rescan after a sync."** True (`rescan()` is only
  wired to two button handlers, `LibraryFrame.kt:113` and
  `settings/SettingsNavigation.kt:79`; `MainActivity.onStart`/`onResume` at
  `MainActivity.kt:416-426` trigger nothing), but it is **not the cause** — a
  manual rescan was measured and changed nothing. An automatic one is a
  separate follow-up — worth having, because deletions also come from file
  managers and other players, but it fixes nothing until the probe below can
  actually see an absent file.
- **"Add a cheap presence-check on app start."** It would call the same broken
  `probe` and find nothing. This was proposed and accepted before the
  measurement, then withdrawn; the user is aware.
- **A Media3-side fix only** (mark the row missing when playback fails). It
  treats the symptom, leaves every untouched phantom row in the list, and does
  not help the sidecar path. Reasonable as a *later* second layer, not as the
  fix.
- `reprise-track-metadata.rpl` on the device is dated 2026-08-10 while the sync
  ran on 08-22. Unverified whether that is correct behaviour (rewritten only on
  metadata change) — **unrelated to this bug**, noted only so it is not
  rediscovered as a lead.

## Verification after the fix

The bug is only fixed when this passes on the real device:

1. Build and install the APK, then trigger ⋮ → Rescan.
2. Capture `adb logcat -s Reprise:V` across the run and expect
   `scan: marked vanished track missing` lines with `reason=deleted` for the
   emptied albums.
3. The header count must drop from 780 to the number of audio files actually
   under the scanned root, and `999 • Anchors & Hearts • Deathlist` must be gone
   from the list. (Whether `Music/Reprise-YouTube` is inside the scanned tree was
   not established — establish it before pinning an exact expected number.)
4. Control arm: a track whose file is still present must **not** be marked.
5. Third-party-deleter arm: delete one track outside Reprise — `adb shell rm`
   on `/sdcard/Music/…` is the same thing a file manager does — rescan, and it
   must be marked too. Pick a track the desktop can re-sync afterwards, and
   read the one-way warning below first.

Note for whoever runs this: marking is one-way on Android — there is no relink
or purge path in the FFI, so a wrongly marked row cannot be brought back short
of `adb shell pm clear org.reprise`, which also destroys scan permission,
settings, queue and position. Test against emptied folders that are already
emptied, and never seed deletions to try it out.
