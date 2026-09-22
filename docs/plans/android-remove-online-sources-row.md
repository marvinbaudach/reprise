---
slug: android-remove-online-sources-row
worktree: /home/marvin/Projects/reprise-android-remove-online-sources-row
branch: feature/android-remove-online-sources-row
phase: refactored
codex_session:
created: 2026-09-22
---
# The phone stops offering a page it has no question for

Remove the Android settings row **Online sources** and its page, drop the
privacy disclosure without replacement, and retire the rulebook sentence that
mandated the page — all in one merge unit.

## Why

`NET-4c` settled that the phone has no artwork question: the download is
always on, there is no switch and no off state. What is left on that page is
two paragraphs of prose and a progress bar that also lives in the Library.

The owner decided on 2026-09-21, having been told first that those two
paragraphs name what leaves the phone (Deezer, MusicBrainz, the Cover Art
Archive) and live nowhere else: **remove the whole row and its page, and drop
the disclosure without replacement.** That is an input to this plan, not a
question it reopens — but it belongs in the PR body as a *decision*, so nobody
later reads it as an oversight.

Confirmed by grep at plan time: after this change **no string in the Android
app names Deezer, MusicBrainz or the Cover Art Archive.** The only two
occurrences are `OnlineSourcesSettingsPage.kt` (deleted) and the row subtitle
in `SettingsOverview.kt` (removed).

## The rulebook is the load-bearing part

`NET-4c` [active] [core] ends with a normative sentence:

> Settings → Online sources remains the page that names what leaves the phone
> and shows a running artwork pass.

Deleting the page while that sentence is `[active]` is, by the rulebook's own
contract (`docs/ux-rules.md:3-5`), a bug in the tree. The amendment is part of
this change, not a follow-up.

