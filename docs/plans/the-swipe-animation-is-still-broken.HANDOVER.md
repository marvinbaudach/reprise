# Handover — the swipe animation is still broken on the device

Written 2026-09-01 ~21:40, at the user's request, after they looked at the
phone and said: *"die animation ist noch komplett kaputt"* and *"als hättest du
komplett am design vorbei gearbeitet"*.

## The one thing to know before reading anything else

`the-whole-screen-moves-with-the-swipe.device-arms.md` (PR #797) reports that
the implementation matches the design. **Do not trust that conclusion.** The
user, looking at the running app, says it is broken. The measurement said
otherwise because the measurement was too narrow, not because the app is fine.

What that document actually established is much smaller than how it reads:

- a title/cover translation ratio of 1.289 vs the specified 1.282, from
  **four frames of one gesture**, of which only two have enough travel to
  constrain the number at all;
- that a crossfade to the neighbour's own cover happens at all, from
  **three frames over 165 ms**;
- that no neutral scaffold plate is drawn.

None of that is a statement about whether the animation *runs* — its
smoothness, its frame pacing, whether it plays at all rather than cutting.
A correct ratio on the frames that exist says nothing about the frames that
are missing.

## Evidence already in hand that points at "broken", and was under-weighted

All of this is in the captures under `~/.cache/reprise-swipe-arms/`, and all of
it was visible when the optimistic conclusion was written.

1. **The cover switches in a single frame.** Arm P1 (previous direction, dev
   build): the cover band is fully the old track at t = 2.190 s and fully the
   new track at t = 2.214 s, with nothing in between. This was reported as
   "0 ms trail, the regression is gone" — which is true and also entirely
   consistent with **a hard cut with no animation at all**. The same data
   supports both readings and the arm did not distinguish them. This is the
   single most important thing for the next session to resolve.

2. **Frame pacing during the gesture is irregular.** Arm R1, inter-frame gaps
   through the drag: 10, 39, 33, 9, 10, 10, **87**, 50 ms. `screenrecord` drops
   duplicate frames, so a long gap means the screen was *static* for that long.
   An 87 ms hold inside a 250 ms gesture is a visible stutter.

3. **The gesture occupies very few distinct frames.** R1: about 4 usable
   frames. V2 crossfade: 3 frames. If the animation were running at the
   device's refresh rate there would be many more distinct frames.

4. **The screen does not come to rest.** The settle precondition failed with a
   drift of 6.15 grey levels where it requires <= 1.5. Sampled once per second
   over 4 seconds at idle, the card band changed by ~5 grey levels per second,
   the title band by ~3.
   **Caveat, stated because the last conclusion was drawn too fast:** the card
   may have been in visualizer mode with bars legitimately animating. That was
   not checked at the time. Re-measure in cover mode, paused, before treating
   this as a defect.

## What is trustworthy in that document

- The **retirement of the old 0.015 brightness gate** is correct and important.
  The redesign deliberately shows the neighbour's own cover in visualizer mode
  (`neighbourCoverOpacity = 1 - near^1.6`, recorded as the approved variant).
  Pointing the old gate at the new model produces a false failure. That part
  should survive whatever happens to the rest.
- The **previous-direction regression from the parked branch does not
  reproduce** on dev in the sense measured: the title and the cover settle in
  the same frame, not 1.18 s apart. See caveat 1 — "same frame" may mean "no
  animation" rather than "correctly synchronised".
- The **harness geometry** (card rest bounds 208/870/591/1253) still holds, and
  the **settle detector does not** — it locks onto the redesign's radial fog.
  `run-arm-redesign.sh` measures settling by pixel drift instead.

## What the next session should do

1. **Ask the user what "kaputt" looks like** before measuring anything. Four
   different defects fit the evidence above — stutter, hard cut with no
   tween, wrong element moving, animation not starting — and they need
   different instruments. Do not guess from the captures.
2. **Measure the animation as motion, not as endpoints.** The existing arms
   compare frames against references. What is needed is the *trajectory*: panel
   offset per frame against wall-clock time, checked for monotonicity, for
   holds, and against the expected spring. `shift.py` in the arms directory
   already produces per-band offsets per frame and is the right starting point;
   the failure was in what was asked of the output, not in the tool.
3. **Capture at a known frame rate.** `screenrecord` dropping duplicates is
   useful for finding holds but makes "how many frames did the animation get"
   unanswerable. Consider a fixed-rate capture for the pacing question.
4. **Check reduced motion / animator scales.** They read 1.0 throughout this
   session, but the code path that honours them is worth reading; the redesign
   plan itself notes reduced motion is untested work it owes.

## Device and repo state

- The phone (`59100DLCQ006SB`) is **restored**: `io.github.marvinbaudach.reprise`
  0.1.74, app data restored from backup, SAF folder re-granted, 749 titles,
  animation scales 1.0. Backups remain at
  `~/.cache/reprise-swipe-arms/restore-2026-09-01-redesign/`.
- Each measurement costs a full uninstall/reinstall cycle: the debug keys do
  not match, `install -r` is always refused, and the SAF grant dies with the
  uninstall.
- **PR #797** (docs only) is open with the over-confident arm document in it.
  It carries a correction banner pointing here. Decide whether to land it with
  the correction or hold it.
- **PR #771** is closed unmerged, with a comment pointing at #797.
- `feature/the-throw-and-the-plate-land-together` is parked, unpushed, at
  `d349a8917d`, outside the GC's scope.
- The redesign itself landed as **#796**; `dev` is at `1f8eacadc8`. The
  `-b` worktree and branch were removed by another session, legitimately —
  `backup/strandb-preremap` still holds the pre-remap commit.
- Build environment that works (the script's defaults do not):
  `ANDROID_HOME=/home/marvin/.local/share/android-sdk`,
  `ANDROID_NDK_HOME=/opt/android-ndk`, then
  `ANDROID_TARGET=aarch64-linux-android ANDROID_ABI=arm64-v8a scripts/android-build.sh`
  before `./gradlew assembleDebug`. A worktree for this exists at
  `/home/marvin/Projects/reprise-prev-arm` (detached at dev) with a built APK.

## The methodological lesson, stated plainly

The criterion was pinned before the device was touched, which was right, and
every claim in it was checked. The mistake was that the criterion asked only
about *positions and presence* — where layers are, whether a cover appears —
and never about *motion over time*. A design document full of formulas invites
exactly that error: the formulas are easy to verify at sampled instants and
that verification feels like proof.

It is not. The user watched the animation and I measured stills.
