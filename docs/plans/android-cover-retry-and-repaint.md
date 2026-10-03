---
slug: android-cover-retry-and-repaint
worktree: /home/marvin/Projects/reprise-android-cover-retry-and-repaint
branch: feature/android-cover-retry-and-repaint
phase: planned
codex_session:
created: 2026-10-02
---
# A downloaded cover reaches every surface, and a failed one is retried when the network returns

## Why

The hardware re-run of 2026-10-02 found two defects on the phone (0.1.170, with the real
library synced from the desktop). They are recorded in
`docs/plans/the-phone-analyses-its-own-music-device-run-2026-09-19.md`, section
"Hardware re-run, 2026-10-02".

- **C4 step 3.** An album cover that failed to download while the phone was offline is never
  fetched again once the network returns. After 2.3 minutes online, now-playing and the list
  row still showed the generated cover, and Reprise logged nothing. Only `am force-stop`
  plus a relaunch fetched it, about 11 s later. The mother plan expected the opposite
  (`the-phone-analyses-its-own-music.md`, around line 228: "the attempt is retried after
  connectivity returns (`TransientFailure`)").
- **C2, mini-player.** A cover that now-playing downloaded showed up there within 5 s, and
  in the album header and the track row. The mini-player kept the generated cover until the
  next relaunch.

## Diagnosis (verified on `origin/dev` @ `ae9c9d10f7`; line numbers are a hint, the symbol names are authoritative)

Paths are relative to `android/app/src/main/java/io/github/marvinbaudach/reprise/` unless
they start with `crates/`.

**Each composition requests its cover once.** `rememberTrackArtworkVisual`
(`TrackCover.kt:336-357`) keys its `remember` on `(trackUri, artworkSize, title, artist,
allowFetch)` and its `DisposableEffect` on `(request, artwork)`. No input carries
connectivity or a "cover arrived" signal.

**Nothing observes connectivity.** Nothing under `android/app/src/main` uses
`ConnectivityManager` or a `NetworkCallback`, and the manifest has only `INTERNET`, no
`ACCESS_NETWORK_STATE`. All network access is in the Rust FFI.

**The FFI already allows a retry.** Offline, `mb_fetch` returns `None` and core returns
`TransientFailure` (`crates/reprise-core/src/cover_download.rs:423`).
`crates/reprise-android-ffi/src/artist_portrait/album_cover.rs:124-142` memoises every
outcome except `TransientFailure` for the life of the process, and only a memoised `NotFound`
short-circuits a later call (`:129`). A second call after the network returns therefore
goes to the network again, while a definitive miss stays a cheap local answer.

**The Kotlin session pins the miss.** `LibrarySession.artworkFor`
(`LibrarySession.kt:265`) memoises a `null` per `(trackUri, size)` and never asks core
again. The reason is that a resolve reads tags through the document provider. Only
`artworkFetched` (`:294`) drops entries, and only those of the fetched track's own URI;
`clearArtworkPaths` (`:332`) drops everything, but only on a scan. A local re-resolve of any
**other** URI on the same album, or of anything after the background cover pass downloads,
therefore returns the memoised `null`.

**The artwork cache pins the placeholder.** `ArtworkCache.putGenerated(..., resolved =
true)` (`ArtworkCache.kt:197-201`) memoises a generated cover per shelf in
`resolvedFallbackShelves`, and `artwork()` (`:78-95`) answers later requests on that shelf
with it. `putArtwork` (`:158-161`) clears only its own size's fallback.
`invalidateArtistArtwork()` (`:181-190`) is the existing counterpart for the ARTIST kind.

**Most surfaces never ask again.** These call sites exist:

| Surface | Call site | Shelf | `allowFetch` |
|---|---|---|---|
| Mini-player | `LibraryFrame.kt:304` | LIST | false |
| List rows | `LibraryTrackRows.kt:388` | LIST | false |
| Album list rows | `BrowseTabs.kt:624` (`album.representativeUri`) | LIST | false |
| Now-playing sheet | `NowPlayingSheet.kt:515` | NOW_PLAYING | false |
| Dock mode | `DockMode.kt:49` | NOW_PLAYING | false |
| Now-playing scene | `NowPlayingScene.kt:240` | NOW_PLAYING | true |
| Album header | `BrowseTabs.kt:190` (`album.representativeUri`) | ARTIST_DETAIL | true |

**Local resolve finds a downloaded cover.** `track_artwork` resolves a cover from the
download cache since #997. Once both memos above are dropped, a purely local re-resolve is
therefore enough to pick up a cover that someone else downloaded. Each such re-resolve
costs one tag read for a track without embedded art.

**The cover pass reports attempts, not hits.** `coversDone`
(`crates/reprise-core/src/artist_portrait/cover_backfill.rs:310-311`) increases once per album
attempted, whether that attempt downloaded a cover, found none, or failed offline.
`toUiProgress()` (`ArtistPhotoBackfillConnection.kt:37-48`) folds it into the same `done`
that drives `MobileSurfaceViewModel.acceptArtistPhotoProgress` (`:277`) and with it the
portrait refresh.

**Artist portraits already use this pattern.** `artistPortraitRevision` is folded into the
remember key in `ArtistCover.kt` and bumped by `TrackArtwork.artistPortraitsChanged()`.
`MainActivity.kt:227` wires it through `bindArtistPortraitRefresh`.

**The cover backfill needs no restart.** The cover pass keeps no marker for a transient
failure, so the next run's `pending_albums` picks the album up again at the next app start
or scan.

**Known gap, deliberately left open (#1051).** If the app starts offline, the cover pass
ends every album as `TransientFailure`. After the network returns, only the visible
`allowFetch` surfaces ask again. Albums the user does not open keep the generated cover in
the Titles list until the next start or scan.

## Decisions (settled in the grill, 2026-10-02)

**D1 — Two revisions with different powers.** `TrackArtwork` carries two observable
revisions.

- `albumCoverRevision` means "a cover arrived somewhere". It only ever causes a **local**
  re-resolve through `resolve` / `track_artwork`, never `albumCoverFetch`. It therefore cannot
  loop and puts no load on the network, so it may reach every surface, whether that surface
  fetches or not.
- `networkReturnRevision` means "the default network came back". It is the only trigger
  allowed to fetch again, and only on a surface that is allowed to fetch (`allowFetch`).

**Every album bump runs three steps in this order:**

1. `LibrarySession.forgetArtworkMisses()` removes only the memoised `null` entries, keeps
   real paths, and increments `artworkGeneration` exactly as `artworkFetched` does, so an
   `artworkFor` already in flight cannot write a stale `null` back. `TrackArtwork` reaches it
   through an injected function, wired the same way as `resolveAlbumCoverFetched`.
2. `ArtworkCache.invalidateAlbumArtwork()` drops every TRACK-kind resolved fallback on every
   shelf. It is the counterpart of `invalidateArtistArtwork()`. The drop has to be global,
   because one album's cover is shared by tracks with other URIs.
3. The revision increments.

A bump therefore costs at most one tag read per **visible** surface that still shows a
generated cover. It does not cost one per track in the library.

**D2 — Only a surface showing a generated cover reacts.**

- `ArtworkVisual` gains a `generated` flag. `generatedVisual` sets it (`TrackCover.kt:257`),
  and every other constructor leaves it `false`.
- A surface that already shows a real cover ignores both revisions. It does not re-decode,
  does not get a new `ImageBitmap` identity, and does not recompute the fog or crossfade
  for an unchanged picture.
- The reaction lives in its own effect, keyed on the revision, and checks the flag when the
  revision changes. It must **not** key the main `DisposableEffect` on `generated`. That
  would load twice, because the arrival of the real cover flips the key again.
- The effect **skips the revision value it finds when it enters the composition**. A
  `LaunchedEffect` always runs once on entry, and without this skip every row scrolling into
  view would resolve a second time.
- The re-resolve for the album revision builds its request with `allowFetch = false`,
  whatever the surface itself allows. The guarantee in D1 lives in the code, not in a
  convention.

**D3 — When the album revision is bumped.**

- (a) **When `fetchedAlbumCoverBitmap` returns a bitmap, immediately.** That path only runs
  once local resolution has come back empty, so a hit there is a newly downloaded cover.
  The bump is posted to the main thread.
- (b) **When the background cover pass reports progress, coalesced.**
  - It reacts only to `coversDone` increases, never to the portrait pass. Otherwise every
    portrait would cost a tag read for each visible row without art.
  - Because `coversDone` counts every attempt, these bumps are coalesced in Kotlin: at most
    one bump per 2 s window, trailing, so the last increase in the window is the one that
    counts.
  - One bump is guaranteed when the run reaches `coversDone == coversTotal` or reports
    `COMPLETE`.
  - The clock is injected, so the tests run without sleeping.
  - Covers get their own callback beside the existing portrait refresh in
    `ArtistPhotoBackfillConnection.kt` / `MobileSurfaceViewModel`.

**D4 — Detecting that the network returned.** *Amended 2026-10-02 after the device
re-check.* The first version used `registerDefaultNetworkCallback` and failed on the phone:
with NordVPN, the default network is the VPN (`tun0`), and Android keeps the app's
default-network request on it while Wi-Fi is off. The VPN still reports `VALIDATED` from an
old probe, so the app never saw "offline" and never a return. The detector therefore watches
the physical uplink instead:

- **What counts as online.** At least one network with `NET_CAPABILITY_INTERNET`,
  `NET_CAPABILITY_VALIDATED` **and `NET_CAPABILITY_NOT_VPN`** exists. It is tracked with
  `registerNetworkCallback` on a request for `INTERNET` + `NOT_VPN`, keeping the set of those
  networks that are currently validated. Without a VPN this matches the default network in
  practice; with one, it follows the real uplink. A return is a transition from an empty set to
  a non-empty one. A second validated network arriving while one already exists (Wi-Fi beside
  cellular) is **not** a return.
- **Known risk, now measured.** Wi-Fi can validate before the VPN tunnel is back up, so a
  single retry can fail. Device round 2 (2026-10-02) showed exactly this: the return was
  logged 11 ms after validation, the fetch 0.46 s later came back empty, and the cover never
  appeared. D4b is the follow-up.
- **Baseline.** The baseline is the state read synchronously at start (every network from
  `allNetworks` with its capabilities, filtered as above). Otherwise a process that starts offline would take the first online
  callback as its baseline and never retry, which is the C4 case seen from a cold start.
- **Where the state lives.** The detector's state lives in `MobileSurfaceViewModel`, so it
  survives a configuration change.
- **Registration.** The callback is registered in `onStart` and unregistered in `onStop`.
  `onStart` feeds in the current state, so a return that happened while the app was in the
  background counts when it comes back.
- **No debounce.** A flapping network costs one cheap request per generated `allowFetch`
  surface, and a memoised `NotFound` never touches the network.
- **Logging.** Two `Log.i` lines with one fixed tag (`RepriseCoverRetry` is fine; the name
  does not matter as long as it is a single constant):
  - one per detected return, giving the transport and whether the network is validated;
  - one per fetch that a return triggered, giving a shortened track URI and whether it hit
    or came back empty.

  These lines exist so that the device check under NordVPN can tell a missed detection
  apart from a failed fetch.
- **Permission.** `android.permission.ACCESS_NETWORK_STATE` is needed. It is a normal
  permission and shows no prompt.

**D4b — Follow-up bumps after a return.** *Added 2026-10-02 after device round 2; the user
chose this over an FFI outcome type and over a fixed delay.*

- **What.** Every return reported by the monitor bumps `networkReturnRevision` at once, as
  before, and schedules three follow-up bumps at **+3 s, +10 s and +30 s** after that return.
  Each follow-up goes through the same path as the first bump: only an `allowFetch` surface
  that still shows a generated cover fetches again. A surface that got its cover ignores it.
- **Why this is cheap.** The core never memoises a transient failure: a DNS error, refused
  connection, timeout or 5xx returns `TransientFailure`, which writes no `.notfound2` marker
  and no entry in the FFI's `ATTEMPTED` memo. A definitive miss (MusicBrainz no-match, CAA
  404) is memoised as `NotFound` in `ATTEMPTED`, so the follow-ups for an album that really
  has no cover never touch the network. The follow-ups therefore only cost network when the
  previous attempt failed transiently, which is the case they exist for.
- **Cancellation.** A loss of every validated non-VPN network cancels the pending follow-ups.
  A new return cancels them and schedules a fresh set from its own time. `stop()` (from
  `onStop`) cancels them too. A return that happened while the app was stopped is reported by
  `start()` as before and schedules its follow-ups from then, so the background variant gets
  them.
- **Accepted gap.** If the app is stopped within 30 s of a foreground return and started again
  while still online, there is no new return, so the remaining follow-ups do not run. #1051
  covers what stays generated.
- **The schedule is injected.** The delays are named constants. The monitor takes a
  scheduler (post-delayed and cancel) with the main-thread `Handler` as the production
  default, so the JVM tests run without sleeping.
- **Logging.** One `Log.i` line under `COVER_RETRY_TAG` per follow-up bump,
  `Network return follow-up n/3`. The existing fetch line stays as it is.
- **The FFI names the outcome in logcat.** `album_cover_fetch` in
  `crates/reprise-android-ffi/src/artist_portrait/album_cover.rs` emits one `tracing` event
  per call (it reaches logcat under the existing `Reprise` tag), naming the outcome:
  `downloaded`, `not_found`, `transient`, `memoised_not_found`, `local` (local art existed)
  or `skipped` (module or network gate off, track missing, blank album or artist). No URL,
  no path beyond the album key. This is diagnosis only: the FFI return type does not change,
  and Kotlin still sees `Ok(None)` for every miss.

**D5 — Out of scope.**

- No backfill restart on network return. A new run gets a new `runId` and would flash a
  PREPARING card on every Wi-Fi/cellular handoff. The gap this leaves is #1051.
- No distinction between `TransientFailure` and `NotFound` across the FFI.
- No open-ended retry timer. D4b's three follow-ups are bounded and only follow a return.
- No Rust change beyond D4b's one log event. The memo drop, the coalescer and the retry
  logic are Kotlin.

**D6 — Android only.** This plan changes only the Android UI. That is the scope, not a
claim about the desktop. NET-5 and NET-6 cover enabling Artwork, not a network return. The
GTK connectivity monitor (`crates/reprise-gnome/src/ui/window/source_connectivity.rs`) does
not touch covers, so the desktop is unchecked. #1052 tracks checking it and, if needed,
extending NET-7b to `[gtk]`. There is nothing to expose over MCP: this is presentation, a
surface repainting what core already has.

## UX rules

Two new rules go into section T of `docs/ux-rules.md`, after `NET-6`. Each one goes in as
`[active] [android]` **in the commit that implements it**, together with its rule-named
Kotlin tests (`fun net_7a_…`, `fun net_7b_…`). `scripts/check-ux-traceability.sh` collects
`fun <id>_` from tests annotated `@Test` under `android/app/src/test`.

- **NET-7a** [active] [android] — A cover the phone downloads reaches every artwork surface
  that shows its album while that surface stays on screen. The now-playing scene and sheet,
  the mini-player, dock mode, track and album list rows and the album header replace their generated cover
  without a relaunch, a navigation or a scroll. This holds for a cover that now-playing or
  the album page downloaded itself and for one the background cover pass downloaded.
  Reaching a surface is a local read and never starts a download of its own. A surface that
  already shows a real cover keeps it unchanged and is not read again.
- **NET-7b** [active] [android] — When the phone's network connection returns (a validated network
  that is not a VPN) after being offline, every visible surface that may download a cover and still shows a generated
  one asks again. A cover found that way reaches the other surfaces by `NET-7a`. A return
  that happened while the app was in the background counts when the app comes back to the
  foreground. A switch between two online networks is not a return. A surface that already
  shows a real cover is not asked again. There is no timer, no polling, and no restart of the
  background pass.

