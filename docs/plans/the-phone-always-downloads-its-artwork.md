---
slug: the-phone-always-downloads-its-artwork
worktree: /home/marvin/Projects/reprise-the-phone-always-downloads-its-artwork
branch: feature/the-phone-always-downloads-its-artwork
phase: reviewed
codex_session:
created: 2026-09-19
---
# The phone always downloads its artwork

Base: `origin/dev` @ `91b2c576c5` (after #985 "Album covers arrive from the
internet" and #986 "The phone computes its own waveform and spectrogram").

## Why

On the phone the artwork download — artist portraits from Deezer, album covers
from MusicBrainz and the Cover Art Archive — sits behind the online-sources
gate, which is off on a fresh database (`online_sources.rs:38-42` in core,
`ARTWORK_MODULE.default_enabled: false`, `modules.rs:141`). The user has to say
yes first: to the offer banner in the library, or to the switch on the "Online
sources" settings page. Marvin's decision (2026-09-19): **on Android the
artwork download is always on — covers and artist photos, no switch.** The
desktop keeps its consent flow; the core default does not move.

Two things follow:

1. The switch, the offer banner and the "On/Off" subtitle go. The "Online
   sources" page stays as the place that says what leaves the phone, and as
   the second place where a running artwork backfill is visible.
2. The backfill no longer has an "enable" trigger. It already starts on every
   app start (`MainActivity.kt:210`) and Rust refuses it while the gate is off
   (`artist_portrait.rs:255-269` in the FFI); with the gate opened on every
   `open`, that start is the trigger — for a fresh install and for an upgraded
   one whose database still stores "off".

The second report from the same day — "the progress bar is missing when I turn
it on in settings" — was made against an unknown build; a pre-#985 build has
no cover pass at all, and nothing in the current code hides the bar except by
design (`runId == 0`: no run; `COMPLETE` with `total == 0`: nothing to fetch).
It is not a task here. The device run checks the bar after this change; if it
is still missing there with work outstanding, that is its own diagnosis.

## Decisions settled in the grill (2026-09-19)

1. **The switch disappears; there is no off state.** Not "default on with a
   switch" — that variant would inherit the upgrade problem (a stored "off"
   cannot be told from "never decided") for a lever nobody asked to keep.
2. **The rule lives in the Android FFI, at `MusicLibrary::open`.** After
   `Db::open_migrated` (`crates/reprise-android-ffi/src/lib.rs:97`) the
   library reads the gate and the artwork module; when either is off it turns
   both on through core's own setters (`online_sources::set_enabled`, then
   `modules::set_enabled(ARTWORK_MODULE)`) — the same two calls, in the same
   order, that `set_online_sources_enabled` makes today
   (`crates/reprise-android-ffi/src/online_sources.rs:16-27`). Read before
   write: `open` runs on every process start, and a database that is already
   on must not be written. Core is not touched: its default stays `false`,
   the desktop keeps its wizard, the first-enable seed
   (`online_sources.rs:44-60` in core) still runs exactly once per database.
   Not in Kotlin (needs the setter, the write-thread guard, and only fakes in
   Robolectric), not in core (a platform distinction the shared code does not
   have).
3. **The "Online sources" page stays as the disclosure page** — section
   "Artwork", two paragraphs, the progress bar of a running backfill beneath.
   The overview row keeps its title, symbol and route with a constant
   subtitle. Not folded into "About": the privacy sentence belongs where a
   user looks for network access, and the bar already has its place there.
4. **A stored "off" is overridden; the backfill runs over any network.** Both
   are consequences of decision 1, accepted as such. No metered-network check,
   no "fetch now" button — neither was asked for, and a "Wi-Fi only" switch
   would be the switch this plan removes. Data volume is small: a cover or
   portrait is ~50–200 kB, fetched only for albums and artists without art of
   their own.
5. **The progress-bar report is a device-run item, not a task.** See "Why".
6. **One strand.** See Parallelität.
7. **Wording** as written in tasks 2 and 3 — hard-coded English in Kotlin,
   like the neighbourhood.

## Facts this plan builds on

- The app runs in one process (no `android:process` in the manifest);
  `MusicLibrary.open` is called once per process through
  `sharedMusicLibrary()` (`SharedMusicLibrary.kt:11-15`), by the activity
  (`MainActivity.kt:65`) and the service (`ReprisePlaybackService.kt:192`).
