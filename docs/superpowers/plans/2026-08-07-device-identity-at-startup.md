# A phone that is already plugged in when Reprise starts

## Measured, not suspected

The device identity work (#336) resolves a phone's USB serial from its volume's
`unix-device` identifier. On the real Pixel it works — but only when the phone
is plugged in *after* Reprise is running. Start Reprise with the phone already
attached and it stays a stranger: named "mtp", no `persistent_id`, transient
settings, an empty selection, and a sync that does nothing.

That was measured twice this evening. First in the app: it started at 19:16 with
the phone attached, and the legacy `mtp://…`-keyed settings row was still there
two minutes later. A simulated replug (`adb shell svc usb setFunctions none`,
then `mtp`) at 19:18 re-keyed it onto `59100DLCQ006SB` within a second, and the
sync that followed did the right thing.

Then in isolation, against the same GVfs, watching what a client can see the
instant it asks:

```
[immediately after get()] volumes=[('Pixel 10 Pro XL', mounted=False, unix-device='/dev/bus/usb/003/040')]
                          mounts=[('mtp', shadowed=False)]
[t=1s]                    volumes=[('Pixel 10 Pro XL', mounted=True,  unix-device='/dev/bus/usb/003/040')]
                          mounts=[('Pixel 10 Pro XL', shadowed=False), ('mtp', shadowed=True)]
```

That is the whole bug in four lines. `GVolumeMonitor` seeds its proxy monitors
asynchronously, and in the first instant:

- the volume **is already there, with its `unix-device` identifier** — the very
  thing the serial is resolved from — but `get_mount()` is still `None`;
- the `GDaemonMount` named "mtp" is **not yet shadowed**, because shadowing is
  established when the proxy monitor claims it.

`projected_devices` skips the volume (`if !mounted { continue }`) and then
accepts "mtp" through the unshadowed-mount fallback. The fallback is meant for
exotic backends no volume claims; here it swallows a volume that exists and is
sitting right next to it.

## What to change

Stop deciding "is this mount somebody's plumbing?" from the shadow flag alone,
which is a fact that arrives late, and start deciding it from something true at
t=0: **a mount whose root URI is some volume's activation root belongs to that
volume.** Both are in hand at the first enumeration.

Two consequences, and they are the fix:

- The unshadowed-mount fallback skips a mount whose root matches a known
  volume's activation root, whether or not the shadow flag has caught up.
- The volume branch stops requiring `get_mount()`. A volume is mounted for our
  purposes when its activation root appears among the listed mounts — which at
  t=0 it does. The "v1 shows only mounted devices" rule is preserved; only the
  way of knowing changes.

The comment above `projected_devices` already tells this story ("the shadowed
one used to win and label a Pixel 'mtp'"). This is the same failure, one layer
down: it is not that the shadowed mount wins, it is that the shadow flag is not
set yet.

## Do it as a pure function

The decision is over two plain lists — the volumes' names, activation roots and
identifiers, and the mounts' roots and shadow flags. Extract it so it can be
unit-tested without GIO:

1. At t=0 (volume present with `unix-device`, `get_mount()` empty, "mtp" not yet
   shadowed) the projection yields **one** device, named from the volume, with
   the serial as `persistent_id` — not "mtp".
2. At t=1s (volume linked, "mtp" shadowed) it yields the same single device.
   The identity must not depend on which instant the question was asked.
3. An `mtp://` mount that genuinely has no volume is still kept — the fallback
   must not be lost.
4. A volume with no matching mount is still skipped: a phone that is merely
   known but not attached must not appear as connected.

Say for each which production line you reverted and which named test went red.

## What must not change

- Names and icons still come from the volume when there is one.
- No polling, no retry timer, no sleep. The information needed is already
  present at the first enumeration; the fix is to read it, not to wait for it.

## A UX rule

Add it next to the other `MTP-*` rules in `docs/ux-rules.md`, in the house
voice: a phone is recognized as the same phone whether it was plugged in before
Reprise started or after.
