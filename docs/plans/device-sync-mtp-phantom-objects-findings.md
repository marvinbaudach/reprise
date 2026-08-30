# Device sync: phantom MTP objects abort every sync

Investigation on 2026-08-29/30 against the live Pixel 10 Pro XL, prompted by
`track_id=485` ("Area 64-66", Emmure) failing to transfer on every run.

## The mechanism, demonstrated

The phone's storage is case-insensitive (sdcardfs), but MTP tracks objects by
handle. Two objects can therefore name the same underlying directory or file:

```
gvfs view:   Music/Reprise/Emmure/Speaker of the Dead    <- tag-cased
             Music/Reprise/Emmure/Speaker Of The Dead    <- real directory
adb view:    Music/Reprise/Emmure/Speaker Of The Dead    (only this one)
```

Steps 1-3 reproduced on 2026-08-30; steps 4-5 observed on 2026-08-29
on a different phantom and not re-driven after this one (it was cleaned up
instead):

1. `gio copy` the same file name through both case variants — both succeed, so
   two MTP object handles now point at one real file.
2. `gio remove` through the first variant succeeds.
3. `gio remove` through the second fails: `libmtp error: could not delete
   object`. The real file is gone; the second handle survives as a phantom.
4. A directory listing still shows the phantom, so `cleanup_partials_in`
   (`crates/reprise-platform-linux/src/device_sync.rs:406`) finds it, tries to
   delete it, and fails.
5. `CleanPartials` is the first effect after `Start`
   (`crates/reprise-core/src/device_sync/machine.rs:315`); its failure sets a
   terminal error at `machine.rs:320`. **Every subsequent sync therefore aborts
   before a single byte moves**, with "could not clean partial sync files".

Observed twice in the wild before it was understood: a phantom
`08 4 Poisons 3 Words.opus.part` blocked all syncs on 2026-08-29 until the
phone's media index was rebuilt.

## Recovery that works

```
adb shell content call --uri content://media/ --method scan_volume \
    --arg external_primary
gio mount -u mtp://<device>/ && gio mount mtp://<device>/
```

The rescan drops MediaProvider entries for files that no longer exist; the
remount drops gvfs's cached listing. Both case-variant directories and the
phantom disappear. Replugging the phone does the same thing but is not
required.

## What is established, and what is not

Established:

- The duplicate case-variant directory exists in the MTP view and not on the
  real filesystem (`adb ls`).
- Deleting through one handle strands the other; that is the phantom source.
- A stranded `.part` aborts every following sync at `CleanPartials`.

Not established:

- Why `track_id=485` specifically fails its transfer with `Could not send
  object info`. It failed on every attempt: 2026-08-29 20:54Z, 2026-08-30
  04:47Z, 05:01Z and 05:05Z.

Ruled out, each by measurement:

- **Corrupt source** — ffprobe reports a valid 152.99 s, 44.1 kHz stereo FLAC
  (plus an embedded MJPEG cover).
- **Invalid target name, or the target already being present** — writing
  `02 Area 64-66.opus.part`, overwriting it, deleting the target and moving the
  partial over it all succeed manually via `gio`.
- **The case-variant directory** — 485, 490 and 494 carry the *same* album tag
  `Speaker of the Dead`, and 490/494 transferred successfully into that folder
  at 07:01 local on 2026-08-30.
- **A self-perpetuating phantom under the same name** — the transfer failed
  again at 05:05Z on a freshly rescanned device where both views agreed and no
  phantom existed.
- **A full staging directory** — `~/.cache/reprise/device-sync` empty, 454 GB
  free on /home, 193 GB free on the phone.
- **The device rejecting zero-byte objects** (which a failed transcode would
  produce) — creating a 0-byte object via `gio` succeeds.

## Suggested next step

Instrument before repairing — every external hypothesis is exhausted. The warning
at `crates/reprise-gnome/src/ui/device_sync/device_sync_effects.rs:206`
(`"device transfer failed"`) logs only `track_id` and the libmtp string; the
deviation note two lines below it already carries `device_path`, so the path is
recoverable but the *step* is not. `replace_track` reports one opaque error for
directory creation, object creation and publish alike. Name the failing
operation there so the next occurrence identifies its own step instead of
requiring this reconstruction.

Two robustness questions the mechanism raises, independent of the cause:

- One unwritable file leaves the run's removals undone: after the failure the
  run ended at 13 of 70 units and `last_synced_at` never advanced, so 31
  pending removals stayed pending indefinitely.
- `cleanup_partials_in` treats an undeletable partial as fatal. A phantom that
  no longer has a file behind it could be logged and skipped instead of
  terminating the run.

## Collateral from this investigation

`02 Area 64-66.opus` was deleted from the phone during the manual probes and,
because the sync demonstrably cannot re-copy it, was restored by hand:
`ffmpeg -c:a libopus -b:a 160k` from the same FLAC, 2 846 989 bytes, against
the app's own 3 120 664. The next successful sync will see a size difference
and replace it.