- The FFI getter `online_sources_enabled()` returns
  `network_allowed(&reader, &ARTWORK_MODULE)` — gate *and* module
  (`online_sources.rs:7-14` in the FFI). Its only readers are the switch, the
  overview subtitle and the offer condition.
- The backfill start (`MainActivity.kt:209-210`) does not check the gate in
  Kotlin; `start_artist_portrait_backfill_with` checks
  `network_allowed_or_off(ARTWORK_MODULE)` (`artist_portrait.rs:255-269`).
- `PendingToggleIntent` (`MainActivity.kt:221`) has two users; it stays.
- The library-frame banner slot is `ArtistPhotoLibraryStatus(...)` at
  `BrowseScreen.kt:864-872`: offer banner plus progress bar.

## Tasks

Per task: the test first, red, then the code. Every task names its tests.
The first commit on the branch is this plan file itself (the precedent is
`f2563246b6`): `land.sh` finds the plan by its `branch:` line, and a worktree
cut from `origin/dev` does not carry an untracked file.

### Task 1 — the gate opens itself on `open` (Rust FFI)

Files: `crates/reprise-android-ffi/src/online_sources.rs`,
`crates/reprise-android-ffi/src/lib.rs`.

- Replace the `#[uniffi::export] impl MusicLibrary` block in
  `online_sources.rs` (the getter and setter) with one crate-private function
  `pub(crate) fn open_artwork_gate(writer: &Db) -> Result<(), rusqlite::Error>`:
  read `online_sources::is_enabled` and `modules::is_enabled(ARTWORK_MODULE)`;
  if both are true return without writing; otherwise
  `online_sources::set_enabled(writer, true)` then
  `modules::set_enabled(writer, &ARTWORK_MODULE, true)` (the same order the
  setter used, so the first-enable seed runs before the explicit artwork
  write and cannot overwrite it).
- Call it in `open_with_portrait_fetcher` (`lib.rs:91-…`, the constructor
  every public `open` delegates to) right after `Db::open_migrated` at
  `lib.rs:97` and before `Db::open_ready` at `:102`, on the raw `Db` the
  writer is at that point, mapping the error to `LibraryError::Database` like
  the neighbouring calls.
- Tests, replacing the four in `online_sources.rs`
  (`the_switch_is_off_on_a_fresh_database`,
  `switching_on_survives_the_first_enable_seed`,
  `switching_off_closes_the_gate_for_fetches`,
  `an_off_and_on_cycle_leaves_the_switch_on`), reusing their fixture:
  - `a_fresh_database_opens_with_the_artwork_gate_on` — after `open`,
    `network_allowed(&reader, &ARTWORK_MODULE)` is true and
    `ONLINE_SOURCES_FIRST_ENABLE_COMPLETED_KEY` is set (the seed ran).
  - `a_stored_off_is_overridden_on_the_next_open` — write `set_enabled(false)`
    and `modules::set_enabled(ARTWORK, false)` through the writer, open the
    same path again, the gate is on.
  - The read-before-write guard has no unit test of its own — the two tests
    above pin the outcome, and the guard is a review item, not a test.
  - `the_gate_is_open_for_fetches` — the existing fetch-gate assertion from
    `switching_off_closes_the_gate_for_fetches`, inverted: a fetch through the
    artwork gate reaches the injected network step.

### Task 2 — the switch and the offer leave the app (Kotlin)

Files: `ArtistPhotoOffer.kt` (delete), `ArtistPhotoOfferBanner.kt`,
`BrowseScreen.kt`, `LibraryScreen.kt`, `MainActivity.kt`,
`MainActivitySurface.kt`, `settings/SettingsNavigation.kt`,
`settings/SettingsOverview.kt`, all under
`android/app/src/main/java/io/github/marvinbaudach/reprise/`.

- `ArtistPhotoOffer.kt`: delete (`shouldOfferArtistPhotos`,
  `rememberArtistPhotoOffer`, `ArtistPhotoOfferState`,
  `ARTIST_PHOTO_OFFER_SETTLED`). Keep `PREFERENCES_NAME` and
  `NOTIFICATION_PERMISSION_ASKED` alive — move them to the file that uses
  them (`grep -rn NOTIFICATION_PERMISSION_ASKED` finds it). The orphaned
  SharedPreferences key `artist_photo_offer_settled` on installed devices is
  left alone; nothing reads it any more.
- `ArtistPhotoOfferBanner.kt`: `ArtistPhotoLibraryStatus` loses
  `offerVisible`, `downloadArtistPhotos`, `declineArtistPhotos` and the private
  `ArtistPhotoOfferBanner`; what remains is the progress bar in the library
  frame. If nothing but the bar is left, delete the file and call
  `ArtistPhotoProgressBar(progress, dismiss)` at `BrowseScreen.kt:864` directly.