## Tasks (test-first, in order)

Tests go under `android/app/src/test/java/io/github/marvinbaudach/reprise/`.

**Reachability rule.** Every rule-named test starts from the **starting state** and walks
the real path. No test calls `albumCoversChanged()` or `networkReturned()` directly to build
the state it checks. The model is
`ArtistArtworkTest.aPortraitWrittenAfterTheFallbackWasResolvedReplacesThatFallback`.
Robolectric and `ui-test-junit4` are already on the test classpath; see
`ArtworkCompositionTest.kt` for `createAndroidComposeRule<ComponentActivity>()`.

1. **Session memo drop.** Add `LibrarySession.forgetArtworkMisses()`.
   - Test: through a fake port, a `null` and a real path are memoised. Afterwards the `null`
     key asks the port again and the real path does not. An `artworkFor` in flight across
     the call does not write its stale `null` back. That last case follows the existing
     generation test for `artworkFetched`, if there is one; if not, write it in the same
     shape.
2. **Cache invalidation.** Add `ArtworkCache.invalidateAlbumArtwork()`. It drops
   TRACK-kind resolved fallbacks on every shelf and keeps real visuals and ARTIST entries.
   - Test (`ArtworkCacheTest`): a LIST fallback and a NOW_PLAYING fallback for two URIs are
     both gone afterwards; a real LIST visual and an ARTIST fallback survive.
