---
slug: the-launcher-reprise-could-not-see
worktree: /home/marvin/Projects/reprise-the-launcher-reprise-could-not-see
branch: feature/the-launcher-reprise-could-not-see
phase: planned
codex_session:
created: 2026-09-18
---
# The launcher Reprise could not see

Fixes **#982** — Niagara Launcher's media control shows Reprise's track but
its play/pause/next/previous buttons do nothing. Input: the device run of
2026-09-18 recorded on the issue (Pixel 10 Pro XL, GrapheneOS / Android 17,
Reprise 0.1.145 = `dev` after #980/#983) and the media3 1.11.1 sources.
Base: `origin/dev` (`3403a6e2cb`). Single strand; the cause was measured in
the plan phase, so the tasks are concrete.

## The defect

With Reprise playing, `dumpsys media_session` shows the session active,
`state=PLAYING(3)`, and `actions=7340027` — PLAY_PAUSE, PAUSE,
SKIP_TO_NEXT and SKIP_TO_PREVIOUS all advertised. A tap on Niagara's pause
or next button leaves the state untouched. The system log proves the tap
arrives: `MediaSessionService: tempAllowlistTargetPkgIfPossible
callingPackage:bitpit.launcher targetPackage:io.github.marvinbaudach.reprise
reason:MediaSessionRecord:pause`. Four milliseconds later Reprise's own
process logs exactly one line and nothing else:

```
D MediaSessionManager: Package bitpit.launcher doesn't exist
```

Control arm, same session: `adb shell input keyevent
KEYCODE_MEDIA_PLAY_PAUSE` pauses and resumes, `KEYCODE_MEDIA_NEXT` changes
the track. The notification and lock-screen controls (System UI) work too.

## The cause, measured

Four links, each read in the media3 1.11.1 sources (`androidx/media` tag
`1.11.1`, `libraries/session/...`):

1. **Package visibility.** Since API 30 an app only sees the packages it
   declares in `<queries>` (plus force-queryable system packages and apps it
   has interacted with). Reprise's manifest has no `<queries>` block at all.
   `dumpsys package queries` on the phone confirms it: Reprise has no
   queries entry, `bitpit.launcher` is not force-queryable
   (`com.android.systemui` is), and there is no interaction-based
   visibility either. So `getApplicationInfo("bitpit.launcher")` throws
   `NameNotFoundException` inside Reprise's process.
2. `legacy/MediaSessionManager.java` — `isTrustedForMediaControl` catches
   exactly that exception, logs `Package … doesn't exist` and returns
   **false** — *before* the `isEnabledNotificationListener` check that
   would have trusted Niagara (`settings get secure
   enabled_notification_listeners` lists
   `bitpit.launcher/….NotificationListener`).
3. `MediaSession.java` — the default `Callback.onConnectAsync` builds
   `AcceptedResultBuilder(session, controller)`, and that constructor hands
   an untrusted controller `DEFAULT_UNTRUSTED_PLAYER_COMMANDS`, which is
   `addAllReadOnlyCommands()` — no `COMMAND_PLAY_PAUSE`, no
   `COMMAND_SEEK_TO_NEXT`, no `COMMAND_SEEK_TO_PREVIOUS`.
4. `MediaSessionLegacyStub.dispatchSessionTaskWithPlayerCommand` →
   `ConnectedControllersManager.isPlayerCommandAvailable(controller,
   command)` is false for every transport command → the task is dropped
   silently (the stub only logs the play-while-stopped case).

Read-only commands are enough for metadata and playback state, which is why
the display works. Media keys take `onMediaButtonEvent`, which has no
per-controller command gate — hence the green control arm. System UI is
force-queryable — hence the working notification.

The suspect in the issue's first version (`getAvailableCommands()` in
`CoreControlledPlayer`) is not involved: the override no longer exists
(#979), and the advertised action mask already carries the skip bits.

## Decisions (binding)

- **Fix at the manifest, not at the session.** Declare the intent that
  makes notification-listener apps visible:

  ```xml
  <queries>
      <intent>
          <action android:name="android.service.notification.NotificationListenerService" />
      </intent>
  </queries>
  ```

  Visibility is package-wide once granted, so `getApplicationInfo` then
  succeeds for every app that exposes a `NotificationListenerService` —
  exactly the set media3 trusts among third-party apps (the other two trust
  paths, `STATUS_BAR_SERVICE` and `MEDIA_CONTENT_CONTROL`, are system apps
  and force-queryable).
- **Not** a custom `MediaSession.Callback.onConnect` that grants
  `DEFAULT_PLAYER_COMMANDS` regardless of trust: that would let *any*
  installed app drive playback, which is the exposure media3's untrusted
  default exists to close. **Not** `QUERY_ALL_PACKAGES`: broader than
  needed and a store-policy flag.
- **No behaviour change in `CoreControlledPlayer`.** Its forwarding is
  correct; the commands never reached it.
- The regression test pins the manifest line, because that is the only
  seam the JVM reaches: trust is decided inside media3 against the
  platform's `PackageManager`. The proof that the symptom is gone is the
  device run below, and the plan says so where the test is written.

## Tasks (in this order — each its own commit)

1. **Manifest.** In `android/app/src/main/AndroidManifest.xml`, add the
   `<queries>` block above `<application>` (after the `<uses-permission>`
   lines), with a comment in the manifest's existing voice that says *why*:
   a media controller that Reprise cannot resolve through `PackageManager`
   is untrusted to media3 and gets read-only commands, so a launcher's
   transport buttons silently do nothing; the intent query makes every
   notification-listener app — the set media3 trusts — visible. Name #982
   and the log line `Package … doesn't exist` so the next reader can
   recognise the failure in logcat.
2. **Regression test.** New
   `android/app/src/test/java/io/github/marvinbaudach/reprise/ManifestControllerVisibilityTest.kt`:
   read `src/main/AndroidManifest.xml` (the `File("src/main/…")` pattern
   the other tests already use, e.g. `NowPlayingCueTest`), parse it with
   `javax.xml.parsers.DocumentBuilderFactory`, and assert that a
   `manifest/queries/intent/action` element carries
   `android:name="android.service.notification.NotificationListenerService"`.
   Assertion message: what breaks when the block goes (untrusted
   controllers, read-only commands, #982). Write it first, watch it fail
   against the unmodified manifest, then land task 1's change — commit
   order stays manifest first, but the red run is part of the log.
3. **Verify locally.** `cd android && ./gradlew :app:testDebugUnitTest
   --tests '*ManifestControllerVisibilityTest*'` green; the whole Android
   suite via `scripts/check-android-suite.sh` still green (it is the
   repo's gate for `android/**`; read its verdict line from the log, never
   through a pipe).

## Out of scope — do not fold in

- Raw `next()` at the last queue position stopping playback (found in the
  swipe work, `HANDOFF-2026-09-17-song-swipe-fixes.md`).
- The `queue_snapshot_file: the Android queue snapshot is running unlocked
  error=try_lock() not supported` warning seen in the same logcat — a
  separate report.
- #981 (list chrome after search → play → back): not reproducible on
  0.1.145, tracked on its own issue.

## Post-landing (device run — the actual proof)

Owned by the session, not by Codex; needs the phone on USB, unlocked, under
`device-lock`, and the wake lock for the build.

1. Build the release APK from the worktree:
   `REPRISE_APK_WT=<worktree> ~/.cache/reprise-apk/build-apk.sh >
   $SCRATCH/apk-build.log 2>&1` (arm64 native lib + `assembleRelease`;
   without `android/keystore.properties` the release is signed with the
   debug key, the same key as the installed 0.1.145, so `adb install -r`
   keeps the library and settings). Confirm the certificate matches before
   installing: `apksigner verify --print-certs` on the new APK against the
   pulled installed `base.apk`.
2. `adb install -r <apk>`, then `adb shell am force-stop
   io.github.marvinbaudach.reprise` — trust is decided when the legacy
   controller connects, and media3 keeps a connected controller for a
   while, so the old untrusted record must go.
3. Red loop again: start playback, home screen, tap Niagara's pause
   (`adb shell input tap 610 979`) → `state=PAUSED(2)` in `dumpsys
   media_session`; tap again → `PLAYING(3)`; tap next (`795 979`) → the
   `description=` line names the next track. Logcat must no longer show
   `Package bitpit.launcher doesn't exist`. Control arm: the media keys
   still work; System UI's notification controls still work.
4. Record the run on #982 and close it.

## Parallelität

Not cut. Tasks 1–3 touch two files, one of which (the manifest) is also
what the test reads; a second strand would have nothing to own. The device
run is sequential by nature (one phone, one lease). Single strand:
`feature/the-launcher-reprise-could-not-see`, file ownership
`android/app/src/main/AndroidManifest.xml`,
`android/app/src/test/java/io/github/marvinbaudach/reprise/ManifestControllerVisibilityTest.kt`.
