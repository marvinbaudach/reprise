# Android: "ERROR_CODE_IO_UNSPECIFIED: Source error" on a synced track

Investigation on 2026-09-01 against the live Pixel 10 Pro XL
(`59100DLCQ006SB`), prompted by a screenshot taken 2026-08-31 21:23 showing the
queue stopped on `A.I.` (Emmure, *Slave to the Game*) with a red banner and a
placeholder cover, 175 upcoming tracks behind it.

## What actually happened

**The file did not exist at the moment it was played.** The desktop device sync
had deleted it and had not yet written it back.

| time | event | source |
|---|---|---|
| 20:49:52–21:15:57 | sync run 87, `failed`, 215 copied, **25 deleted** — among them `Emmure/Slave To The Game/12 A.I.mp3`, detail *"no longer covered by the selection"* | `sync_events`, `sync_runs` |
| **21:23** | **user screenshot: source error, placeholder cover** | the screenshot |
| 21:25:47–21:45:12 | sync run 88, `failed`, 151 copied, 70 deleted | `sync_runs` |
| 21:27:51 | `12 A.I.mp3` written back to the phone | file mtime on device |
| 21:51:17 | music folder re-picked (`takePersistableUriPermission`) | `dumpsys activity permissions` |
| later | user runs a library scan | user report |

The error window is the ~12 minutes between run 87 removing the file and run 88
restoring it. The Android library row survived from an older scan and still
pointed at the SAF URI, so the queue happily selected a track whose bytes were
gone.

Both symptoms share that one cause. Queue-row artwork is not extracted at scan
time — `TrackCover.kt` resolves it live per row, keyed by the same
`request.trackUri`, and logs `"Could not read artwork for …"` on failure
(`TrackCover.kt:91`). Audio and cover therefore go through one document URI:
when the file is absent the cover falls back to the placeholder *and* the
source fails. It is not two bugs.

## Verified as self-healed

Re-driven today on the live device (09:29): the track plays, the seek track
draws, and the Emmure artwork renders in both the row and the mini player. No
banner, no `Playback error` in `adb logcat`.

Everything that could have made this permanent was ruled out, each measured:

- **File intact.** `adb pull` + `ffmpeg -v error -f null -` decodes all
  199.09 s, exit 0; size 8163326 matches `tracks.file_size` exactly.
- **MediaStore clean.** `_id=14754`, correct `_size`, `is_pending=0`,
  `is_trashed=0`.
- **SAF grant held.** A persisted read/write grant on
  `content://com.android.externalstorage.documents/tree/primary%3AMusic%2FReprise`
  exists for `io.github.marvinbaudach.reprise`.
- **URI well-formed** and identical in shape to its eight album siblings, which
  all played.
- **Not the case-collision defect.** Both spellings of `Slave To The Game`
  resolve to the same directory on the case-insensitive volume, and the DB path
  matches the resident spelling.

Caveat on the 21:51 folder re-pick: `takePersistableUriPermission` is only
reachable from the folder-picker callback
(`AndroidLibrarySessionPort.kt:39`), so a grant *was* re-taken at 21:51. It
cannot be the cause of the 21:23 failure, because the same screenshot shows
cover art rendering for ten other albums — reads through that tree were working.

## The defects worth fixing

The sync churn is the trigger, but the app's reaction to it is the bug in the
photograph.

### 1. One unreadable file kills the whole queue — and that already breaks an active UX rule

The behaviour is not an open design question. `docs/ux-rules.md` specifies it,
`[active] [core]`, in three places:

- **FB-6** — "Exception: the currently playing queue item faults → skip. A
  track shows one toast *Track unavailable — skipped*."
- **PLAY-5a** — "the playing track is never stopped by this (if the playing
  track itself faults, FB-6 applies: skip + one toast)."
- **PLAY-5b** — "No background event (deleted, unmounted, sync removal,
  watcher) stops the playing track."

A sync removal that deletes the playing track's file is precisely the case
PLAY-5b names. So this is a regression against a specified rule, not a new
feature.