3. **`generated` flag.** Add `ArtworkVisual.generated`, set only by `generatedVisual`.
   - Test: a generated visual reports `true`, a resolved one reports `false`.
4. **NET-7a, on-demand path.**
   - Add `TrackArtwork.albumCoverRevision` and a private `albumCoversChanged()` that runs the
     three steps from D1. It is called when `fetchedAlbumCoverBitmap` succeeds.
   - `rememberTrackArtworkVisual` re-resolves locally, with `allowFetch = false`, when the
     revision changes while it shows a generated cover. It skips the value it entered the
     composition with.
   - Robolectric Compose tests:
     - `net_7a_a_cover_now_playing_downloads_reaches_the_mini_player`: a LIST `TrackCover`
       and a NOW_PLAYING `allowFetch` surface for the same track. The fake resolver returns
       `null` until the fake fetcher "downloads", then a path. The LIST surface first
       resolves to the generated cover through the real path, then shows the real one. The
       fetch count stays 1.
     - `net_7a_a_cover_reaches_rows_of_other_tracks_on_the_album`: a second URI on the same
       album. **This test runs through a real `LibrarySession` over a fake port**, not
       through a fake resolver, so the memo from the diagnosis is part of what it checks.
     - `net_7a_a_cover_reaches_a_now_playing_surface_that_does_not_fetch`: an `allowFetch =
       false` surface on the NOW_PLAYING shelf, the shape of the sheet and of dock mode.
     - `net_7a_a_real_cover_is_not_read_again`: count resolve calls on a surface that
       already shows a real cover.
     - A plain test: a surface entering the composition after a bump does not resolve
       twice.
   - Add NET-7a to `docs/ux-rules.md` in this commit.
