---
slug: the-app-asks-before-fetching-artist-photos
worktree: /home/marvin/Projects/reprise-the-app-asks-before-fetching-artist-photos
branch: feature/the-app-asks-before-fetching-artist-photos
phase: coded
codex_session:
created: 2026-08-30
---
# The app asks before it fetches artist photos

## Goal

On Android, offer the artist-photo opt-in once, in the Library, instead of
leaving the switch buried three levels deep in Settings → Online sources — and
without touching the `NET-1a` default.

Evidence and root cause:
`docs/plans/android-artist-portraits-missing-findings.md`. In short: artist
portraits on Android come only from a Deezer fetch on the phone, that fetch is
gated on `ARTWORK_MODULE`, the gate is off by default, device sync carries no
images, and so all 68 artists showed fallback gradients.

## Why this shape

The desktop already answered this question and wrote the answer down in
`docs/ux-rules.md` §T. Those rules are tagged `[gtk]`, so Android is not bound
by them — but they are the project's own reasoning about this exact decision,
and the plan follows them unless there is a reason not to.

- **NET-4** `[gtk]` — "Discovery without nagging: exactly one dismissible banner
  appears in the Library … Once dismissed or acted on it never appears again;
  it is never shown when the global gate is already on, and is **never a modal
  or a toast**."
- **NET-4a** `[gtk]` — a fresh install is asked once by the first-run wizard.
- **NET-2a** `[core]` — a fresh install starts with the gate off and **no
  startup dialog**.

Where this plan diverges from the desktop it says so and why (the affirmative
action, below).

## Decisions taken in the grill

1. **Banner, not dialog.** "Never a modal or a toast" is the project's own
   wording about this question. The Compose `AlertDialog` precedent
   (`TrackContextMenu.kt:156`, `PlaybackSettingsScreen.kt:219`) is the wrong
   pattern here.
2. **No scan condition.** `LibrarySession.kt:131` `autoScan()` already runs on
   any start more than `AUTOMATIC_SCAN_INTERVAL_MS` (5 min) after the last
   scan, so "after a scan" and "the library has artists" are nearly the same
   trigger. Keying on the library alone is one moving part fewer, cannot miss
   its moment, and still never fires on an empty library.
3. **The gate coupling is accepted, as a named assumption.** See below.
4. **The live-refresh fix is in scope.** A banner whose "yes" produces no
   visible change teaches that the feature does not work.
5. **"Not now" means never again** — the same wording and the same semantics as
   the desktop banner in NET-4. The permanent path is the settings page, and
   the banner says so.
6. **The banner is State, not an acknowledgement**, in the taxonomy of
   `TransientMessage.kt`: its condition can be re-read at any time, so it needs
   no timer and no lifetime of its own. It is rendered the way `browseError`
   and `ArtistPhotoProgressBar` are.
7. **It lives in the existing slot above the pager**, so it shows regardless of
   the selected tab. Restricting it to the Artists tab would mean someone who
   never opens that tab is never asked.
8. **The affirmative action enables directly** rather than sending the user to
   Preferences — a deliberate divergence from NET-4, reasoned below.
9. Copy as given in task 3.
10. **One strand.** The cut was attempted and does not carry; see
    `## Parallelität`.

### The named assumption about the gate

`crates/reprise-android-ffi/src/online_sources.rs:16-26` sets the global gate
`online-sources-enabled` **and** `ARTWORK_MODULE` together, and the core's
first-enable seed (`crates/reprise-core/src/online_sources.rs:88-98`) then
writes `RADIO_MODULE = true`.

This is accepted rather than fixed, because today the Android app has exactly
one online feature: `rg` over `android/app/src/main/java/` finds no radio,
podcast or YouTube surface, `android-ffi` references only `ARTWORK_MODULE`, and
the app keeps its own database in `filesDir`. The radio flag is inert cargo, and
on Android the "Online sources" gate *is* the artist-photo switch, so the banner
copy is not misleading.

> **Assumption, to be re-checked before it breaks:** the moment Android gains a
> second online feature, this coupling must be split — a banner about artist
> photos would then silently enable that second feature too. Splitting it means
> a narrower FFI entry point and touching the four `android-ffi` online-source
> tests. Do it *before* the second feature lands, not after.

### Why the affirmative action diverges from NET-4