- `BrowseScreen.kt`: drop the parameters `onlineSourcesEnabled`,
  `setOnlineSourcesEnabled`, `artistPhotoOfferSettled`, `downloadArtistPhotos`,
  `declineArtistPhotos` (`:148-150` and their uses at `:864-872`,
  `:1078-1079`).
- `LibraryScreen.kt:43-45, 97-103`: the same parameters and their plumbing.
- `MainActivity.kt`: the `onlineSourcesEnabled` state (`:216`), the
  `onlineSourcesIntent` (`:221`) and the `setOnlineSourcesEnabled` handler
  (`:293-316`), the `rememberArtistPhotoOffer` wiring, and the surface read at
  `:412`. `PendingToggleIntent` stays (two users). **Keep `:209-210`** —
  `connectArtistPhotoBackfill` and the `startArtistPhotoBackfill()` on start
  are now the only trigger.
- `MainActivitySurface.kt:49-50`: drop `onlineSourcesEnabled` and
  `setOnlineSourcesEnabled`.
- `SettingsNavigation.kt:27-28, 55, 108-109`: drop the two parameters; the
  page call keeps `progress` and `dismissProgress`.
- `SettingsOverview.kt:72-76`: the row stays (title "Online sources", symbol
  "cloud", route unchanged); subtitle becomes the constant
  `"Artwork from Deezer, MusicBrainz and the Cover Art Archive"`.
- Tests:
  - delete `android/app/src/test/.../ArtistPhotoOfferTest.kt`;
  - `MainActivitySettingsNavigationTest.kt`: keep
    `theOnlineSourcesPageOpensAndBackReturnsToTheOverview`; delete
    `aFailedOnlineSourcesWriteKeepsTheSwitchAndOverviewOff`,
    `aSecondOnlineSourcesTapSubmitsTheOppositeTargetWithoutMovingEarly`,
    `net_4b_downloadUsesTheSettingsEnablePathAndSettlesBeforeTheWrite`; add
    `theOverviewNamesTheArtworkSources` (subtitle text present, no "On"/"Off");
  - `MainActivityConfigurationTest.kt:727-738`: remove the
    `blockOnlineSourcesWrites` / `awaitOnlineSourcesWrite` /
    `releaseOnlineSourcesWrites` helpers and whatever test only they served;
  - `ArtistPhotoProgressBarTest.kt`: adapt to the composable that survives in
    the library frame; the bar's own assertions do not change;
  - every remaining test fake that supplied `onlineSourcesEnabled` /
    `setOnlineSourcesEnabled` to `MainActivitySurface` loses those lines.

### Task 3 — the "Online sources" page says what leaves the phone (Kotlin)

Files: `settings/OnlineSourcesSettingsPage.kt`,
`android/app/src/test/.../OnlineSourcesSettingsPageTest.kt`.

- Signature: `OnlineSourcesSettingsPage(progress, dismissProgress, back)` — no
  `enabled`, no `setEnabled`.
- Content, top to bottom, same `LazyColumn` and spacing as today:
  1. `SettingsSectionTitle("Artwork")`
  2. body text: `"Reprise downloads artist portraits from Deezer and album
     covers from MusicBrainz and the Cover Art Archive. It fetches after an
     automatic scan, a manual scan or a restore, and while an album without a
     cover of its own is playing. A cover is only fetched for an album that
     has none — one already showing art never triggers a request."`
  3. body text: `"For that, artist names from your library are sent to Deezer
     and album titles to MusicBrainz. The app sends nothing else to the
     internet."`
  4. `ArtistPhotoProgressBar(progress, dismissProgress, inSettings = true)`
- Tests, replacing the four in `OnlineSourcesSettingsPageTest.kt`:
  - `thePageHasNoSwitch` — no toggleable node;
  - `thePageNamesTheThreeSources` — "Deezer", "MusicBrainz", "Cover Art
    Archive" present;
  - `thePageNamesTheCoverFetchPolicy` — keep the existing assertion;
  - `thePageShowsARunningBackfill` — with a `progress` whose phase is
    running, the `artist-photo-progress` tag is present; with `null` it is
    absent.

### Task 4 — gates

```
cargo fmt --check
cargo clippy -p reprise-core -p reprise-android-ffi --all-targets -- -D warnings
cargo test -p reprise-core -p reprise-android-ffi
scripts/check-android-suite.sh
npm --prefix android run lint
scripts/check-android-theme.sh
```