5. **NET-7a, backfill path.**
   - Add a cover-only progress callback that `coversDone` increases drive, beside the
     portrait refresh in `ArtistPhotoBackfillConnection.kt` and `MobileSurfaceViewModel`.
     Coalesce it as D3 (b) says, with an injected clock. It calls `artwork.albumCoversChanged()`,
     which becomes `internal`.
   - Wire it in `MainActivity`'s production branch next to `bindArtistPortraitRefresh`, as a
     single line.
   - Tests:
     - `net_7a_the_cover_pass_repaints_a_generated_row`: a progress update with a rising
       `coversDone` repaints a row that resolved to the generated cover, through the real
       `LibrarySession` memo.
     - Coalescing: several increases within one window give one bump; reaching
       `coversDone == coversTotal` always gives the final bump; a new `runId` resets the
       window.
     - A portrait-only increase does not bump the album revision.
6. **NET-7b, detector.** Add a pure `NetworkReturnDetector` in a new file, for example
   `NetworkReturn.kt`. Its state is held by `MobileSurfaceViewModel`.
   - The baseline is the first observation.
   - Each offline-to-online transition gives one event.
   - Repeated online observations give none, including a switch from one online network to
     another.
   - A transition across stop and start counts.
   - Plain JVM tests: `net_7b_…` for the transition, plus the cold-start-offline case and the
     online-to-online case.