The desktop banner offers "Review in Preferences" because it covers four
sources at once and cannot sensibly say "all on". Android has one switch, and
the banner already states what is sent. Routing the user to a sub-page to flip
a single switch is friction with no added information — it is exactly the
three-levels-deep path that produced the original bug report. This reasoning
belongs in the NET-4b text so it is not later mistaken for sloppiness.

## What exists today

| Piece | Where |
|---|---|
| The switch | `android/…/settings/OnlineSourcesSettingsPage.kt:18-69`, row at `:44-49` |
| Switch plumbing | `settings/SettingsNavigation.kt:109` passes `setEnabled` |
| Enable → backfill | `MainActivity.kt:300-304` — on success `startArtistPhotoBackfill()`, else `cancelArtistPhotoBackfill()` |
| Progress state | `MobileSurfaceViewModel.kt:123-137` — `artistPhotoProgress`, `visibleArtistPhotoProgress`; `startArtistPhotoBackfill()` at `:182-189` |
| Progress bar + its slot | `ArtistPhotoProgressBar.kt:65`, rendered at `BrowseScreen.kt:584`, above the `HorizontalPager` |
| Backfill FFI binding | `ArtistPhotoBackfillConnection.kt:9-27` |
| Prefetch worker | `ArtistPortraitPrefetch.kt:17-59` |
| Message taxonomy | `TransientMessage.kt` — State vs. acknowledgement |
| Automatic scan | `LibrarySession.kt:131` `autoScan()`, `:13` `AUTOMATIC_SCAN_INTERVAL_MS` |
| Scan states | `LibraryScreenState.kt:82` — `Scanning` `:87`, `Browse` `:98` |
| One-shot flag precedent | `MainActivity.kt` — `PREFERENCES_NAME = "reprise_android"`, `NOTIFICATION_PERMISSION_ASKED` |
| Gate + module coupling | `crates/reprise-android-ffi/src/online_sources.rs:16-26` |
| First-enable seed | `crates/reprise-core/src/online_sources.rs:88-98` |
| Artist cover rendering | `ArtistCover.kt`, `ArtworkCache.kt` |
| Tests | Robolectric 4.16.1 + Compose UI Test, `android/app/src/test/java/de/reprise/spike/`, `./gradlew -p android testDebugUnitTest` |

Android strings are hardcoded in Kotlin; `res/values/strings.xml` holds only
`app_name` and one plurals entry. New copy follows that convention — English
only, no i18n.

## Tasks

1. **Persist the settled state.** Add an `ARTIST_PHOTO_OFFER_SETTLED` key to the
   existing `reprise_android` SharedPreferences, next to
   `NOTIFICATION_PERMISSION_ASKED` and with the same read/write shape. Only the
   two banner actions write it.

2. **Decide when to offer.** One pure function, unit-testable without Compose,
   over: gate state, settled flag, artist count. It returns true only when the
   gate is off, the flag is unset, and the artist count is ≥ 1. No scan
   parameter — see decision 2. Take the artist count from the `Browse` state the
   Library already renders (the "68 artists" in the header); the scan summary
   carries only `added/updated/errors` and no artist count.

   Evaluate the condition **only in `Browse`, never in `Scanning`.** `Browse` is
   the state the header's artist count comes from, and a banner drawn while a
   scan is still running would land in the same slot as the progress bar.

3. **Render the banner** at `BrowseScreen.kt:584`, in the slot that already
   holds `ArtistPhotoProgressBar`, following that composable's shape and the
   State semantics of `TransientMessage.kt` — no timer, no self-dismissal.

   The two never coexist, and that is what makes the shared slot safe: the
   affirmative action sets the flag before the backfill starts, so the banner's
   condition is already false by the time the progress bar has anything to show.
   Keep that ordering.

   > **Show artist photos?**
   > Reprise can download artist portraits from Deezer. Only artist names are
   > sent, and album covers work without this.
   > `Download artist photos` · `Not now`

   The second sentence is deliberate: the original report came in as "album
   covers do not load", and the settings page already carries the same
   reassurance ("With downloads off, album covers remain available."). Nobody
   should read this banner and conclude their album covers depend on it.

4. **Wire the affirmative action** to the exact path the switch already uses at
   `MainActivity.kt:300-304` — `setOnlineSourcesEnabled(true)` followed by
   `startArtistPhotoBackfill()` — and set the flag. Do not add a second enabling
   path; `NET-1a` keeps its single authority. "Not now" sets the flag and
   nothing else.