**The fix site is Android-only.** `crates/reprise-android-ffi/src/playback_session.rs:452`:

```rust
PlayerEvent::Error(message) => {
    state.snapshot.state = AndroidPlaybackState::Stopped;
    state.snapshot.error = Some(message.into_message());
    state.current_loaded = false;
    (FollowUp::Stop, None, None)
}
```

The arm immediately above it — automatic advance — is the idiom to copy:
`state.queue.advance_auto()`, `state.adopt_current()`,
`FollowUp::Feed(state.next_uri())`, and only `state.stop()` when the queue is
genuinely exhausted. What is missing is a bounded guard so a wholly unreadable
library cannot spin through every entry.

This is the playback-side twin of
`docs/plans/one-bad-file-no-longer-stops-the-sync.md` — same principle, other
subsystem.

**Correction to an earlier reading of this: the blast radius is small.**
`PlayerEvent::Error` has three independent consumers, not one shared arm:

| surface | consumer | today |
|---|---|---|
| Android | `reprise-android-ffi/src/playback_session.rs:452` | **stops** — the bug |
| GNOME | `reprise-gnome/src/ui/playback/playback_faults.rs` | already implements FB-6 via `reprise_core::playback::playback_fault_policy` |
| Linux runtime service | `reprise-runtime/src/transport.rs` | stops; used by `reprise-platform-linux`, reachable via MPRIS |

`reprise-android-ffi` depends only on `reprise-core` and `reprise-view`, and
`reprise-gnome` depends on `reprise-runtime-client`/`-protocol` — **neither
crate depends on `reprise-runtime`.** So a fix in `playback_session.rs` cannot
touch the desktop, and GNOME needs no change at all. The shared piece worth
reusing is `PlaybackFaultPolicy` in `reprise-core`, which already encodes
FB-6's "one toast" cardinality.

Whether `transport.rs` should be fixed too is a real but *separate* question
about the headless runtime service. Note `playback_faults.rs` decides via
`Path::new(&summary.path).is_file()`, which cannot work for a SAF URI — the
Android arm needs its own predicate, not that one.

### 2. The error text cannot be diagnosed

`android/app/src/main/java/de/reprise/spike/Media3PlaybackPort.kt:91`:

```kotlin
override fun onPlayerError(error: PlaybackException) {
    val detail = error.message ?: error.errorCodeName
    emit(AndroidPlayerEvent.Error("${error.errorCodeName}: $detail"))
}
```

`error.message` for a source-type `PlaybackException` is the constant string
`"Source error"`. The nested `error.cause` — the `FileNotFoundException` that
would have named this bug on sight — is discarded, and nothing is logged. The
banner therefore says the same eleven words for a missing file, a permission
loss, and a corrupt container.

Walk the `cause` chain into the emitted message and log it at `E` with the
stack. `docs/plans/android-flat-seek-track-findings.md` already names this
failure class ("der Fehlpfad ist stumm"); this is the second sighting.

### 3. Desktop: the sync deletes and re-copies the same files run after run

`sync_events` shows `Emmure/Slave To The Game/12 A.I.mp3` deleted in run 87 as
*"no longer covered by the selection"* and rewritten in run 88, and the same
album loses six files per run in 82, 87 and 88. Run 87 also deleted
`It's Not Just a Party…` under **both** apostrophe spellings, so character
drift is still generating duplicate device objects.

Membership churn in the smart list is the likely driver — the 296-of-796
"Top rated" dependency noted in `android-flat-seek-track-findings.md`. Separate
work; recorded here because it is what puts files in the deleted state that
defect 1 then trips over.

## Out of scope, seen in passing

- **Queue search does not filter.** Typing into "Search queue" leaves all 192
  rows in place. Reproduced twice; unrelated to this bug.
- **Filename-shaped titles** (`02 Lifted`, `3 Axle`, `4 Poisons 3 Words`,
  `6 Gallon Gasoline Stomach`) with no cover. These are source-tag/artwork gaps
  in the King Conquer and Suicide Silence files, not a scan failure — the rows
  play fine and their metadata is what the tags say.