## Rules for the implementer — read first

**Hard environment facts** (measured, carried over from
`the-phone-analyses-its-own-music.md`):

- The Android suite needs **JDK 21**; the system default kills Robolectric.
  `JAVA_HOME=/usr/lib/jvm/java-21-openjdk` before every Gradle call.
- The FFI tests depend on `readdir` order; the suite is green with `TMPDIR=/tmp`.
- `BUILD SUCCESSFUL` proves nothing: Gradle reports `:app:testDebugUnitTest`
  as up-to-date and runs nothing. The verdict is in
  `android/app/build/test-results/testDebugUnitTest/*.xml`;
  `scripts/check-android-suite.sh` checks their freshness itself.
- Run the Android suite **only** through `scripts/check-android-suite.sh` —
  it builds the host `.so`, regenerates the UniFFI bindings from it and sets
  `LD_LIBRARY_PATH` itself. A fresh worktree has no `android/local.properties`;
  copy it from the main checkout.
- The UniFFI bindings under `android/app/src/main/java/uniffi/` are generated
  and gitignored. Task 1 removes two exported methods, so between task 1 and
  task 3 the Kotlin side does not compile — **do not run the Android suite
  there**; the first meaningful Android run is after task 3. After task 1 run
  only the binding step, by hand (the suite script has no separate entry
  point; these are its own lines `scripts/check-android-suite.sh:145-151`):

  ```
  cargo build --locked --release -p reprise-android-ffi
  rm -rf android/app/src/main/java/uniffi
  cargo run --locked --release --bin uniffi-bindgen -p reprise-android-ffi -- \
    generate --library target/release/libreprise_android_ffi.so \
    --language kotlin --out-dir android/app/src/main/java
  ```
- `scripts/check-android-theme.sh` is a text scan: no `Color.` / `Color(` in
  Kotlin outside `ui/theme/`, KDoc included.
- Long runs go to a file, never to the console; the verdict is read with
  `grep -c '^test result: FAILED'` on the log, not from the last line.
- Never build under `/tmp`; the worktree's own `target/` (AGENTS.md).
- **No device, no `adb`, no emulator.**
- Rust gates scoped to `-p reprise-core -p reprise-android-ffi`, never
  `--workspace`, never `cargo audit`. The only release build is the FFI's
  own, which the binding step and the suite script make themselves.

**Do not touch:** anything under `crates/reprise-core/`, the desktop crates,
`docs/design/android-download-progress.design.html` (a historical mock),
`ArtistPhotoBackfillConnection.kt`, `MobileSurfaceViewModel.kt`,
`artist_portrait.rs` in the FFI. No renames.

## Verification — the device run (by the pipeline session, before landing)

Holder of `device-lock`; release build.

1. **Upgrade path:** install over the existing install (gate stored off). On
   launch, without touching anything, the artwork progress bar appears in the
   library frame and on Settings → Online sources; logcat shows Deezer and
   MusicBrainz requests; `covers/downloaded/` fills under the app cache.
2. **Fresh path:** clear app data, add a folder. No offer banner after the
   scan; the backfill runs and the bar shows.
3. Settings → Online sources has no switch; the overview row reads the
   constant subtitle.
4. Kill and relaunch: the second `open` writes nothing (no settings error in
   logcat); the backfill resumes only for what is still missing.

## Risks

- Users who deliberately switched the download off lose that choice — the
  decision (grill 4), not a side effect.
- Mobile data: the backfill already ran over any network when switched on;
  now it does so for everyone. Not mitigated here (grill 4).
- A test fake somewhere still hands `onlineSourcesEnabled` to a surface
  constructor the compiler no longer accepts — the Kotlin build finds it; the
  fix is deleting the line, not keeping the parameter.

## Parallelität

**Not cut — one strand.** The FFI boundary forbids it: task 1 removes two
exported methods and tasks 2–3 remove their only Kotlin callers, and the
Android suite regenerates the bindings from the Rust in the same worktree, so
neither side can be green without the other. A strand that owned only the
Kotlin files would run its suite against an FFI that still exports the
methods and a gate that is still off — its tests would pass and prove
nothing about the feature. Beyond that the change is two Rust files and about
twelve Kotlin files, most of them deletions; a second cargo and Gradle build
would cost more wall-clock than it saves.

Post-merge cross-checks: none — no strand boundary, nothing set aside.