5. **Refresh the visible list as portraits land.** Measured on the device: the
   already-rendered artist list kept its fallback gradients through the whole
   68/68 backfill and only showed photos after an app restart. Make the rows
   rebind as portraits arrive, in the spirit of `NET-6` — scroll position and
   selection intact. Touches `ArtistCover.kt` / `ArtworkCache.kt` and the
   progress state in `MobileSurfaceViewModel.kt`.

   **Establish the mechanism before fixing it.** The bug is known only by
   observation; its cause is not. Two layers can produce exactly this symptom
   and need opposite fixes: a stale entry in the in-memory LRU of
   `ArtworkCache.kt:44-73` (which has a cross-size fallback and would keep
   handing out the fallback cover even after the file exists), or a missing
   recomposition trigger on the Compose side (the cache is fine, nothing tells
   the rows to re-read it). Determine which one it is first — a test written
   against the wrong layer goes green while the bug survives — then fix that
   layer and write the regression test against it.

6. **Tests** in `android/app/src/test/java/de/reprise/spike/`:
   - the decision function: offered on the fresh path; not offered when the gate
     is on; not offered when the flag is set; not offered with zero artists
   - both actions set the flag, and the banner does not come back after either
   - the affirmative action goes through the same enable path as the settings
     switch
   - Compose: the banner renders in the Library and disappears on either action
   - a regression test for task 5: rows rebind when a portrait arrives, without
     losing scroll position

7. **Document it.** Add `NET-4b` to `docs/ux-rules.md` §T, directly after
   NET-4a:

   > **NET-4b** `[active]` `[android]` — On Android the artist-photo question is
   > asked by exactly one dismissible banner in the Library, shown whenever the
   > global gate is off, the question has not been settled, and the library
   > holds at least one artist. It carries "Download artist photos" and "Not
   > now"; either one settles the question for good, and the permanent path
   > stays Settings → Online sources. It is never a modal or a toast, and never
   > appears while the gate is already on. Unlike the `NET-4` banner it enables
   > directly instead of pointing at the settings page, because Android has a
   > single online switch and the banner already names what is sent.

## Out of scope

- `ARTWORK_MODULE.default_enabled` and the 15 tests that pin the opt-in.
- The GTK side. NET-4 and NET-4a keep working unchanged.
- Teaching device sync to carry portraits — a separate, larger piece recorded in
  the findings doc.
- The remaining loose ends from the findings doc: 812 known titles vs. 2279
  files on the device, leftover `.reprise-analysis.part` files, the
  0.1.71 / 0.1.84 version skew, and the metainfo line that promises "Missing
  covers are retrieved automatically".

## Verification

- `./gradlew -p android testDebugUnitTest` green.
- On a device with the gate off and a populated library: the banner appears in
  the Library on all three tabs; "Download artist photos" starts the backfill and
  the visible rows fill in **without** an app restart; the banner is gone and
  does not return after a restart.
- The "Not now" path is covered by the Robolectric tests only, not on the
  device. The release build is not debuggable — `run-as` is refused — so there
  is no way to clear just `ARTIST_PHOTO_OFFER_SETTLED`; getting back to an
  unsettled state means clearing app data, which also wipes the library and
  every portrait the affirmative check just downloaded. Not worth it for a
  branch the unit tests cover exactly.

## Parallelität

The cut was attempted and does not carry.

Task 5 lands in `ArtistCover.kt` / `ArtworkCache.kt` / `MobileSurfaceViewModel.kt`;
tasks 1–4 and 6 land in `BrowseScreen.kt` (784 lines), `MainActivity.kt`
(797 lines), a new banner file — and **also** `MobileSurfaceViewModel.kt`
(390 lines). That overlap is not incidental: `MobileSurfaceViewModel` is exactly
where the two halves talk to each other, since the banner triggers the backfill
whose result the live refresh displays. Two Codex agents would meet there on
their first commit, and the collision would only surface at merge time.

Task 7 (one paragraph in `docs/ux-rules.md`) is the only genuinely disjoint
piece. One worktree and one merge for a single documentation paragraph buys no
wall-clock, so it stays in the strand.

**Decision: one strand.** No `strands:` key, no suffix files.

**Post-merge cross-checks:** none — a single strand has no seam to check.