**Mechanism: a replacement, not an in-place edit** (grill decision 1). The
contract at `docs/ux-rules.md:18-21` says IDs are append-only and a *meaning*
change becomes a new sub-rule; the old one stays as a signpost and its tests
are re-hung in the same commit. Dropping that sentence retires a commitment,
so it is a meaning change. `d424de1647` (#999, one commit old) is the
template — NET-4b → NET-4c with its two Rust tests renamed in the same commit.

**The second half of the sentence gets no new rule** (grill decision 2). "…and
shows a running artwork pass" stays factually true — `ArtistPhotoProgressBar`
keeps its Library host in `BrowseScreen.kt`, driven by
`MobileSurfaceViewModel.visibleArtistPhotoProgress`, entirely independent of
Settings. It is covered by the existing `ArtistPhotoProgressBarTest`, just not
rule-named, exactly as #988 and #1007 shipped Android behaviour without a rule.
A rule for it would have to be a separate `[android]` rule with its own Kotlin
test, because a rule carries exactly one level tag and NET-4d is `[core]`.

## What NOT to touch

- `ArtistPhotoProgressBar`'s Library host in `BrowseScreen.kt`, and
  `visibleArtistPhotoProgress` / `dismissArtistPhotoProgress` on the view
  model. Only the `inSettings = true` styling branch loses its last caller.
- The behaviour in `crates/reprise-android-ffi/src/online_sources.rs`. The
  artwork gate at the FFI boundary is what NET-4c/4d is *about* and stays;
  only two test function names change.
- `SET-9`, `SET-10` and `LYR-7` mention "Online sources" and are all `[gtk]` —
  the desktop Preferences, untouched.
- `NET-4b` names the page too, but it is `[replaced by NET-4c]`: a signpost,
  never edited.
- No string resources are involved; every Android string here is hardcoded
  Kotlin.
- No migration. The settings route lives only in `rememberNavController()`,
  i.e. saved instance state, and an app update clears the task anyway.

## Method note: instruct by symbol, not by line number

The Android package is `io.github.marvinbaudach.reprise`, not the
`de.reprise.spike` that AGENTS.md and the handoffs still name, and every line
number in those handoffs is a few off. The tasks below name grep targets.

## Tasks

Test-first in the deletion form: change the expectation, watch it fail, then
remove the code that made the old expectation true.

### T1 — The overview lists four sections

1. `MainActivitySettingsNavigationTest.kt`:
   - rename `overviewListsExactlyTheFiveSectionsThatExist` →
     `overviewListsExactlyTheFourSectionsThatExist`;
   - in it, drop the `onNodeWithText("Online sources")` assertion and the
     subtitle assertion, take `assertCountEquals(5)` to `4`, and **add**
     `compose.onNodeWithText("Online sources").assertDoesNotExist()` — the
     count alone documents no intent (grill decision 3);
   - take every other surviving
     `onAllNodesWithTag("settings-overview-row").assertCountEquals(5)` to `4`.
     The `assertCountEquals(0)` assertions in that file are "the overview is
     not visible" checks and stay `0`;
   - delete `theOverviewNamesTheArtworkSources` (it asserts only the vanishing
     subtitle) and `theOnlineSourcesPageOpensAndBackReturnsToTheOverview` in
     full.

   Run the suite → red.
2. `SettingsPageTransitionTest.kt`: the mid-transition `assertCountEquals(5)`
   → `4`.
3. `settings/SettingsOverview.kt`: remove the
   `ONLINE_SOURCES("online-sources")` enum member and its `SettingsSection`
   entry (`symbol = "cloud"`, title "Online sources", subtitle "Artwork from
   Deezer, MusicBrainz and the Cover Art Archive"). Suite → green.

### T2 — The page and its registration go

1. In **one** step, delete `settings/OnlineSourcesSettingsPage.kt`,
   `OnlineSourcesSettingsPageTest.kt` in full, and — in
   `ArtistPhotoProgressBarTest.kt` — the test
   `onlineSourcesUsesTheSameProgressLabels` together with its
   `OnlineSourcesSettingsPage` import.

   Why in one step: Kotlin compiles the whole test source set, so a tree in
   which the page is gone but a test still names it does not build. Every
   "run the suite" in this plan must produce a test result, never an
   unresolved-reference build failure. Every other test in
   `ArtistPhotoProgressBarTest.kt` stays and is the proof the Library host is
   untouched.
2. `settings/SettingsNavigation.kt`: remove the
   `page(SettingsRoute.ONLINE_SOURCES)` block, the `artistPhotoProgress` /
   `dismissArtistPhotoProgress` parameters, and the now-unused
   `ArtistPhotoProgress` import.
3. `BrowseScreen.kt`: drop the two arguments where `SettingsNavigation(...)` is
   called inside `SettingsOverlay`. Leave the `ArtistPhotoProgressBar(...)`
   call in the Library and both view-model members exactly as they are; remove
   an import only if the compiler reports it unused.

### T3 — `inSettings` loses its last caller

Grill decision 6: the parameter goes, no caller is left to justify it.

The test side of this already happened in T2.1 — it had to, or the tree would
not compile in between.

1. `ArtistPhotoProgressBar.kt`: remove the `inSettings` parameter from both
   `ArtistPhotoProgressBar` and `ArtistPhotoProgressCard` and the pass-through
   between them, and collapse all three conditionals to their `false` arm:
   the outer `Modifier.padding(horizontal = 12.dp).padding(bottom = 8.dp)`,
   `vertical = 11.dp`, and `Spacer(Modifier.height(8.dp))`. No test passes
   `inSettings` explicitly, so nothing else moves and the Library appearance is
   unchanged.

### T4 — NET-4c is replaced by NET-4d

1. `docs/ux-rules.md`: the `NET-4c` heading becomes
   `- **NET-4c** [replaced by NET-4d] — On Android the artwork download is
   always on` — **body untouched**, including its last sentence. Then insert,
   directly after NET-4c and before NET-5, verbatim:

   ```
   - **NET-4d** [active] [core] — On Android the artwork download is always on
     and there is no question to settle: no banner, no switch, no off state.
     `MusicLibrary::open` reads the global online-sources gate and the Artwork
     module and, when either is off, turns both on through core's own setters —
     on a fresh database and on one that still stores an earlier "off" alike. A
     database that already has both on is not written again. Core's default
     stays off and the desktop keeps the wizard of `NET-4a`; the platform
     decision lives at the FFI boundary.
   ```

   The heading shape matters: `check-ux-traceability.sh:12` only reads a level
   out of `[(active|planned)] [(core|gtk|e2e|web|android|manual)]`.
2. `crates/reprise-android-ffi/src/online_sources.rs`: rename both tests,
   prefix only, keeping the rest of each name:
   - `net_4c_a_fresh_database_opens_with_the_artwork_gate_on` →
     `net_4d_a_fresh_database_opens_with_the_artwork_gate_on`
   - `net_4c_a_stored_off_is_overridden_on_the_next_open` →
     `net_4d_a_stored_off_is_overridden_on_the_next_open`

   Without the rename the gate fires twice: NET-4d `[active]` with no test,
   and two tests citing a replaced ID.
3. `scripts/check-ux-traceability.sh` green.

### T5 — Evidence

**Control arm, measured at plan time on `dev` = `8eadb83e2c`:**
`scripts/check-ux-traceability.sh` → exit 0, "UX traceability ok: 434 active
rules covered". So a red traceability gate during the run is this change's
doing, not inherited — after T4 the number must read **435**.

Record the numbers from *this* run, never a historical count:

- `scripts/check-android-suite.sh` — suites/tests and `verdict=fresh`
- `npm --prefix android run lint`
- `scripts/check-ux-traceability.sh`
- the three scoped cargo commands below

No device run (grill decision 5). The Robolectric tests walk the real Compose
tree and see the overview rows; a screenshot of four rows adds nothing, and the
plan stays landable whether or not the phone is attached.

## Gates — scoped to the crate, not the workspace

The Rust side of this change is **two test-function renames in one crate**
(grill decision 4). Run, one at a time:

1. `cargo fmt --check`
2. `cargo clippy -p reprise-android-ffi --all-targets -- -D warnings`
3. `cargo test -p reprise-android-ffi`

Do **not** run the workspace forms of clippy or test: on this machine that
costs ~30 minutes and has been killed by memory pressure with other agents on
the box (`android-only-plans-tell-codex-to-skip-cargo-gates` — its "skip cargo
entirely" advice does *not* apply here, because this plan does touch Rust).
One Gradle or cargo invocation at a time. `check-merge-readiness.sh` is not
run; its GTK and workspace halves are vacuous for this diff — record that in
the PR body, as #1007 did.

## Landing

One PR into `dev`, squash-merged, so the rulebook amendment and the deletion
reach `dev` as a single commit — which is what the rule contract requires.

Subject (grill decision 7): **The phone stops offering a page it has no
question for**. The rule change belongs in the body, not the subject.

The PR body records:

- the owner's decision that the privacy disclosure goes without replacement,
  and that afterwards nothing in the app names Deezer, MusicBrainz or the
  Cover Art Archive;
- `NET-4c` → `NET-4d` and why it is a replacement rather than an edit;
- which gates were skipped and why.

`land.sh` handles the Android patch version bump.

## Parallelität

**No cut — one strand, and a cut here would be incorrect, not merely
unhelpful.** The rulebook amendment (T4) and the Kotlin removal (T1–T3) must
arrive in `dev` as one commit: an `[active]` rule mandating a page that is gone
is a live deviation by the contract's own definition, and
`check-ux-traceability.sh` runs in the merge gate. Splitting the work across
two branches would park that state in `dev` for as long as the second branch
takes.

The size makes the point moot anyway: four Kotlin files edited, two deleted,
one rulebook edit, one Rust rename — well under a single Codex run, and the
tasks are strictly sequential, because each expectation must go red before the
code that satisfied it is removed.

Post-merge cross-checks: none. Every verification in T5 reads only files this
single strand owns.