7. **NET-7b, monitor and wiring.**
   - Add `ACCESS_NETWORK_STATE` to `AndroidManifest.xml`.
   - Add a `NetworkReturnMonitor` adapter in the same new file. It uses a network callback
     on `INTERNET` + `NOT_VPN` and counts validated networks (D4, amended), takes the synchronous baseline, delivers on the
     main thread, and writes the two log lines from D4.
   - Add `TrackArtwork.networkReturnRevision`. When it changes, an `allowFetch` surface that
     shows a generated cover runs its normal fetching load once.
   - `MainActivity` (762 of 800 lines) gets exactly one call each in `onStart` and `onStop`.
     All logic lives in the sibling file.
   - Tests:
     - `net_7b_a_cover_that_failed_offline_is_fetched_when_the_network_returns`: a fetch
       fails while the detector reads offline. The detector is then fed online, and the
       surface shows the cover. The mini-player surface for the same track follows by
       NET-7a.
     - `net_7b_a_real_cover_is_not_fetched_again_when_the_network_returns`.
     - A Robolectric `ShadowConnectivityManager` test for the adapter's baseline.
   - Add NET-7b to `docs/ux-rules.md` in this commit.
7b. **NET-7b, follow-up bumps (D4b).** Added after device round 2; tasks 1–7 are done.
   - In `NetworkReturn.kt`: the three delays as named constants, an injected scheduler
     (production default: the main-thread `Handler`'s `postDelayed` / `removeCallbacks`),
     scheduling on each reported return, cancellation on losing every validated network, on a
     new return and on `stop()`, and the `Network return follow-up n/3` log line. If the file
     would pass 800 lines, put the scheduler in a cohesive sibling.
   - In `album_cover.rs`: the single `tracing` outcome event from D4b, covering every return
     path of `album_cover_fetch_with`.
   - Plain JVM tests with a fake scheduler:
     - `net_7b_a_return_schedules_three_follow_ups`: one return gives the immediate bump and
       then exactly three more at +3/+10/+30 s, in order.
     - Losing the network before +10 s cancels the remaining follow-ups.
     - A second return at +5 s cancels the first set and schedules a fresh one from +5 s.
     - `stop()` cancels pending follow-ups; a return reported by the next `start()` schedules
       a fresh set.
   - Robolectric Compose test,
     `net_7b_a_fetch_that_fails_right_after_the_return_is_retried_by_a_follow_up`: the fake
     fetcher returns nothing on the first network-return attempt and a path on the next. After
     the return and the first follow-up the surface shows the real cover, and the fetch count
     is 2. A surface that already shows a real cover is not fetched by the follow-ups.
   - A Rust unit test beside the existing `album_cover` tests that the outcome classification
     used by the log event maps `NotFound`, `TransientFailure`, the memo hit and the gates as
     D4b names them. Keep it a pure function so the test needs no logger capture.
   - Update NET-7b in `docs/ux-rules.md` if its text says the fetch runs once: the fetch now
     runs on the return and up to three follow-ups.
   - The PR body says `Closes #998`: NET-7a is that issue.
8. **Gates.** Run every gate with the worktree-local environment prefix:
   `ANDROID_HOME` / `ANDROID_SDK_ROOT=/home/marvin/.local/share/android-sdk`,
   `ANDROID_USER_HOME="$PWD/.cache/android-user-home"`, `XDG_DATA_HOME="$PWD/.cache/xdg-data"`,
   `GRADLE_USER_HOME="$PWD/.gradle-user-home"`.
   - `scripts/check-android-suite.sh`
   - `npm --prefix android run lint`
   - `scripts/check-android-theme.sh`. A raw `Color.` in KDoc fails it, and the Android suite
     passing does not mean this gate passes.
   - `scripts/check-ux-traceability.sh`
   - Because D4b touches `reprise-android-ffi`: `cargo fmt --check`,
     `cargo clippy -p reprise-android-ffi --all-targets -- -D warnings` and
     `cargo test -p reprise-android-ffi`.
   - Every touched code file stays under 800 lines.

## Verification after Codex, before landing (orchestrator, not Codex)

This is a device re-check on the physical phone, run under `device-lock` and `wake-lock`.

- Build and install the branch's APK. Install it over 0.1.170 with the same signing key.
  Never uninstall.
- Probe files go only in `/sdcard/Music/Reprise/Reprise-Probe/`. Delete them and rescan
  afterwards.
- Leave airplane mode and Wi-Fi as you found them. The user runs airplane mode on, Wi-Fi on
  and NordVPN, so this is the setup in which `VALIDATED` has to arrive through the VPN.
- Read the result through the log tag from D4.

| Check | Steps | Expected |
|---|---|---|
| C4 step 3 | Go offline with `svc wifi disable`. Play a probe track without art and see the generated cover. Turn Wi-Fi on. | The cover appears in now-playing, the mini-player and the list row **without a relaunch**, within about 15 s of validation. The log shows one return and one fetch. |
| C4, background variant | As above, but the app goes to the background before Wi-Fi comes on and returns afterwards. | The cover appears on return. |
| C2, mini-player | Online. Play a probe track without art. | The mini-player shows the cover within a few seconds of now-playing. |
| Cost | Rescan with probe albums so the cover pass runs, and scroll the Titles list while it does. | No visible jank. |

Since D4b, the C4 rows also expect the `Network return follow-up n/3` lines, and the
`Reprise` tag names each fetch's outcome. A `transient` outcome followed by `downloaded` on a
later follow-up is the pass case under NordVPN.

If C4 stays red under the VPN, the log decides what failed. No return logged means the
detection failed. A return logged with an empty fetch means the fetch failed. The fix then
targets that case.

## Parallelität

**No cut: one strand.**

- Tasks 4, 5 and 7 all change `TrackCover.kt` (`TrackArtwork`, `rememberTrackArtworkVisual`).
- Tasks 1, 4 and 5 share `LibrarySession.kt` through the memo drop.
- Tasks 5 and 7 both wire into `MainActivity.kt` and `MobileSurfaceViewModel.kt`.
- The only disjoint piece is the detector and monitor in `NetworkReturn.kt` plus the manifest
  line. Its sole consumer is task 7, so making it a strand would create a merge-order chain
  with nothing running in parallel, plus a second Gradle build.

Merge order does not apply. The only post-merge check is the device re-check above, which
reads the installed app and not a sibling strand.
